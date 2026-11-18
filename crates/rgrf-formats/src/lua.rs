//! Lua 5.1 syntax check (no execution, no syntax tree): finds the errors the client would report
//! when it loads a `.lub`/`.lua` file, with the line number.
//!
//! The RO client embeds Lua 5.1 (`luafiles514`), so this follows the 5.1 grammar: no `goto`, no
//! integer division, no `::` labels, and the 5.1 rule that a `(` on a new line after a call is
//! ambiguous. It checks syntax only; a file can still fail at run time (an undefined variable, a
//! missing table entry), which no static check of one file can find.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for LuaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for LuaError {}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Name,
    Number,
    Str,
    Kw(&'static str),
    Op(&'static str),
    Eof,
}

const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

// Longest operators first.
const OPS: &[&str] = &[
    "...", "..", "==", "~=", "<=", ">=", "+", "-", "*", "/", "%", "^", "#", "<", ">", "=", "(",
    ")", "{", "}", "[", "]", ";", ":", ",", ".",
];

struct Lexer<'a> {
    s: &'a [u8],
    pos: usize,
    line: usize,
}

type Res<T> = Result<T, LuaError>;

impl<'a> Lexer<'a> {
    fn err<T>(&self, line: usize, msg: impl Into<String>) -> Res<T> {
        Err(LuaError {
            line,
            message: msg.into(),
        })
    }

    fn peek(&self, n: usize) -> Option<u8> {
        self.s.get(self.pos + n).copied()
    }

    /// `[` followed by `=`* and `[`: the level of a long bracket, with the opener's length.
    fn long_bracket(&self) -> Option<(usize, usize)> {
        if self.peek(0) != Some(b'[') {
            return None;
        }
        let mut n = 1;
        while self.peek(n) == Some(b'=') {
            n += 1;
        }
        (self.peek(n) == Some(b'[')).then_some((n - 1, n + 1))
    }

    fn skip_long(&mut self, level: usize, open_len: usize, what: &str) -> Res<()> {
        let start = self.line;
        self.pos += open_len;
        // A newline right after the opener is not part of the string.
        if self.peek(0) == Some(b'\r') {
            self.pos += 1;
            if self.peek(0) == Some(b'\n') {
                self.pos += 1;
            }
            self.line += 1;
        } else if self.peek(0) == Some(b'\n') {
            self.pos += 1;
            if self.peek(0) == Some(b'\r') {
                self.pos += 1;
            }
            self.line += 1;
        }
        loop {
            match self.peek(0) {
                None => return self.err(start, format!("unfinished {what}")),
                Some(b']') => {
                    let mut n = 1;
                    while self.peek(n) == Some(b'=') {
                        n += 1;
                    }
                    if n - 1 == level && self.peek(n) == Some(b']') {
                        self.pos += n + 1;
                        return Ok(());
                    }
                    self.pos += 1;
                }
                Some(b'\n') => {
                    self.line += 1;
                    self.pos += 1;
                }
                Some(_) => self.pos += 1,
            }
        }
    }

    fn string(&mut self, quote: u8) -> Res<()> {
        let line = self.line;
        self.pos += 1;
        loop {
            match self.peek(0) {
                None | Some(b'\n') | Some(b'\r') => {
                    return self.err(line, "unfinished string");
                }
                Some(c) if c == quote => {
                    self.pos += 1;
                    return Ok(());
                }
                Some(b'\\') => {
                    self.pos += 1;
                    match self.peek(0) {
                        None => return self.err(line, "unfinished string"),
                        Some(b'\n') => {
                            self.line += 1;
                            self.pos += 1;
                            if self.peek(0) == Some(b'\r') {
                                self.pos += 1;
                            }
                        }
                        Some(b'\r') => {
                            self.line += 1;
                            self.pos += 1;
                            if self.peek(0) == Some(b'\n') {
                                self.pos += 1;
                            }
                        }
                        Some(d) if d.is_ascii_digit() => {
                            // up to three decimal digits, must fit a byte
                            let mut v = 0u32;
                            let mut n = 0;
                            while n < 3 && self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                                v = v * 10 + u32::from(self.peek(0).unwrap() - b'0');
                                self.pos += 1;
                                n += 1;
                            }
                            if v > 255 {
                                return self.err(line, "escape sequence too large");
                            }
                        }
                        Some(_) => self.pos += 1,
                    }
                }
                Some(_) => self.pos += 1,
            }
        }
    }

    fn number(&mut self) -> Res<()> {
        let line = self.line;
        let start = self.pos;
        if self.peek(0) == Some(b'0') && matches!(self.peek(1), Some(b'x' | b'X')) {
            self.pos += 2;
            let digits = self.pos;
            while self.peek(0).is_some_and(|c| c.is_ascii_hexdigit()) {
                self.pos += 1;
            }
            if self.pos == digits {
                return self.err(line, "malformed number");
            }
        } else {
            while self
                .peek(0)
                .is_some_and(|c| c.is_ascii_digit() || c == b'.')
            {
                self.pos += 1;
            }
            if matches!(self.peek(0), Some(b'e' | b'E')) {
                self.pos += 1;
                if matches!(self.peek(0), Some(b'+' | b'-')) {
                    self.pos += 1;
                }
                let digits = self.pos;
                while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
                    self.pos += 1;
                }
                if self.pos == digits {
                    return self.err(line, "malformed number");
                }
            }
        }
        // A letter or digit glued to a number is an error in Lua (`3x`, `1..2`).
        if self
            .peek(0)
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return self.err(line, "malformed number");
        }
        let text = &self.s[start..self.pos];
        if !text.starts_with(b"0x")
            && !text.starts_with(b"0X")
            && text.iter().filter(|&&c| c == b'.').count() > 1
        {
            return self.err(line, "malformed number");
        }
        Ok(())
    }

    /// Next token and the line it starts on.
    fn next(&mut self) -> Res<(Tok, usize)> {
        loop {
            let Some(c) = self.peek(0) else {
                return Ok((Tok::Eof, self.line));
            };
            match c {
                b'\n' => {
                    self.line += 1;
                    self.pos += 1;
                }
                b' ' | b'\t' | b'\r' | 0x0b | 0x0c => self.pos += 1,
                b'-' if self.peek(1) == Some(b'-') => {
                    self.pos += 2;
                    if let Some((level, open)) = self.long_bracket() {
                        self.skip_long(level, open, "long comment")?;
                    } else {
                        while !matches!(self.peek(0), None | Some(b'\n')) {
                            self.pos += 1;
                        }
                    }
                }
                _ => break,
            }
        }
        let line = self.line;
        let c = self.s[self.pos];
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = self.pos;
            while self
                .peek(0)
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
            {
                self.pos += 1;
            }
            let word = &self.s[start..self.pos];
            let kw = KEYWORDS.iter().find(|k| k.as_bytes() == word);
            return Ok((kw.map_or(Tok::Name, |k| Tok::Kw(k)), line));
        }
        if c.is_ascii_digit() || (c == b'.' && self.peek(1).is_some_and(|d| d.is_ascii_digit())) {
            self.number()?;
            return Ok((Tok::Number, line));
        }
        if c == b'"' || c == b'\'' {
            self.string(c)?;
            return Ok((Tok::Str, line));
        }
        if let Some((level, open)) = self.long_bracket() {
            self.skip_long(level, open, "long string")?;
            return Ok((Tok::Str, line));
        }
        if c == b'[' && self.peek(1) == Some(b'=') {
            return self.err(line, "invalid long string delimiter");
        }
        for op in OPS {
            if self.s[self.pos..].starts_with(op.as_bytes()) {
                self.pos += op.len();
                return Ok((Tok::Op(op), line));
            }
        }
        self.err(
            line,
            format!("unexpected symbol '{}'", char::from(c).escape_default()),
        )
    }
}

struct Parser<'a> {
    lx: Lexer<'a>,
    tok: Tok,
    line: usize,
    /// Line of the token before the current one (for the 5.1 "ambiguous syntax" rule).
    last_line: usize,
    depth: usize,
}

const MAX_DEPTH: usize = 200;

impl<'a> Parser<'a> {
    fn new(src: &'a [u8]) -> Res<Self> {
        let mut lx = Lexer {
            s: src,
            pos: 0,
            line: 1,
        };
        // A first line starting with '#' is skipped (shebang).
        if lx.peek(0) == Some(b'#') {
            while !matches!(lx.peek(0), None | Some(b'\n')) {
                lx.pos += 1;
            }
        }
        let (tok, line) = lx.next()?;
        Ok(Self {
            lx,
            tok,
            line,
            last_line: 1,
            depth: 0,
        })
    }

    fn err<T>(&self, msg: impl Into<String>) -> Res<T> {
        Err(LuaError {
            line: self.line,
            message: msg.into(),
        })
    }

    fn describe(&self) -> String {
        match &self.tok {
            Tok::Name => "a name".into(),
            Tok::Number => "a number".into(),
            Tok::Str => "a string".into(),
            Tok::Kw(k) => format!("'{k}'"),
            Tok::Op(o) => format!("'{o}'"),
            Tok::Eof => "end of file".into(),
        }
    }

    fn advance(&mut self) -> Res<()> {
        self.last_line = self.line;
        let (t, l) = self.lx.next()?;
        self.tok = t;
        self.line = l;
        Ok(())
    }

    fn is_op(&self, op: &str) -> bool {
        matches!(&self.tok, Tok::Op(o) if *o == op)
    }
    fn is_kw(&self, kw: &str) -> bool {
        matches!(&self.tok, Tok::Kw(k) if *k == kw)
    }

    fn accept_op(&mut self, op: &str) -> Res<bool> {
        if self.is_op(op) {
            self.advance()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn expect_op(&mut self, op: &str) -> Res<()> {
        if self.accept_op(op)? {
            Ok(())
        } else {
            self.err(format!("'{op}' expected near {}", self.describe()))
        }
    }

    fn expect_kw(&mut self, kw: &str, opener: &str, opener_line: usize) -> Res<()> {
        if self.is_kw(kw) {
            return self.advance();
        }
        if opener_line == self.line {
            self.err(format!("'{kw}' expected near {}", self.describe()))
        } else {
            self.err(format!(
                "'{kw}' expected (to close '{opener}' at line {opener_line}) near {}",
                self.describe()
            ))
        }
    }

    fn name(&mut self) -> Res<()> {
        if self.tok == Tok::Name {
            self.advance()
        } else {
            self.err(format!("a name expected near {}", self.describe()))
        }
    }

    fn enter(&mut self) -> Res<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return self.err("chunk has too many syntax levels");
        }
        Ok(())
    }

    fn block_follow(&self) -> bool {
        matches!(&self.tok, Tok::Eof)
            || matches!(&self.tok, Tok::Kw(k) if matches!(*k, "else" | "elseif" | "end" | "until"))
    }

    fn block(&mut self) -> Res<()> {
        self.enter()?;
        while !self.block_follow() {
            let was_return = self.is_kw("return");
            self.statement()?;
            self.accept_op(";")?;
            if was_return {
                break;
            }
        }
        self.depth -= 1;
        Ok(())
    }

    fn statement(&mut self) -> Res<()> {
        let line = self.line;
        match self.tok.clone() {
            Tok::Kw("if") => {
                self.advance()?;
                self.expr()?;
                self.expect_kw("then", "then", line)?;
                self.block()?;
                while self.is_kw("elseif") {
                    self.advance()?;
                    self.expr()?;
                    self.expect_kw("then", "then", line)?;
                    self.block()?;
                }
                if self.is_kw("else") {
                    self.advance()?;
                    self.block()?;
                }
                self.expect_kw("end", "if", line)
            }
            Tok::Kw("while") => {
                self.advance()?;
                self.expr()?;
                self.expect_kw("do", "do", line)?;
                self.block()?;
                self.expect_kw("end", "while", line)
            }
            Tok::Kw("do") => {
                self.advance()?;
                self.block()?;
                self.expect_kw("end", "do", line)
            }
            Tok::Kw("for") => {
                self.advance()?;
                self.name()?;
                if self.is_op("=") {
                    self.advance()?;
                    self.expr()?;
                    self.expect_op(",")?;
                    self.expr()?;
                    if self.accept_op(",")? {
                        self.expr()?;
                    }
                } else if self.is_op(",") || self.is_kw("in") {
                    while self.accept_op(",")? {
                        self.name()?;
                    }
                    self.expect_kw("in", "for", line)?;
                    self.expr_list()?;
                } else {
                    return self.err(format!("'=' or 'in' expected near {}", self.describe()));
                }
                self.expect_kw("do", "do", line)?;
                self.block()?;
                self.expect_kw("end", "for", line)
            }
            Tok::Kw("repeat") => {
                self.advance()?;
                self.block()?;
                self.expect_kw("until", "repeat", line)?;
                self.expr()
            }
            Tok::Kw("function") => {
                self.advance()?;
                self.name()?;
                while self.accept_op(".")? {
                    self.name()?;
                }
                if self.accept_op(":")? {
                    self.name()?;
                }
                self.func_body(line)
            }
            Tok::Kw("local") => {
                self.advance()?;
                if self.is_kw("function") {
                    self.advance()?;
                    self.name()?;
                    return self.func_body(line);
                }
                self.name()?;
                while self.accept_op(",")? {
                    self.name()?;
                }
                if self.accept_op("=")? {
                    self.expr_list()?;
                }
                Ok(())
            }
            Tok::Kw("return") => {
                self.advance()?;
                if !self.block_follow() && !self.is_op(";") {
                    self.expr_list()?;
                }
                Ok(())
            }
            Tok::Kw("break") => self.advance(),
            _ => self.expr_statement(),
        }
    }

    fn expr_statement(&mut self) -> Res<()> {
        let is_call = self.suffixed_expr()?;
        if self.is_op("=") || self.is_op(",") {
            if is_call {
                return self.err(format!("syntax error near {}", self.describe()));
            }
            while self.accept_op(",")? {
                if self.suffixed_expr()? {
                    return self.err("syntax error");
                }
            }
            self.expect_op("=")?;
            self.expr_list()
        } else if is_call {
            Ok(())
        } else {
            self.err(format!("syntax error near {}", self.describe()))
        }
    }

    fn func_body(&mut self, line: usize) -> Res<()> {
        self.expect_op("(")?;
        if !self.is_op(")") {
            loop {
                if self.accept_op("...")? {
                    break;
                }
                self.name()?;
                if !self.accept_op(",")? {
                    break;
                }
            }
        }
        self.expect_op(")")?;
        self.block()?;
        self.expect_kw("end", "function", line)
    }

    fn expr_list(&mut self) -> Res<()> {
        self.expr()?;
        while self.accept_op(",")? {
            self.expr()?;
        }
        Ok(())
    }

    fn primary_expr(&mut self) -> Res<()> {
        match &self.tok {
            Tok::Name => self.advance(),
            Tok::Op("(") => {
                let line = self.line;
                self.advance()?;
                self.expr()?;
                if self.is_op(")") {
                    self.advance()
                } else if line == self.line {
                    self.err(format!("')' expected near {}", self.describe()))
                } else {
                    self.err(format!(
                        "')' expected (to close '(' at line {line}) near {}",
                        self.describe()
                    ))
                }
            }
            _ => self.err(format!("unexpected symbol near {}", self.describe())),
        }
    }

    /// Returns true when the expression ends in a function call.
    fn suffixed_expr(&mut self) -> Res<bool> {
        self.primary_expr()?;
        let mut call = false;
        loop {
            match &self.tok {
                Tok::Op(".") => {
                    self.advance()?;
                    self.name()?;
                    call = false;
                }
                Tok::Op("[") => {
                    self.advance()?;
                    self.expr()?;
                    self.expect_op("]")?;
                    call = false;
                }
                Tok::Op(":") => {
                    self.advance()?;
                    self.name()?;
                    self.call_args()?;
                    call = true;
                }
                Tok::Op("(") | Tok::Str | Tok::Op("{") => {
                    self.call_args()?;
                    call = true;
                }
                _ => return Ok(call),
            }
        }
    }

    fn call_args(&mut self) -> Res<()> {
        match &self.tok {
            Tok::Op("(") => {
                // Lua 5.1: a '(' on a new line may continue the previous expression or start a new statement.
                if self.line != self.last_line {
                    return self.err("ambiguous syntax (function call x new statement)");
                }
                self.advance()?;
                if !self.is_op(")") {
                    self.expr_list()?;
                }
                let line = self.line;
                if !self.accept_op(")")? {
                    return self.err(format!(
                        "')' expected near {} (call opened before line {line})",
                        self.describe()
                    ));
                }
                Ok(())
            }
            Tok::Op("{") => self.table(),
            Tok::Str => self.advance(),
            _ => self.err(format!(
                "function arguments expected near {}",
                self.describe()
            )),
        }
    }

    fn table(&mut self) -> Res<()> {
        let line = self.line;
        self.expect_op("{")?;
        loop {
            if self.is_op("}") {
                break;
            }
            match &self.tok {
                Tok::Name => {
                    // `name = expr` or an expression starting with a name: needs one token of lookahead.
                    let save = (
                        self.lx.pos,
                        self.lx.line,
                        self.tok.clone(),
                        self.line,
                        self.last_line,
                    );
                    self.advance()?;
                    if self.is_op("=") {
                        self.advance()?;
                        self.expr()?;
                    } else {
                        (
                            self.lx.pos,
                            self.lx.line,
                            self.tok,
                            self.line,
                            self.last_line,
                        ) = save;
                        self.expr()?;
                    }
                }
                Tok::Op("[") => {
                    self.advance()?;
                    self.expr()?;
                    self.expect_op("]")?;
                    self.expect_op("=")?;
                    self.expr()?;
                }
                _ => self.expr()?,
            }
            if !(self.accept_op(",")? || self.accept_op(";")?) {
                break;
            }
        }
        if self.is_op("}") {
            return self.advance();
        }
        if line == self.line {
            self.err(format!("'}}' expected near {}", self.describe()))
        } else {
            self.err(format!(
                "'}}' expected (to close '{{' at line {line}) near {}",
                self.describe()
            ))
        }
    }

    fn simple_expr(&mut self) -> Res<()> {
        match &self.tok {
            Tok::Number | Tok::Str | Tok::Kw("nil" | "true" | "false") | Tok::Op("...") => {
                self.advance()
            }
            Tok::Op("{") => self.table(),
            Tok::Kw("function") => {
                let line = self.line;
                self.advance()?;
                self.func_body(line)
            }
            _ => {
                self.suffixed_expr()?;
                Ok(())
            }
        }
    }

    fn binary_priority(&self) -> Option<(u8, u8)> {
        let (l, r) = match &self.tok {
            Tok::Op("+" | "-") => (6, 6),
            Tok::Op("*" | "/" | "%") => (7, 7),
            Tok::Op("^") => (10, 9),
            Tok::Op("..") => (5, 4),
            Tok::Op("==" | "~=" | "<" | "<=" | ">" | ">=") => (3, 3),
            Tok::Kw("and") => (2, 2),
            Tok::Kw("or") => (1, 1),
            _ => return None,
        };
        Some((l, r))
    }

    fn expr(&mut self) -> Res<()> {
        self.subexpr(0)
    }

    fn subexpr(&mut self, limit: u8) -> Res<()> {
        self.enter()?;
        if matches!(&self.tok, Tok::Kw("not") | Tok::Op("-" | "#")) {
            self.advance()?;
            self.subexpr(8)?;
        } else {
            self.simple_expr()?;
        }
        while let Some((left, right)) = self.binary_priority() {
            if left <= limit {
                break;
            }
            self.advance()?;
            self.subexpr(right)?;
        }
        self.depth -= 1;
        Ok(())
    }
}

/// Checks `source` (the bytes of a `.lub` or `.lua` file) against the Lua 5.1 grammar.
pub fn check(source: &[u8]) -> Result<(), LuaError> {
    if source.starts_with(b"\x1bLua") {
        return Err(LuaError {
            line: 1,
            message: "compiled Lua (bytecode), not text".into(),
        });
    }
    let mut p = Parser::new(source)?;
    p.block()?;
    if p.tok != Tok::Eof {
        return p.err(format!("'<eof>' expected near {}", p.describe()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(src: &str) {
        if let Err(e) = check(src.as_bytes()) {
            panic!("should be valid but: {e}\n{src}");
        }
    }

    fn bad(src: &str, line: usize) {
        match check(src.as_bytes()) {
            Ok(()) => panic!("should be invalid:\n{src}"),
            Err(e) => assert_eq!(e.line, line, "{e} for:\n{src}"),
        }
    }

    #[test]
    fn valid_programs() {
        ok("");
        ok("-- only a comment");
        ok("x = 1");
        ok("local a, b = 1, 2; local function f(x, ...) return x, ... end");
        ok("t = { 1, 2, [3] = 4, name = 'x', nested = { a = { b = 1 } }; 5 }");
        ok("t = {\n  [1] = { \"a\", \"b\", },\n}");
        ok("if a then b() elseif c then d() else e() end");
        ok("for i = 1, 10, 2 do print(i) end for k, v in pairs(t) do end");
        ok("while x do break end repeat local y = 1 until y > 0");
        ok("function a.b.c:d(x) return self end");
        ok("x = a and b or c .. d ^ -e % f");
        ok("x = #t + -1 * not true");
        ok("s = [[long\nstring]] .. [==[with ]] inside]==]");
        ok("--[[ block\ncomment ]] x = 1 --[==[ another ]==]");
        ok("x = 0x1F + 3.5e-2 + .5 + 1e10");
        ok("f{1,2} g'str' h\"s\" i[[x]]");
        ok("a.b['c'].d = function() end");
        ok("return");
        ok("return 1, 2;");
        ok("x = 'esc \\n \\\\ \\' \\065 \\\n continued'");
        ok("#!shebang line\nx = 1");
        ok("local t = {f = function(a) return a end, g = 2}");
        ok("a, b.c, d[1] = 1, 2, 3");
        ok("x = (a or b)(c)");
        ok("do local x end");
    }

    #[test]
    fn invalid_programs_with_the_right_line() {
        bad("x = ", 1);
        bad("x = 1\ny = = 2", 2);
        bad("if a then\n b()\n", 3);
        bad("t = {\n 1,\n 2\n", 4);
        bad("t = { 1 2 }", 1);
        bad("f(1, 2", 1);
        bad("x = 'unfinished\ny = 1", 1);
        bad("x = [[never closed", 1);
        bad("--[[ never closed", 1);
        bad("local = 1", 1);
        bad("x = 1 +", 1);
        bad("for i = 1 do end", 1);
        bad("function end", 1);
        bad("return 1\nx = 2", 2);
        bad("goto x", 1);
        bad("x = 1 // 2", 1);
        bad("x = 3x", 1);
        bad("x = 1.2.3", 1);
        bad("x = '\\300'", 1);
        bad("f()\n(g)()", 2);
        bad("x = @", 1);
        bad("a + b", 1);
        bad("a, b", 1);
        bad("f() = 1", 1);
        bad("end", 1);
        bad("x = {,}", 1);
    }

    #[test]
    fn errors_name_the_opening_line() {
        let e = check(b"function f()\n  if x then\n    y()\nend\n").unwrap_err();
        assert!(e.message.contains("to close 'function' at line 1"), "{e}");
    }

    #[test]
    fn bytecode_and_deep_nesting_are_rejected_cleanly() {
        let e = check(b"\x1bLuaQ\x00").unwrap_err();
        assert!(e.message.contains("bytecode"));
        let deep = format!("x = {}1{}", "(".repeat(500), ")".repeat(500));
        assert!(
            check(deep.as_bytes())
                .unwrap_err()
                .message
                .contains("too many")
        );
    }

    #[test]
    fn non_ascii_text_is_fine_in_strings_and_comments_only() {
        ok("x = '\u{c0}\u{af}' -- \u{b0}\u{a1}");
        bad("\u{c0}\u{af} = 1", 1);
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        let junk: Vec<u8> = (0..4000u32).map(|i| (i * 7919 % 251) as u8).collect();
        let _ = check(&junk);
        for cut in 0..60 {
            let src = b"t = { a = 1, [2] = function(x) return x end, 'str', [[l]] } --[=[c]=]";
            let _ = check(&src[..cut.min(src.len())]);
        }
    }
}
