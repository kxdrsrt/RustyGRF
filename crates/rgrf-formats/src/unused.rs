//! Which files of a client nothing names (`rgrf client unused`).
//!
//! The client finds most files by names written in maps, animations, effects, Lua tables, XML
//! and the program itself, often without the folder and sometimes without the extension, or
//! put together from pieces. So names are collected generously: every piece of text that could be
//! a file name goes into a set (lowercase, both with and without the extension), and a file counts
//! as named when its name is in the set. A file reported as unnamed is therefore *probably* not
//! used, never certainly: names built at run time cannot be seen.

use std::collections::HashSet;

/// Separators between names inside text or compiled Lua: control bytes (this includes the NULs of
/// string lengths) and quotes, brackets and other punctuation that is not part of a file name.
fn is_separator(b: u8) -> bool {
    b < 0x20
        || matches!(
            b,
            b'"' | b'\''
                | b','
                | b';'
                | b'('
                | b')'
                | b'<'
                | b'>'
                | b'='
                | b'|'
                | b'['
                | b']'
                | b'{'
                | b'}'
                | 0x7f
        )
}

/// The set of names found so far (lowercase bytes).
#[derive(Default)]
pub struct Names {
    set: HashSet<Vec<u8>>,
}

impl Names {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `name` as a file name (the last path piece, with and without its extension).
    pub fn add_name(&mut self, name: &[u8]) {
        let base = name
            .rsplit(|&b| b == b'\\' || b == b'/')
            .next()
            .unwrap_or(name);
        if base.is_empty() || base.len() > 120 {
            return;
        }
        let lower = base.to_ascii_lowercase();
        if let Some(dot) = lower.iter().rposition(|&b| b == b'.').filter(|&d| d > 0) {
            self.set.insert(lower[..dot].to_vec());
        }
        self.set.insert(lower);
    }

    /// Adds every name-like piece of arbitrary bytes (text, compiled Lua, a program).
    pub fn scan(&mut self, bytes: &[u8]) {
        for token in bytes.split(|&b| is_separator(b)) {
            if token.len() < 2 || token.len() > 240 {
                continue;
            }
            self.add_name(token);
            // "a.bmp b.bmp" and "icon\\x.bmp 3": pieces between spaces are names too
            if token.contains(&b' ') {
                for piece in token.split(|&b| b == b' ').filter(|p| p.len() >= 2) {
                    self.add_name(piece);
                }
            }
        }
    }

    /// Whether a file (its path or its name) is named.
    pub fn names(&self, path: &[u8]) -> bool {
        let base = path
            .rsplit(|&b| b == b'\\' || b == b'/')
            .next()
            .unwrap_or(path)
            .to_ascii_lowercase();
        let stem = base
            .iter()
            .rposition(|&b| b == b'.')
            .filter(|&d| d > 0)
            .map(|d| &base[..d]);
        self.set.contains(&base) || stem.is_some_and(|s| self.set.contains(s))
    }

    pub fn merge(&mut self, other: &Names) {
        self.set.extend(other.set.iter().cloned());
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_names_in_text_lua_and_compiled_strings() {
        let mut n = Names::new();
        n.scan(b"icon = \"UI\\\\Basic\\\\Btn_OK.bmp\"; other='poring'\n<file>x y.tga</file>");
        // a compiled Lua string: 4-byte length, the text, a NUL
        n.scan(b"\x0b\x00\x00\x00sprite\\a.spr\x00");
        for p in [
            "data\\texture\\btn_ok.BMP",
            "data\\sprite\\poring.spr",
            "y.tga",
            "a.spr",
        ] {
            assert!(n.names(p.as_bytes()), "{p}");
        }
        assert!(!n.names(b"data\\texture\\unrelated.bmp"));
    }

    #[test]
    fn korean_names_are_kept_as_bytes() {
        let mut n = Names::new();
        n.scan(b"\xb8\xf3\xbd\xba\xc5\xcd\\\xc6\xf7\xb8\xae.spr");
        assert!(n.names(b"data\\sprite\\\xb8\xf3\xbd\xba\xc5\xcd\\\xc6\xf7\xb8\xae.SPR"));
    }
}
