//! Source code transformer
//!
//! Uses syn to parse Rust source and transform it:
//! - Make all items public
//! - Remove fn main()
//! - Add serde derives

use anyhow::Result;
use quote::ToTokens;
use std::path::Path;
use syn::{
    parse_file, visit_mut::VisitMut, Attribute, Fields, Item, ItemEnum, ItemFn, ItemMod,
    ItemStruct, Type,
};

/// Transform a source file to lib format
pub fn transform_to_lib(path: &Path, remove_main: bool, add_serde: bool) -> Result<String> {
    let source = std::fs::read_to_string(path)?;
    transform_source(&source, remove_main, add_serde)
}

/// Transform a module file
pub fn transform_module(source: &str, add_serde: bool) -> Result<String> {
    transform_source(source, false, add_serde)
}

fn transform_source(source: &str, remove_main: bool, add_serde: bool) -> Result<String> {
    let mut ast = parse_file(source)?;

    // Filter out problematic inner attributes that don't apply to the companion library
    ast.attrs.retain(|attr| !is_problematic_inner_attr(attr));

    // Apply transformations
    let mut transformer = PublicityTransformer { add_serde };
    transformer.visit_file_mut(&mut ast);

    // Remove fn main if requested
    if remove_main {
        ast.items.retain(|item| !is_main_fn(item));
    }

    // Generate output
    let tokens = ast.to_token_stream();
    let code = prettyplease::unparse(&syn::parse2(tokens)?);

    // Prepend allow attributes to suppress warnings in generated code
    Ok(format!(
        "#![allow(unused_imports, dead_code, unexpected_cfgs)]\n\n{}",
        code
    ))
}

/// Check if an inner attribute should be filtered out for the companion library
fn is_problematic_inner_attr(attr: &Attribute) -> bool {
    let path = attr.path();

    // Filter out no_std attribute
    if path.is_ident("no_std") {
        return true;
    }

    // Filter out cfg_attr that references features we don't support
    // e.g., #![cfg_attr(not(feature = "std"), no_std)]
    if path.is_ident("cfg_attr") {
        let tokens = attr.to_token_stream().to_string();
        // Filter any cfg_attr that mentions "std" feature or "no_std"
        if tokens.contains("\"std\"") || tokens.contains("no_std") {
            return true;
        }
    }

    // Filter out feature attribute (used for nightly features)
    if path.is_ident("feature") {
        return true;
    }

    false
}

/// Check if any field in a struct contains a reference type
/// Reference types cannot implement Deserialize, so we skip serde derive for these
fn has_reference_fields(fields: &Fields) -> bool {
    for field in fields {
        if type_contains_reference(&field.ty) {
            return true;
        }
    }
    false
}

/// Recursively check if a type contains a reference
fn type_contains_reference(ty: &Type) -> bool {
    match ty {
        // Direct reference type
        Type::Reference(_) => true,

        // Check inside Option<T>, Vec<T>, etc.
        Type::Path(type_path) => {
            for segment in &type_path.path.segments {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    for arg in &args.args {
                        if let syn::GenericArgument::Type(inner_ty) = arg {
                            if type_contains_reference(inner_ty) {
                                return true;
                            }
                        }
                    }
                }
            }
            false
        }

        // Check tuple types
        Type::Tuple(tuple) => tuple.elems.iter().any(type_contains_reference),

        // Check array/slice types
        Type::Array(arr) => type_contains_reference(&arr.elem),
        Type::Slice(slice) => type_contains_reference(&slice.elem),

        // Check pointer types (also problematic for serde)
        Type::Ptr(_) => true,

        // Other types are assumed safe
        _ => false,
    }
}

fn is_main_fn(item: &Item) -> bool {
    if let Item::Fn(f) = item {
        f.sig.ident == "main"
    } else {
        false
    }
}

/// Visitor that makes all items public and optionally adds serde derives
struct PublicityTransformer {
    add_serde: bool,
}

impl VisitMut for PublicityTransformer {
    fn visit_item_struct_mut(&mut self, node: &mut ItemStruct) {
        // Make struct public
        node.vis = syn::parse_quote!(pub);

        // Make all fields public
        for field in &mut node.fields {
            field.vis = syn::parse_quote!(pub);
        }

        // Add serde derives if requested, but skip if struct has reference fields
        // Reference types like &'a T cannot implement Deserialize
        if self.add_serde && !has_reference_fields(&node.fields) {
            add_serde_derive(&mut node.attrs);
        }

        // Continue visiting
        syn::visit_mut::visit_item_struct_mut(self, node);
    }

    fn visit_item_enum_mut(&mut self, node: &mut ItemEnum) {
        // Make enum public
        node.vis = syn::parse_quote!(pub);

        // NOTE: Do NOT add pub to enum variant fields!
        // Rust enum variants and their fields automatically share the visibility
        // of the enum they are in. Adding pub is a compile error.

        // Add serde derives if requested
        if self.add_serde {
            add_serde_derive(&mut node.attrs);
        }

        syn::visit_mut::visit_item_enum_mut(self, node);
    }

    fn visit_item_fn_mut(&mut self, node: &mut ItemFn) {
        // Make function public (except main, which will be removed)
        if node.sig.ident != "main" {
            node.vis = syn::parse_quote!(pub);
        }
        syn::visit_mut::visit_item_fn_mut(self, node);
    }

    fn visit_item_mod_mut(&mut self, node: &mut ItemMod) {
        // Make module public
        node.vis = syn::parse_quote!(pub);

        // If inline module, visit contents
        if let Some((_, ref mut items)) = node.content {
            for item in items {
                self.visit_item_mut(item);
            }
        }

        // Don't call default visit to avoid double-visiting
    }

    fn visit_item_type_mut(&mut self, node: &mut syn::ItemType) {
        node.vis = syn::parse_quote!(pub);
        syn::visit_mut::visit_item_type_mut(self, node);
    }

    fn visit_item_const_mut(&mut self, node: &mut syn::ItemConst) {
        node.vis = syn::parse_quote!(pub);
        syn::visit_mut::visit_item_const_mut(self, node);
    }

    fn visit_item_static_mut(&mut self, node: &mut syn::ItemStatic) {
        node.vis = syn::parse_quote!(pub);
        syn::visit_mut::visit_item_static_mut(self, node);
    }

    fn visit_item_use_mut(&mut self, node: &mut syn::ItemUse) {
        // Make use statements public (pub use) so they're re-exported
        // This is critical for user types to be accessible via `use ferrumpy_snapshot::*;`
        node.vis = syn::parse_quote!(pub);
        syn::visit_mut::visit_item_use_mut(self, node);
    }
}

/// Add serde derive attributes to a struct/enum
fn add_serde_derive(attrs: &mut Vec<Attribute>) {
    // Check if serde derives already exist
    let has_serde = attrs.iter().any(|attr| {
        if let Some(ident) = attr.path().get_ident() {
            if ident == "derive" {
                let tokens = attr.to_token_stream().to_string();
                return tokens.contains("Serialize") || tokens.contains("Deserialize");
            }
        }
        false
    });

    if has_serde {
        return;
    }

    // Add new derive attribute with serde
    // (In a more sophisticated implementation, we could extend an existing derive,
    // but for simplicity we add a separate attribute)
    let new_derive: Attribute = syn::parse_quote!(
        #[derive(serde::Serialize, serde::Deserialize)]
    );
    attrs.push(new_derive);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_simple_struct() {
        let source = r#"
struct User {
    name: String,
    age: u32,
}
"#;
        let result = transform_source(source, false, true).unwrap();
        assert!(result.contains("pub struct User"));
        assert!(result.contains("pub name"));
        assert!(result.contains("Serialize"));
    }

    #[test]
    fn test_remove_main() {
        let source = r#"
fn main() {
    println!("Hello");
}

fn helper() -> i32 {
    42
}
"#;
        let result = transform_source(source, true, false).unwrap();
        assert!(!result.contains("fn main"));
        assert!(result.contains("pub fn helper"));
    }
}
