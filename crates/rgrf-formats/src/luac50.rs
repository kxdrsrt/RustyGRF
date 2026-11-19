//! Lua 5.0 bytecode (62 `.lub` of the kRO `data.grf` are in this older format): reading the chunk,
//! checking its structure, a `luac -l` style listing and the translation to the 5.1 form that the
//! decompiler works on (`to_lua51`).
//!
//! Layout (checked against all 62 real files, each read to its last byte): header `ESC Lua`, `0x50`,
//! endianness, sizes of int / size_t / instruction, bit widths of OP / A / B / C (6 / 8 / 9 / 9),
//! size of a number and a test number; a function is: source, line defined, upvalues, parameters,
//! vararg, stack size, then line info, locals, upvalue names, constants, nested functions, code.
//! Instruction: OP in bits 0-5, C in 6-14, B in 15-23, A in 24-31 (A on top, unlike 5.1).

use crate::luac::{Constant, Local, LuacError};

/// The test number of the header, 31415926.535897933 as a double.
const TEST_NUMBER_BITS: u64 = 0x417d_f5e7_6893_09b6;

pub const OPS: [&str; 35] = [
    "MOVE",
    "LOADK",
    "LOADBOOL",
    "LOADNIL",
    "GETUPVAL",
    "GETGLOBAL",
    "GETTABLE",
    "SETGLOBAL",
    "SETUPVAL",
    "SETTABLE",
    "NEWTABLE",
    "SELF",
    "ADD",
    "SUB",
    "MUL",
    "DIV",
    "POW",
    "UNM",
    "NOT",
    "CONCAT",
    "JMP",
    "EQ",
    "LT",
    "LE",
    "TEST",
    "CALL",
    "TAILCALL",
    "RETURN",
    "FORLOOP",
    "TFORLOOP",
    "TFORPREP",
    "SETLIST",
    "SETLISTO",
    "CLOSE",
    "CLOSURE",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub source: Vec<u8>,
    pub line_defined: u32,
    pub upvalue_count: u8,
    pub params: u8,
    pub is_vararg: u8,
    pub max_stack: u8,
    pub lines: Vec<u32>,
    pub locals: Vec<Local>,
    pub upvalue_names: Vec<Vec<u8>>,
    pub constants: Vec<Constant>,
    pub protos: Vec<Function>,
    pub code: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub main: Function,
}

/// A decoded instruction: opcode, A, B, C (and Bx / sBx for the wide forms).
#[derive(Debug, Clone, Copy)]
pub struct Instr {
    pub op: u8,
    pub a: u32,
    pub b: u32,
    pub c: u32,
}

impl Instr {
    pub fn decode(raw: u32) -> Self {
        Self {
            op: (raw & 0x3f) as u8,
            c: (raw >> 6) & 0x1ff,
            b: (raw >> 15) & 0x1ff,
            a: raw >> 24,
        }
    }
    pub fn bx(&self) -> u32 {
        (self.b << 9) | self.c
    }
    pub fn sbx(&self) -> i64 {
        i64::from(self.bx()) - 131_071
    }
    pub fn name(&self) -> &'static str {
        OPS.get(usize::from(self.op)).copied().unwrap_or("?")
    }
}

struct Reader<'a> {
    d: &'a [u8],
    at: usize,
    /// Size of `size_t` and of an instruction: 4 (the client) or 8 (a 64-bit Lua).
    word: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], LuacError> {
        let end = self.at.checked_add(n).ok_or(LuacError::Truncated)?;
        let s = self.d.get(self.at..end).ok_or(LuacError::Truncated)?;
        self.at = end;
        Ok(s)
    }
    fn byte(&mut self) -> Result<u8, LuacError> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, LuacError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn count(&mut self, unit: usize) -> Result<usize, LuacError> {
        let n = self.u32()? as usize;
        if n.saturating_mul(unit.max(1)) > self.d.len() - self.at {
            return Err(LuacError::Truncated);
        }
        Ok(n)
    }
    fn size(&mut self) -> Result<usize, LuacError> {
        let b = self.take(self.word)?;
        let mut v = [0u8; 8];
        v[..b.len()].copy_from_slice(b);
        usize::try_from(u64::from_le_bytes(v)).map_err(|_| LuacError::Truncated)
    }
    fn instruction(&mut self) -> Result<u32, LuacError> {
        let b = self.take(self.word)?;
        if b[4.min(b.len())..].iter().any(|&x| x != 0) {
            return Err(LuacError::Unsupported(
                "instruction wider than 32 bits".into(),
            ));
        }
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn string(&mut self) -> Result<Vec<u8>, LuacError> {
        let len = self.size()?;
        if len == 0 {
            return Ok(Vec::new());
        }
        let s = self.take(len)?;
        Ok(s[..len - 1].to_vec())
    }
}

fn read_function(r: &mut Reader<'_>, depth: usize) -> Result<Function, LuacError> {
    if depth > 200 {
        return Err(LuacError::Truncated);
    }
    let source = r.string()?;
    let line_defined = r.u32()?;
    let upvalue_count = r.byte()?;
    let params = r.byte()?;
    let is_vararg = r.byte()?;
    let max_stack = r.byte()?;
    let n = r.count(4)?;
    let mut lines = Vec::with_capacity(n);
    for _ in 0..n {
        lines.push(r.u32()?);
    }
    let n = r.count(4)?;
    let mut locals = Vec::with_capacity(n);
    for _ in 0..n {
        locals.push(Local {
            name: r.string()?,
            start: r.u32()?,
            end: r.u32()?,
        });
    }
    let n = r.count(4)?;
    let mut upvalue_names = Vec::with_capacity(n);
    for _ in 0..n {
        upvalue_names.push(r.string()?);
    }
    let n = r.count(1)?;
    let mut constants = Vec::with_capacity(n);
    for _ in 0..n {
        constants.push(match r.byte()? {
            0 => Constant::Nil,
            3 => {
                let b = r.take(8)?;
                Constant::Number(f64::from_le_bytes(
                    b.try_into().map_err(|_| LuacError::Truncated)?,
                ))
            }
            4 => Constant::Str(r.string()?),
            t => return Err(LuacError::Unsupported(format!("constant type {t}"))),
        });
    }
    let n = r.count(1)?;
    let mut protos = Vec::with_capacity(n);
    for _ in 0..n {
        protos.push(read_function(r, depth + 1)?);
    }
    let n = r.count(r.word)?;
    let mut code = Vec::with_capacity(n);
    for _ in 0..n {
        code.push(r.instruction()?);
    }
    Ok(Function {
        source,
        line_defined,
        upvalue_count,
        params,
        is_vararg,
        max_stack,
        lines,
        locals,
        upvalue_names,
        constants,
        protos,
        code,
    })
}

/// Reads a Lua 5.0 chunk (the client's 32-bit layout, or the 64-bit one of [`dump`]); the whole
/// file must be used (trailing bytes are an error).
pub fn parse(bytes: &[u8]) -> Result<Chunk, LuacError> {
    if bytes.len() < 4 || &bytes[..4] != b"\x1bLua" {
        return Err(LuacError::NotBytecode);
    }
    if bytes.get(4) != Some(&0x50) {
        return Err(LuacError::Unsupported(format!(
            "Lua version {:#x}",
            bytes.get(4).copied().unwrap_or(0)
        )));
    }
    // endianness 1, int 4, size_t and instruction 4 (or 8), OP/A/B/C 6/8/9/9, number 8, then the
    // test number
    let head = bytes.get(5..14).ok_or(LuacError::Truncated)?;
    let word = usize::from(head[2]);
    if head[3] != head[2]
        || !matches!(word, 4 | 8)
        || head[..2] != [1, 4]
        || head[4..] != [6, 8, 9, 9, 8]
    {
        return Err(LuacError::Unsupported(
            "sizes other than little-endian, int 4, size_t/instruction 4 or 8, OP/A/B/C 6/8/9/9, double"
                .into(),
        ));
    }
    let test = bytes.get(14..22).ok_or(LuacError::Truncated)?;
    if u64::from_le_bytes(test.try_into().map_err(|_| LuacError::Truncated)?) != TEST_NUMBER_BITS {
        return Err(LuacError::Unsupported(
            "number format is not a double".into(),
        ));
    }
    let mut r = Reader {
        d: bytes,
        at: 22,
        word,
    };
    let main = read_function(&mut r, 0)?;
    if r.at != bytes.len() {
        return Err(LuacError::Unsupported(format!(
            "{} bytes after the chunk",
            bytes.len() - r.at
        )));
    }
    Ok(Chunk { main })
}

/// Writes a 5.0 chunk in the layout of a 64-bit Lua 5.0 (int 4, size_t 8, instruction 8), so that
/// such a Lua can load the client's 32-bit files (used by the verification tool `tools/lua-verify`).
pub fn dump(chunk: &Chunk) -> Vec<u8> {
    dump_layout(chunk, 8)
}

/// Writes a 5.0 chunk in the client's 32-bit layout (int 4, size_t 4, instruction 4): the bytes a
/// 32-bit `luac` 5.0 makes, and the format of the `.lub` files in the archives.
pub fn dump32(chunk: &Chunk) -> Vec<u8> {
    dump_layout(chunk, 4)
}

/// `word` is the size of `size_t` and of an instruction (4 or 8).
fn dump_layout(chunk: &Chunk, word: u8) -> Vec<u8> {
    fn int(out: &mut Vec<u8>, v: usize) {
        out.extend((v as u32).to_le_bytes());
    }
    fn size(out: &mut Vec<u8>, v: usize, word: u8) {
        out.extend(&(v as u64).to_le_bytes()[..usize::from(word)]);
    }
    // Size 0 would load as a NULL string; only the source may be NULL (it then takes the parent's).
    fn string(out: &mut Vec<u8>, s: &[u8], word: u8) {
        size(out, s.len() + 1, word);
        out.extend(s);
        out.push(0);
    }
    fn function(out: &mut Vec<u8>, f: &Function, word: u8) {
        if f.source.is_empty() {
            size(out, 0, word);
        } else {
            string(out, &f.source, word);
        }
        int(out, f.line_defined as usize);
        out.extend([f.upvalue_count, f.params, f.is_vararg, f.max_stack]);
        int(out, f.lines.len());
        for l in &f.lines {
            int(out, *l as usize);
        }
        int(out, f.locals.len());
        for l in &f.locals {
            string(out, &l.name, word);
            int(out, l.start as usize);
            int(out, l.end as usize);
        }
        int(out, f.upvalue_names.len());
        for n in &f.upvalue_names {
            string(out, n, word);
        }
        int(out, f.constants.len());
        for k in &f.constants {
            match k {
                Constant::Nil => out.push(0),
                // 5.0 has no boolean constants; the reader never produces one
                Constant::Bool(_) => out.push(0),
                Constant::Number(v) => {
                    out.push(3);
                    out.extend(v.to_le_bytes());
                }
                Constant::Str(s) => {
                    out.push(4);
                    string(out, s, word);
                }
            }
        }
        int(out, f.protos.len());
        for p in &f.protos {
            function(out, p, word);
        }
        int(out, f.code.len());
        for c in &f.code {
            out.extend(&u64::from(*c).to_le_bytes()[..usize::from(word)]);
        }
    }
    // ESC Lua, 5.0, little-endian, int 4, size_t and instruction `word`, OP/A/B/C 6/8/9/9, number 8
    let mut out = b"\x1bLua\x50\x01\x04".to_vec();
    out.extend([word, word, 0x06, 0x08, 0x09, 0x09, 0x08]);
    out.extend(TEST_NUMBER_BITS.to_le_bytes());
    function(&mut out, &chunk.main, word);
    out
}

/// Structural check of every function: known opcodes, constant and function indices in range,
/// jumps inside the code, the code ends in RETURN. Returns the first problem found.
pub fn validate(chunk: &Chunk) -> Result<(), String> {
    fn go(f: &Function, path: &str) -> Result<(), String> {
        let n = f.code.len() as i64;
        if f.code.is_empty() {
            return Err(format!("{path}: no code"));
        }
        for (pc, &raw) in f.code.iter().enumerate() {
            let i = Instr::decode(raw);
            let bad = |what: &str| format!("{path} at {pc}: {what}");
            if usize::from(i.op) >= OPS.len() {
                return Err(bad(&format!("unknown opcode {}", i.op)));
            }
            match i.name() {
                "LOADK" if i.bx() as usize >= f.constants.len() => {
                    return Err(bad("constant out of range"));
                }
                "GETGLOBAL" | "SETGLOBAL"
                    if !matches!(f.constants.get(i.bx() as usize), Some(Constant::Str(_))) =>
                {
                    return Err(bad("global name is not a string constant"));
                }
                "CLOSURE" if i.bx() as usize >= f.protos.len() => {
                    return Err(bad("function index out of range"));
                }
                "SETTABLE" | "ADD" | "SUB" | "MUL" | "DIV" | "POW" | "EQ" | "LT" | "LE"
                | "GETTABLE" | "SELF" => {
                    let operands: &[u32] = if matches!(i.name(), "GETTABLE" | "SELF") {
                        &[i.c]
                    } else {
                        &[i.b, i.c]
                    };
                    if operands
                        .iter()
                        .any(|&x| x >= RK_BASE && ((x - RK_BASE) as usize) >= f.constants.len())
                    {
                        return Err(bad("constant operand out of range"));
                    }
                }
                "JMP" | "FORLOOP" | "TFORPREP" => {
                    let t = pc as i64 + 1 + i.sbx();
                    if t < 0 || t > n {
                        return Err(bad("jump out of range"));
                    }
                }
                _ => {}
            }
        }
        if Instr::decode(*f.code.last().unwrap_or(&0)).name() != "RETURN" {
            return Err(format!("{path}: does not end in RETURN"));
        }
        for (k, p) in f.protos.iter().enumerate() {
            go(p, &format!("{path}/{k}"))?;
        }
        Ok(())
    }
    go(&chunk.main, "main")
}

/// Opcode number of a 5.1 instruction by name (63 = unknown, shown as unsupported by the decompiler).
fn op51(name: &str) -> u32 {
    crate::luac::OPS
        .iter()
        .position(|n| *n == name)
        .map_or(63, |i| i as u32)
}

fn enc51(op: u32, a: u32, b: u32, c: u32) -> u32 {
    op | (a << 6) | (c << 14) | (b << 23)
}

/// In 5.0 an operand is a constant from 250 on (5.1: from 256, and at most 256 of them). The
/// operands are left as they are; the decompiler asks `luadec::rk_base` for the threshold.
pub const RK_BASE: u32 = 250;

/// Translates one 5.0 instruction to the 5.1 encoding. What 5.1 cannot say the same way (numeric
/// `for`, `SETLISTO`, `TEST` that also copies a register) becomes opcode 63: the decompiler then
/// leaves that function as a listing.
fn convert(raw: u32) -> u32 {
    let i = Instr::decode(raw);
    let name = i.name();
    let (a, b, c) = (i.a, i.b, i.c);
    match name {
        "LOADK" | "GETGLOBAL" | "SETGLOBAL" | "CLOSURE" => {
            // 5.0 and 5.1 both have Bx = B * 512 + C
            op51(name) | (a << 6) | (i.bx() << 14)
        }
        "JMP" => op51("JMP") | (i.bx() << 14),
        // generic `for`: the jump to the loop test
        "TFORPREP" => op51("JMP") | (i.bx() << 14),
        "TFORLOOP" => enc51(op51("TFORLOOP"), a, 0, c),
        "SETTABLE" | "ADD" | "SUB" | "MUL" | "DIV" | "POW" | "EQ" | "LT" | "LE" => {
            enc51(op51(name), a, b, c)
        }
        "GETTABLE" | "SELF" => enc51(op51(name), a, b, c),
        // `if R(B) <=> C then R(A) := R(B)`: a plain TEST when A is B, else it needs TESTSET
        "TEST" if a == b => enc51(op51("TEST"), a, 0, c),
        "TEST" => enc51(op51("TESTSET"), a, b, c),
        "SETLIST" => {
            // R(A)[Bx - Bx % 32 + i] := R(A + i), 1 <= i <= Bx % 32 + 1
            let n = (i.bx() & 31) + 1;
            enc51(op51("SETLIST"), a, n, (i.bx() >> 5) + 1)
        }
        // open-ended constructor `{ 1, 2, f() }`: 5.1 says it with B = 0
        "SETLISTO" => enc51(op51("SETLIST"), a, 0, (i.bx() >> 5) + 1),
        // belongs to a numeric `for`; `numeric_for` rewrites the whole loop head
        "FORLOOP" => enc51(63, a, b, c),
        _ => enc51(op51(name), a, b, c),
    }
}

/// Translates the code of a function. A 5.0 numeric `for` is `SUB R(A) R(A) R(A+2)`, `JMP` to the
/// `FORLOOP`, the body, `FORLOOP A` back to the body; 5.1 has `FORPREP` for the first two. The
/// instruction count must stay the same, so the `SUB` becomes `FORPREP` (jumping to the
/// `FORLOOP`), the `JMP` becomes a `CLOSE` the decompiler ignores, and the `FORLOOP` jumps back to
/// the instruction after the `FORPREP`, as in 5.1.
fn numeric_for(code: &[u32]) -> Vec<u32> {
    let mut out: Vec<u32> = code.iter().map(|&r| convert(r)).collect();
    for p in 0..code.len().saturating_sub(1) {
        let (sub, jmp) = (Instr::decode(code[p]), Instr::decode(code[p + 1]));
        if sub.name() != "SUB" || jmp.name() != "JMP" || sub.b != sub.a || sub.c != sub.a + 2 {
            continue;
        }
        let Ok(f) = usize::try_from(p as i64 + 2 + jmp.sbx()) else {
            continue;
        };
        let Some(&raw) = code.get(f) else { continue };
        let tail = Instr::decode(raw);
        if tail.name() != "FORLOOP" || tail.a != sub.a || f <= p + 1 {
            continue;
        }
        let sbx51 = |from: usize, to: usize| ((to as i64 - from as i64 - 1) + 131071) as u32;
        out[p] = op51("FORPREP") | (sub.a << 6) | (sbx51(p, f) << 14);
        out[p + 1] = enc51(op51("CLOSE"), sub.a, 0, 0);
        out[f] = op51("FORLOOP") | (sub.a << 6) | (sbx51(f, p + 1) << 14);
    }
    out
}

/// The chunk in the 5.1 representation the decompiler works on (`orig_code` keeps the 5.0 code).
pub fn to_lua51(chunk: &Chunk) -> crate::luac::Chunk {
    fn go(f: &Function) -> crate::luac::Function {
        crate::luac::Function {
            source: f.source.clone(),
            line_defined: f.line_defined,
            last_line: 0,
            upvalue_count: f.upvalue_count,
            params: f.params,
            is_vararg: f.is_vararg,
            max_stack: f.max_stack,
            code: numeric_for(&f.code),
            constants: f.constants.clone(),
            protos: f.protos.iter().map(go).collect(),
            lines: f.lines.clone(),
            locals: f.locals.clone(),
            upvalue_names: f.upvalue_names.clone(),
            orig_code: Some(f.code.clone()),
        }
    }
    crate::luac::Chunk {
        main: go(&chunk.main),
    }
}

/// The whole translated chunk back in its 5.0 form (for the listing).
pub fn chunk_from_lua51(c: &crate::luac::Chunk) -> Option<Chunk> {
    fn go(f: &crate::luac::Function) -> Option<Function> {
        let mut out = from_lua51(f)?;
        out.protos = f.protos.iter().map(go).collect::<Option<_>>()?;
        Some(out)
    }
    Some(Chunk { main: go(&c.main)? })
}

/// The 5.0 form of a translated function, for listings.
pub fn from_lua51(f: &crate::luac::Function) -> Option<Function> {
    Some(Function {
        source: f.source.clone(),
        line_defined: f.line_defined,
        upvalue_count: f.upvalue_count,
        params: f.params,
        is_vararg: f.is_vararg,
        max_stack: f.max_stack,
        lines: f.lines.clone(),
        locals: f.locals.clone(),
        upvalue_names: f.upvalue_names.clone(),
        constants: f.constants.clone(),
        protos: Vec::new(),
        code: f.orig_code.clone()?,
    })
}

fn quote(s: &[u8], names: &dyn Fn(&[u8]) -> String) -> String {
    let t: String = names(s).chars().take(60).collect();
    format!("{t:?}")
}

/// A listing in the style of `luac -l`.
pub fn disassemble(chunk: &Chunk, names: &dyn Fn(&[u8]) -> String) -> String {
    let mut out = String::new();
    list(&chunk.main, "main", names, &mut out);
    out
}

/// The listing of one function (without its nested functions).
pub fn list_function(f: &Function, names: &dyn Fn(&[u8]) -> String) -> String {
    let mut out = String::new();
    list(f, "main", names, &mut out);
    out
}

fn list(f: &Function, label: &str, names: &dyn Fn(&[u8]) -> String, out: &mut String) {
    out.push_str(&format!(
        "\n{label} <{}:{}> ({} instructions, {} params, {} slots, {} upvalues, {} constants, {} functions)\n",
        names(&f.source),
        f.line_defined,
        f.code.len(),
        f.params,
        f.max_stack,
        f.upvalue_count,
        f.constants.len(),
        f.protos.len()
    ));
    let k = |idx: u32| match f.constants.get(idx as usize) {
        Some(Constant::Str(s)) => quote(s, names),
        Some(Constant::Number(v)) => format!("{v}"),
        Some(Constant::Nil) => "nil".into(),
        Some(Constant::Bool(b)) => b.to_string(),
        None => "?".into(),
    };
    for (pc, &raw) in f.code.iter().enumerate() {
        let i = Instr::decode(raw);
        let line = f.lines.get(pc).copied().unwrap_or(0);
        let args = match i.name() {
            "LOADK" => format!("R{} K{} ; {}", i.a, i.bx(), k(i.bx())),
            "GETGLOBAL" | "SETGLOBAL" => format!("R{} K{} ; {}", i.a, i.bx(), k(i.bx())),
            "JMP" | "FORLOOP" | "TFORPREP" => {
                format!("{} -> {}", i.a, pc as i64 + 1 + i.sbx())
            }
            "CLOSURE" => format!("R{} function {}", i.a, i.bx()),
            _ => format!("{} {} {}", i.a, i.b, i.c),
        };
        out.push_str(&format!("{pc:>6} [{line:>4}] {:<9} {args}\n", i.name()));
    }
    for (n, p) in f.protos.iter().enumerate() {
        list(p, &format!("function {n}"), names, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }

    /// An instruction in the 5.0 layout: OP, A, C, B.
    fn op(op: u32, a: u32, c: u32, b: u32) -> u32 {
        op | (c << 6) | (b << 15) | (a << 24)
    }

    /// `x = "hi"` as Lua 5.0 bytecode.
    fn sample() -> Vec<u8> {
        let mut b = b"\x1bLua\x50\x01\x04\x04\x04\x06\x08\x09\x09\x08".to_vec();
        b.extend(TEST_NUMBER_BITS.to_le_bytes());
        b.extend(le(3)); // source "@t"
        b.extend(b"@t\0");
        b.extend(le(0)); // line defined
        b.extend([0, 0, 0, 2]); // upvalues, params, vararg, stack
        b.extend(le(2)); // line info
        b.extend(le(1));
        b.extend(le(1));
        b.extend(le(0)); // locals
        b.extend(le(0)); // upvalue names
        b.extend(le(2)); // constants
        b.push(4);
        b.extend(le(3));
        b.extend(b"hi\0");
        b.push(4);
        b.extend(le(2));
        b.extend(b"x\0");
        b.extend(le(0)); // functions
        b.extend(le(3)); // code: LOADK R0 K0; SETGLOBAL R0 K1; RETURN R0 1
        for raw in [op(1, 0, 0, 0), op(7, 0, 1, 0), op(27, 0, 0, 1)] {
            b.extend(raw.to_le_bytes());
        }
        b
    }

    #[test]
    fn dump_widens_sizes_and_instructions_for_64_bit_lua() {
        let d = dump(&parse(&sample()).unwrap());
        assert_eq!(&d[..14], b"\x1bLua\x50\x01\x04\x08\x08\x06\x08\x09\x09\x08");
        // source as size_t 8 with terminator, and the three instructions as 8-byte words at the end
        assert_eq!(&d[22..33], &[3, 0, 0, 0, 0, 0, 0, 0, b'@', b't', 0]);
        let code = &d[d.len() - 28..];
        assert_eq!(&code[..4], &le(3));
        let return_op = u64::from(op(27, 0, 0, 1)).to_le_bytes();
        assert_eq!(&code[20..], &return_op);
    }

    #[test]
    fn dump32_writes_the_client_layout_back() {
        let bytes = sample();
        assert_eq!(dump32(&parse(&bytes).unwrap()), bytes);
        // and the 64-bit layout reads back as the same chunk
        let c = parse(&bytes).unwrap();
        assert_eq!(parse(&dump(&c)).unwrap(), c);
    }

    #[test]
    fn reads_validates_and_lists_a_chunk() {
        let c = parse(&sample()).unwrap();
        assert_eq!(c.main.code.len(), 3);
        assert_eq!(validate(&c), Ok(()));
        let text = disassemble(&c, &|b| String::from_utf8_lossy(b).into_owned());
        assert!(text.contains("LOADK"), "{text}");
        assert!(text.contains("SETGLOBAL R0 K1 ; \"x\""), "{text}");
    }

    #[test]
    fn damaged_chunks_are_errors_not_panics() {
        let ok = sample();
        for cut in 0..ok.len() {
            assert!(parse(&ok[..cut]).is_err() || cut == ok.len());
        }
        let mut extra = ok.clone();
        extra.push(0);
        assert!(parse(&extra).is_err(), "trailing bytes");
        let mut bad = ok;
        let last = bad.len() - 4;
        bad[last..].copy_from_slice(&0x3fu32.to_le_bytes());
        let c = parse(&bad).unwrap();
        assert!(validate(&c).is_err(), "unknown opcode");
    }
}
