//! Compiler from Lua 5.1 source text to the bytecode of `luac` (a [`Chunk`]), so that edited
//! scripts can be written back as `.lub`.
//!
//! This is a port of the lexer, parser and code generator of Lua 5.1.5 (`llex.c`, `lparser.c`,
//! `lcode.c`; Copyright PUC-Rio, MIT licence, see docs/05-licensing.md). It keeps the structure
//! and the register allocation of the original so that the generated code is the same as the one
//! the real `luac` makes. The tool `tools/lua-verify` checks that: the decompiled text of every
//! compiled script of the real archives compiles to the original code again.
//!
//! Differences from `luac`: strings are bytes (no locale), numbers are read with Rust's parser
//! (decimal, exponents, hexadecimal integers), and the chunk name is given by the caller.

use std::collections::HashMap;

use crate::luac::{Chunk, Constant, Function, Local};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub line: u32,
    /// Complete message in the style of Lua: `name:line: message near 'token'`.
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CompileError {}

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
    pub const MUL: u32 = 14;
    pub const DIV: u32 = 15;
    pub const MOD: u32 = 16;
    pub const POW: u32 = 17;
    pub const UNM: u32 = 18;
    pub const NOT: u32 = 19;
    pub const LEN: u32 = 20;
    pub const CONCAT: u32 = 21;
    pub const JMP: u32 = 22;
    pub const EQ: u32 = 23;
    pub const LT: u32 = 24;
    pub const LE: u32 = 25;
    pub const TEST: u32 = 26;
    pub const TESTSET: u32 = 27;
    pub const CALL: u32 = 28;
    pub const TAILCALL: u32 = 29;
    pub const RETURN: u32 = 30;
    pub const FORLOOP: u32 = 31;
    pub const FORPREP: u32 = 32;
    pub const TFORLOOP: u32 = 33;
    pub const SETLIST: u32 = 34;
    pub const CLOSE: u32 = 35;
    pub const CLOSURE: u32 = 36;
    pub const VARARG: u32 = 37;
}

const NO_JUMP: i32 = -1;
const NO_REG: i32 = 255;
const MAXARG_BX: i32 = 262_143;
const MAXARG_SBX: i32 = 131_071;
const MAXARG_C: i32 = 511;
const BITRK: i32 = 256;
const MAXINDEXRK: i32 = 255;
const MAXSTACK: i32 = 250;
const LFIELDS_PER_FLUSH: i32 = 50;
const LUAI_MAXVARS: i32 = 200;
const LUAI_MAXUPVALUES: i32 = 60;
const LUAI_MAXCCALLS: u32 = 200;
const MULTRET: i32 = -1;
const VARARG_HASARG: u8 = 1;
const VARARG_ISVARARG: u8 = 2;
const VARARG_NEEDSARG: u8 = 4;

fn get_op(i: u32) -> u32 {
    i & 0x3f
}
fn get_a(i: u32) -> i32 {
    ((i >> 6) & 0xff) as i32
}
fn get_c(i: u32) -> i32 {
    ((i >> 14) & 0x1ff) as i32
}
fn get_b(i: u32) -> i32 {
    ((i >> 23) & 0x1ff) as i32
}
fn get_sbx(i: u32) -> i32 {
    (i >> 14) as i32 - MAXARG_SBX
}
fn set_op(i: &mut u32, o: u32) {
    *i = (*i & !0x3f) | o;
}
fn set_a(i: &mut u32, a: i32) {
    *i = (*i & !(0xff << 6)) | ((a as u32 & 0xff) << 6);
}
fn set_b(i: &mut u32, b: i32) {
    *i = (*i & !(0x1ff << 23)) | ((b as u32 & 0x1ff) << 23);
}
fn set_c(i: &mut u32, c: i32) {
    *i = (*i & !(0x1ff << 14)) | ((c as u32 & 0x1ff) << 14);
}
fn set_sbx(i: &mut u32, v: i32) {
    *i = (*i & 0x3fff) | (((v + MAXARG_SBX) as u32) << 14);
}
fn create_abc(o: u32, a: i32, b: i32, c: i32) -> u32 {
    o | ((a as u32 & 0xff) << 6) | ((c as u32 & 0x1ff) << 14) | ((b as u32 & 0x1ff) << 23)
}
fn create_abx(o: u32, a: i32, bx: u32) -> u32 {
    o | ((a as u32 & 0xff) << 6) | (bx << 14)
}
fn is_k(x: i32) -> bool {
    x & BITRK != 0
}
fn rk_as_k(x: i32) -> i32 {
    x | BITRK
}
/// Instructions that test and are followed by a jump (`testTMode`).
fn test_t_mode(o: u32) -> bool {
    matches!(o, op::EQ | op::LT | op::LE | op::TEST | op::TESTSET)
}

/// `luaO_int2fb`: the "floating point byte" of table size hints.
fn int2fb(mut x: u32) -> i32 {
    let mut e = 0;
    while x >= 16 {
        x = (x + 1) >> 1;
        e += 1;
    }
    if x < 8 {
        x as i32
    } else {
        ((e + 1) << 3) | (x as i32 - 8)
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
const TK_CONCAT: u16 = 278;
const TK_DOTS: u16 = 279;
const TK_EQ: u16 = 280;
const TK_GE: u16 = 281;
const TK_LE: u16 = 282;
const TK_NE: u16 = 283;
const TK_NUMBER: u16 = 284;
const TK_NAME: u16 = 285;
const TK_STRING: u16 = 286;
const TK_EOS: u16 = 287;

const TOKENS: [&str; 31] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while", "..", "...", "==",
    ">=", "<=", "~=", "<number>", "<name>", "<string>", "<eof>",
];

const EOZ: i32 = -1;

#[derive(Clone, Default)]
struct Tok {
    token: u16,
    r: f64,
    /// Contents of a name or string.
    s: Vec<u8>,
    /// The text as written (for messages).
    raw: Vec<u8>,
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    cur: i32,
    line: u32,
    lastline: u32,
    t: Tok,
    ahead: Option<Tok>,
    buf: Vec<u8>,
    source: Vec<u8>,
}

fn is_digit(c: i32) -> bool {
    (b'0' as i32..=b'9' as i32).contains(&c)
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

fn chunk_id(source: &[u8]) -> String {
    const MAXSRC: usize = 80;
    match source.first() {
        Some(b'=') => String::from_utf8_lossy(&source[1..source.len().min(MAXSRC)]).into_owned(),
        Some(b'@') => {
            let name = String::from_utf8_lossy(&source[1..]).into_owned();
            if name.len() > MAXSRC - 1 {
                format!("...{}", &name[name.len() - (MAXSRC - 4)..])
            } else {
                name
            }
        }
        _ => {
            let first = source.split(|&b| b == b'\n').next().unwrap_or(&[]);
            let truncated = first.len() < source.len() || first.len() > 40;
            let shown: String = String::from_utf8_lossy(&first[..first.len().min(40)]).into_owned();
            if truncated {
                format!("[string \"{shown}...\"]")
            } else {
                format!("[string \"{shown}\"]")
            }
        }
    }
}

fn token_text(token: u16) -> String {
    if token < FIRST_RESERVED {
        if token < 32 || token == 127 {
            format!("char({token})")
        } else {
            (token as u8 as char).to_string()
        }
    } else {
        TOKENS[usize::from(token - FIRST_RESERVED)].to_owned()
    }
}

fn str2d(s: &[u8]) -> Option<f64> {
    let t = std::str::from_utf8(s).ok()?;
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16).ok().map(|v| v as f64);
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite())
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

    fn is_newline(&self) -> bool {
        self.cur == 10 || self.cur == 13
    }

    fn error_at(&self, msg: &str, near: Option<&[u8]>) -> CompileError {
        let mut message = format!("{}:{}: {msg}", chunk_id(&self.source), self.line);
        if let Some(n) = near {
            message.push_str(&format!(" near '{}'", String::from_utf8_lossy(n)));
        }
        CompileError {
            line: self.line,
            message,
        }
    }

    fn lexerror(&self, msg: &str, token: u16) -> CompileError {
        let near = match token {
            0 => None,
            TK_NAME | TK_STRING | TK_NUMBER => Some(self.buf.clone()),
            t => Some(token_text(t).into_bytes()),
        };
        self.error_at(msg, near.as_deref())
    }

    fn syntaxerror(&self, msg: &str) -> CompileError {
        let near = match self.t.token {
            TK_NAME | TK_STRING | TK_NUMBER => self.t.raw.clone(),
            t => token_text(t).into_bytes(),
        };
        self.error_at(msg, Some(&near))
    }

    fn inc_line(&mut self) -> R {
        let old = self.cur;
        self.advance();
        if self.is_newline() && self.cur != old {
            self.advance();
        }
        self.line += 1;
        if self.line >= i32::MAX as u32 {
            return Err(self.syntaxerror("chunk has too many lines"));
        }
        Ok(())
    }

    fn check_next(&mut self, set: &[u8]) -> bool {
        if self.cur >= 0 && set.contains(&(self.cur as u8)) {
            self.save_and_next();
            true
        } else {
            false
        }
    }

    fn read_numeral(&mut self) -> R<f64> {
        loop {
            self.save_and_next();
            if !(is_digit(self.cur) || self.cur == i32::from(b'.')) {
                break;
            }
        }
        if self.check_next(b"Ee") {
            self.check_next(b"+-");
        }
        while is_alnum(self.cur) || self.cur == i32::from(b'_') {
            self.save_and_next();
        }
        str2d(&self.buf).ok_or_else(|| self.lexerror("malformed number", TK_NUMBER))
    }

    fn skip_sep(&mut self) -> i32 {
        let mut count = 0;
        let s = self.cur;
        self.save_and_next();
        while self.cur == i32::from(b'=') {
            self.save_and_next();
            count += 1;
        }
        if self.cur == s { count } else { -count - 1 }
    }

    fn read_long_string(&mut self, want_value: bool, sep: i32) -> R<Vec<u8>> {
        self.save_and_next(); // skip 2nd '['
        if self.is_newline() {
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
                c if c == i32::from(b']') => {
                    if self.skip_sep() == sep {
                        self.save_and_next(); // skip 2nd ']'
                        break;
                    }
                }
                10 | 13 => {
                    self.save(10);
                    self.inc_line()?;
                    if !want_value {
                        self.buf.clear();
                    }
                }
                _ => {
                    if want_value {
                        self.save_and_next();
                    } else {
                        self.advance();
                    }
                }
            }
        }
        if !want_value {
            return Ok(Vec::new());
        }
        let skip = (2 + sep) as usize;
        Ok(self.buf[skip..self.buf.len() - skip].to_vec())
    }

    fn read_string(&mut self, del: i32) -> R<Vec<u8>> {
        self.save_and_next();
        while self.cur != del {
            match self.cur {
                EOZ => return Err(self.lexerror("unfinished string", TK_EOS)),
                10 | 13 => return Err(self.lexerror("unfinished string", TK_STRING)),
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
                        10 | 13 => {
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
        self.save_and_next(); // skip delimiter
        Ok(self.buf[1..self.buf.len() - 1].to_vec())
    }

    fn llex(&mut self) -> R<Tok> {
        self.buf.clear();
        let mut t = Tok::default();
        loop {
            let c = self.cur;
            match c {
                10 | 13 => {
                    self.inc_line()?;
                }
                c if c == i32::from(b'-') => {
                    self.advance();
                    if self.cur != i32::from(b'-') {
                        t.token = u16::from(b'-');
                        return Ok(t);
                    }
                    self.advance();
                    if self.cur == i32::from(b'[') {
                        let sep = self.skip_sep();
                        self.buf.clear();
                        if sep >= 0 {
                            self.read_long_string(false, sep)?;
                            self.buf.clear();
                            continue;
                        }
                    }
                    while !self.is_newline() && self.cur != EOZ {
                        self.advance();
                    }
                }
                c if c == i32::from(b'[') => {
                    let sep = self.skip_sep();
                    if sep >= 0 {
                        t.s = self.read_long_string(true, sep)?;
                        t.raw = self.buf.clone();
                        t.token = TK_STRING;
                        return Ok(t);
                    } else if sep == -1 {
                        t.token = u16::from(b'[');
                        return Ok(t);
                    }
                    return Err(self.lexerror("invalid long string delimiter", TK_STRING));
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
                    t.raw = self.buf.clone();
                    t.token = TK_STRING;
                    return Ok(t);
                }
                c if c == i32::from(b'.') => {
                    self.save_and_next();
                    if self.check_next(b".") {
                        t.token = if self.check_next(b".") {
                            TK_DOTS
                        } else {
                            TK_CONCAT
                        };
                        return Ok(t);
                    } else if !is_digit(self.cur) {
                        t.token = u16::from(b'.');
                        return Ok(t);
                    }
                    t.r = self.read_numeral()?;
                    t.raw = self.buf.clone();
                    t.token = TK_NUMBER;
                    return Ok(t);
                }
                EOZ => {
                    t.token = TK_EOS;
                    return Ok(t);
                }
                c if is_space(c) => self.advance(),
                c if is_digit(c) => {
                    t.r = self.read_numeral()?;
                    t.raw = self.buf.clone();
                    t.token = TK_NUMBER;
                    return Ok(t);
                }
                c if is_alpha(c) || c == i32::from(b'_') => {
                    loop {
                        self.save_and_next();
                        if !(is_alnum(self.cur) || self.cur == i32::from(b'_')) {
                            break;
                        }
                    }
                    let name = self.buf.clone();
                    if let Some(i) = TOKENS[..21].iter().position(|w| w.as_bytes() == name) {
                        t.token = FIRST_RESERVED + i as u16;
                    } else {
                        t.token = TK_NAME;
                        t.raw = name.clone();
                        t.s = name;
                    }
                    return Ok(t);
                }
                c => {
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
    Num,
    Local,
    Upval,
    Global,
    Indexed,
    Jmp,
    Relocable,
    NonReloc,
    Call,
    Vararg,
}

#[derive(Clone, Copy, Debug)]
struct ExpDesc {
    k: K,
    info: i32,
    aux: i32,
    nval: f64,
    t: i32,
    f: i32,
}

impl ExpDesc {
    fn new(k: K, info: i32) -> Self {
        Self {
            k,
            info,
            aux: 0,
            nval: 0.0,
            t: NO_JUMP,
            f: NO_JUMP,
        }
    }
    fn has_jumps(&self) -> bool {
        self.t != self.f
    }
    fn is_numeral(&self) -> bool {
        self.k == K::Num && self.t == NO_JUMP && self.f == NO_JUMP
    }
}

fn has_multret(k: K) -> bool {
    matches!(k, K::Call | K::Vararg)
}

#[derive(Hash, PartialEq, Eq)]
enum CKey {
    Nil,
    Bool(bool),
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
    actvar: Vec<u16>,
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
    Len,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BinOpr {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
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
const PRIORITY: [(u32, u32); 15] = [
    (6, 6),
    (6, 6),
    (7, 7),
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
const UNARY_PRIORITY: u32 = 8;

struct Parser<'a> {
    lx: Lexer<'a>,
    funcs: Vec<FuncState>,
    nccalls: u32,
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

    fn code(&mut self, i: u32) -> i32 {
        self.dischargejpc();
        let line = self.lx.lastline;
        let fs = self.fs();
        fs.f.code.push(i);
        fs.f.lines.push(line);
        fs.pc() - 1
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
            if fs.pc() == 0 {
                if from >= fs.nactvar {
                    return;
                }
            } else {
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

    fn ret(&mut self, first: i32, nret: i32) {
        self.code_abc(op::RETURN, first, nret + 1, 0);
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

    fn need_value(&self, mut list: i32) -> bool {
        while list != NO_JUMP {
            let i = self.fsr().f.code[self.getjumpcontrol(list)];
            if get_op(i) != op::TESTSET {
                return true;
            }
            list = self.getjump(list);
        }
        false
    }

    fn patchtestreg(&mut self, node: i32, reg: i32) -> bool {
        let at = self.getjumpcontrol(node);
        let code = &mut self.fs().f.code;
        let i = code[at];
        if get_op(i) != op::TESTSET {
            return false;
        }
        if reg != NO_REG && reg != get_b(i) {
            set_a(&mut code[at], reg);
        } else {
            code[at] = create_abc(op::TEST, get_b(i), 0, get_c(i));
        }
        true
    }

    fn removevalues(&mut self, mut list: i32) {
        while list != NO_JUMP {
            self.patchtestreg(list, NO_REG);
            list = self.getjump(list);
        }
    }

    fn patchlistaux(&mut self, mut list: i32, vtarget: i32, reg: i32, dtarget: i32) -> R {
        while list != NO_JUMP {
            let next = self.getjump(list);
            if self.patchtestreg(list, reg) {
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
            self.patchtohere(list)
        } else {
            self.patchlistaux(list, target, NO_REG, target)
        }
    }

    fn patchtohere(&mut self, list: i32) -> R {
        self.getlabel();
        let mut jpc = self.fsr().jpc;
        self.concat(&mut jpc, list);
        self.fs().jpc = jpc;
        Ok(())
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
        if !is_k(reg) && reg >= self.fsr().nactvar {
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

    fn bool_k(&mut self, b: bool) -> i32 {
        self.addk(CKey::Bool(b), Constant::Bool(b))
    }

    fn nil_k(&mut self) -> i32 {
        self.addk(CKey::Nil, Constant::Nil)
    }

    fn getcode_mut(&mut self, e: &ExpDesc) -> &mut u32 {
        &mut self.fs().f.code[e.info as usize]
    }

    fn setreturns(&mut self, e: &mut ExpDesc, nresults: i32) -> R {
        if e.k == K::Call {
            set_c(self.getcode_mut(e), nresults + 1);
        } else if e.k == K::Vararg {
            set_b(self.getcode_mut(e), nresults + 1);
            let reg = self.fsr().freereg;
            set_a(self.getcode_mut(e), reg);
            self.reserveregs(1)?;
        }
        Ok(())
    }

    fn setmultret(&mut self, e: &mut ExpDesc) -> R {
        self.setreturns(e, MULTRET)
    }

    fn setoneret(&mut self, e: &mut ExpDesc) {
        if e.k == K::Call {
            e.k = K::NonReloc;
            e.info = get_a(*self.getcode_mut(e));
        } else if e.k == K::Vararg {
            set_b(self.getcode_mut(e), 2);
            e.k = K::Relocable;
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
            K::Vararg | K::Call => self.setoneret(e),
            _ => {}
        }
    }

    fn code_label(&mut self, a: i32, b: i32, jump: i32) -> i32 {
        self.getlabel();
        self.code_abc(op::LOADBOOL, a, b, jump)
    }

    fn discharge2reg(&mut self, e: &mut ExpDesc, reg: i32) -> R {
        self.dischargevars(e);
        match e.k {
            K::Nil => self.nil(reg, 1),
            K::False | K::True => {
                self.code_abc(op::LOADBOOL, reg, i32::from(e.k == K::True), 0);
            }
            K::Kst => {
                self.code_abx(op::LOADK, reg, e.info as u32);
            }
            K::Num => {
                let k = self.number_k(e.nval);
                self.code_abx(op::LOADK, reg, k as u32);
            }
            K::Relocable => set_a(self.getcode_mut(e), reg),
            K::NonReloc => {
                if reg != e.info {
                    self.code_abc(op::MOVE, reg, e.info, 0);
                }
            }
            _ => return Ok(()),
        }
        e.info = reg;
        e.k = K::NonReloc;
        Ok(())
    }

    fn discharge2anyreg(&mut self, e: &mut ExpDesc) -> R {
        if e.k != K::NonReloc {
            self.reserveregs(1)?;
            let reg = self.fsr().freereg - 1;
            self.discharge2reg(e, reg)?;
        }
        Ok(())
    }

    fn exp2reg(&mut self, e: &mut ExpDesc, reg: i32) -> R {
        self.discharge2reg(e, reg)?;
        if e.k == K::Jmp {
            let mut t = e.t;
            self.concat(&mut t, e.info);
            e.t = t;
        }
        if e.has_jumps() {
            let mut p_f = NO_JUMP;
            let mut p_t = NO_JUMP;
            if self.need_value(e.t) || self.need_value(e.f) {
                let fj = if e.k == K::Jmp { NO_JUMP } else { self.jump() };
                p_f = self.code_label(reg, 0, 1);
                p_t = self.code_label(reg, 1, 0);
                self.patchtohere(fj)?;
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

    fn exp2rk(&mut self, e: &mut ExpDesc) -> R<i32> {
        self.exp2val(e)?;
        match e.k {
            K::Num | K::True | K::False | K::Nil => {
                if self.fsr().nk() <= MAXINDEXRK {
                    e.info = match e.k {
                        K::Nil => self.nil_k(),
                        K::Num => self.number_k(e.nval),
                        k => self.bool_k(k == K::True),
                    };
                    e.k = K::Kst;
                    return Ok(rk_as_k(e.info));
                }
            }
            K::Kst if e.info <= MAXINDEXRK => return Ok(rk_as_k(e.info)),
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
                return Ok(self.condjump(op::TEST, get_b(ie), 0, i32::from(cond == 0)));
            }
        }
        self.discharge2anyreg(e)?;
        self.freeexp(e);
        Ok(self.condjump(op::TESTSET, NO_REG, e.info, cond))
    }

    fn goiftrue(&mut self, e: &mut ExpDesc) -> R {
        self.dischargevars(e);
        let pc = match e.k {
            K::Kst | K::Num | K::True => NO_JUMP,
            K::Jmp => {
                self.invertjump(e);
                e.info
            }
            _ => self.jumponcond(e, 0)?,
        };
        let mut f = e.f;
        self.concat(&mut f, pc);
        e.f = f;
        self.patchtohere(e.t)?;
        e.t = NO_JUMP;
        Ok(())
    }

    fn goiffalse(&mut self, e: &mut ExpDesc) -> R {
        self.dischargevars(e);
        let pc = match e.k {
            K::Nil | K::False => NO_JUMP,
            K::Jmp => e.info,
            _ => self.jumponcond(e, 1)?,
        };
        let mut t = e.t;
        self.concat(&mut t, pc);
        e.t = t;
        self.patchtohere(e.f)?;
        e.f = NO_JUMP;
        Ok(())
    }

    fn codenot(&mut self, e: &mut ExpDesc) -> R {
        self.dischargevars(e);
        match e.k {
            K::Nil | K::False => e.k = K::True,
            K::Kst | K::Num | K::True => e.k = K::False,
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

    fn constfolding(o: u32, e1: &mut ExpDesc, e2: &ExpDesc) -> bool {
        if !e1.is_numeral() || !e2.is_numeral() {
            return false;
        }
        let (v1, v2) = (e1.nval, e2.nval);
        let r = match o {
            op::ADD => v1 + v2,
            op::SUB => v1 - v2,
            op::MUL => v1 * v2,
            op::DIV => {
                if v2 == 0.0 {
                    return false;
                }
                v1 / v2
            }
            op::MOD => {
                if v2 == 0.0 {
                    return false;
                }
                v1 - (v1 / v2).floor() * v2
            }
            op::POW => v1.powf(v2),
            op::UNM => -v1,
            _ => return false,
        };
        if r.is_nan() {
            return false;
        }
        e1.nval = r;
        true
    }

    fn codearith(&mut self, o: u32, e1: &mut ExpDesc, e2: &mut ExpDesc) -> R {
        if Self::constfolding(o, e1, e2) {
            return Ok(());
        }
        let o2 = if o != op::UNM && o != op::LEN {
            self.exp2rk(e2)?
        } else {
            0
        };
        let o1 = self.exp2rk(e1)?;
        if o1 > o2 {
            self.freeexp(e1);
            self.freeexp(e2);
        } else {
            self.freeexp(e2);
            self.freeexp(e1);
        }
        e1.info = self.code_abc(o, 0, o1, o2);
        e1.k = K::Relocable;
        Ok(())
    }

    fn codecomp(&mut self, o: u32, mut cond: i32, e1: &mut ExpDesc, e2: &mut ExpDesc) -> R {
        let mut o1 = self.exp2rk(e1)?;
        let mut o2 = self.exp2rk(e2)?;
        self.freeexp(e2);
        self.freeexp(e1);
        if cond == 0 && o != op::EQ {
            std::mem::swap(&mut o1, &mut o2);
            cond = 1;
        }
        e1.info = self.condjump(o, cond, o1, o2);
        e1.k = K::Jmp;
        Ok(())
    }

    fn prefix(&mut self, o: UnOpr, e: &mut ExpDesc) -> R {
        let mut e2 = ExpDesc::new(K::Num, 0);
        match o {
            UnOpr::Minus => {
                if !e.is_numeral() {
                    self.exp2anyreg(e)?;
                }
                self.codearith(op::UNM, e, &mut e2)?;
            }
            UnOpr::Not => self.codenot(e)?,
            UnOpr::Len => {
                self.exp2anyreg(e)?;
                self.codearith(op::LEN, e, &mut e2)?;
            }
            UnOpr::None => {}
        }
        Ok(())
    }

    fn infix(&mut self, o: BinOpr, v: &mut ExpDesc) -> R {
        match o {
            BinOpr::And => self.goiftrue(v)?,
            BinOpr::Or => self.goiffalse(v)?,
            BinOpr::Concat => self.exp2nextreg(v)?,
            BinOpr::Add | BinOpr::Sub | BinOpr::Mul | BinOpr::Div | BinOpr::Mod | BinOpr::Pow => {
                if !v.is_numeral() {
                    self.exp2rk(v)?;
                }
            }
            _ => {
                self.exp2rk(v)?;
            }
        }
        Ok(())
    }

    fn posfix(&mut self, o: BinOpr, e1: &mut ExpDesc, e2: &mut ExpDesc) -> R {
        match o {
            BinOpr::And => {
                self.dischargevars(e2);
                let mut f = e2.f;
                self.concat(&mut f, e1.f);
                e2.f = f;
                *e1 = *e2;
            }
            BinOpr::Or => {
                self.dischargevars(e2);
                let mut t = e2.t;
                self.concat(&mut t, e1.t);
                e2.t = t;
                *e1 = *e2;
            }
            BinOpr::Concat => {
                self.exp2val(e2)?;
                if e2.k == K::Relocable && get_op(*self.getcode_mut(e2)) == op::CONCAT {
                    self.freeexp(e1);
                    set_b(self.getcode_mut(e2), e1.info);
                    e1.k = K::Relocable;
                    e1.info = e2.info;
                } else {
                    self.exp2nextreg(e2)?;
                    self.codearith(op::CONCAT, e1, e2)?;
                }
            }
            BinOpr::Add => self.codearith(op::ADD, e1, e2)?,
            BinOpr::Sub => self.codearith(op::SUB, e1, e2)?,
            BinOpr::Mul => self.codearith(op::MUL, e1, e2)?,
            BinOpr::Div => self.codearith(op::DIV, e1, e2)?,
            BinOpr::Mod => self.codearith(op::MOD, e1, e2)?,
            BinOpr::Pow => self.codearith(op::POW, e1, e2)?,
            BinOpr::Eq => self.codecomp(op::EQ, 1, e1, e2)?,
            BinOpr::Ne => self.codecomp(op::EQ, 0, e1, e2)?,
            BinOpr::Lt => self.codecomp(op::LT, 1, e1, e2)?,
            BinOpr::Le => self.codecomp(op::LE, 1, e1, e2)?,
            BinOpr::Gt => self.codecomp(op::LT, 0, e1, e2)?,
            BinOpr::Ge => self.codecomp(op::LE, 0, e1, e2)?,
            BinOpr::None => {}
        }
        Ok(())
    }

    fn setlist(&mut self, base: i32, nelems: i32, tostore: i32) {
        let c = (nelems - 1) / LFIELDS_PER_FLUSH + 1;
        let b = if tostore == MULTRET { 0 } else { tostore };
        if c <= MAXARG_C {
            self.code_abc(op::SETLIST, base, b, c);
        } else {
            self.code_abc(op::SETLIST, base, b, 0);
            self.code(c as u32);
        }
        self.fs().freereg = base + 1;
    }

    // ----- lparser.c: helpers -----------------------------------------------------------------

    fn error_expected(&self, token: u16) -> CompileError {
        self.lx
            .syntaxerror(&format!("'{}' expected", token_text(token)))
    }

    fn errorlimit(&self, limit: i32, what: &str) -> CompileError {
        let msg = if self.fsr().f.line_defined == 0 {
            format!("main function has more than {limit} {what}")
        } else {
            format!(
                "function at line {} has more than {limit} {what}",
                self.fsr().f.line_defined
            )
        };
        self.lx.lexerror(&msg, 0)
    }

    fn testnext(&mut self, c: u16) -> R<bool> {
        if self.lx.t.token == c {
            self.lx.next()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn check(&self, c: u16) -> R {
        if self.lx.t.token != c {
            return Err(self.error_expected(c));
        }
        Ok(())
    }

    fn checknext(&mut self, c: u16) -> R {
        self.check(c)?;
        self.lx.next()
    }

    fn check_match(&mut self, what: u16, who: u16, wh: u32) -> R {
        if !self.testnext(what)? {
            if wh == self.lx.line {
                return Err(self.error_expected(what));
            }
            return Err(self.lx.syntaxerror(&format!(
                "'{}' expected (to close '{}' at line {wh})",
                token_text(what),
                token_text(who)
            )));
        }
        Ok(())
    }

    fn str_checkname(&mut self) -> R<Vec<u8>> {
        self.check(TK_NAME)?;
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

    fn registerlocalvar(&mut self, name: Vec<u8>) -> R<u16> {
        if self.fsr().f.locals.len() >= i16::MAX as usize {
            return Err(self.lx.syntaxerror("too many local variables"));
        }
        let fs = self.fs();
        fs.f.locals.push(Local {
            name,
            start: 0,
            end: 0,
        });
        Ok((fs.f.locals.len() - 1) as u16)
    }

    fn new_localvar(&mut self, name: Vec<u8>, n: i32) -> R {
        let total = self.fsr().nactvar + n + 1;
        if total > LUAI_MAXVARS {
            return Err(self.errorlimit(LUAI_MAXVARS, "local variables"));
        }
        let idx = self.registerlocalvar(name)?;
        let fs = self.fs();
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
            let idx = fs.actvar[(fs.nactvar - i) as usize] as usize;
            fs.f.locals[idx].start = pc;
        }
    }

    fn removevars(&mut self, tolevel: i32) {
        let fs = self.fs();
        let pc = fs.pc() as u32;
        while fs.nactvar > tolevel {
            fs.nactvar -= 1;
            let idx = fs.actvar[fs.nactvar as usize] as usize;
            fs.f.locals[idx].end = pc;
        }
    }

    fn indexupvalue(&mut self, level: usize, name: &[u8], v: &ExpDesc) -> R<i32> {
        let fs = &self.funcs[level];
        for (i, u) in fs.upvals.iter().enumerate() {
            if u.0 == v.k && u.1 == v.info {
                return Ok(i as i32);
            }
        }
        let n = fs.upvals.len() as i32;
        if n + 1 > LUAI_MAXUPVALUES {
            let line = fs.f.line_defined;
            let msg = if line == 0 {
                format!("main function has more than {LUAI_MAXUPVALUES} upvalues")
            } else {
                format!("function at line {line} has more than {LUAI_MAXUPVALUES} upvalues")
            };
            return Err(self.lx.lexerror(&msg, 0));
        }
        let fs = &mut self.funcs[level];
        fs.f.upvalue_names.push(name.to_vec());
        fs.upvals.push((v.k, v.info));
        Ok(n)
    }

    fn searchvar(fs: &FuncState, n: &[u8]) -> i32 {
        for i in (0..fs.nactvar).rev() {
            if fs.f.locals[fs.actvar[i as usize] as usize].name == n {
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
        if has_multret(e.k) {
            extra += 1;
            if extra < 0 {
                extra = 0;
            }
            self.setreturns(e, extra)?;
            if extra > 1 {
                self.reserveregs(extra - 1)?;
            }
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

    fn enterlevel(&mut self) -> R {
        self.nccalls += 1;
        if self.nccalls > LUAI_MAXCCALLS {
            return Err(self.lx.lexerror("chunk has too many syntax levels", 0));
        }
        Ok(())
    }

    fn leavelevel(&mut self) {
        self.nccalls -= 1;
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

    fn leaveblock(&mut self) -> R {
        let bl = self.fs().blocks.pop().expect("a block is open");
        self.removevars(bl.nactvar);
        if bl.upval {
            self.code_abc(op::CLOSE, bl.nactvar, 0, 0);
        }
        let fs = self.fs();
        fs.freereg = fs.nactvar;
        self.patchtohere(bl.breaklist)
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
            last_line: 0,
            upvalue_count: 0,
            params: 0,
            is_vararg: 0,
            max_stack: 2,
            code: Vec::new(),
            constants: Vec::new(),
            protos: Vec::new(),
            lines: Vec::new(),
            locals: Vec::new(),
            upvalue_names: Vec::new(),
            orig_code: None,
        };
        if self.funcs.is_empty() {
            f.source = self.lx.source.clone();
        }
        self.funcs.push(FuncState {
            f,
            h: HashMap::new(),
            lasttarget: -1,
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
        self.ret(0, 0);
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
        self.checknext(u16::from(b']'))
    }

    fn recfield(&mut self, cc: &mut ConsControl) -> R {
        let reg = self.fsr().freereg;
        let mut key = ExpDesc::new(K::Void, 0);
        let mut val = ExpDesc::new(K::Void, 0);
        if self.lx.t.token == TK_NAME {
            self.checkname(&mut key)?;
        } else {
            self.yindex(&mut key)?;
        }
        cc.nh += 1;
        self.checknext(u16::from(b'='))?;
        let rkkey = self.exp2rk(&mut key)?;
        self.expr(&mut val)?;
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
            self.setlist(cc.t_info, cc.na, cc.tostore);
            cc.tostore = 0;
        }
        Ok(())
    }

    fn lastlistfield(&mut self, cc: &mut ConsControl) -> R {
        if cc.tostore == 0 {
            return Ok(());
        }
        if has_multret(cc.v.k) {
            let mut v = cc.v;
            self.setmultret(&mut v)?;
            cc.v = v;
            self.setlist(cc.t_info, cc.na, MULTRET);
            cc.na -= 1;
        } else {
            if cc.v.k != K::Void {
                let mut v = cc.v;
                self.exp2nextreg(&mut v)?;
                cc.v = v;
            }
            self.setlist(cc.t_info, cc.na, cc.tostore);
        }
        Ok(())
    }

    fn listfield(&mut self, cc: &mut ConsControl) -> R {
        let mut v = ExpDesc::new(K::Void, 0);
        self.expr(&mut v)?;
        cc.v = v;
        cc.na += 1;
        cc.tostore += 1;
        Ok(())
    }

    fn constructor(&mut self, t: &mut ExpDesc) -> R {
        let line = self.lx.line;
        let pc = self.code_abc(op::NEWTABLE, 0, 0, 0);
        *t = ExpDesc::new(K::Relocable, pc);
        self.exp2nextreg(t)?;
        let mut cc = ConsControl {
            v: ExpDesc::new(K::Void, 0),
            t_info: t.info,
            nh: 0,
            na: 0,
            tostore: 0,
        };
        self.checknext(u16::from(b'{'))?;
        loop {
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
        let fs = self.fs();
        set_b(&mut fs.f.code[pc as usize], int2fb(cc.na as u32));
        set_c(&mut fs.f.code[pc as usize], int2fb(cc.nh as u32));
        Ok(())
    }

    fn parlist(&mut self) -> R {
        let mut nparams = 0;
        self.fs().f.is_vararg = 0;
        if self.lx.t.token != u16::from(b')') {
            loop {
                match self.lx.t.token {
                    TK_NAME => {
                        let n = self.str_checkname()?;
                        self.new_localvar(n, nparams)?;
                        nparams += 1;
                    }
                    TK_DOTS => {
                        self.lx.next()?;
                        // compatibility mode of Lua 5.1: `arg` is the default name
                        self.new_localvar(b"arg".to_vec(), nparams)?;
                        nparams += 1;
                        self.fs().f.is_vararg = VARARG_HASARG | VARARG_NEEDSARG;
                        self.fs().f.is_vararg |= VARARG_ISVARARG;
                    }
                    _ => {
                        return Err(self.lx.syntaxerror("<name> or '...' expected"));
                    }
                }
                if self.fsr().f.is_vararg != 0 || !self.testnext(u16::from(b','))? {
                    break;
                }
            }
        }
        self.adjustlocalvars(nparams);
        let fs = self.fs();
        fs.f.params = (fs.nactvar - i32::from(fs.f.is_vararg & VARARG_HASARG)) as u8;
        let n = fs.nactvar;
        self.reserveregs(n)
    }

    fn body(&mut self, e: &mut ExpDesc, needself: bool, line: u32) -> R {
        self.open_func();
        self.fs().f.line_defined = line;
        self.checknext(u16::from(b'('))?;
        if needself {
            self.new_localvar(b"self".to_vec(), 0)?;
            self.adjustlocalvars(1);
        }
        self.parlist()?;
        self.checknext(u16::from(b')'))?;
        self.chunk()?;
        self.fs().f.last_line = self.lx.line;
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
                    self.setmultret(&mut args)?;
                }
                self.check_match(u16::from(b')'), u16::from(b'('), line)?;
            }
            c if c == u16::from(b'{') => self.constructor(&mut args)?,
            TK_STRING => {
                let s = self.lx.t.s.clone();
                self.codestring(&mut args, &s);
                self.lx.next()?;
            }
            _ => return Err(self.lx.syntaxerror("function arguments expected")),
        }
        let base = f.info;
        let nparams = if has_multret(args.k) {
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
                *v = ExpDesc::new(K::Num, 0);
                v.nval = self.lx.t.r;
            }
            TK_STRING => {
                let s = self.lx.t.s.clone();
                self.codestring(v, &s);
            }
            TK_NIL => *v = ExpDesc::new(K::Nil, 0),
            TK_TRUE => *v = ExpDesc::new(K::True, 0),
            TK_FALSE => *v = ExpDesc::new(K::False, 0),
            TK_DOTS => {
                if self.fsr().f.is_vararg == 0 {
                    return Err(self
                        .lx
                        .syntaxerror("cannot use '...' outside a vararg function"));
                }
                self.fs().f.is_vararg &= !VARARG_NEEDSARG;
                *v = ExpDesc::new(K::Vararg, self.code_abc(op::VARARG, 0, 1, 0));
            }
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
            c if c == u16::from(b'#') => UnOpr::Len,
            _ => UnOpr::None,
        }
    }

    fn binopr(t: u16) -> BinOpr {
        match t {
            c if c == u16::from(b'+') => BinOpr::Add,
            c if c == u16::from(b'-') => BinOpr::Sub,
            c if c == u16::from(b'*') => BinOpr::Mul,
            c if c == u16::from(b'/') => BinOpr::Div,
            c if c == u16::from(b'%') => BinOpr::Mod,
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

    fn subexpr(&mut self, v: &mut ExpDesc, limit: u32) -> R<BinOpr> {
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
        self.subexpr(v, 0).map(|_| ())
    }

    fn block_follow(t: u16) -> bool {
        matches!(t, TK_ELSE | TK_ELSEIF | TK_END | TK_UNTIL | TK_EOS)
    }

    fn block(&mut self) -> R {
        self.enterblock(false);
        self.chunk()?;
        self.leaveblock()
    }

    fn check_conflict(&mut self, lhs: &mut [ExpDesc], v: &ExpDesc) {
        let extra = self.fsr().freereg;
        let mut conflict = false;
        for lh in lhs.iter_mut() {
            if lh.k == K::Indexed {
                if lh.info == v.info {
                    conflict = true;
                    lh.info = extra;
                }
                if lh.aux == v.info {
                    conflict = true;
                    lh.aux = extra;
                }
            }
        }
        if conflict {
            let reg = self.fsr().freereg;
            self.code_abc(op::MOVE, reg, v.info, 0);
            if let Err(e) = self.reserveregs(1) {
                self.deferred.get_or_insert(e);
            }
        }
    }

    fn assignment(&mut self, lhs: &mut Vec<ExpDesc>, idx: usize, nvars: i32) -> R {
        let mut e = ExpDesc::new(K::Void, 0);
        if !(K::Local as u8 <= lhs[idx].k as u8 && lhs[idx].k as u8 <= K::Indexed as u8) {
            return Err(self.lx.syntaxerror("syntax error"));
        }
        if self.testnext(u16::from(b','))? {
            let mut nv = ExpDesc::new(K::Void, 0);
            self.primaryexp(&mut nv)?;
            if nv.k == K::Local {
                self.check_conflict(lhs, &nv);
            }
            if nvars > (LUAI_MAXCCALLS - self.nccalls) as i32 {
                return Err(self.errorlimit(
                    (LUAI_MAXCCALLS - self.nccalls) as i32,
                    "variables in assignment",
                ));
            }
            lhs.push(nv);
            self.assignment(lhs, lhs.len() - 1, nvars + 1)?;
        } else {
            self.checknext(u16::from(b'='))?;
            let nexps = self.explist1(&mut e)?;
            if nexps != nvars {
                self.adjust_assign(nvars, nexps, &mut e)?;
                if nexps > nvars {
                    self.fs().freereg -= nexps - nvars;
                }
            } else {
                self.setoneret(&mut e);
                let var = lhs[idx];
                return self.storevar(&var, &mut e);
            }
        }
        let mut e = ExpDesc::new(K::NonReloc, self.fsr().freereg - 1);
        let var = lhs[idx];
        self.storevar(&var, &mut e)
    }

    fn cond(&mut self) -> R<i32> {
        let mut v = ExpDesc::new(K::Void, 0);
        self.expr(&mut v)?;
        if v.k == K::Nil {
            v.k = K::False;
        }
        self.goiftrue(&mut v)?;
        Ok(v.f)
    }

    fn breakstat(&mut self) -> R {
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

    fn whilestat(&mut self, line: u32) -> R {
        self.lx.next()?;
        let whileinit = self.getlabel();
        let condexit = self.cond()?;
        self.enterblock(true);
        self.checknext(TK_DO)?;
        self.block()?;
        let j = self.jump();
        self.patchlist(j, whileinit)?;
        self.check_match(TK_END, TK_WHILE, line)?;
        self.leaveblock()?;
        self.patchtohere(condexit)
    }

    fn repeatstat(&mut self, line: u32) -> R {
        let repeat_init = self.getlabel();
        self.enterblock(true);
        self.enterblock(false);
        self.lx.next()?;
        self.chunk()?;
        self.check_match(TK_UNTIL, TK_REPEAT, line)?;
        let condexit = self.cond()?;
        let upval = self.fsr().blocks.last().is_some_and(|b| b.upval);
        if !upval {
            self.leaveblock()?;
            self.patchlist(condexit, repeat_init)?;
        } else {
            self.breakstat()?;
            self.patchtohere(condexit)?;
            self.leaveblock()?;
            let j = self.jump();
            self.patchlist(j, repeat_init)?;
        }
        self.leaveblock()
    }

    fn exp1(&mut self) -> R {
        let mut e = ExpDesc::new(K::Void, 0);
        self.expr(&mut e)?;
        self.exp2nextreg(&mut e)
    }

    fn forbody(&mut self, base: i32, line: u32, nvars: i32, isnum: bool) -> R {
        self.adjustlocalvars(3);
        self.checknext(TK_DO)?;
        let prep = if isnum {
            self.code_asbx(op::FORPREP, base, NO_JUMP)
        } else {
            self.jump()
        };
        self.enterblock(false);
        self.adjustlocalvars(nvars);
        self.reserveregs(nvars)?;
        self.block()?;
        self.leaveblock()?;
        self.patchtohere(prep)?;
        let endfor = if isnum {
            self.code_asbx(op::FORLOOP, base, NO_JUMP)
        } else {
            self.code_abc(op::TFORLOOP, base, 0, nvars)
        };
        self.fixline(line);
        let target = if isnum { endfor } else { self.jump() };
        self.patchlist(target, prep + 1)
    }

    fn fornum(&mut self, varname: Vec<u8>, line: u32) -> R {
        let base = self.fsr().freereg;
        self.new_localvar(b"(for index)".to_vec(), 0)?;
        self.new_localvar(b"(for limit)".to_vec(), 1)?;
        self.new_localvar(b"(for step)".to_vec(), 2)?;
        self.new_localvar(varname, 3)?;
        self.checknext(u16::from(b'='))?;
        self.exp1()?;
        self.checknext(u16::from(b','))?;
        self.exp1()?;
        if self.testnext(u16::from(b','))? {
            self.exp1()?;
        } else {
            let reg = self.fsr().freereg;
            let k = self.number_k(1.0);
            self.code_abx(op::LOADK, reg, k as u32);
            self.reserveregs(1)?;
        }
        self.forbody(base, line, 1, true)
    }

    fn forlist(&mut self, indexname: Vec<u8>) -> R {
        let mut e = ExpDesc::new(K::Void, 0);
        let mut nvars = 0;
        let base = self.fsr().freereg;
        for n in [&b"(for generator)"[..], b"(for state)", b"(for control)"] {
            self.new_localvar(n.to_vec(), nvars)?;
            nvars += 1;
        }
        self.new_localvar(indexname, nvars)?;
        nvars += 1;
        while self.testnext(u16::from(b','))? {
            let n = self.str_checkname()?;
            self.new_localvar(n, nvars)?;
            nvars += 1;
        }
        self.checknext(TK_IN)?;
        let line = self.lx.line;
        let n = self.explist1(&mut e)?;
        self.adjust_assign(3, n, &mut e)?;
        self.checkstack(3)?;
        self.forbody(base, line, nvars - 3, false)
    }

    fn forstat(&mut self, line: u32) -> R {
        self.enterblock(true);
        self.lx.next()?;
        let varname = self.str_checkname()?;
        match self.lx.t.token {
            c if c == u16::from(b'=') => self.fornum(varname, line)?,
            c if c == u16::from(b',') || c == TK_IN => self.forlist(varname)?,
            _ => return Err(self.lx.syntaxerror("'=' or 'in' expected")),
        }
        self.check_match(TK_END, TK_FOR, line)?;
        self.leaveblock()
    }

    fn test_then_block(&mut self) -> R<i32> {
        self.lx.next()?;
        let condexit = self.cond()?;
        self.checknext(TK_THEN)?;
        self.block()?;
        Ok(condexit)
    }

    fn ifstat(&mut self, line: u32) -> R {
        let mut escapelist = NO_JUMP;
        let mut flist = self.test_then_block()?;
        while self.lx.t.token == TK_ELSEIF {
            let j = self.jump();
            self.concat(&mut escapelist, j);
            self.patchtohere(flist)?;
            flist = self.test_then_block()?;
        }
        if self.lx.t.token == TK_ELSE {
            let j = self.jump();
            self.concat(&mut escapelist, j);
            self.patchtohere(flist)?;
            self.lx.next()?;
            self.block()?;
        } else {
            self.concat(&mut escapelist, flist);
        }
        self.patchtohere(escapelist)?;
        self.check_match(TK_END, TK_IF, line)
    }

    fn localfunc(&mut self) -> R {
        let n = self.str_checkname()?;
        self.new_localvar(n, 0)?;
        let v = ExpDesc::new(K::Local, self.fsr().freereg);
        self.reserveregs(1)?;
        self.adjustlocalvars(1);
        let mut b = ExpDesc::new(K::Void, 0);
        let line = self.lx.line;
        self.body(&mut b, false, line)?;
        self.storevar(&v, &mut b)?;
        let fs = self.fs();
        let idx = fs.actvar[(fs.nactvar - 1) as usize] as usize;
        fs.f.locals[idx].start = fs.pc() as u32;
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
            set_c(self.getcode_mut(&v), 1);
        } else {
            let mut lhs = vec![v];
            self.assignment(&mut lhs, 0, 1)?;
        }
        Ok(())
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
            if has_multret(e.k) {
                self.setmultret(&mut e)?;
                if e.k == K::Call && n == 1 {
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
        self.ret(first, nret);
        Ok(())
    }

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
                self.lx.next()?;
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

struct ConsControl {
    v: ExpDesc,
    /// Register of the table being built.
    t_info: i32,
    nh: i32,
    na: i32,
    tostore: i32,
}

/// Compiles Lua 5.1 source. `source` is the chunk name stored in the bytecode (`luac` uses
/// `@file`); nested functions carry no name of their own.
pub fn compile(text: &[u8], source: &[u8]) -> Result<Chunk, CompileError> {
    // a leading `#` line (shebang) is skipped like `luaL_loadfile` does; the newline stays
    let text = match text.first() {
        Some(b'#') => &text[text.iter().position(|&b| b == b'\n').unwrap_or(text.len())..],
        _ => text,
    };
    let mut p = Parser {
        lx: Lexer::new(text, source),
        funcs: Vec::new(),
        nccalls: 0,
        deferred: None,
    };
    p.open_func();
    p.fs().f.is_vararg = VARARG_ISVARARG;
    p.lx.next()?;
    p.chunk()?;
    p.check(TK_EOS)?;
    let main = p.close_func();
    if let Some(e) = p.deferred {
        return Err(e);
    }
    Ok(Chunk { main: main.f })
}

/// How the non-ASCII characters of a text become bytes in the compiled strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    /// UTF-8 when the first line says `strings: UTF-8` (as the decompiler writes it for files whose
    /// strings are UTF-8), else CP949.
    Auto,
    Utf8,
    /// The client's encoding: every run of non-ASCII characters is written as CP949, as Latin-1
    /// when it is not Korean, and as UTF-8 when it fits neither.
    Cp949,
}

/// Compiles a text typed or edited as Unicode. `cp949` turns a run of characters into CP949 bytes
/// (`None` when a character is not Korean); see [`TextEncoding`].
pub fn compile_text(
    text: &str,
    source: &[u8],
    encoding: TextEncoding,
    cp949: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Chunk, CompileError> {
    compile(&text_bytes(text, encoding, cp949), source)
}

/// The source bytes of a text typed as Unicode (shared with the Lua 5.0 compiler).
pub(crate) fn text_bytes<'a>(
    text: &'a str,
    encoding: TextEncoding,
    cp949: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> std::borrow::Cow<'a, [u8]> {
    let utf8 = match encoding {
        TextEncoding::Utf8 => true,
        TextEncoding::Cp949 => false,
        TextEncoding::Auto => text
            .lines()
            .next()
            .is_some_and(|l| l.starts_with("--") && l.contains("strings: UTF-8")),
    };
    if utf8 || text.is_ascii() {
        return std::borrow::Cow::Borrowed(text.as_bytes());
    }
    let mut bytes = Vec::with_capacity(text.len());
    let mut run = String::new();
    let flush = |run: &mut String, bytes: &mut Vec<u8>| {
        if run.is_empty() {
            return;
        }
        if let Some(b) = cp949(run) {
            bytes.extend(b);
        } else if run.chars().all(|c| (c as u32) < 0x100) {
            bytes.extend(run.chars().map(|c| c as u32 as u8));
        } else {
            bytes.extend(run.as_bytes());
        }
        run.clear();
    };
    for c in text.chars() {
        if c.is_ascii() {
            flush(&mut run, &mut bytes);
            bytes.push(c as u8);
        } else {
            run.push(c);
        }
    }
    flush(&mut run, &mut bytes);
    std::borrow::Cow::Owned(bytes)
}

/// Removes the debug information (`luac -s`): line numbers, local and upvalue names, source name.
pub fn strip(chunk: &mut Chunk) {
    fn go(f: &mut Function) {
        f.source.clear();
        f.lines.clear();
        f.locals.clear();
        f.upvalue_names.clear();
        f.line_defined = 0;
        f.last_line = 0;
        f.protos.iter_mut().for_each(go);
    }
    go(&mut chunk.main);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::luac;

    fn code(src: &str) -> Vec<u32> {
        compile(src.as_bytes(), b"=t").unwrap().main.code
    }

    #[test]
    fn compiles_what_luac_makes_for_small_chunks() {
        // x = 1  ->  LOADK 0 0; SETGLOBAL 0 1; RETURN 0 1  (checked against luac 5.1.5)
        let c = compile(b"x = 1", b"@t.lua").unwrap();
        assert_eq!(c.main.code.len(), 3);
        assert_eq!(get_op(c.main.code[0]), op::LOADK);
        assert_eq!(get_op(c.main.code[1]), op::SETGLOBAL);
        assert_eq!(get_op(c.main.code[2]), op::RETURN);
        assert_eq!(
            c.main.constants,
            [Constant::Str(b"x".to_vec()), Constant::Number(1.0)]
        );
        assert_eq!(c.main.source, b"@t.lua");
        assert_eq!((c.main.is_vararg, c.main.max_stack), (2, 2));
    }

    #[test]
    fn folds_constants_and_shares_equal_ones() {
        let c = compile(b"a = 2 * 3 + 1; b = 7; c = 7", b"=t").unwrap().main;
        let numbers: Vec<_> = c
            .constants
            .iter()
            .filter(|k| matches!(k, Constant::Number(_)))
            .collect();
        assert_eq!(numbers, [&Constant::Number(7.0)]);
    }

    #[test]
    fn control_flow_functions_and_upvalues() {
        let c = compile(
            b"local n = 0\nfunction f(a, ...)\n for i = 1, a do n = n + i end\n \
              while a > 0 do a = a - 1 if a == 3 then break end end\n \
              repeat n = n - 1 until n < 0\n for k, v in pairs({1, 2, x = 3}) do print(k, v) end\n \
              return n and a or ...\nend",
            b"=t",
        )
        .unwrap();
        let f = &c.main.protos[0];
        assert_eq!((f.params, f.upvalue_count), (1, 1));
        assert!(f.is_vararg & VARARG_ISVARARG != 0);
        assert_eq!(f.upvalue_names, [b"n".to_vec()]);
        assert!(luac::validate(&c).is_ok());
    }

    #[test]
    fn strings_numbers_and_comments() {
        let c = compile(
            b"--[==[ long\ncomment ]==]\ns = 'a\\65\\n\\\"b' .. [[x\ny]] .. \"\\xff\"\n-- rest\nn = 0x10 + 1e2 + .5",
            b"=t",
        )
        .unwrap()
        .main;
        assert!(c.constants.contains(&Constant::Str(b"aA\n\"b".to_vec())));
        assert!(c.constants.contains(&Constant::Str(b"x\ny".to_vec())));
        assert!(c.constants.contains(&Constant::Number(116.5)));
    }

    #[test]
    fn errors_carry_name_line_and_token() {
        let e = compile(b"x = 1\ny = = 2", b"@a.lua").unwrap_err();
        assert_eq!(e.line, 2);
        assert_eq!(e.message, "a.lua:2: unexpected symbol near '='");
        for (src, want) in [
            ("x = ", "unexpected symbol near '<eof>'"),
            ("if x then", "'end' expected near '<eof>'"),
            ("for i = 1 do end", "',' expected near 'do'"),
            ("local t = {1", "'}' expected near '<eof>'"),
            ("break", "no loop to break near '<eof>'"),
            ("x = 'abc", "unfinished string near '<eof>'"),
            ("x = 3x", "malformed number near '3x'"),
            (
                "function f() return ... end",
                "cannot use '...' outside a vararg function near '...'",
            ),
            (
                "f()\n(g)()",
                "ambiguous syntax (function call x new statement) near '('",
            ),
        ] {
            let e = compile(src.as_bytes(), b"=t").unwrap_err();
            assert!(e.message.ends_with(want), "{src}: {}", e.message);
        }
    }

    #[test]
    fn deep_nesting_is_an_error_not_a_stack_overflow() {
        let src = format!("x = {}1{}", "(".repeat(500), ")".repeat(500));
        assert!(compile(src.as_bytes(), b"=t").is_err());
        let src = "do ".repeat(300) + &"end ".repeat(300);
        assert!(compile(src.as_bytes(), b"=t").is_err());
    }

    #[test]
    fn jumps_are_patched_for_and_or_chains() {
        let c = code("local a, b = ...; x = a and b or 1; if not a then x = 2 end");
        assert!(
            c.iter()
                .any(|&i| get_op(i) == op::TESTSET || get_op(i) == op::TEST)
        );
        assert!(
            c.iter()
                .all(|&i| get_op(i) != op::JMP || get_sbx(i) != NO_JUMP)
        );
    }
}
