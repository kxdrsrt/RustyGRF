//! Reads the data of a Lua script without running it: the top-level assignments of table
//! constructors (`Name = { [KEY.X] = "text", field = 3, { "a", "b" } }`), strings as bytes. This
//! is what the client's tables (`jobname.lub`, `itemInfo.lub`, ...) are made of. Everything else
//! (functions, calls, loops, arithmetic) is skipped, so a value that needs evaluation is
//! [`Value::Other`].
//!
//! [`source`] turns a `.lub` or `.lua` file into text that [`parse`] can read: compiled Lua is
//! decompiled with every byte of a string kept as it is (CP949 stays CP949).

use std::collections::HashMap;

use crate::luac;
use crate::luadec;

#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    /// The n-th positional entry (0-based).
    Pos(usize),
    /// `name = ...`
    Name(String),
    /// `["text"] = ...`
    Str(Vec<u8>),
    /// `[12] = ...`
    Num(f64),
    /// `[A.B] = ...` (a constant of another table)
    Path(String),
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(Vec<u8>),
    Num(f64),
    Bool(bool),
    Nil,
    /// `A.B.C` or `A["b"]`
    Path(String),
    Table(Table),
    Other,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    pub fields: Vec<(Key, Value)>,
}

impl Table {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields.iter().find_map(|(k, v)| match k {
            Key::Name(n) if n == name => Some(v),
            _ => None,
        })
    }

    pub fn get_str(&self, name: &str) -> Option<&[u8]> {
        match self.get(name)? {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The positional entries, in order.
    pub fn positional(&self) -> impl Iterator<Item = &Value> {
        self.fields.iter().filter_map(|(k, v)| match k {
            Key::Pos(_) => Some(v),
            _ => None,
        })
    }
}

impl Value {
    pub fn as_str(&self) -> Option<&[u8]> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_table(&self) -> Option<&Table> {
        match self {
            Self::Table(t) => Some(t),
            _ => None,
        }
    }
}

/// `target = value`, `target[key] = value` or `target.key = value` at the top level of a script.
#[derive(Debug, Clone, PartialEq)]
pub struct Assign {
    /// The variable, `A` or `A.B` for `A.B.c = ...`.
    pub target: String,
    pub key: Option<Key>,
    pub value: Value,
}

/// The tables of one or several scripts, later assignments adding to or replacing earlier ones.
#[derive(Debug, Default)]
pub struct Tables {
    pub map: HashMap<String, Table>,
}

impl Tables {
    pub fn add(&mut self, assigns: Vec<Assign>) {
        for a in assigns {
            match (a.key, a.value) {
                (None, Value::Table(t)) => {
                    self.map.insert(a.target, t);
                }
                (Some(k), v) => self.map.entry(a.target).or_default().fields.push((k, v)),
                (None, _) => {}
            }
        }
    }

    /// The number a constant like `jobtbl.JT_PORING` stands for.
    pub fn number(&self, path: &str) -> Option<f64> {
        let (table, name) = path.rsplit_once('.')?;
        match self.map.get(table)?.get(name)? {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }
}

/// Whether a script's text holds UTF-8 strings (so its names cannot be compared with the CP949
/// names of the archives).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Text {
    /// Strings are the bytes of the file (CP949 in the client's own scripts).
    Bytes,
    /// Strings with non-ASCII characters are UTF-8.
    Utf8,
}

/// The text of a script: compiled Lua (5.0 or 5.1) is decompiled, source is returned as it is.
/// Returns `None` for compiled Lua that cannot be read.
pub fn source(data: &[u8]) -> Option<(Vec<u8>, Text)> {
    if !data.starts_with(b"\x1bLua") {
        let utf8 = !data.is_ascii() && std::str::from_utf8(data).is_ok();
        return Some((data.to_vec(), if utf8 { Text::Utf8 } else { Text::Bytes }));
    }
    let chunk = luac::parse_any(data).ok()?;
    // every byte becomes the character of the same value, so the strings come back unchanged
    let text = luadec::decompile(&chunk, &|b: &[u8]| {
        b.iter().map(|&c| char::from(c)).collect()
    });
    let utf8 = text
        .lines()
        .next()
        .is_some_and(|l| l.contains("; strings: UTF-8"));
    // characters above 0xFF are UTF-8 text of the decompiler, the others are bytes
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        match u8::try_from(u32::from(c)) {
            Ok(b) => out.push(b),
            Err(_) => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    Some((out, if utf8 { Text::Utf8 } else { Text::Bytes }))
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Name(String),
    Str(Vec<u8>),
    Num(f64),
    /// One of the punctuation marks and operators.
    Op(&'static str),
    Eof,
}

const OPS: &[&str] = &[
    "...", "..", "==", "~=", "<=", ">=", "+", "-", "*", "/", "%", "^", "#", "<", ">", "=", "(",
    ")", "{", "}", "[", "]", ";", ":", ",", ".",
];

fn tokenize(s: &[u8]) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if s[i..].starts_with(b"--") {
            i += 2;
            if let Some((level, start)) = long_open(s, i) {
                i = long_end(s, start, level).1;
            } else {
                while i < s.len() && s[i] != b'\n' {
                    i += 1;
                }
            }
        } else if c == b'"' || c == b'\'' {
            let (text, end) = quoted(s, i);
            out.push(Tok::Str(text));
            i = end;
        } else if let Some((level, start)) = long_open(s, i) {
            let (end, next) = long_end(s, start, level);
            out.push(Tok::Str(s[start..end].to_vec()));
            i = next;
        } else if c.is_ascii_digit() || (c == b'.' && s.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            let (n, end) = number(s, i);
            out.push(Tok::Num(n));
            i = end;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < s.len() && (s[i].is_ascii_alphanumeric() || s[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Name(
                String::from_utf8_lossy(&s[start..i]).into_owned(),
            ));
        } else if let Some(op) = OPS.iter().find(|o| s[i..].starts_with(o.as_bytes())) {
            out.push(Tok::Op(op));
            i += op.len();
        } else {
            // a byte outside Lua (stray high byte): not part of any statement we read
            i += 1;
        }
    }
    out.push(Tok::Eof);
    out
}

/// `[[`, `[=[`, ... at `i`: (level, index of the first content byte).
fn long_open(s: &[u8], i: usize) -> Option<(usize, usize)> {
    if s.get(i) != Some(&b'[') {
        return None;
    }
    let mut j = i + 1;
    while s.get(j) == Some(&b'=') {
        j += 1;
    }
    (s.get(j) == Some(&b'[')).then_some((j - i - 1, j + 1))
}

/// (index of the closing bracket, index after it); the end of the text when it is not closed.
fn long_end(s: &[u8], from: usize, level: usize) -> (usize, usize) {
    let mut close = vec![b']'];
    close.extend(std::iter::repeat_n(b'=', level));
    close.push(b']');
    let mut j = from;
    while j + close.len() <= s.len() {
        if s[j..].starts_with(&close) {
            return (j, j + close.len());
        }
        j += 1;
    }
    (s.len(), s.len())
}

fn quoted(s: &[u8], start: usize) -> (Vec<u8>, usize) {
    let q = s[start];
    let mut out = Vec::new();
    let mut i = start + 1;
    while i < s.len() {
        let c = s[i];
        if c == q {
            return (out, i + 1);
        }
        if c == b'\n' {
            // an unclosed string ends at the line, like a broken file would
            return (out, i);
        }
        if c != b'\\' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let Some(&e) = s.get(i) else { break };
        i += 1;
        match e {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'a' => out.push(7),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'v' => out.push(11),
            b'0'..=b'9' => {
                let mut v = u32::from(e - b'0');
                for _ in 0..2 {
                    match s.get(i) {
                        Some(d) if d.is_ascii_digit() => {
                            v = v * 10 + u32::from(d - b'0');
                            i += 1;
                        }
                        _ => break,
                    }
                }
                out.push(v as u8);
            }
            b'\r' if s.get(i) == Some(&b'\n') => {
                i += 1;
                out.push(b'\n');
            }
            other => out.push(other),
        }
    }
    (out, i)
}

fn number(s: &[u8], start: usize) -> (f64, usize) {
    let mut i = start;
    if s[i] == b'0' && matches!(s.get(i + 1), Some(b'x' | b'X')) {
        i += 2;
        while i < s.len() && s[i].is_ascii_hexdigit() {
            i += 1;
        }
        let digits = std::str::from_utf8(&s[start + 2..i]).unwrap_or("0");
        return (u64::from_str_radix(digits, 16).unwrap_or(0) as f64, i);
    }
    while i < s.len() && (s[i].is_ascii_digit() || s[i] == b'.') {
        i += 1;
    }
    if matches!(s.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(s.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if s.get(j).is_some_and(u8::is_ascii_digit) {
            while j < s.len() && s[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    let text = std::str::from_utf8(&s[start..i]).unwrap_or("0");
    (text.parse().unwrap_or(0.0), i)
}

const MAX_DEPTH: usize = 100;

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos.min(self.toks.len() - 1)]
    }

    fn at(&self, n: usize) -> &Tok {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)]
    }

    fn next(&mut self) -> Tok {
        let t = self.peek().clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Tok::Op(o) if *o == op)
    }

    fn is_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Tok::Name(n) if n == kw)
    }

    fn eat_op(&mut self, op: &str) -> bool {
        let hit = self.is_op(op);
        if hit {
            self.pos += 1;
        }
        hit
    }

    /// Skips one balanced `( ... )`, `{ ... }` or `[ ... ]` starting at the open mark.
    fn skip_group(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.next() {
                Tok::Op("(" | "{" | "[") => depth += 1,
                Tok::Op(")" | "}" | "]") => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return;
                    }
                }
                Tok::Eof => return,
                _ => {}
            }
        }
    }

    /// Skips `function ... end` (the keyword already read).
    fn skip_function(&mut self) {
        let mut depth = 1usize;
        loop {
            match self.next() {
                Tok::Name(n) => match n.as_str() {
                    "function" | "if" | "do" | "repeat" => depth += 1,
                    "end" | "until" => {
                        depth -= 1;
                        if depth == 0 {
                            return;
                        }
                    }
                    _ => {}
                },
                Tok::Eof => return,
                _ => {}
            }
        }
    }

    fn table(&mut self, depth: usize) -> Table {
        // the `{` is read
        let mut t = Table::default();
        let mut n = 0usize;
        loop {
            while self.eat_op(",") || self.eat_op(";") {}
            if self.is_op("}") || matches!(self.peek(), Tok::Eof) {
                self.eat_op("}");
                return t;
            }
            let start = self.pos;
            if self.is_op("[") && !matches!(self.at(1), Tok::Op("[" | "=")) {
                self.pos += 1;
                let k = self.key_expr(depth);
                if self.eat_op("]") && self.eat_op("=") {
                    let v = self.value(depth + 1);
                    t.fields.push((k, v));
                    continue;
                }
                self.pos = start;
            } else if matches!(self.peek(), Tok::Name(_)) && matches!(self.at(1), Tok::Op("=")) {
                let Tok::Name(name) = self.next() else {
                    unreachable!()
                };
                self.pos += 1;
                let v = self.value(depth + 1);
                t.fields.push((Key::Name(name), v));
                continue;
            }
            let v = self.value(depth + 1);
            t.fields.push((Key::Pos(n), v));
            n += 1;
            if self.pos == start {
                self.pos += 1;
            }
        }
    }

    fn key_expr(&mut self, depth: usize) -> Key {
        match self.value(depth + 1) {
            Value::Str(s) => Key::Str(s),
            Value::Num(n) => Key::Num(n),
            Value::Path(p) => Key::Path(p),
            _ => Key::Other,
        }
    }

    /// One expression. Only constructors, strings, numbers, constants and booleans are
    /// understood; anything else is consumed as [`Value::Other`].
    fn value(&mut self, depth: usize) -> Value {
        let first = self.unary(depth);
        let mut v = first;
        // binary operators: the result is not a constant any more
        loop {
            let op = match self.peek() {
                Tok::Op(
                    "+" | "-" | "*" | "/" | "%" | "^" | "<" | ">" | "<=" | ">=" | "==" | "~=",
                ) => true,
                Tok::Op("..") => {
                    // two plain strings join
                    self.pos += 1;
                    let rhs = self.unary(depth);
                    v = match (v, rhs) {
                        (Value::Str(mut a), Value::Str(b)) => {
                            a.extend(b);
                            Value::Str(a)
                        }
                        _ => Value::Other,
                    };
                    continue;
                }
                Tok::Name(n) if n == "and" || n == "or" => true,
                _ => false,
            };
            if !op {
                return v;
            }
            self.pos += 1;
            self.unary(depth);
            v = Value::Other;
        }
    }

    fn unary(&mut self, depth: usize) -> Value {
        if depth > MAX_DEPTH {
            return Value::Other;
        }
        if self.is_op("-") {
            self.pos += 1;
            return match self.unary(depth + 1) {
                Value::Num(n) => Value::Num(-n),
                _ => Value::Other,
            };
        }
        if self.is_op("#") || self.is_kw("not") {
            self.pos += 1;
            self.unary(depth + 1);
            return Value::Other;
        }
        self.primary(depth)
    }

    fn primary(&mut self, depth: usize) -> Value {
        let v = match self.next() {
            Tok::Str(s) => Value::Str(s),
            Tok::Num(n) => Value::Num(n),
            Tok::Op("{") if depth >= MAX_DEPTH => {
                self.pos -= 1;
                self.skip_group();
                Value::Other
            }
            Tok::Op("{") => Value::Table(self.table(depth)),
            Tok::Op("(") => {
                self.pos -= 1;
                self.skip_group();
                Value::Other
            }
            Tok::Name(n) => match n.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                "nil" => Value::Nil,
                "function" => {
                    self.skip_function();
                    return Value::Other;
                }
                _ => Value::Path(n),
            },
            _ => {
                return Value::Other;
            }
        };
        // `a.b`, `a["b"]`, calls and method calls
        let mut v = v;
        loop {
            if self.is_op(".") && matches!(self.at(1), Tok::Name(_)) {
                self.pos += 1;
                let Tok::Name(n) = self.next() else {
                    unreachable!()
                };
                v = match v {
                    Value::Path(p) => Value::Path(format!("{p}.{n}")),
                    _ => Value::Other,
                };
            } else if self.is_op("[") && !matches!(self.at(1), Tok::Op("[" | "=")) {
                let start = self.pos;
                self.pos += 1;
                let k = self.value(depth + 1);
                if !self.eat_op("]") {
                    self.pos = start;
                    return Value::Other;
                }
                v = match (v, k) {
                    (Value::Path(p), Value::Str(s)) => {
                        Value::Path(format!("{p}.{}", String::from_utf8_lossy(&s)))
                    }
                    _ => Value::Other,
                };
            } else if self.is_op("(") || self.is_op(":") {
                if self.eat_op(":") {
                    self.pos += 1;
                }
                if self.is_op("(") {
                    self.skip_group();
                }
                v = Value::Other;
            } else if matches!(self.peek(), Tok::Str(_)) && matches!(v, Value::Path(_)) {
                // `f "text"`
                self.pos += 1;
                v = Value::Other;
            } else if self.is_op("{") && matches!(v, Value::Path(_)) {
                self.skip_group();
                v = Value::Other;
            } else {
                return v;
            }
        }
    }

    /// `Name`, `.Name` and `[expr]` parts of an assignment target: (variable, key).
    fn target(&mut self) -> Option<(String, Option<Key>)> {
        let Tok::Name(first) = self.peek().clone() else {
            return None;
        };
        self.pos += 1;
        let mut path = first;
        let mut key: Option<Key> = None;
        loop {
            let step = if self.is_op(".") && matches!(self.at(1), Tok::Name(_)) {
                self.pos += 1;
                let Tok::Name(n) = self.next() else {
                    unreachable!()
                };
                Key::Name(n)
            } else if self.is_op("[") && !matches!(self.at(1), Tok::Op("[" | "=")) {
                self.pos += 1;
                let k = self.key_expr(0);
                if !self.eat_op("]") {
                    return None;
                }
                k
            } else {
                return Some((path, key));
            };
            // a further step: the earlier key becomes part of the variable
            match key.replace(step) {
                None => {}
                Some(Key::Name(n)) => path = format!("{path}.{n}"),
                Some(Key::Str(s)) => path = format!("{path}.{}", String::from_utf8_lossy(&s)),
                Some(_) => return None,
            }
        }
    }
}

/// The top-level assignments of a script.
pub fn parse(src: &[u8]) -> Vec<Assign> {
    let mut p = Parser {
        toks: tokenize(src),
        pos: 0,
    };
    let mut out = Vec::new();
    while !matches!(p.peek(), Tok::Eof) {
        let before = p.pos;
        match p.peek().clone() {
            Tok::Name(n) if n == "function" => {
                p.pos += 1;
                p.skip_function();
            }
            Tok::Name(n) if n == "local" => {
                p.pos += 1;
                if p.is_kw("function") {
                    p.pos += 1;
                    p.skip_function();
                } else if matches!(p.at(1), Tok::Op("=")) && matches!(p.peek(), Tok::Name(_)) {
                    let Tok::Name(name) = p.next() else {
                        unreachable!()
                    };
                    p.pos += 1;
                    let value = p.value(0);
                    out.push(Assign {
                        target: name,
                        key: None,
                        value,
                    });
                }
            }
            Tok::Name(n) if matches!(n.as_str(), "if" | "do" | "for" | "while" | "repeat") => {
                // a block at the top level is not evaluated: skip it, `for`/`while` have a `do`
                p.pos += 1;
                if matches!(n.as_str(), "for" | "while") {
                    while !p.is_kw("do") && !matches!(p.peek(), Tok::Eof) {
                        p.pos += 1;
                    }
                    p.pos += 1;
                }
                p.skip_function();
            }
            Tok::Name(_) => {
                if let Some((target, key)) = p.target()
                    && p.eat_op("=")
                {
                    let value = p.value(0);
                    out.push(Assign { target, key, value });
                } else if p.is_op("(") {
                    p.skip_group();
                }
            }
            _ => {}
        }
        if p.pos == before {
            p.pos += 1;
        }
    }
    out
}

/// Every string below a table (values and string keys), with the field name it stands under
/// (the last name on the way, `None` for entries without one).
pub fn strings<'a>(t: &'a Table, mut f: impl FnMut(Option<&'a str>, &'a [u8])) {
    fn go<'a>(t: &'a Table, name: Option<&'a str>, f: &mut impl FnMut(Option<&'a str>, &'a [u8])) {
        for (k, v) in &t.fields {
            let here = match k {
                Key::Name(n) => Some(n.as_str()),
                _ => name,
            };
            if let Key::Str(s) = k {
                f(None, s);
            }
            match v {
                Value::Str(s) => f(here, s),
                Value::Table(inner) => go(inner, here, f),
                _ => {}
            }
        }
    }
    go(t, None, &mut f);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables(src: &str) -> Tables {
        let mut t = Tables::default();
        t.add(parse(src.as_bytes()));
        t
    }

    #[test]
    fn reads_constructors_and_ignores_code() {
        let t = tables(
            r#"
-- a comment [[ with brackets
jobtbl = { JT_A = 1, JT_B = 0x10, JT_C = -3 }
Names = {
    [jobtbl.JT_A] = "poring",
    ["map.rsw"] = { "a\\b", 'c\104', [[long]] },
    nested = { x = { 1, 2 }, y = f(1, { 2 }) .. "z" },
}
Names[jobtbl.JT_B] = "added"
local function skip() Names = nil end
for i = 1, 3 do Names[i] = "loop" end
if x then Names.late = "if" end
Names.plain = "dot"
"#,
        );
        assert_eq!(t.number("jobtbl.JT_B"), Some(16.0));
        assert_eq!(t.number("jobtbl.JT_C"), Some(-3.0));
        let names = &t.map["Names"];
        assert_eq!(names.fields.len(), 5);
        assert_eq!(names.fields[0].0, Key::Path("jobtbl.JT_A".into()));
        assert_eq!(names.fields[0].1.as_str(), Some(&b"poring"[..]));
        let inner = names.fields[1].1.as_table().unwrap();
        let strs: Vec<_> = inner.positional().filter_map(Value::as_str).collect();
        assert_eq!(strs, [&b"a\\b"[..], b"ch", b"long"]);
        let nested = names.fields[2].1.as_table().unwrap();
        assert_eq!(nested.get("y"), Some(&Value::Other));
        assert_eq!(names.fields[3].1.as_str(), Some(&b"added"[..]));
        assert_eq!(names.fields[4].0, Key::Name("plain".into()));
    }

    #[test]
    fn compiled_files_keep_their_bytes() {
        // a string constant with CP949 bytes survives the decompiler unchanged
        let (text, kind) = source(b"T = { \"\xb8\xf3\" }").unwrap();
        assert_eq!(kind, Text::Bytes);
        let t = parse(&text);
        assert_eq!(
            t[0].value.as_table().unwrap().fields[0].1.as_str(),
            Some(&b"\xb8\xf3"[..])
        );
        assert_eq!(source(b"x = \"\xed\x95\x9c\"").unwrap().1, Text::Utf8);
        assert!(source(b"\x1bLua garbage").is_none());
    }
}
