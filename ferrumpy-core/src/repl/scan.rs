// Copyright 2020 The Evcxr Authors.
// Licensed under the Apache License, Version 2.0 or MIT.
// Ported to FerrumPy.

use std::iter::Peekable;
use std::str::CharIndices;

/// Return type for `validate_source_fragment`
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FragmentValidity {
    Valid,
    Invalid,
    Incomplete,
}

impl FragmentValidity {
    pub fn as_str(&self) -> &'static str {
        match self {
            FragmentValidity::Valid => "valid",
            FragmentValidity::Invalid => "invalid",
            FragmentValidity::Incomplete => "incomplete",
        }
    }
}

pub fn validate_source_fragment(source: &str) -> FragmentValidity {
    let mut stack: Vec<Bracket> = vec![];
    let mut attr_end_stack_depth: Option<usize> = None;
    let mut expects_attr_item = false;

    let mut input = source.char_indices().peekable();
    while let Some((i, c)) = input.next() {
        let mut is_attr_target = true;

        match c {
            '/' => match input.peek() {
                Some((_, '/')) => {
                    eat_comment_line(&mut input);
                    is_attr_target = false;
                }
                Some((_, '*')) => {
                    input.next();
                    if !eat_comment_block(&mut input) {
                        return FragmentValidity::Incomplete;
                    }
                    is_attr_target = false;
                }
                _ => {}
            },
            '(' => stack.push(Bracket::Round),
            '[' => stack.push(Bracket::Square),
            '{' => stack.push(Bracket::Curly),
            ')' | ']' | '}' => match (stack.pop(), c) {
                (Some(Bracket::Round), ')') | (Some(Bracket::Curly), '}') => {}
                (Some(Bracket::Square), ']') => {
                    if let Some(end_stack_depth) = attr_end_stack_depth {
                        if stack.len() == end_stack_depth {
                            attr_end_stack_depth = None;
                            expects_attr_item = true;
                            is_attr_target = false;
                        }
                    }
                }
                _ => return FragmentValidity::Invalid,
            },
            '\'' => match eat_char(&mut input) {
                Some(EatCharRes::SawInvalid) => return FragmentValidity::Invalid,
                Some(_) => {}
                None => return FragmentValidity::Incomplete,
            },
            '\"' => {
                if let Some(kind) = check_raw_str(source, i) {
                    if !eat_string(&mut input, kind) {
                        return FragmentValidity::Incomplete;
                    }
                } else {
                    return FragmentValidity::Invalid;
                }
            }
            '#' => {
                if let Some((_, '[')) = input.peek() {
                    attr_end_stack_depth = Some(stack.len());
                }
            }
            _ => {
                if c.is_whitespace() {
                    is_attr_target = false;
                }
            }
        }

        if is_attr_target {
            expects_attr_item = false;
        }
    }

    if stack.is_empty() && !expects_attr_item {
        FragmentValidity::Valid
    } else {
        FragmentValidity::Incomplete
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Bracket {
    Round,
    Square,
    Curly,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum StrKind {
    Normal,
    RawStr { hashes: usize },
}

fn check_raw_str(s: &str, quote_idx: usize) -> Option<StrKind> {
    let sb = s.as_bytes();
    let index_back = |offset: usize| {
        quote_idx
            .checked_sub(offset)
            .and_then(|i| sb.get(i).copied())
    };

    match index_back(1) {
        Some(b'r') => Some(StrKind::RawStr { hashes: 0 }),
        Some(b'#') => {
            let mut count = 1;
            loop {
                match index_back(1 + count) {
                    Some(b'#') => count += 1,
                    Some(b'r') => break,
                    _ => return None,
                }
            }
            Some(StrKind::RawStr { hashes: count })
        }
        _ => Some(StrKind::Normal),
    }
}

fn eat_string(iter: &mut Peekable<CharIndices<'_>>, kind: StrKind) -> bool {
    let (hashes, escapes) = match kind {
        StrKind::Normal => (0, true),
        StrKind::RawStr { hashes } => (hashes, false),
    };

    while let Some((_, c)) = iter.next() {
        match c {
            '"' => {
                if hashes == 0 {
                    return true;
                }
                let mut seen = 0;
                while let Some((_, '#')) = iter.peek() {
                    iter.next();
                    seen += 1;
                    if seen == hashes {
                        return true;
                    }
                }
            }
            '\\' if escapes => {
                iter.next();
            }
            _ => {}
        }
    }
    false
}

fn eat_comment_line(iter: &mut Peekable<CharIndices<'_>>) {
    for (_, c) in iter {
        if c == '\n' {
            break;
        }
    }
}

fn eat_comment_block(iter: &mut Peekable<CharIndices<'_>>) -> bool {
    let mut depth = 1;
    while depth != 0 {
        match iter.next() {
            Some((_, '/')) if iter.peek().map(|p| p.1) == Some('*') => {
                iter.next();
                depth += 1;
            }
            Some((_, '*')) if iter.peek().map(|p| p.1) == Some('/') => {
                iter.next();
                depth -= 1;
            }
            Some(_) => {}
            None => return false,
        }
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EatCharRes {
    AteChar,
    SawLifetime,
    SawInvalid,
}

fn eat_char(input: &mut Peekable<CharIndices<'_>>) -> Option<EatCharRes> {
    let mut scratch = input.clone();
    let res = do_eat_char(&mut scratch);
    if let Some(EatCharRes::AteChar) | None = res {
        *input = scratch;
    }
    res
}

fn do_eat_char(input: &mut Peekable<CharIndices<'_>>) -> Option<EatCharRes> {
    let (_, next_c) = input.next()?;
    if next_c == '\n' || next_c == '\r' || next_c == '\t' {
        return Some(EatCharRes::SawInvalid);
    }

    if next_c == '\\' {
        let (_, c) = input.next()?;
        if !['\\', '\'', '"', 'x', 'u', 'n', 't', 'r', '0'].contains(&c) {
            return Some(EatCharRes::SawInvalid);
        }
        for (_, c) in input {
            if c == '\'' {
                return Some(EatCharRes::AteChar);
            }
            if c == '\n' {
                return Some(EatCharRes::SawInvalid);
            }
        }
        None
    } else {
        let could_be_lifetime = next_c.is_alphabetic() || next_c == '_'; // Simplified UnicodeXID
        let (_, maybe_end) = input.next()?;
        if maybe_end == '\'' {
            Some(EatCharRes::AteChar)
        } else if could_be_lifetime {
            Some(EatCharRes::SawLifetime)
        } else {
            Some(EatCharRes::SawInvalid)
        }
    }
}
