//! Compiler from Lua 5.0 source text to the bytecode of `luac` 5.0 (a [`luac50::Chunk`]), the
//! counterpart of [`crate::luacomp`] for the 62 `.lub` of the kRO archive that are Lua 5.0.
//!
//! A port of the lexer, parser and code generator of Lua 5.0.3 (`llex.c`, `lparser.c`, `lcode.c`;
//! Copyright PUC-Rio, MIT licence, see docs/05-licensing.md). It keeps the structure and the register
//! allocation of the original, so that the generated code is the one the real `luac` 5.0.3 makes.
//! What 5.0 does differently from 5.1: no `%`, no `#`, no constant folding (only `-<number>`), long
//! strings and comments nest (`[[ [[ ]] ]]`), `...` only as a parameter (the extra arguments are the
//! local table `arg`), `for` loops with FORLOOP / TFORPREP / TFORLOOP, SETLIST / SETLISTO, `TEST A B C`,
//! and an instruction layout with A on top. `tools/lua-verify` compares the bytes with a real Lua 5.0.3.
//!
//! Differences from `luac`: strings are bytes (no locale), and the chunk name is given by the caller.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::luac::{Constant, Local};
use crate::luac50::{Chunk, Function};
pub use crate::luacomp::{CompileError, TextEncoding};

type R<T = ()> = Result<T, CompileError>;

// ---------------------------------------------------------------------------------------------
// Opcodes and instruction fields (lopcodes.h)
// ---------------------------------------------------------------------------------------------

mod op {
    pub const MOVE: u32 = 0;
    pub const LOADK: u32 = 1;
    pub const LOADBOOL: u32 = 2;
    pub const LOADNIL: u32 = 3;
    pub const GETUPVAL: u32 = 4;
    pub const GETGLOBAL: u32 = 5;
    pub const GETTABLE: u32 = 6;
    pub const SETGLOBAL: u32 = 7;
    pub const SETUPVAL: u32 = 8;
    pub const SETTABLE: u32 = 9;
    pub const NEWTABLE: u32 = 10;
    pub const SELF: u32 = 11;
    pub const ADD: u32 = 12;
    pub const SUB: u32 = 13;
    pub const UNM: u32 = 17;
    pub const NOT: u32 = 18;
    pub const CONCAT: u32 = 19;
    pub const JMP: u32 = 20;
    pub const EQ: u32 = 21;
    pub const LT: u32 = 22;
    pub const LE: u32 = 23;
    pub const TEST: u32 = 24;
    pub const CALL: u32 = 25;
    pub const TAILCALL: u32 = 26;
    pub const RETURN: u32 = 27;
    pub const FORLOOP: u32 = 28;
    pub const TFORLOOP: u32 = 29;
    pub const TFORPREP: u32 = 30;
    pub const SETLIST: u32 = 31;
    pub const SETLISTO: u32 = 32;
    pub const CLOSE: u32 = 33;
    pub const CLOSURE: u32 = 34;
}

const NO_JUMP: i32 = -1;
const NO_REG: i32 = 255;
const MAXARG_BX: i32 = 262_143;
const MAXARG_SBX: i32 = 131_071;
const MAXARG_C: i32 = 511;
const MAXSTACK: i32 = 250;
const MAXVARS: i32 = 200;
const MAXUPVALUES: i32 = 32;
const MAXPARAMS: i32 = 100;
const MAXPARSERLEVEL: u32 = 200;
const MAXEXPWHILE: i32 = 100;
const LFIELDS_PER_FLUSH: i32 = 32;
const MULTRET: i32 = -1;

// The instruction: OP in bits 0-5, C in 6-14, B in 15-23, A in 24-31 (Bx = B and C together).
fn get_op(i: u32) -> u32 {
    i & 0x3f
}
fn get_a(i: u32) -> i32 {
    (i >> 24) as i32
}
fn get_b(i: u32) -> i32 {
    ((i >> 15) & 0x1ff) as i32
}
fn get_c(i: u32) -> i32 {
    ((i >> 6) & 0x1ff) as i32
}
fn get_sbx(i: u32) -> i32 {
    ((i >> 6) & 0x3_ffff) as i32 - MAXARG_SBX
}
fn set_op(i: &mut u32, o: u32) {
    *i = (*i & !0x3f) | o;
}
fn set_a(i: &mut u32, a: i32) {
    *i = (*i & 0x00ff_ffff) | ((a as u32 & 0xff) << 24);
}
fn set_b(i: &mut u32, b: i32) {
    *i = (*i & !(0x1ff << 15)) | ((b as u32 & 0x1ff) << 15);
}
fn set_c(i: &mut u32, c: i32) {
    *i = (*i & !(0x1ff << 6)) | ((c as u32 & 0x1ff) << 6);
}
fn set_sbx(i: &mut u32, v: i32) {
    *i = (*i & !(0x3_ffff << 6)) | (((v + MAXARG_SBX) as u32 & 0x3_ffff) << 6);
}
fn create_abc(o: u32, a: i32, b: i32, c: i32) -> u32 {
    o | ((a as u32 & 0xff) << 24) | ((b as u32 & 0x1ff) << 15) | ((c as u32 & 0x1ff) << 6)
}
fn create_abx(o: u32, a: i32, bx: u32) -> u32 {
    o | ((a as u32 & 0xff) << 24) | ((bx & 0x3_ffff) << 6)
}
/// Instructions that test and are followed by a jump (`OpModeT`).
fn test_t_mode(o: u32) -> bool {
    matches!(o, op::EQ | op::LT | op::LE | op::TEST | op::TFORLOOP)
}

/// `luaO_int2fb`: the "floating point byte" of the array size hint.
fn int2fb(mut x: u32) -> i32 {
    let mut m = 0;
    while x >= 8 {
        x = (x + 1) >> 1;
        m += 1;
    }
    (m << 3) | x as i32
}

/// `luaO_log2`, with -1 for 0.
fn log2(x: u32) -> i32 {
    if x == 0 {
        -1
    } else {
        31 - x.leading_zeros() as i32
    }
}

// ---------------------------------------------------------------------------------------------
// Lexer (llex.c)
// ---------------------------------------------------------------------------------------------

const FIRST_RESERVED: u16 = 257;
const TK_AND: u16 = 257;
const TK_BREAK: u16 = 258;
const TK_DO: u16 = 259;
const TK_ELSE: u16 = 260;
const TK_ELSEIF: u16 = 261;
const TK_END: u16 = 262;
const TK_FALSE: u16 = 263;
const TK_FOR: u16 = 264;
const TK_FUNCTION: u16 = 265;
const TK_IF: u16 = 266;
const TK_IN: u16 = 267;
const TK_LOCAL: u16 = 268;
const TK_NIL: u16 = 269;
const TK_NOT: u16 = 270;
const TK_OR: u16 = 271;
const TK_REPEAT: u16 = 272;
const TK_RETURN: u16 = 273;
const TK_THEN: u16 = 274;
const TK_TRUE: u16 = 275;
const TK_UNTIL: u16 = 276;
const TK_WHILE: u16 = 277;
const TK_NAME: u16 = 278;
const TK_CONCAT: u16 = 279;
const TK_DOTS: u16 = 280;
const TK_EQ: u16 = 281;
const TK_GE: u16 = 282;
const TK_LE: u16 = 283;
const TK_NE: u16 = 284;
const TK_NUMBER: u16 = 285;
const TK_STRING: u16 = 286;
const TK_EOS: u16 = 287;

const NUM_RESERVED: usize = 21;
const TOKENS: [&str; 31] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while", "*name", "..", "...",
    "==", ">=", "<=", "~=", "*number", "*string", "<eof>",
];

const EOZ: i32 = -1;

#[derive(Clone, Default)]
struct Tok {
    token: u16,
    r: f64,
    /// Contents of a name or string.
    s: Vec<u8>,
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    cur: i32,
    line: u32,
    lastline: u32,
    t: Tok,
    ahead: Option<Tok>,
    /// The token buffer of the C lexer: it keeps the text of the last name, string or number,
    /// which is what error messages show.
    buf: Vec<u8>,
    source: Vec<u8>,
}

fn is_digit(c: i32) -> bool {
    (i32::from(b'0')..=i32::from(b'9')).contains(&c)
}
fn is_alpha(c: i32) -> bool {
    u8::try_from(c).is_ok_and(|b| b.is_ascii_alphabetic())
}
fn is_alnum(c: i32) -> bool {
    is_digit(c) || is_alpha(c)
}
fn is_space(c: i32) -> bool {
    matches!(c, 0x20 | 0x09..=0x0d)
}
fn is_cntrl(c: i32) -> bool {
    (0..32).contains(&c) || c == 127
}

/// `luaO_chunkid` with the buffer size 80 of the lexer.
fn chunk_id(source: &[u8]) -> String {
    const BUFFLEN: usize = 80;
    let lossy = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    // the C functions stop at a NUL byte
    let source = &source[..source.iter().position(|&b| b == 0).unwrap_or(source.len())];
    match source.first() {
        Some(b'=') => lossy(&source[1..source.len().min(BUFFLEN)]),
        Some(b'@') => {
            let name = &source[1..];
            let room = BUFFLEN - " `...' ".len() - 1;
            if name.len() > room {
                format!("...{}", lossy(&name[name.len() - room..]))
            } else {
                lossy(name)
            }
        }
        _ => {
            let room = BUFFLEN - " [string \"...\"] ".len() - 1;
            let first = source
                .iter()
                .position(|&b| b == b'\n')
                .unwrap_or(source.len());
            let len = first.min(room);
            if len < source.len() {
                format!("[string \"{}...\"]", lossy(&source[..len]))
            } else {
                format!("[string \"{}\"]", lossy(source))
            }
        }
    }
}

fn token_text(token: u16) -> Vec<u8> {
    if token < FIRST_RESERVED {
        vec![token as u8]
    } else {
        TOKENS[usize::from(token - FIRST_RESERVED)]
            .as_bytes()
            .to_vec()
    }
}

/// `luaO_str2d` for what the lexer collects: digits, a point, digits, an exponent.
fn str2d(s: &[u8]) -> Option<f64> {
    let digits = |b: &[u8]| b.iter().take_while(|c| c.is_ascii_digit()).count();
    let mut i = digits(s);
    let mut mantissa = i;
    if s.get(i) == Some(&b'.') {
        let frac = digits(&s[i + 1..]);
        mantissa += frac;
        i += 1 + frac;
    }
    if mantissa == 0 {
        return None;
    }
    if matches!(s.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(s.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let exp = digits(&s[i..]);
        if exp == 0 {
            return None;
        }
        i += exp;
    }
    if i != s.len() {
        return None;
    }
    std::str::from_utf8(s).ok()?.parse::<f64>().ok()
}

impl<'a> Lexer<'a> {
    fn new(src: &'a [u8], source: &[u8]) -> Self {
        let mut lx = Self {
            src,
            pos: 0,
            cur: EOZ,
            line: 1,
            lastline: 1,
            t: Tok::default(),
            ahead: None,
            buf: Vec::new(),
            source: source.to_vec(),
        };
        lx.advance();
        if lx.cur == i32::from(b'#') {
            // a first line starting with `#` is skipped; its newline is kept
            while lx.cur != 10 && lx.cur != EOZ {
                lx.advance();
            }
        }
        lx
    }

    fn advance(&mut self) {
        self.cur = match self.src.get(self.pos) {
            Some(&b) => {
                self.pos += 1;
                i32::from(b)
            }
            None => EOZ,
        };
    }

    fn save(&mut self, c: i32) {
        self.buf.push(c as u8);
    }

    fn save_and_next(&mut self) {
        self.save(self.cur);
        self.advance();
    }

    fn error_at(&self, msg: &str, near: &[u8]) -> CompileError {
        // the C code formats the text as a string: it ends at a NUL byte
        let near = &near[..near.iter().position(|&b| b == 0).unwrap_or(near.len())];
        CompileError {
            line: self.line,
            message: format!(
                "{}:{}: {msg} near `{}'",
                chunk_id(&self.source),
                self.line,
                String::from_utf8_lossy(near)
            ),
        }
    }

    fn lexerror(&self, msg: &str, token: u16) -> CompileError {
        if token == TK_EOS {
            self.error_at(msg, &token_text(token))
        } else {
            self.error_at(msg, &self.buf)
        }
    }

    fn syntaxerror(&self, msg: &str) -> CompileError {
        match self.t.token {
            TK_NAME => self.error_at(msg, &self.t.s),
            TK_STRING | TK_NUMBER => self.error_at(msg, &self.buf),
            t => self.error_at(msg, &token_text(t)),
        }
    }

    fn checklimit(&self, val: i32, limit: i32, what: &str) -> R {
        if val > limit {
            return Err(self.syntaxerror(&format!("too many {what} (limit={limit})")));
        }
        Ok(())
    }

    fn inc_line(&mut self) -> R {
        self.advance(); // skip '\n'
        self.line += 1;
        if self.line >= i32::MAX as u32 - 2 {
            return Err(self.syntaxerror("too many lines in a chunk (limit=2147483645)"));
        }
        Ok(())
    }

    fn read_numeral(&mut self, comma: bool) -> R<f64> {
        self.buf.clear();
        if comma {
            self.save(i32::from(b'.'));
        }
        while is_digit(self.cur) {
            self.save_and_next();
        }
        if self.cur == i32::from(b'.') {
            self.save_and_next();
            if self.cur == i32::from(b'.') {
                self.save_and_next();
                return Err(self.lexerror(
                    "ambiguous syntax (decimal point x string concatenation)",
                    TK_NUMBER,
                ));
            }
        }
        while is_digit(self.cur) {
            self.save_and_next();
        }
        if self.cur == i32::from(b'e') || self.cur == i32::from(b'E') {
            self.save_and_next();
            if self.cur == i32::from(b'+') || self.cur == i32::from(b'-') {
                self.save_and_next();
            }
            while is_digit(self.cur) {
                self.save_and_next();
            }
        }
        str2d(&self.buf).ok_or_else(|| self.lexerror("malformed number", TK_NUMBER))
    }

    /// `[[ ... ]]` (nesting) as a string or, without `want_value`, as a comment. The first `[` has
    /// been read, the current character is the second.
    fn read_long_string(&mut self, want_value: bool) -> R<Vec<u8>> {
        let mut cont = 0;
        self.buf.clear();
        self.save(i32::from(b'['));
        self.save_and_next();
        if self.cur == 10 {
            self.inc_line()?;
        }
        loop {
            match self.cur {
                EOZ => {
                    return Err(self.lexerror(
                        if want_value {
                            "unfinished long string"
                        } else {
                            "unfinished long comment"
                        },
                        TK_EOS,
                    ));
                }
                c if c == i32::from(b'[') => {
                    self.save_and_next();
                    if self.cur == i32::from(b'[') {
                        cont += 1;
                        self.save_and_next();
                    }
                }
                c if c == i32::from(b']') => {
                    self.save_and_next();
                    if self.cur == i32::from(b']') {
                        if cont == 0 {
                            break;
                        }
                        cont -= 1;
                        self.save_and_next();
                    }
                }
                10 => {
                    self.save(10);
                    self.inc_line()?;
                    if !want_value {
                        self.buf.clear();
                    }
                }
                _ => self.save_and_next(),
            }
        }
        self.save_and_next(); // skip the second `]`
        if want_value {
            Ok(self.buf[2..self.buf.len() - 2].to_vec())
        } else {
            Ok(Vec::new())
        }
    }

    fn read_string(&mut self, del: i32) -> R<Vec<u8>> {
        self.buf.clear();
        self.save_and_next();
        while self.cur != del {
            match self.cur {
                EOZ => return Err(self.lexerror("unfinished string", TK_EOS)),
                10 => return Err(self.lexerror("unfinished string", TK_STRING)),
                92 => {
                    self.advance(); // do not save the backslash
                    let c = match self.cur {
                        0x61 => 7,
                        0x62 => 8,
                        0x66 => 12,
                        0x6e => 10,
                        0x72 => 13,
                        0x74 => 9,
                        0x76 => 11,
                        10 => {
                            self.save(10);
                            self.inc_line()?;
                            continue;
                        }
                        EOZ => continue,
                        c if !is_digit(c) => {
                            self.save_and_next();
                            continue;
                        }
                        _ => {
                            let mut i = 0;
                            let mut v = 0i32;
                            loop {
                                v = 10 * v + (self.cur - i32::from(b'0'));
                                self.advance();
                                i += 1;
                                if !(i < 3 && is_digit(self.cur)) {
                                    break;
                                }
                            }
                            if v > 255 {
                                return Err(self.lexerror("escape sequence too large", TK_STRING));
                            }
                            self.save(v);
                            continue;
                        }
                    };
                    self.save(c);
                    self.advance();
                }
                _ => self.save_and_next(),
            }
        }
        self.save_and_next(); // skip the delimiter
        Ok(self.buf[1..self.buf.len() - 1].to_vec())
    }

    fn llex(&mut self) -> R<Tok> {
        let mut t = Tok::default();
        loop {
            let c = self.cur;
            match c {
                10 => self.inc_line()?,
                c if c == i32::from(b'-') => {
                    self.advance();
                    if self.cur != i32::from(b'-') {
                        t.token = u16::from(b'-');
                        return Ok(t);
                    }
                    self.advance();
                    let mut long = false;
                    if self.cur == i32::from(b'[') {
                        self.advance();
                        long = self.cur == i32::from(b'[');
                    }
                    if long {
                        self.read_long_string(false)?;
                    } else {
                        while self.cur != 10 && self.cur != EOZ {
                            self.advance();
                        }
                    }
                }
                c if c == i32::from(b'[') => {
                    self.advance();
                    if self.cur != i32::from(b'[') {
                        t.token = u16::from(b'[');
                    } else {
                        t.s = self.read_long_string(true)?;
                        t.token = TK_STRING;
                    }
                    return Ok(t);
                }
                c if c == i32::from(b'=') => {
                    self.advance();
                    t.token = if self.cur == i32::from(b'=') {
                        self.advance();
                        TK_EQ
                    } else {
                        u16::from(b'=')
                    };
                    return Ok(t);
                }
                c if c == i32::from(b'<') => {
                    self.advance();
                    t.token = if self.cur == i32::from(b'=') {
                        self.advance();
                        TK_LE
                    } else {
                        u16::from(b'<')
                    };
                    return Ok(t);
                }
                c if c == i32::from(b'>') => {
                    self.advance();
                    t.token = if self.cur == i32::from(b'=') {
                        self.advance();
                        TK_GE
                    } else {
                        u16::from(b'>')
                    };
                    return Ok(t);
                }
                c if c == i32::from(b'~') => {
                    self.advance();
                    t.token = if self.cur == i32::from(b'=') {
                        self.advance();
                        TK_NE
                    } else {
                        u16::from(b'~')
                    };
                    return Ok(t);
                }
                c if c == i32::from(b'"') || c == i32::from(b'\'') => {
                    t.s = self.read_string(c)?;
                    t.token = TK_STRING;
                    return Ok(t);
                }
                c if c == i32::from(b'.') => {
                    self.advance();
                    if self.cur == i32::from(b'.') {
                        self.advance();
                        t.token = if self.cur == i32::from(b'.') {
                            self.advance();
                            TK_DOTS
                        } else {
                            TK_CONCAT
                        };
                    } else if !is_digit(self.cur) {
                        t.token = u16::from(b'.');
                    } else {
                        t.r = self.read_numeral(true)?;
                        t.token = TK_NUMBER;
                    }
                    return Ok(t);
                }
                EOZ => {
                    t.token = TK_EOS;
                    return Ok(t);
                }
                c if is_space(c) => self.advance(),
                c if is_digit(c) => {
                    t.r = self.read_numeral(false)?;
                    t.token = TK_NUMBER;
                    return Ok(t);
                }
                c if is_alpha(c) || c == i32::from(b'_') => {
                    self.buf.clear();
                    loop {
                        self.save_and_next();
                        if !(is_alnum(self.cur) || self.cur == i32::from(b'_')) {
                            break;
                        }
                    }
                    let name = self.buf.clone();
                    if let Some(i) = TOKENS[..NUM_RESERVED]
                        .iter()
                        .position(|w| w.as_bytes() == name)
                    {
                        t.token = FIRST_RESERVED + i as u16;
                    } else {
                        t.token = TK_NAME;
                        t.s = name;
                    }
                    return Ok(t);
                }
                c => {
                    if is_cntrl(c) {
                        return Err(
                            self.error_at("invalid control char", format!("char({c})").as_bytes())
                        );
                    }
                    self.advance();
                    t.token = c as u16;
                    return Ok(t);
                }
            }
        }
    }

    fn next(&mut self) -> R {
        self.lastline = self.line;
        if let Some(a) = self.ahead.take() {
            self.t = a;
        } else {
            self.t = self.llex()?;
        }
        Ok(())
    }

    fn lookahead(&mut self) -> R {
        self.ahead = Some(self.llex()?);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// Parser state (lparser.h)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum K {
    Void,
    Nil,
    True,
    False,
    /// A constant of the table; `info` is its index.
    Kst,
    Local,
    Upval,
    Global,
    Indexed,
    Jmp,
    Relocable,
    NonReloc,
    Call,
}

#[derive(Clone, Copy, Debug)]
struct ExpDesc {
    k: K,
    info: i32,
    aux: i32,
    t: i32,
    f: i32,
}

impl ExpDesc {
    fn new(k: K, info: i32) -> Self {
        Self {
            k,
            info,
            aux: 0,
            t: NO_JUMP,
            f: NO_JUMP,
        }
    }
    fn has_jumps(&self) -> bool {
        self.t != self.f
    }
}

#[derive(Hash, PartialEq, Eq)]
enum CKey {
    /// `nil` cannot be a table key in Lua: the code generator uses the table itself.
    Nil,
    Num(u64),
    Str(Vec<u8>),
}

struct BlockCnt {
    breaklist: i32,
    nactvar: i32,
    upval: bool,
    isbreakable: bool,
}

struct FuncState {
    f: Function,
    h: HashMap<CKey, i32>,
    lasttarget: i32,
    jpc: i32,
    freereg: i32,
    nactvar: i32,
    actvar: Vec<usize>,
    /// Kind (`K::Local` or `K::Upval`) and index of every upvalue.
    upvals: Vec<(K, i32)>,
    blocks: Vec<BlockCnt>,
}

impl FuncState {
    fn pc(&self) -> i32 {
        self.f.code.len() as i32
    }
    fn nk(&self) -> i32 {
        self.f.constants.len() as i32
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UnOpr {
    Minus,
    Not,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BinOpr {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Ne,
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    None,
}

/// Left and right priority of every binary operator, in the order of [`BinOpr`].
const PRIORITY: [(i32, i32); 14] = [
    (6, 6),
    (6, 6),
    (7, 7),
    (7, 7),
    (10, 9),
    (5, 4),
    (3, 3),
    (3, 3),
    (3, 3),
    (3, 3),
    (3, 3),
    (3, 3),
    (2, 2),
    (1, 1),
];
const UNARY_PRIORITY: i32 = 8;

struct ConsControl {
    v: ExpDesc,
    /// Register of the table being built.
    t_info: i32,
    nh: i32,
    na: i32,
    tostore: i32,
}

struct Parser<'a> {
    lx: Lexer<'a>,
    funcs: Vec<FuncState>,
    nestlevel: u32,
    /// An error found in a place that cannot return one (jump patching); reported at the end.
    deferred: Option<CompileError>,
}

impl Parser<'_> {
    fn fs(&mut self) -> &mut FuncState {
        self.funcs.last_mut().expect("a function is open")
    }
    fn fsr(&self) -> &FuncState {
        self.funcs.last().expect("a function is open")
    }

    // ----- lcode.c ---------------------------------------------------------------------------

    fn code_line(&mut self, i: u32, line: u32) -> i32 {
        self.dischargejpc();
        let fs = self.fs();
        fs.f.code.push(i);
        fs.f.lines.push(line);
        fs.pc() - 1
    }

    fn code(&mut self, i: u32) -> i32 {
        let line = self.lx.lastline;
        self.code_line(i, line)
    }

    fn code_abc(&mut self, o: u32, a: i32, b: i32, c: i32) -> i32 {
        self.code(create_abc(o, a, b, c))
    }

    fn code_abx(&mut self, o: u32, a: i32, bx: u32) -> i32 {
        self.code(create_abx(o, a, bx))
    }

    fn code_asbx(&mut self, o: u32, a: i32, sbx: i32) -> i32 {
        self.code_abx(o, a, (sbx + MAXARG_SBX) as u32)
    }

    fn fixline(&mut self, line: u32) {
        let fs = self.fs();
        let n = fs.f.lines.len();
        fs.f.lines[n - 1] = line;
    }

    fn nil(&mut self, from: i32, n: i32) {
        let fs = self.fsr();
        if fs.pc() > fs.lasttarget {
            let prev = fs.f.code[fs.pc() as usize - 1];
            if get_op(prev) == op::LOADNIL {
                let (pfrom, pto) = (get_a(prev), get_b(prev));
                if pfrom <= from && from <= pto + 1 {
                    if from + n - 1 > pto {
                        let fs = self.fs();
                        let last = fs.f.code.len() - 1;
                        set_b(&mut fs.f.code[last], from + n - 1);
                    }
                    return;
                }
            }
        }
        self.code_abc(op::LOADNIL, from, from + n - 1, 0);
    }

    fn jump(&mut self) -> i32 {
        let jpc = self.fs().jpc;
        self.fs().jpc = NO_JUMP;
        let mut j = self.code_asbx(op::JMP, 0, NO_JUMP);
        self.concat(&mut j, jpc);
        j
    }

    fn condjump(&mut self, o: u32, a: i32, b: i32, c: i32) -> i32 {
        self.code_abc(o, a, b, c);
        self.jump()
    }

    fn fixjump(&mut self, pc: i32, dest: i32) -> R {
        let offset = dest - (pc + 1);
        if offset.abs() > MAXARG_SBX {
            return Err(self.lx.syntaxerror("control structure too long"));
        }
        set_sbx(&mut self.fs().f.code[pc as usize], offset);
        Ok(())
    }

    fn getlabel(&mut self) -> i32 {
        let fs = self.fs();
        fs.lasttarget = fs.pc();
        fs.pc()
    }

    fn getjump(&self, pc: i32) -> i32 {
        let offset = get_sbx(self.fsr().f.code[pc as usize]);
        if offset == NO_JUMP {
            NO_JUMP
        } else {
            pc + 1 + offset
        }
    }

    fn getjumpcontrol(&self, pc: i32) -> usize {
        let code = &self.fsr().f.code;
        if pc >= 1 && test_t_mode(get_op(code[pc as usize - 1])) {
            pc as usize - 1
        } else {
            pc as usize
        }
    }

    /// Whether the list has a jump that does not produce a value (or produces an inverted one).
    fn need_value(&self, mut list: i32, cond: i32) -> bool {
        while list != NO_JUMP {
            let i = self.fsr().f.code[self.getjumpcontrol(list)];
            if get_op(i) != op::TEST || get_a(i) != NO_REG || get_c(i) != cond {
                return true;
            }
            list = self.getjump(list);
        }
        false
    }

    fn patchtestreg(&mut self, at: usize, reg: i32) {
        let code = &mut self.fs().f.code;
        let reg = if reg == NO_REG { get_b(code[at]) } else { reg };
        set_a(&mut code[at], reg);
    }

    fn removevalues(&mut self, mut list: i32) {
        while list != NO_JUMP {
            let at = self.getjumpcontrol(list);
            if get_op(self.fsr().f.code[at]) == op::TEST {
                self.patchtestreg(at, NO_REG);
            }
            list = self.getjump(list);
        }
    }

    fn patchlistaux(&mut self, mut list: i32, vtarget: i32, reg: i32, dtarget: i32) -> R {
        while list != NO_JUMP {
            let next = self.getjump(list);
            let at = self.getjumpcontrol(list);
            let i = self.fsr().f.code[at];
            if get_op(i) == op::TEST && get_a(i) == NO_REG {
                self.patchtestreg(at, reg);
                self.fixjump(list, vtarget)?;
            } else {
                self.fixjump(list, dtarget)?;
            }
            list = next;
        }
        Ok(())
    }

    fn dischargejpc(&mut self) {
        let (jpc, pc) = (self.fsr().jpc, self.fsr().pc());
        if let Err(e) = self.patchlistaux(jpc, pc, NO_REG, pc) {
            self.deferred.get_or_insert(e);
        }
        self.fs().jpc = NO_JUMP;
    }

    fn patchlist(&mut self, list: i32, target: i32) -> R {
        if target == self.fsr().pc() {
            self.patchtohere(list);
            Ok(())
        } else {
            self.patchlistaux(list, target, NO_REG, target)
        }
    }

    fn patchtohere(&mut self, list: i32) {
        self.getlabel();
        let mut jpc = self.fsr().jpc;
        self.concat(&mut jpc, list);
        self.fs().jpc = jpc;
    }

    fn concat(&mut self, l1: &mut i32, l2: i32) {
        if l2 == NO_JUMP {
            return;
        }
        if *l1 == NO_JUMP {
            *l1 = l2;
        } else {
            let mut list = *l1;
            loop {
                let next = self.getjump(list);
                if next == NO_JUMP {
                    break;
                }
                list = next;
            }
            if let Err(e) = self.fixjump(list, l2) {
                self.deferred.get_or_insert(e);
            }
        }
    }

    fn checkstack(&mut self, n: i32) -> R {
        let newstack = self.fsr().freereg + n;
        if newstack > i32::from(self.fsr().f.max_stack) {
            if newstack >= MAXSTACK {
                return Err(self.lx.syntaxerror("function or expression too complex"));
            }
            self.fs().f.max_stack = newstack as u8;
        }
        Ok(())
    }

    fn reserveregs(&mut self, n: i32) -> R {
        self.checkstack(n)?;
        self.fs().freereg += n;
        Ok(())
    }

    fn freereg(&mut self, reg: i32) {
        if reg >= self.fsr().nactvar && reg < MAXSTACK {
            self.fs().freereg -= 1;
        }
    }

    fn freeexp(&mut self, e: &ExpDesc) {
        if e.k == K::NonReloc {
            self.freereg(e.info);
        }
    }

    fn addk(&mut self, key: CKey, v: Constant) -> i32 {
        let fs = self.fs();
        if let Some(&i) = fs.h.get(&key) {
            return i;
        }
        let i = fs.nk();
        fs.f.constants.push(v);
        fs.h.insert(key, i);
        if i >= MAXARG_BX {
            let e = self.lx.syntaxerror("constant table overflow");
            self.deferred.get_or_insert(e);
        }
        i
    }

    fn string_k(&mut self, s: &[u8]) -> i32 {
        self.addk(CKey::Str(s.to_vec()), Constant::Str(s.to_vec()))
    }

    fn number_k(&mut self, r: f64) -> i32 {
        // -0 and 0 are the same table key in Lua
        let bits = if r == 0.0 { 0 } else { r.to_bits() };
        self.addk(CKey::Num(bits), Constant::Number(r))
    }

    fn nil_k(&mut self) -> i32 {
        self.addk(CKey::Nil, Constant::Nil)
    }

    fn getcode_mut(&mut self, e: &ExpDesc) -> &mut u32 {
        &mut self.fs().f.code[e.info as usize]
    }

    fn setcallreturns(&mut self, e: &mut ExpDesc, nresults: i32) {
        if e.k == K::Call {
            set_c(self.getcode_mut(e), nresults + 1);
            if nresults == 1 {
                e.k = K::NonReloc;
                e.info = get_a(*self.getcode_mut(e));
            }
        }
    }

    fn dischargevars(&mut self, e: &mut ExpDesc) {
        match e.k {
            K::Local => e.k = K::NonReloc,
            K::Upval => {
                e.info = self.code_abc(op::GETUPVAL, 0, e.info, 0);
                e.k = K::Relocable;
            }
            K::Global => {
                e.info = self.code_abx(op::GETGLOBAL, 0, e.info as u32);
                e.k = K::Relocable;
            }
            K::Indexed => {
                self.freereg(e.aux);
                self.freereg(e.info);
                e.info = self.code_abc(op::GETTABLE, 0, e.info, e.aux);
                e.k = K::Relocable;
            }
            K::Call => self.setcallreturns(e, 1),
            _ => {}
        }
    }

    fn code_label(&mut self, a: i32, b: i32, jump: i32) -> i32 {
        self.getlabel();
        self.code_abc(op::LOADBOOL, a, b, jump)
    }

    fn discharge2reg(&mut self, e: &mut ExpDesc, reg: i32) {
        self.dischargevars(e);
        match e.k {
            K::Nil => self.nil(reg, 1),
            K::False | K::True => {
                self.code_abc(op::LOADBOOL, reg, i32::from(e.k == K::True), 0);
            }
            K::Kst => {
                self.code_abx(op::LOADK, reg, e.info as u32);
            }
            K::Relocable => set_a(self.getcode_mut(e), reg),
            K::NonReloc => {
                if reg != e.info {
                    self.code_abc(op::MOVE, reg, e.info, 0);
                }
            }
            _ => return,
        }
        e.info = reg;
        e.k = K::NonReloc;
    }

    fn discharge2anyreg(&mut self, e: &mut ExpDesc) -> R {
        if e.k != K::NonReloc {
            self.reserveregs(1)?;
            let reg = self.fsr().freereg - 1;
            self.discharge2reg(e, reg);
        }
        Ok(())
    }

    fn exp2reg(&mut self, e: &mut ExpDesc, reg: i32) -> R {
        self.discharge2reg(e, reg);
        if e.k == K::Jmp {
            let mut t = e.t;
            self.concat(&mut t, e.info);
            e.t = t;
        }
        if e.has_jumps() {
            let mut p_f = NO_JUMP;
            let mut p_t = NO_JUMP;
            if self.need_value(e.t, 1) || self.need_value(e.f, 0) {
                let fj = if e.k == K::Jmp { NO_JUMP } else { self.jump() };
                p_f = self.code_label(reg, 0, 1);
                p_t = self.code_label(reg, 1, 0);
                self.patchtohere(fj);
            }
            let fin = self.getlabel();
            self.patchlistaux(e.f, fin, reg, p_f)?;
            self.patchlistaux(e.t, fin, reg, p_t)?;
        }
        e.f = NO_JUMP;
        e.t = NO_JUMP;
        e.info = reg;
        e.k = K::NonReloc;
        Ok(())
    }

    fn exp2nextreg(&mut self, e: &mut ExpDesc) -> R {
        self.dischargevars(e);
        self.freeexp(e);
        self.reserveregs(1)?;
        let reg = self.fsr().freereg - 1;
        self.exp2reg(e, reg)
    }

    fn exp2anyreg(&mut self, e: &mut ExpDesc) -> R<i32> {
        self.dischargevars(e);
        if e.k == K::NonReloc {
            if !e.has_jumps() {
                return Ok(e.info);
            }
            if e.info >= self.fsr().nactvar {
                self.exp2reg(e, e.info)?;
                return Ok(e.info);
            }
        }
        self.exp2nextreg(e)?;
        Ok(e.info)
    }

    fn exp2val(&mut self, e: &mut ExpDesc) -> R {
        if e.has_jumps() {
            self.exp2anyreg(e)?;
        } else {
            self.dischargevars(e);
        }
        Ok(())
    }

    /// An operand that is a register or (as `MAXSTACK + index`) a constant.
    fn exp2rk(&mut self, e: &mut ExpDesc) -> R<i32> {
        self.exp2val(e)?;
        match e.k {
            K::Nil if self.fsr().nk() + MAXSTACK <= MAXARG_C => {
                e.info = self.nil_k();
                e.k = K::Kst;
                return Ok(e.info + MAXSTACK);
            }
            K::Kst if e.info + MAXSTACK <= MAXARG_C => return Ok(e.info + MAXSTACK),
            _ => {}
        }
        self.exp2anyreg(e)
    }

    fn storevar(&mut self, var: &ExpDesc, ex: &mut ExpDesc) -> R {
        match var.k {
            K::Local => {
                self.freeexp(ex);
                return self.exp2reg(ex, var.info);
            }
            K::Upval => {
                let e = self.exp2anyreg(ex)?;
                self.code_abc(op::SETUPVAL, e, var.info, 0);
            }
            K::Global => {
                let e = self.exp2anyreg(ex)?;
                self.code_abx(op::SETGLOBAL, e, var.info as u32);
            }
            K::Indexed => {
                let e = self.exp2rk(ex)?;
                self.code_abc(op::SETTABLE, var.info, var.aux, e);
            }
            _ => {}
        }
        self.freeexp(ex);
        Ok(())
    }

    fn self_(&mut self, e: &mut ExpDesc, key: &mut ExpDesc) -> R {
        self.exp2anyreg(e)?;
        self.freeexp(e);
        let func = self.fsr().freereg;
        self.reserveregs(2)?;
        let k = self.exp2rk(key)?;
        self.code_abc(op::SELF, func, e.info, k);
        self.freeexp(key);
        e.info = func;
        e.k = K::NonReloc;
        Ok(())
    }

    fn invertjump(&mut self, e: &ExpDesc) {
        let at = self.getjumpcontrol(e.info);
        let code = &mut self.fs().f.code;
        let a = get_a(code[at]);
        set_a(&mut code[at], i32::from(a == 0));
    }

    fn jumponcond(&mut self, e: &mut ExpDesc, cond: i32) -> R<i32> {
        if e.k == K::Relocable {
            let ie = *self.getcode_mut(e);
            if get_op(ie) == op::NOT {
                let fs = self.fs();
                fs.f.code.pop();
                fs.f.lines.pop();
                return Ok(self.condjump(op::TEST, get_b(ie), get_b(ie), i32::from(cond == 0)));
            }
        }
        self.discharge2anyreg(e)?;
        self.freeexp(e);
        Ok(self.condjump(op::TEST, NO_REG, e.info, cond))
    }

    fn goiftrue(&mut self, e: &mut ExpDesc) -> R {
        self.dischargevars(e);
        let pc = match e.k {
            K::Kst | K::True => NO_JUMP,
            K::False => self.jump(),
            K::Jmp => {
                self.invertjump(e);
                e.info
            }
            _ => self.jumponcond(e, 0)?,
        };
        let mut f = e.f;
        self.concat(&mut f, pc);
        e.f = f;
        Ok(())
    }

    fn goiffalse(&mut self, e: &mut ExpDesc) -> R {
        self.dischargevars(e);
        let pc = match e.k {
            K::Nil | K::False => NO_JUMP,
            K::True => self.jump(),
            K::Jmp => e.info,
            _ => self.jumponcond(e, 1)?,
        };
        let mut t = e.t;
        self.concat(&mut t, pc);
        e.t = t;
        Ok(())
    }

    fn codenot(&mut self, e: &mut ExpDesc) -> R {
        self.dischargevars(e);
        match e.k {
            K::Nil | K::False => e.k = K::True,
            K::Kst | K::True => e.k = K::False,
            K::Jmp => self.invertjump(e),
            K::Relocable | K::NonReloc => {
                self.discharge2anyreg(e)?;
                self.freeexp(e);
                e.info = self.code_abc(op::NOT, 0, e.info, 0);
                e.k = K::Relocable;
            }
            _ => {}
        }
        std::mem::swap(&mut e.f, &mut e.t);
        self.removevalues(e.f);
        self.removevalues(e.t);
        Ok(())
    }

    fn indexed(&mut self, t: &mut ExpDesc, k: &mut ExpDesc) -> R {
        t.aux = self.exp2rk(k)?;
        t.k = K::Indexed;
        Ok(())
    }

    fn prefix(&mut self, o: UnOpr, e: &mut ExpDesc) -> R {
        if o == UnOpr::Minus {
            self.exp2val(e)?;
            let negated = match (e.k, self.fsr().f.constants.get(e.info as usize)) {
                (K::Kst, Some(Constant::Number(n))) => Some(-n),
                _ => None,
            };
            if let Some(n) = negated {
                e.info = self.number_k(n);
            } else {
                self.exp2anyreg(e)?;
                self.freeexp(e);
                e.info = self.code_abc(op::UNM, 0, e.info, 0);
                e.k = K::Relocable;
            }
            Ok(())
        } else {
            self.codenot(e)
        }
    }

    fn infix(&mut self, o: BinOpr, v: &mut ExpDesc) -> R {
        match o {
            BinOpr::And => {
                self.goiftrue(v)?;
                self.patchtohere(v.t);
                v.t = NO_JUMP;
            }
            BinOpr::Or => {
                self.goiffalse(v)?;
                self.patchtohere(v.f);
                v.f = NO_JUMP;
            }
            BinOpr::Concat => self.exp2nextreg(v)?,
            _ => {
                self.exp2rk(v)?;
            }
        }
        Ok(())
    }

    fn codebinop(&mut self, res: &mut ExpDesc, o: BinOpr, mut o1: i32, mut o2: i32) {
        if o <= BinOpr::Pow {
            res.info = self.code_abc(op::ADD + (o as u32 - BinOpr::Add as u32), 0, o1, o2);
            res.k = K::Relocable;
        } else {
            let opc = match o {
                BinOpr::Ne | BinOpr::Eq => op::EQ,
                BinOpr::Lt | BinOpr::Gt => op::LT,
                _ => op::LE,
            };
            let mut cond = 1;
            if o >= BinOpr::Gt {
                std::mem::swap(&mut o1, &mut o2);
            } else if o == BinOpr::Ne {
                cond = 0;
            }
            res.info = self.condjump(opc, cond, o1, o2);
            res.k = K::Jmp;
        }
    }

    fn posfix(&mut self, o: BinOpr, e1: &mut ExpDesc, e2: &mut ExpDesc) -> R {
        match o {
            BinOpr::And => {
                self.dischargevars(e2);
                let mut f = e1.f;
                self.concat(&mut f, e2.f);
                *e1 = ExpDesc { f, ..*e2 };
            }
            BinOpr::Or => {
                self.dischargevars(e2);
                let mut t = e1.t;
                self.concat(&mut t, e2.t);
                *e1 = ExpDesc { t, ..*e2 };
            }
            BinOpr::Concat => {
                self.exp2val(e2)?;
                if e2.k == K::Relocable && get_op(*self.getcode_mut(e2)) == op::CONCAT {
                    self.freeexp(e1);
                    set_b(self.getcode_mut(e2), e1.info);
                    e1.k = e2.k;
                    e1.info = e2.info;
                } else {
                    self.exp2nextreg(e2)?;
                    self.freeexp(e2);
                    self.freeexp(e1);
                    e1.info = self.code_abc(op::CONCAT, 0, e1.info, e2.info);
                    e1.k = K::Relocable;
                }
            }
            _ => {
                let o1 = self.exp2rk(e1)?;
                let o2 = self.exp2rk(e2)?;
                self.freeexp(e2);
                self.freeexp(e1);
                self.codebinop(e1, o, o1, o2);
            }
        }
        Ok(())
    }

    // ----- lparser.c: helpers -----------------------------------------------------------------

    fn error_expected(&self, token: u16) -> CompileError {
        self.lx.syntaxerror(&format!(
            "`{}' expected",
            String::from_utf8_lossy(&token_text(token))
        ))
    }

    fn testnext(&mut self, c: u16) -> R<bool> {
        if self.lx.t.token == c {
            self.lx.next()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn check(&mut self, c: u16) -> R {
        if !self.testnext(c)? {
            return Err(self.error_expected(c));
        }
        Ok(())
    }

    fn check_match(&mut self, what: u16, who: u16, wh: u32) -> R {
        if !self.testnext(what)? {
            if wh == self.lx.line {
                return Err(self.error_expected(what));
            }
            return Err(self.lx.syntaxerror(&format!(
                "`{}' expected (to close `{}' at line {wh})",
                String::from_utf8_lossy(&token_text(what)),
                String::from_utf8_lossy(&token_text(who))
            )));
        }
        Ok(())
    }

    fn str_checkname(&mut self) -> R<Vec<u8>> {
        if self.lx.t.token != TK_NAME {
            return Err(self.lx.syntaxerror("<name> expected"));
        }
        let s = std::mem::take(&mut self.lx.t.s);
        self.lx.next()?;
        Ok(s)
    }

    fn codestring(&mut self, e: &mut ExpDesc, s: &[u8]) {
        *e = ExpDesc::new(K::Kst, self.string_k(s));
    }

    fn checkname(&mut self, e: &mut ExpDesc) -> R {
        let s = self.str_checkname()?;
        self.codestring(e, &s);
        Ok(())
    }

    fn new_localvar(&mut self, name: Vec<u8>, n: i32) -> R {
        let total = self.fsr().nactvar + n + 1;
        self.lx.checklimit(total, MAXVARS, "local variables")?;
        let fs = self.fs();
        fs.f.locals.push(Local {
            name,
            start: 0,
            end: 0,
        });
        let idx = fs.f.locals.len() - 1;
        let at = (fs.nactvar + n) as usize;
        if fs.actvar.len() <= at {
            fs.actvar.resize(at + 1, 0);
        }
        fs.actvar[at] = idx;
        Ok(())
    }

    fn adjustlocalvars(&mut self, nvars: i32) {
        let fs = self.fs();
        fs.nactvar += nvars;
        let pc = fs.pc() as u32;
        for i in (1..=nvars).rev() {
            let idx = fs.actvar[(fs.nactvar - i) as usize];
            fs.f.locals[idx].start = pc;
        }
    }

    fn removevars(&mut self, tolevel: i32) {
        let fs = self.fs();
        let pc = fs.pc() as u32;
        while fs.nactvar > tolevel {
            fs.nactvar -= 1;
            let idx = fs.actvar[fs.nactvar as usize];
            fs.f.locals[idx].end = pc;
        }
    }

    fn create_local(&mut self, name: &[u8]) -> R {
        self.new_localvar(name.to_vec(), 0)?;
        self.adjustlocalvars(1);
        Ok(())
    }

    fn indexupvalue(&mut self, level: usize, name: &[u8], v: &ExpDesc) -> R<i32> {
        let fs = &self.funcs[level];
        if let Some(i) = fs.upvals.iter().position(|u| u.0 == v.k && u.1 == v.info) {
            return Ok(i as i32);
        }
        let n = fs.upvals.len() as i32;
        self.lx.checklimit(n + 1, MAXUPVALUES, "upvalues")?;
        let fs = &mut self.funcs[level];
        fs.f.upvalue_names.push(name.to_vec());
        fs.upvals.push((v.k, v.info));
        Ok(n)
    }

    fn searchvar(fs: &FuncState, n: &[u8]) -> i32 {
        for i in (0..fs.nactvar).rev() {
            if fs.f.locals[fs.actvar[i as usize]].name == n {
                return i;
            }
        }
        -1
    }

    fn markupval(fs: &mut FuncState, level: i32) {
        if let Some(bl) = fs.blocks.iter_mut().rev().find(|b| b.nactvar <= level) {
            bl.upval = true;
        }
    }

    fn singlevaraux(
        &mut self,
        level: Option<usize>,
        n: &[u8],
        var: &mut ExpDesc,
        base: bool,
    ) -> R<K> {
        let Some(level) = level else {
            *var = ExpDesc::new(K::Global, NO_REG);
            return Ok(K::Global);
        };
        let v = Self::searchvar(&self.funcs[level], n);
        if v >= 0 {
            *var = ExpDesc::new(K::Local, v);
            if !base {
                Self::markupval(&mut self.funcs[level], v);
            }
            return Ok(K::Local);
        }
        let upper = level.checked_sub(1);
        if self.singlevaraux(upper, n, var, false)? == K::Global {
            return Ok(K::Global);
        }
        var.info = self.indexupvalue(level, n, var)?;
        var.k = K::Upval;
        Ok(K::Upval)
    }

    fn singlevar(&mut self, var: &mut ExpDesc) -> R {
        let name = self.str_checkname()?;
        let level = Some(self.funcs.len() - 1);
        if self.singlevaraux(level, &name, var, true)? == K::Global {
            var.info = self.string_k(&name);
        }
        Ok(())
    }

    fn adjust_assign(&mut self, nvars: i32, nexps: i32, e: &mut ExpDesc) -> R {
        let mut extra = nvars - nexps;
        if e.k == K::Call {
            extra += 1;
            if extra <= 0 {
                extra = 0;
            } else {
                self.reserveregs(extra - 1)?;
            }
            self.setcallreturns(e, extra);
        } else {
            if e.k != K::Void {
                self.exp2nextreg(e)?;
            }
            if extra > 0 {
                let reg = self.fsr().freereg;
                self.reserveregs(extra)?;
                self.nil(reg, extra);
            }
        }
        Ok(())
    }

    fn code_params(&mut self, nparams: i32, dots: bool) -> R {
        self.adjustlocalvars(nparams);
        let n = self.fsr().nactvar;
        self.lx.checklimit(n, MAXPARAMS, "parameters")?;
        self.fs().f.params = n as u8;
        self.fs().f.is_vararg = u8::from(dots);
        if dots {
            self.create_local(b"arg")?;
        }
        let n = self.fsr().nactvar;
        self.reserveregs(n)
    }

    fn enterlevel(&mut self) -> R {
        self.nestlevel += 1;
        if self.nestlevel > MAXPARSERLEVEL {
            return Err(self.lx.syntaxerror("too many syntax levels"));
        }
        Ok(())
    }

    fn leavelevel(&mut self) {
        self.nestlevel -= 1;
    }

    fn enterblock(&mut self, isbreakable: bool) {
        let fs = self.fs();
        let nactvar = fs.nactvar;
        fs.blocks.push(BlockCnt {
            breaklist: NO_JUMP,
            nactvar,
            upval: false,
            isbreakable,
        });
    }

    fn leaveblock(&mut self) {
        let bl = self.fs().blocks.pop().expect("a block is open");
        self.removevars(bl.nactvar);
        if bl.upval {
            self.code_abc(op::CLOSE, bl.nactvar, 0, 0);
        }
        let fs = self.fs();
        fs.freereg = fs.nactvar;
        self.patchtohere(bl.breaklist);
    }

    fn pushclosure(&mut self, func: FuncState, v: &mut ExpDesc) -> R {
        let np = self.fsr().f.protos.len() as i32;
        if np >= MAXARG_BX {
            return Err(self.lx.syntaxerror("constant table overflow"));
        }
        self.fs().f.protos.push(func.f);
        *v = ExpDesc::new(K::Relocable, self.code_abx(op::CLOSURE, 0, np as u32));
        for (k, info) in &func.upvals {
            let o = if *k == K::Local {
                op::MOVE
            } else {
                op::GETUPVAL
            };
            self.code_abc(o, 0, *info, 0);
        }
        Ok(())
    }

    fn open_func(&mut self) {
        let mut f = Function {
            source: Vec::new(),
            line_defined: 0,
            upvalue_count: 0,
            params: 0,
            is_vararg: 0,
            max_stack: 2,
            lines: Vec::new(),
            locals: Vec::new(),
            upvalue_names: Vec::new(),
            constants: Vec::new(),
            protos: Vec::new(),
            code: Vec::new(),
        };
        // nested functions share the name of the main function (and are dumped without it)
        if self.funcs.is_empty() {
            f.source = self.lx.source.clone();
        }
        self.funcs.push(FuncState {
            f,
            h: HashMap::new(),
            lasttarget: 0,
            jpc: NO_JUMP,
            freereg: 0,
            nactvar: 0,
            actvar: Vec::new(),
            upvals: Vec::new(),
            blocks: Vec::new(),
        });
    }

    fn close_func(&mut self) -> FuncState {
        self.removevars(0);
        self.code_abc(op::RETURN, 0, 1, 0);
        let mut fs = self.funcs.pop().expect("a function is open");
        fs.f.upvalue_count = fs.upvals.len() as u8;
        fs
    }

    // ----- lparser.c: grammar -----------------------------------------------------------------

    fn field(&mut self, v: &mut ExpDesc) -> R {
        let mut key = ExpDesc::new(K::Void, 0);
        self.exp2anyreg(v)?;
        self.lx.next()?;
        self.checkname(&mut key)?;
        self.indexed(v, &mut key)
    }

    fn yindex(&mut self, v: &mut ExpDesc) -> R {
        self.lx.next()?;
        self.expr(v)?;
        self.exp2val(v)?;
        self.check(u16::from(b']'))
    }

    fn recfield(&mut self, cc: &mut ConsControl) -> R {
        let reg = self.fsr().freereg;
        let mut key = ExpDesc::new(K::Void, 0);
        let mut val = ExpDesc::new(K::Void, 0);
        if self.lx.t.token == TK_NAME {
            self.lx
                .checklimit(cc.nh, i32::MAX - 2, "items in a constructor")?;
            cc.nh += 1;
            self.checkname(&mut key)?;
        } else {
            self.yindex(&mut key)?;
        }
        self.check(u16::from(b'='))?;
        self.exp2rk(&mut key)?;
        self.expr(&mut val)?;
        let rkkey = self.exp2rk(&mut key)?;
        let rkval = self.exp2rk(&mut val)?;
        self.code_abc(op::SETTABLE, cc.t_info, rkkey, rkval);
        self.fs().freereg = reg;
        Ok(())
    }

    fn closelistfield(&mut self, cc: &mut ConsControl) -> R {
        if cc.v.k == K::Void {
            return Ok(());
        }
        let mut v = cc.v;
        self.exp2nextreg(&mut v)?;
        cc.v = v;
        cc.v.k = K::Void;
        if cc.tostore == LFIELDS_PER_FLUSH {
            self.code_abx(op::SETLIST, cc.t_info, (cc.na - 1) as u32);
            cc.tostore = 0;
            self.fs().freereg = cc.t_info + 1;
        }
        Ok(())
    }

    fn lastlistfield(&mut self, cc: &mut ConsControl) -> R {
        if cc.tostore == 0 {
            return Ok(());
        }
        if cc.v.k == K::Call {
            let mut v = cc.v;
            self.setcallreturns(&mut v, MULTRET);
            cc.v = v;
            self.code_abx(op::SETLISTO, cc.t_info, (cc.na - 1) as u32);
        } else {
            if cc.v.k != K::Void {
                let mut v = cc.v;
                self.exp2nextreg(&mut v)?;
                cc.v = v;
            }
            self.code_abx(op::SETLIST, cc.t_info, (cc.na - 1) as u32);
        }
        self.fs().freereg = cc.t_info + 1;
        Ok(())
    }

    fn listfield(&mut self, cc: &mut ConsControl) -> R {
        let mut v = ExpDesc::new(K::Void, 0);
        self.expr(&mut v)?;
        cc.v = v;
        self.lx
            .checklimit(cc.na, MAXARG_BX, "items in a constructor")?;
        cc.na += 1;
        cc.tostore += 1;
        Ok(())
    }

    fn constructor(&mut self, t: &mut ExpDesc) -> R {
        let line = self.lx.line;
        let pc = self.code_abc(op::NEWTABLE, 0, 0, 0);
        *t = ExpDesc::new(K::Relocable, pc);
        let mut cc = ConsControl {
            v: ExpDesc::new(K::Void, 0),
            t_info: 0,
            nh: 0,
            na: 0,
            tostore: 0,
        };
        self.exp2nextreg(t)?;
        cc.t_info = t.info;
        self.check(u16::from(b'{'))?;
        loop {
            self.testnext(u16::from(b';'))?;
            if self.lx.t.token == u16::from(b'}') {
                break;
            }
            self.closelistfield(&mut cc)?;
            match self.lx.t.token {
                TK_NAME => {
                    self.lx.lookahead()?;
                    if self.lx.ahead.as_ref().map(|a| a.token) != Some(u16::from(b'=')) {
                        self.listfield(&mut cc)?;
                    } else {
                        self.recfield(&mut cc)?;
                    }
                }
                c if c == u16::from(b'[') => self.recfield(&mut cc)?,
                _ => self.listfield(&mut cc)?,
            }
            if !(self.testnext(u16::from(b','))? || self.testnext(u16::from(b';'))?) {
                break;
            }
        }
        self.check_match(u16::from(b'}'), u16::from(b'{'), line)?;
        self.lastlistfield(&mut cc)?;
        let code = &mut self.fs().f.code[pc as usize];
        set_b(code, int2fb(cc.na as u32));
        set_c(code, log2(cc.nh as u32) + 1);
        Ok(())
    }

    fn parlist(&mut self) -> R {
        let mut nparams = 0;
        let mut dots = false;
        if self.lx.t.token != u16::from(b')') {
            loop {
                match self.lx.t.token {
                    TK_DOTS => {
                        dots = true;
                        self.lx.next()?;
                    }
                    TK_NAME => {
                        let n = self.str_checkname()?;
                        self.new_localvar(n, nparams)?;
                        nparams += 1;
                    }
                    _ => return Err(self.lx.syntaxerror("<name> or `...' expected")),
                }
                if dots || !self.testnext(u16::from(b','))? {
                    break;
                }
            }
        }
        self.code_params(nparams, dots)
    }

    fn body(&mut self, e: &mut ExpDesc, needself: bool, line: u32) -> R {
        self.open_func();
        self.fs().f.line_defined = line;
        self.check(u16::from(b'('))?;
        if needself {
            self.create_local(b"self")?;
        }
        self.parlist()?;
        self.check(u16::from(b')'))?;
        self.chunk()?;
        self.check_match(TK_END, TK_FUNCTION, line)?;
        let func = self.close_func();
        self.pushclosure(func, e)
    }

    fn explist1(&mut self, v: &mut ExpDesc) -> R<i32> {
        let mut n = 1;
        self.expr(v)?;
        while self.testnext(u16::from(b','))? {
            self.exp2nextreg(v)?;
            self.expr(v)?;
            n += 1;
        }
        Ok(n)
    }

    fn funcargs(&mut self, f: &mut ExpDesc) -> R {
        let mut args = ExpDesc::new(K::Void, 0);
        let line = self.lx.line;
        match self.lx.t.token {
            c if c == u16::from(b'(') => {
                if line != self.lx.lastline {
                    return Err(self
                        .lx
                        .syntaxerror("ambiguous syntax (function call x new statement)"));
                }
                self.lx.next()?;
                if self.lx.t.token == u16::from(b')') {
                    args.k = K::Void;
                } else {
                    self.explist1(&mut args)?;
                    self.setcallreturns(&mut args, MULTRET);
                }
                self.check_match(u16::from(b')'), u16::from(b'('), line)?;
            }
            c if c == u16::from(b'{') => self.constructor(&mut args)?,
            TK_STRING => {
                let s = std::mem::take(&mut self.lx.t.s);
                self.codestring(&mut args, &s);
                self.lx.next()?;
            }
            _ => return Err(self.lx.syntaxerror("function arguments expected")),
        }
        let base = f.info;
        let nparams = if args.k == K::Call {
            MULTRET
        } else {
            if args.k != K::Void {
                self.exp2nextreg(&mut args)?;
            }
            self.fsr().freereg - (base + 1)
        };
        *f = ExpDesc::new(K::Call, self.code_abc(op::CALL, base, nparams + 1, 2));
        self.fixline(line);
        self.fs().freereg = base + 1;
        Ok(())
    }

    fn prefixexp(&mut self, v: &mut ExpDesc) -> R {
        match self.lx.t.token {
            c if c == u16::from(b'(') => {
                let line = self.lx.line;
                self.lx.next()?;
                self.expr(v)?;
                self.check_match(u16::from(b')'), u16::from(b'('), line)?;
                self.dischargevars(v);
                Ok(())
            }
            TK_NAME => self.singlevar(v),
            _ => Err(self.lx.syntaxerror("unexpected symbol")),
        }
    }

    fn primaryexp(&mut self, v: &mut ExpDesc) -> R {
        self.prefixexp(v)?;
        loop {
            match self.lx.t.token {
                c if c == u16::from(b'.') => self.field(v)?,
                c if c == u16::from(b'[') => {
                    let mut key = ExpDesc::new(K::Void, 0);
                    self.exp2anyreg(v)?;
                    self.yindex(&mut key)?;
                    self.indexed(v, &mut key)?;
                }
                c if c == u16::from(b':') => {
                    let mut key = ExpDesc::new(K::Void, 0);
                    self.lx.next()?;
                    self.checkname(&mut key)?;
                    self.self_(v, &mut key)?;
                    self.funcargs(v)?;
                }
                c if c == u16::from(b'(') || c == TK_STRING || c == u16::from(b'{') => {
                    self.exp2nextreg(v)?;
                    self.funcargs(v)?;
                }
                _ => return Ok(()),
            }
        }
    }

    fn simpleexp(&mut self, v: &mut ExpDesc) -> R {
        match self.lx.t.token {
            TK_NUMBER => {
                let r = self.lx.t.r;
                *v = ExpDesc::new(K::Kst, self.number_k(r));
            }
            TK_STRING => {
                let s = std::mem::take(&mut self.lx.t.s);
                self.codestring(v, &s);
            }
            TK_NIL => *v = ExpDesc::new(K::Nil, 0),
            TK_TRUE => *v = ExpDesc::new(K::True, 0),
            TK_FALSE => *v = ExpDesc::new(K::False, 0),
            c if c == u16::from(b'{') => return self.constructor(v),
            TK_FUNCTION => {
                self.lx.next()?;
                let line = self.lx.line;
                return self.body(v, false, line);
            }
            _ => return self.primaryexp(v),
        }
        self.lx.next()
    }

    fn unopr(t: u16) -> UnOpr {
        match t {
            TK_NOT => UnOpr::Not,
            c if c == u16::from(b'-') => UnOpr::Minus,
            _ => UnOpr::None,
        }
    }

    fn binopr(t: u16) -> BinOpr {
        match t {
            c if c == u16::from(b'+') => BinOpr::Add,
            c if c == u16::from(b'-') => BinOpr::Sub,
            c if c == u16::from(b'*') => BinOpr::Mul,
            c if c == u16::from(b'/') => BinOpr::Div,
            c if c == u16::from(b'^') => BinOpr::Pow,
            TK_CONCAT => BinOpr::Concat,
            TK_NE => BinOpr::Ne,
            TK_EQ => BinOpr::Eq,
            c if c == u16::from(b'<') => BinOpr::Lt,
            TK_LE => BinOpr::Le,
            c if c == u16::from(b'>') => BinOpr::Gt,
            TK_GE => BinOpr::Ge,
            TK_AND => BinOpr::And,
            TK_OR => BinOpr::Or,
            _ => BinOpr::None,
        }
    }

    /// `subexpr -> (simpleexp | unop subexpr) { binop subexpr }` with the binary operators of
    /// priority above `limit`; returns the first operator not treated.
    fn subexpr(&mut self, v: &mut ExpDesc, limit: i32) -> R<BinOpr> {
        self.enterlevel()?;
        let uop = Self::unopr(self.lx.t.token);
        if uop != UnOpr::None {
            self.lx.next()?;
            self.subexpr(v, UNARY_PRIORITY)?;
            self.prefix(uop, v)?;
        } else {
            self.simpleexp(v)?;
        }
        let mut op = Self::binopr(self.lx.t.token);
        while op != BinOpr::None && PRIORITY[op as usize].0 > limit {
            let mut v2 = ExpDesc::new(K::Void, 0);
            self.lx.next()?;
            self.infix(op, v)?;
            let nextop = self.subexpr(&mut v2, PRIORITY[op as usize].1)?;
            self.posfix(op, v, &mut v2)?;
            op = nextop;
        }
        self.leavelevel();
        Ok(op)
    }

    fn expr(&mut self, v: &mut ExpDesc) -> R {
        self.subexpr(v, -1).map(|_| ())
    }

    fn block_follow(t: u16) -> bool {
        matches!(t, TK_ELSE | TK_ELSEIF | TK_END | TK_UNTIL | TK_EOS)
    }

    fn block(&mut self) -> R {
        self.enterblock(false);
        self.chunk()?;
        self.leaveblock();
        Ok(())
    }

    /// In an assignment to a local variable that an earlier target (a table access) uses, the
    /// local's value is saved in a new register and the earlier assignment uses that copy.
    fn check_conflict(&mut self, lhs: &mut [ExpDesc], v: &ExpDesc) -> R {
        let extra = self.fsr().freereg;
        let mut conflict = false;
        for lh in lhs.iter_mut().filter(|lh| lh.k == K::Indexed) {
            if lh.info == v.info {
                conflict = true;
                lh.info = extra;
            }
            if lh.aux == v.info {
                conflict = true;
                lh.aux = extra;
            }
        }
        if conflict {
            let reg = self.fsr().freereg;
            self.code_abc(op::MOVE, reg, v.info, 0);
            self.reserveregs(1)?;
        }
        Ok(())
    }

    /// `assignment -> primaryexp { ',' primaryexp } '=' explist1`; `lhs` holds the first target.
    /// (The C code recurses once per target and stores on the way back; this stores the same
    /// way, last target first.)
    fn assignment(&mut self, lhs: &mut Vec<ExpDesc>) -> R {
        loop {
            let k = lhs[lhs.len() - 1].k;
            if !matches!(k, K::Local | K::Upval | K::Global | K::Indexed) {
                return Err(self.lx.syntaxerror("syntax error"));
            }
            if !self.testnext(u16::from(b','))? {
                break;
            }
            let mut nv = ExpDesc::new(K::Void, 0);
            self.primaryexp(&mut nv)?;
            if nv.k == K::Local {
                self.check_conflict(lhs, &nv)?;
            }
            lhs.push(nv);
        }
        let nvars = lhs.len() as i32;
        let mut e = ExpDesc::new(K::Void, 0);
        self.check(u16::from(b'='))?;
        let nexps = self.explist1(&mut e)?;
        let mut stored = lhs.len();
        if nexps != nvars {
            self.adjust_assign(nvars, nexps, &mut e)?;
            if nexps > nvars {
                self.fs().freereg -= nexps - nvars;
            }
        } else {
            self.setcallreturns(&mut e, 1);
            stored -= 1;
            self.storevar(&lhs[stored], &mut e)?;
        }
        for var in lhs[..stored].iter().rev() {
            let mut e = ExpDesc::new(K::NonReloc, self.fsr().freereg - 1);
            self.storevar(var, &mut e)?;
        }
        Ok(())
    }

    /// Reads a condition; returns it with the jumps taken when it is false still open.
    fn cond(&mut self) -> R<ExpDesc> {
        let mut v = ExpDesc::new(K::Void, 0);
        self.expr(&mut v)?;
        if v.k == K::Nil {
            v.k = K::False;
        }
        self.goiftrue(&mut v)?;
        self.patchtohere(v.t);
        Ok(v)
    }

    fn whilestat(&mut self, line: u32) -> R {
        self.lx.next()?;
        let whileinit = self.jump();
        let expinit = self.getlabel();
        let mut v = ExpDesc::new(K::Void, 0);
        self.expr(&mut v)?;
        if v.k == K::Kst {
            v.k = K::True;
        }
        let lineexp = self.lx.line;
        self.goiffalse(&mut v)?;
        let jpc = self.fsr().jpc;
        let mut f = v.f;
        self.concat(&mut f, jpc);
        v.f = f;
        self.fs().jpc = NO_JUMP;
        // the condition is coded after the body: take its code out and put it back later
        let sizeexp = self.fsr().pc() - expinit;
        if sizeexp > MAXEXPWHILE {
            return Err(self.lx.syntaxerror("`while' condition too complex"));
        }
        let fs = self.fs();
        let codeexp: Vec<u32> = fs.f.code[expinit as usize..].to_vec();
        fs.f.code.truncate(expinit as usize);
        fs.f.lines.truncate(expinit as usize);
        self.enterblock(true);
        self.check(TK_DO)?;
        let blockinit = self.getlabel();
        self.block()?;
        self.patchtohere(whileinit);
        let shift = self.fsr().pc() - expinit;
        if v.t != NO_JUMP {
            v.t += shift;
        }
        if v.f != NO_JUMP {
            v.f += shift;
        }
        for i in codeexp {
            self.code_line(i, lineexp);
        }
        self.check_match(TK_END, TK_WHILE, line)?;
        self.leaveblock();
        self.patchlist(v.t, blockinit)?;
        self.patchtohere(v.f);
        Ok(())
    }

    fn repeatstat(&mut self, line: u32) -> R {
        let repeat_init = self.getlabel();
        self.enterblock(true);
        self.lx.next()?;
        self.block()?;
        self.check_match(TK_UNTIL, TK_REPEAT, line)?;
        let v = self.cond()?;
        self.patchlist(v.f, repeat_init)?;
        self.leaveblock();
        Ok(())
    }

    fn exp1(&mut self) -> R {
        let mut e = ExpDesc::new(K::Void, 0);
        self.expr(&mut e)?;
        self.exp2nextreg(&mut e)
    }

    fn forbody(&mut self, base: i32, line: u32, nvars: i32, isnum: bool) -> R {
        self.adjustlocalvars(nvars);
        self.check(TK_DO)?;
        self.enterblock(true);
        let prep = self.getlabel();
        self.block()?;
        self.patchtohere(prep - 1);
        let endfor = if isnum {
            self.code_asbx(op::FORLOOP, base, NO_JUMP)
        } else {
            self.code_abc(op::TFORLOOP, base, 0, nvars - 3)
        };
        self.fixline(line);
        let list = if isnum { endfor } else { self.jump() };
        self.patchlist(list, prep)?;
        self.leaveblock();
        Ok(())
    }

    fn fornum(&mut self, varname: Vec<u8>, line: u32) -> R {
        let base = self.fsr().freereg;
        self.new_localvar(varname, 0)?;
        self.new_localvar(b"(for limit)".to_vec(), 1)?;
        self.new_localvar(b"(for step)".to_vec(), 2)?;
        self.check(u16::from(b'='))?;
        self.exp1()?;
        self.check(u16::from(b','))?;
        self.exp1()?;
        if self.testnext(u16::from(b','))? {
            self.exp1()?;
        } else {
            let reg = self.fsr().freereg;
            let k = self.number_k(1.0);
            self.code_abx(op::LOADK, reg, k as u32);
            self.reserveregs(1)?;
        }
        let r = self.fsr().freereg;
        self.code_abc(op::SUB, r - 3, r - 3, r - 1);
        self.jump();
        self.forbody(base, line, 3, true)
    }

    fn forlist(&mut self, indexname: Vec<u8>) -> R {
        let mut e = ExpDesc::new(K::Void, 0);
        let mut nvars = 0;
        let base = self.fsr().freereg;
        self.new_localvar(b"(for generator)".to_vec(), nvars)?;
        nvars += 1;
        self.new_localvar(b"(for state)".to_vec(), nvars)?;
        nvars += 1;
        self.new_localvar(indexname, nvars)?;
        nvars += 1;
        while self.testnext(u16::from(b','))? {
            let n = self.str_checkname()?;
            self.new_localvar(n, nvars)?;
            nvars += 1;
        }
        self.check(TK_IN)?;
        let line = self.lx.line;
        let nexps = self.explist1(&mut e)?;
        self.adjust_assign(nvars, nexps, &mut e)?;
        self.checkstack(3)?;
        self.code_asbx(op::TFORPREP, base, NO_JUMP);
        self.forbody(base, line, nvars, false)
    }

    fn forstat(&mut self, line: u32) -> R {
        self.enterblock(false);
        self.lx.next()?;
        let varname = self.str_checkname()?;
        match self.lx.t.token {
            c if c == u16::from(b'=') => self.fornum(varname, line)?,
            c if c == u16::from(b',') || c == TK_IN => self.forlist(varname)?,
            _ => return Err(self.lx.syntaxerror("`=' or `in' expected")),
        }
        self.check_match(TK_END, TK_FOR, line)?;
        self.leaveblock();
        Ok(())
    }

    fn test_then_block(&mut self) -> R<ExpDesc> {
        self.lx.next()?;
        let v = self.cond()?;
        self.check(TK_THEN)?;
        self.block()?;
        Ok(v)
    }

    fn ifstat(&mut self, line: u32) -> R {
        let mut escapelist = NO_JUMP;
        let mut v = self.test_then_block()?;
        while self.lx.t.token == TK_ELSEIF {
            let j = self.jump();
            self.concat(&mut escapelist, j);
            self.patchtohere(v.f);
            v = self.test_then_block()?;
        }
        if self.lx.t.token == TK_ELSE {
            let j = self.jump();
            self.concat(&mut escapelist, j);
            self.patchtohere(v.f);
            self.lx.next()?;
            self.block()?;
        } else {
            self.concat(&mut escapelist, v.f);
        }
        self.patchtohere(escapelist);
        self.check_match(TK_END, TK_IF, line)
    }

    fn localfunc(&mut self) -> R {
        let name = self.str_checkname()?;
        self.new_localvar(name, 0)?;
        let v = ExpDesc::new(K::Local, self.fsr().freereg);
        let mut b = ExpDesc::new(K::Void, 0);
        self.reserveregs(1)?;
        self.adjustlocalvars(1);
        let line = self.lx.line;
        self.body(&mut b, false, line)?;
        self.storevar(&v, &mut b)?;
        // debug information will only see the variable after this point
        let fs = self.fs();
        let idx = fs.actvar[fs.nactvar as usize - 1];
        let pc = fs.pc() as u32;
        fs.f.locals[idx].start = pc;
        Ok(())
    }

    fn localstat(&mut self) -> R {
        let mut nvars = 0;
        let mut e = ExpDesc::new(K::Void, 0);
        loop {
            let n = self.str_checkname()?;
            self.new_localvar(n, nvars)?;
            nvars += 1;
            if !self.testnext(u16::from(b','))? {
                break;
            }
        }
        let nexps = if self.testnext(u16::from(b'='))? {
            self.explist1(&mut e)?
        } else {
            e.k = K::Void;
            0
        };
        self.adjust_assign(nvars, nexps, &mut e)?;
        self.adjustlocalvars(nvars);
        Ok(())
    }

    fn funcname(&mut self, v: &mut ExpDesc) -> R<bool> {
        let mut needself = false;
        self.singlevar(v)?;
        while self.lx.t.token == u16::from(b'.') {
            self.field(v)?;
        }
        if self.lx.t.token == u16::from(b':') {
            needself = true;
            self.field(v)?;
        }
        Ok(needself)
    }

    fn funcstat(&mut self, line: u32) -> R {
        let mut v = ExpDesc::new(K::Void, 0);
        let mut b = ExpDesc::new(K::Void, 0);
        self.lx.next()?;
        let needself = self.funcname(&mut v)?;
        self.body(&mut b, needself, line)?;
        self.storevar(&v, &mut b)?;
        self.fixline(line);
        Ok(())
    }

    fn exprstat(&mut self) -> R {
        let mut v = ExpDesc::new(K::Void, 0);
        self.primaryexp(&mut v)?;
        if v.k == K::Call {
            self.setcallreturns(&mut v, 0);
            Ok(())
        } else {
            let mut lhs = vec![v];
            self.assignment(&mut lhs)
        }
    }

    fn retstat(&mut self) -> R {
        let mut e = ExpDesc::new(K::Void, 0);
        self.lx.next()?;
        let (first, nret);
        if Self::block_follow(self.lx.t.token) || self.lx.t.token == u16::from(b';') {
            first = 0;
            nret = 0;
        } else {
            let n = self.explist1(&mut e)?;
            if e.k == K::Call {
                self.setcallreturns(&mut e, MULTRET);
                if n == 1 {
                    set_op(self.getcode_mut(&e), op::TAILCALL);
                }
                first = self.fsr().nactvar;
                nret = MULTRET;
            } else if n == 1 {
                first = self.exp2anyreg(&mut e)?;
                nret = n;
            } else {
                self.exp2nextreg(&mut e)?;
                first = self.fsr().nactvar;
                nret = n;
            }
        }
        self.code_abc(op::RETURN, first, nret + 1, 0);
        Ok(())
    }

    fn breakstat(&mut self) -> R {
        self.lx.next()?;
        let mut upval = false;
        let mut found = None;
        for (i, bl) in self.fsr().blocks.iter().enumerate().rev() {
            if bl.isbreakable {
                found = Some(i);
                break;
            }
            upval |= bl.upval;
        }
        let Some(i) = found else {
            return Err(self.lx.syntaxerror("no loop to break"));
        };
        if upval {
            let n = self.fsr().blocks[i].nactvar;
            self.code_abc(op::CLOSE, n, 0, 0);
        }
        let j = self.jump();
        let mut list = self.fsr().blocks[i].breaklist;
        self.concat(&mut list, j);
        self.fs().blocks[i].breaklist = list;
        Ok(())
    }

    /// Returns whether the statement must be the last of its block.
    fn statement(&mut self) -> R<bool> {
        let line = self.lx.line;
        match self.lx.t.token {
            TK_IF => self.ifstat(line)?,
            TK_WHILE => self.whilestat(line)?,
            TK_DO => {
                self.lx.next()?;
                self.block()?;
                self.check_match(TK_END, TK_DO, line)?;
            }
            TK_FOR => self.forstat(line)?,
            TK_REPEAT => self.repeatstat(line)?,
            TK_FUNCTION => self.funcstat(line)?,
            TK_LOCAL => {
                self.lx.next()?;
                if self.testnext(TK_FUNCTION)? {
                    self.localfunc()?;
                } else {
                    self.localstat()?;
                }
            }
            TK_RETURN => {
                self.retstat()?;
                return Ok(true);
            }
            TK_BREAK => {
                self.breakstat()?;
                return Ok(true);
            }
            _ => self.exprstat()?,
        }
        Ok(false)
    }

    fn chunk(&mut self) -> R {
        let mut islast = false;
        self.enterlevel()?;
        while !islast && !Self::block_follow(self.lx.t.token) {
            islast = self.statement()?;
            self.testnext(u16::from(b';'))?;
            let fs = self.fs();
            fs.freereg = fs.nactvar;
        }
        self.leavelevel();
        Ok(())
    }
}

impl Parser<'_> {
    fn parse_chunk(&mut self) -> R<Chunk> {
        self.open_func();
        self.lx.next()?;
        self.chunk()?;
        if self.lx.t.token != TK_EOS {
            return Err(self.lx.syntaxerror("<eof> expected"));
        }
        let main = self.close_func();
        Ok(Chunk { main: main.f })
    }
}

/// Compiles Lua 5.0 source. `source` is the chunk name stored in the bytecode (`luac` uses
/// `@file`); nested functions carry no name of their own.
pub fn compile(text: &[u8], source: &[u8]) -> Result<Chunk, CompileError> {
    let mut p = Parser {
        lx: Lexer::new(text, source),
        funcs: Vec::new(),
        nestlevel: 0,
        deferred: None,
    };
    let parsed = p.parse_chunk();
    // an error found while patching jumps came first, whatever happened after it
    if let Some(e) = p.deferred.take() {
        return Err(e);
    }
    parsed
}

/// Compiles a text typed or edited as Unicode; see [`TextEncoding`]. `cp949` turns a run of
/// characters into CP949 bytes (`None` when a character is not Korean).
pub fn compile_text(
    text: &str,
    source: &[u8],
    encoding: TextEncoding,
    cp949: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Chunk, CompileError> {
    let bytes: Cow<[u8]> = crate::luacomp::text_bytes(text, encoding, cp949);
    compile(&bytes, source)
}

/// Removes the debug information like `luac -s` of Lua 5.0: line numbers, local and upvalue names;
/// the main function gets the name `=(none)`.
pub fn strip(chunk: &mut Chunk) {
    fn go(f: &mut Function) {
        f.source.clear();
        f.lines.clear();
        f.locals.clear();
        f.upvalue_names.clear();
        f.protos.iter_mut().for_each(go);
    }
    go(&mut chunk.main);
    chunk.main.source = b"=(none)".to_vec();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::luac50;

    fn ops(f: &Function) -> Vec<u32> {
        f.code.iter().map(|&i| get_op(i)).collect()
    }

    #[test]
    fn compiles_what_luac_makes_for_a_small_chunk() {
        // x = 1  ->  LOADK 0 0; SETGLOBAL 0 1; RETURN 0 1, as `luac` 5.0.3 lists it
        let c = compile(b"x = 1", b"@t.lua").unwrap();
        assert_eq!(ops(&c.main), [op::LOADK, op::SETGLOBAL, op::RETURN]);
        assert_eq!(
            c.main.constants,
            [Constant::Str(b"x".to_vec()), Constant::Number(1.0)]
        );
        assert_eq!(c.main.source, b"@t.lua");
        assert_eq!((c.main.is_vararg, c.main.max_stack), (0, 2));
        assert_eq!(c.main.lines, [1, 1, 1]);
        assert!(luac50::validate(&c).is_ok());
        let bytes = luac50::dump32(&c);
        assert_eq!(luac50::parse(&bytes).unwrap(), c);
    }

    #[test]
    fn five_zero_codegen_differs_from_five_one() {
        // no constant folding, `-5` makes a second constant, nil is a constant of its own
        let c = compile(b"a = 2 * 3\nb = -5\nc = nil == a", b"=t")
            .unwrap()
            .main;
        assert!(ops(&c).contains(&(op::ADD + 2)), "MUL is not folded");
        assert!(c.constants.contains(&Constant::Number(5.0)));
        assert!(c.constants.contains(&Constant::Number(-5.0)));
        assert!(c.constants.contains(&Constant::Nil));
        // `...` is the local table `arg`; no instruction for it
        let c = compile(b"function f(a, ...) return arg end", b"=t").unwrap();
        let f = &c.main.protos[0];
        assert_eq!((f.params, f.is_vararg), (1, 1));
        assert_eq!(f.locals[1].name, b"arg");
        // loops and constructors
        let c = compile(
            b"for i = 1, 3 do end for k, v in t do end t = {1, 2, x = 3, f()}",
            b"=t",
        )
        .unwrap()
        .main;
        let o = ops(&c);
        for want in [
            op::FORLOOP,
            op::TFORPREP,
            op::TFORLOOP,
            op::SETLISTO,
            op::NEWTABLE,
        ] {
            assert!(o.contains(&want), "missing opcode {want}: {o:?}");
        }
        assert_eq!(
            get_b(c.code[o.iter().position(|&x| x == op::NEWTABLE).unwrap()]),
            3
        );
    }

    #[test]
    fn lexer_rules_of_five_zero() {
        let c = compile(
            b"--[[ a [[nested]] comment ]] s = [[x\n[[y]] ]] .. '\\65\\\n' .. \"\\q\"\n-- rest\nn = 1e2 + .5 + 5.",
            b"=t",
        )
        .unwrap()
        .main;
        assert!(c.constants.contains(&Constant::Str(b"x\n[[y]] ".to_vec())));
        assert!(c.constants.contains(&Constant::Str(b"A\n".to_vec())));
        assert!(c.constants.contains(&Constant::Str(b"q".to_vec())));
        assert!(c.constants.contains(&Constant::Number(100.0)));
        // no `%`, no hexadecimal numbers, no `#`
        for src in ["x = 1 % 2", "x = 0x10", "x = #t"] {
            assert!(compile(src.as_bytes(), b"=t").is_err(), "{src}");
        }
        // a first line starting with `#` is skipped, the line count goes on
        let c = compile(b"#!lua\nx = 1", b"=t").unwrap().main;
        assert_eq!(c.lines[0], 2);
    }

    #[test]
    fn errors_carry_name_line_and_token() {
        let e = compile(b"x = 1\ny = = 2", b"@a.lua").unwrap_err();
        assert_eq!(e.line, 2);
        assert_eq!(e.message, "a.lua:2: unexpected symbol near `='");
        for (src, want) in [
            ("x = ", "unexpected symbol near `<eof>'"),
            ("if x then", "`end' expected near `<eof>'"),
            ("for i = 1 do end", "`,' expected near `do'"),
            ("local t = {1", "`}' expected near `<eof>'"),
            ("break", "no loop to break near `<eof>'"),
            ("x = 'abc", "unfinished string near `<eof>'"),
            ("x = 3e", "malformed number near `3e'"),
            ("x = ...", "unexpected symbol near `...'"),
            ("x = [[abc", "unfinished long string near `<eof>'"),
            (
                "f()\n(g)()",
                "ambiguous syntax (function call x new statement) near `('",
            ),
            ("x = \u{1}", "invalid control char near `char(1)'"),
        ] {
            let e = compile(src.as_bytes(), b"=t").unwrap_err();
            assert!(e.message.ends_with(want), "{src}: {}", e.message);
        }
    }

    #[test]
    fn garbage_is_an_error_or_a_chunk_never_a_panic() {
        let src = format!("x = {}1{}", "(".repeat(500), ")".repeat(500));
        assert!(compile(src.as_bytes(), b"=t").is_err());
        let src = "do ".repeat(300) + &"end ".repeat(300);
        assert!(compile(src.as_bytes(), b"=t").is_err());
        let src = "a,".repeat(100_000) + "a = 1";
        assert!(compile(src.as_bytes(), b"=t").is_err());
        let seed = b"local a, b = 1\nfunction f(x, ...) for i = 1, x do if a and not b or x then return {i, [x] = a} end end end\nwhile a do repeat a = a.b:c(1, 'x') until a end";
        let mut state = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..4000 {
            let mut bytes = seed.to_vec();
            for _ in 0..1 + next() % 4 {
                let at = (next() % bytes.len() as u64) as usize;
                match next() % 3 {
                    0 => bytes[at] = next() as u8,
                    1 => {
                        bytes.remove(at);
                    }
                    _ => bytes.insert(at, next() as u8),
                }
            }
            let _ = compile(&bytes, b"=t");
        }
        for _ in 0..2000 {
            let n = (next() % 64) as usize;
            let bytes: Vec<u8> = (0..n).map(|_| next() as u8).collect();
            let _ = compile(&bytes, b"=t");
        }
    }
}
