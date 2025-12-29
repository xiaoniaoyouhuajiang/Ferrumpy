#!/usr/bin/env python3
"""
Unit tests for FerrumPy type normalization.

Run with: python -m pytest tests/test_type_normalization.py -v
Or: python tests/test_type_normalization.py
"""

import sys
import os

# Add project root to path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))

from python.ferrumpy.serializer import (
    normalize_type_name,
    _remove_allocators,
    _split_generic_params,
    _simplify_module_path,
)


class TestCTypeConversion:
    """Test C-style type to Rust type conversion."""
    
    def test_signed_integers(self):
        assert normalize_type_name('int') == 'i32'
        assert normalize_type_name('signed int') == 'i32'
        assert normalize_type_name('long') == 'i64'
        assert normalize_type_name('long long') == 'i64'
        assert normalize_type_name('short') == 'i16'
        assert normalize_type_name('signed char') == 'i8'
    
    def test_unsigned_integers(self):
        assert normalize_type_name('unsigned int') == 'u32'
        assert normalize_type_name('unsigned long') == 'u64'
        assert normalize_type_name('unsigned short') == 'u16'
        assert normalize_type_name('unsigned char') == 'u8'
    
    def test_floats(self):
        assert normalize_type_name('float') == 'f32'
        assert normalize_type_name('double') == 'f64'
    
    def test_bool_char(self):
        assert normalize_type_name('_Bool') == 'bool'
        assert normalize_type_name('char') == 'char'


class TestAllocatorRemoval:
    """Test removal of allocator parameters."""
    
    def test_vec_with_global(self):
        assert _remove_allocators('Vec<i32, alloc::alloc::Global>') == 'Vec<i32>'
        assert _remove_allocators('Vec<i32, Global>') == 'Vec<i32>'
    
    def test_nested_vec_with_global(self):
        result = _remove_allocators('Vec<Vec<i32, alloc::alloc::Global>, alloc::alloc::Global>')
        assert result == 'Vec<Vec<i32>>'
    
    def test_hashmap_with_randomstate(self):
        result = _remove_allocators('HashMap<String, i32, std::hash::random::RandomState>')
        assert result == 'HashMap<String, i32>'
    
    def test_no_allocator(self):
        assert _remove_allocators('Vec<i32>') == 'Vec<i32>'
        assert _remove_allocators('String') == 'String'


class TestModulePathSimplification:
    """Test module path simplification."""
    
    def test_alloc_types(self):
        assert _simplify_module_path('alloc::vec::Vec') == 'Vec'
        assert _simplify_module_path('alloc::string::String') == 'String'
        assert _simplify_module_path('alloc::boxed::Box') == 'Box'
        assert _simplify_module_path('alloc::sync::Arc') == 'Arc'
        assert _simplify_module_path('alloc::rc::Rc') == 'Rc'
    
    def test_core_types(self):
        assert _simplify_module_path('core::option::Option') == 'Option'
        assert _simplify_module_path('core::result::Result') == 'Result'
    
    def test_std_types(self):
        assert _simplify_module_path('std::collections::hash::map::HashMap') == 'HashMap'
        assert _simplify_module_path('std::cell::RefCell') == 'RefCell'
    
    def test_user_types(self):
        assert _simplify_module_path('rust_sample::User') == 'User'
        assert _simplify_module_path('my_crate::models::Config') == 'Config'


class TestGenericParamSplitting:
    """Test splitting of generic parameters."""
    
    def test_simple_params(self):
        assert _split_generic_params('i32, String') == ['i32', 'String']
        assert _split_generic_params('i32') == ['i32']
    
    def test_nested_generics(self):
        result = _split_generic_params('Vec<i32>, Option<String>')
        assert result == ['Vec<i32>', 'Option<String>']
    
    def test_deeply_nested(self):
        result = _split_generic_params('Vec<Vec<i32>>, HashMap<String, Vec<i64>>')
        assert result == ['Vec<Vec<i32>>', 'HashMap<String, Vec<i64>>']


class TestFullNormalization:
    """Test full type normalization pipeline."""
    
    def test_simple_rust_types(self):
        assert normalize_type_name('i32') == 'i32'
        assert normalize_type_name('String') == 'String'
        assert normalize_type_name('bool') == 'bool'
    
    def test_vec_normalization(self):
        # Basic Vec
        assert normalize_type_name('alloc::vec::Vec<i32>') == 'Vec<i32>'
        # Vec with allocator
        assert normalize_type_name('alloc::vec::Vec<i32, alloc::alloc::Global>') == 'Vec<i32>'
        # Vec with C type inside
        assert normalize_type_name('Vec<int>') == 'Vec<i32>'
        assert normalize_type_name('Vec<unsigned long>') == 'Vec<u64>'
    
    def test_option_normalization(self):
        assert normalize_type_name('core::option::Option<i32>') == 'Option<i32>'
        assert normalize_type_name('Option<int>') == 'Option<i32>'
    
    def test_result_normalization(self):
        result = normalize_type_name('core::result::Result<i32, alloc::string::String>')
        assert result == 'Result<i32, String>'
    
    def test_hashmap_normalization(self):
        result = normalize_type_name('std::collections::hash::map::HashMap<alloc::string::String, i32, std::hash::random::RandomState>')
        assert result == 'HashMap<String, i32>'
    
    def test_smart_pointers(self):
        assert normalize_type_name('alloc::sync::Arc<i32>') == 'Arc<i32>'
        assert normalize_type_name('alloc::rc::Rc<i32, alloc::alloc::Global>') == 'Rc<i32>'
    
    def test_nested_vec(self):
        # Vec<Vec<i32>> with allocators
        result = normalize_type_name('alloc::vec::Vec<alloc::vec::Vec<i32, alloc::alloc::Global>, alloc::alloc::Global>')
        assert result == 'Vec<Vec<i32>>'
    
    def test_user_types(self):
        assert normalize_type_name('rust_sample::User') == 'User'
        assert normalize_type_name('my_crate::models::Config') == 'Config'
    
    def test_user_type_with_generic(self):
        result = normalize_type_name('rust_sample::Wrapper<i32>')
        assert result == 'Wrapper<i32>'
    
    def test_empty_and_simple(self):
        assert normalize_type_name('') == ''
        assert normalize_type_name('i32') == 'i32'


class TestRealWorldCases:
    """Test cases from actual LLDB output."""
    
    def test_rust_sample_variables(self):
        """Test types seen in rust_sample test program."""
        # String
        assert normalize_type_name('alloc::string::String') == 'String'
        
        # Vec<i32>
        result = normalize_type_name('alloc::vec::Vec<int, alloc::alloc::Global>')
        assert result == 'Vec<i32>'
        
        # Option<i32>
        result = normalize_type_name('core::option::Option<int>')
        assert result == 'Option<i32>'
        
        # Result<i32, String>
        result = normalize_type_name('core::result::Result<int, alloc::string::String>')
        assert result == 'Result<i32, String>'
        
        # HashMap<String, i32>
        result = normalize_type_name('std::collections::hash::map::HashMap<alloc::string::String, int, std::hash::random::RandomState>')
        assert result == 'HashMap<String, i32>'
        
        # Arc<User>
        result = normalize_type_name('alloc::sync::Arc<rust_sample::User, alloc::alloc::Global>')
        assert result == 'Arc<User>'
        
        # Rc<i32>
        result = normalize_type_name('alloc::rc::Rc<int, alloc::alloc::Global>')
        assert result == 'Rc<i32>'
        
        # User struct
        assert normalize_type_name('rust_sample::User') == 'User'
        
        # Config struct
        assert normalize_type_name('rust_sample::Config') == 'Config'


def run_tests():
    """Run all tests and report results."""
    import traceback
    
    test_classes = [
        TestCTypeConversion,
        TestAllocatorRemoval,
        TestModulePathSimplification,
        TestGenericParamSplitting,
        TestFullNormalization,
        TestRealWorldCases,
    ]
    
    total_passed = 0
    total_failed = 0
    failures = []
    
    for test_class in test_classes:
        instance = test_class()
        class_name = test_class.__name__
        
        for method_name in dir(instance):
            if method_name.startswith('test_'):
                try:
                    getattr(instance, method_name)()
                    print(f"  ✓ {class_name}.{method_name}")
                    total_passed += 1
                except AssertionError as e:
                    print(f"  ✗ {class_name}.{method_name}")
                    failures.append((f"{class_name}.{method_name}", str(e), traceback.format_exc()))
                    total_failed += 1
                except Exception as e:
                    print(f"  ✗ {class_name}.{method_name} (Exception: {e})")
                    failures.append((f"{class_name}.{method_name}", str(e), traceback.format_exc()))
                    total_failed += 1
    
    print()
    print(f"Type Normalization Tests: {total_passed}/{total_passed + total_failed} passed")
    
    if failures:
        print("\nFailures:")
        for name, msg, tb in failures:
            print(f"  {name}: {msg}")
        return 1
    
    return 0


if __name__ == '__main__':
    sys.exit(run_tests())
