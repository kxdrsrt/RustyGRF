//! Lua 5.1 bytecode (`.lub` files of the client): reading the chunk and a `luac -l` style listing.
//! Decompiling to source is in [`crate::luadec`].

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LuacError {
    #[error("not Lua bytecode")]
    NotBytecode,
    #[error("unsupported Lua bytecode: {0}")]
    Unsupported(String),
    #[error("the bytecode is truncated or damaged")]
    Truncated,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    Nil,
    Bool(bool),
    Number(f64),
    /// Raw bytes (the client's scripts hold CP949 text).
    Str(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Local {
    pub name: Vec<u8>,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub source: Vec<u8>,
    pub line_defined: u32,
    pub last_line: u32,
    pub upvalue_count: u8,
    pub params: u8,
    pub is_vararg: u8,
    pub max_stack: u8,
    pub code: Vec<u32>,
    pub constants: Vec<Constant>,
    pub protos: Vec<Function>,
    pub lines: Vec<u32>,
    pub locals: Vec<Local>,
    pub upvalue_names: Vec<Vec<u8>>,
    /// Set for a function translated from Lua 5.0 (see `luac50::to_lua51`): its original code, for
    /// listings. The decompiler also uses it to know the 5.0 register layout of generic `for` loops.
    pub orig_code: Option<Vec<u32>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub main: Function,
}

pub const OPS: [&str; 38] = [
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
    "MOD",
    "POW",
    "UNM",
    "NOT",
    "LEN",
    "CONCAT",
    "JMP",
    "EQ",
    "LT",
    "LE",
    "TEST",
    "TESTSET",
    "CALL",
    "TAILCALL",
    "RETURN",
    "FORLOOP",
    "FORPREP",
    "TFORLOOP",
    "SETLIST",
    "CLOSE",
    "CLOSURE",
    "VARARG",
];

/// One decoded instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            a: (raw >> 6) & 0xff,
            c: (raw >> 14) & 0x1ff,
            b: (raw >> 23) & 0x1ff,
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
    int_size: usize,
    size_t: usize,
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
    fn uint(&mut self, n: usize) -> Result<u64, LuacError> {
        let b = self.take(n)?;
        Ok(b.iter().rev().fold(0u64, |a, &x| (a << 8) | u64::from(x)))
    }
    fn int(&mut self) -> Result<u32, LuacError> {
        let n = self.int_size;
        u32::try_from(self.uint(n)?).map_err(|_| LuacError::Truncated)
    }
    fn count(&mut self, unit: usize) -> Result<usize, LuacError> {
        let n = self.int()? as usize;
        if n.saturating_mul(unit.max(1)) > self.d.len() - self.at {
            return Err(LuacError::Truncated);
        }
        Ok(n)
    }
    fn string(&mut self) -> Result<Vec<u8>, LuacError> {
        let n = self.size_t;
        let len = usize::try_from(self.uint(n)?).map_err(|_| LuacError::Truncated)?;
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
    let line_defined = r.int()?;
    let last_line = r.int()?;
    let upvalue_count = r.byte()?;
    let params = r.byte()?;
    let is_vararg = r.byte()?;
    let max_stack = r.byte()?;
    let n = r.count(4)?;
    let mut code = Vec::with_capacity(n);
    for _ in 0..n {
        code.push(u32::try_from(r.uint(4)?).map_err(|_| LuacError::Truncated)?);
    }
    let n = r.count(1)?;
    let mut constants = Vec::with_capacity(n);
    for _ in 0..n {
        constants.push(match r.byte()? {
            0 => Constant::Nil,
            1 => Constant::Bool(r.byte()? != 0),
            3 => Constant::Number(f64::from_bits(r.uint(8)?)),
            4 => Constant::Str(r.string()?),
            t => return Err(LuacError::Unsupported(format!("constant type {t}"))),
        });
    }
    let n = r.count(1)?;
    let mut protos = Vec::with_capacity(n);
    for _ in 0..n {
        protos.push(read_function(r, depth + 1)?);
    }
    let n = r.count(4)?;
    let mut lines = Vec::with_capacity(n);
    for _ in 0..n {
        lines.push(r.int()?);
    }
    let n = r.count(4)?;
    let mut locals = Vec::with_capacity(n);
    for _ in 0..n {
        locals.push(Local {
            name: r.string()?,
            start: r.int()?,
            end: r.int()?,
        });
    }
    let n = r.count(1)?;
    let mut upvalue_names = Vec::with_capacity(n);
    for _ in 0..n {
        upvalue_names.push(r.string()?);
    }
    Ok(Function {
        source,
        line_defined,
        last_line,
        upvalue_count,
        params,
        is_vararg,
        max_stack,
        code,
        constants,
        protos,
        lines,
        locals,
        upvalue_names,
        orig_code: None,
    })
}

pub fn parse(bytes: &[u8]) -> Result<Chunk, LuacError> {
    if bytes.len() < 12 || &bytes[..4] != b"\x1bLua" {
        return Err(LuacError::NotBytecode);
    }
    if bytes[4] != 0x51 {
        return Err(LuacError::Unsupported(format!(
            "Lua version {:#x}",
            bytes[4]
        )));
    }
    if bytes[5] != 0 {
        return Err(LuacError::Unsupported("a non-standard format".into()));
    }
    if bytes[6] != 1 {
        return Err(LuacError::Unsupported("big-endian bytecode".into()));
    }
    let (int_size, size_t, instr, number) = (
        usize::from(bytes[7]),
        usize::from(bytes[8]),
        usize::from(bytes[9]),
        usize::from(bytes[10]),
    );
    if !matches!(int_size, 4 | 8)
        || !matches!(size_t, 4 | 8)
        || instr != 4
        || number != 8
        || bytes[11] != 0
    {
        return Err(LuacError::Unsupported(
            "sizes other than int 4/8, size_t 4/8, instruction 4, number 8 (double)".into(),
        ));
    }
    let mut r = Reader {
        d: bytes,
        at: 12,
        int_size,
        size_t,
    };
    let main = read_function(&mut r, 0)?;
    Ok(Chunk { main })
}

/// Writes a Lua 5.1 chunk in the layout of a 64-bit host (int 4 bytes, size_t 8 bytes): the
/// client's files are 32-bit (size_t 4), which a 64-bit Lua refuses to load. Used to run the
/// original code in a real Lua for comparisons.
pub fn dump(chunk: &Chunk) -> Vec<u8> {
    fn int(out: &mut Vec<u8>, v: usize) {
        out.extend((v as u32).to_le_bytes());
    }
    fn string(out: &mut Vec<u8>, s: &[u8]) {
        // Size 0 would load as a NULL string, which Lua's GC dereferences for local names.
        out.extend(((s.len() + 1) as u64).to_le_bytes());
        out.extend(s);
        out.push(0);
    }
    fn function(out: &mut Vec<u8>, f: &Function) {
        if f.source.is_empty() {
            // NULL source: Lua takes the parent's name
            out.extend(0u64.to_le_bytes());
        } else {
            string(out, &f.source);
        }
        int(out, f.line_defined as usize);
        int(out, f.last_line as usize);
        out.extend([f.upvalue_count, f.params, f.is_vararg, f.max_stack]);
        int(out, f.code.len());
        for c in &f.code {
            out.extend(c.to_le_bytes());
        }
        int(out, f.constants.len());
        for k in &f.constants {
            match k {
                Constant::Nil => out.push(0),
                Constant::Bool(b) => out.extend([1, u8::from(*b)]),
                Constant::Number(v) => {
                    out.push(3);
                    out.extend(v.to_le_bytes());
                }
                Constant::Str(s) => {
                    out.push(4);
                    string(out, s);
                }
            }
        }
        int(out, f.protos.len());
        for p in &f.protos {
            function(out, p);
        }
        int(out, f.lines.len());
        for l in &f.lines {
            int(out, *l as usize);
        }
        int(out, f.locals.len());
        for l in &f.locals {
            string(out, &l.name);
            int(out, l.start as usize);
            int(out, l.end as usize);
        }
        int(out, f.upvalue_names.len());
        for n in &f.upvalue_names {
            string(out, n);
        }
    }
    // ESC Lua, 5.1, official format, little-endian, int 4, size_t 8, instruction 4, number 8, not integral
    let mut out = b"\x1bLua\x51\x00\x01\x04\x08\x04\x08\x00".to_vec();
    function(&mut out, &chunk.main);
    out
}

/// Reads compiled Lua of either version the client uses; 5.0 is translated to the 5.1 form
/// (`Function::orig_code` is set then).
pub fn parse_any(bytes: &[u8]) -> Result<Chunk, LuacError> {
    if bytes.get(4) == Some(&0x50) && bytes.starts_with(b"\x1bLua") {
        let c = crate::luac50::parse(bytes)?;
        crate::luac50::validate(&c).map_err(LuacError::Unsupported)?;
        return Ok(crate::luac50::to_lua51(&c));
    }
    parse(bytes)
}

/// Structural check of every function of a Lua 5.1 chunk: known opcodes, constant and function
/// indices in range, jumps inside the code, the code ends in RETURN. Returns the first problem.
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
                "JMP" | "FORLOOP" | "FORPREP" => {
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

/// What a compiled `.lub` turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BytecodeInfo {
    /// `"5.0"` or `"5.1"`.
    pub version: &'static str,
    pub functions: usize,
    pub instructions: usize,
}

/// Reads and structurally checks compiled Lua of either version the client uses.
pub fn check_bytecode(bytes: &[u8]) -> Result<BytecodeInfo, String> {
    fn count51(f: &Function) -> (usize, usize) {
        f.protos.iter().fold((1, f.code.len()), |(a, b), p| {
            let (x, y) = count51(p);
            (a + x, b + y)
        })
    }
    fn count50(f: &crate::luac50::Function) -> (usize, usize) {
        f.protos.iter().fold((1, f.code.len()), |(a, b), p| {
            let (x, y) = count50(p);
            (a + x, b + y)
        })
    }
    match bytes.get(4) {
        Some(0x50) => {
            let c = crate::luac50::parse(bytes).map_err(|e| e.to_string())?;
            crate::luac50::validate(&c)?;
            let (functions, instructions) = count50(&c.main);
            Ok(BytecodeInfo {
                version: "5.0",
                functions,
                instructions,
            })
        }
        _ => {
            let c = parse(bytes).map_err(|e| e.to_string())?;
            validate(&c)?;
            let (functions, instructions) = count51(&c.main);
            Ok(BytecodeInfo {
                version: "5.1",
                functions,
                instructions,
            })
        }
    }
}

fn quote(s: &[u8]) -> String {
    let mut out = String::from("\"");
    for &b in s {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(char::from(b)),
            _ => out.push_str(&format!("\\{b:03}")),
        }
    }
    out.push('"');
    out
}

fn num(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn konst(c: &Constant) -> String {
    match c {
        Constant::Nil => "nil".into(),
        Constant::Bool(b) => b.to_string(),
        Constant::Number(v) => num(*v),
        Constant::Str(s) => quote(s),
    }
}

/// A listing in the style of `luac -l`. `names` turns raw bytes into the text shown (the page uses
/// the name mode it shows file names in).
pub fn disassemble(chunk: &Chunk, names: &dyn Fn(&[u8]) -> String) -> String {
    if let Some(c50) = crate::luac50::chunk_from_lua51(chunk) {
        return crate::luac50::disassemble(&c50, names);
    }
    let mut out = String::new();
    list(&chunk.main, "main", names, &mut out);
    out
}

fn list(f: &Function, label: &str, names: &dyn Fn(&[u8]) -> String, out: &mut String) {
    out.push_str(&format!(
        "\n{label} <{}:{},{}> ({} instructions, {} params, {} slots, {} upvalues, {} constants, {} functions)\n",
        names(&f.source),
        f.line_defined,
        f.last_line,
        f.code.len(),
        f.params,
        f.max_stack,
        f.upvalue_count,
        f.constants.len(),
        f.protos.len()
    ));
    for (pc, &raw) in f.code.iter().enumerate() {
        let i = Instr::decode(raw);
        let line = f.lines.get(pc).copied().unwrap_or(0);
        let k = |idx: u32| {
            f.constants
                .get(idx as usize)
                .map_or("?".into(), |c| match c {
                    Constant::Str(s) => names(s).chars().take(60).collect::<String>().pipe_quote(),
                    other => konst(other),
                })
        };
        let rk = |v: u32| {
            if v >= 256 {
                format!("K{} ; {}", v - 256, k(v - 256))
            } else {
                format!("R{v}")
            }
        };
        let args = match i.name() {
            "MOVE" | "UNM" | "NOT" | "LEN" => format!("R{} R{}", i.a, i.b),
            "LOADK" => format!("R{} K{} ; {}", i.a, i.bx(), k(i.bx())),
            "LOADBOOL" => format!("R{} {} {}", i.a, i.b, i.c),
            "LOADNIL" => format!("R{} R{}", i.a, i.b),
            "GETUPVAL" | "SETUPVAL" => format!("R{} U{}", i.a, i.b),
            "GETGLOBAL" | "SETGLOBAL" => format!("R{} K{} ; {}", i.a, i.bx(), k(i.bx())),
            "GETTABLE" => format!("R{} R{} {}", i.a, i.b, rk(i.c)),
            "SETTABLE" => format!("R{} {} {}", i.a, rk(i.b), rk(i.c)),
            "NEWTABLE" => format!("R{} {} {}", i.a, i.b, i.c),
            "SELF" => format!("R{} R{} {}", i.a, i.b, rk(i.c)),
            "ADD" | "SUB" | "MUL" | "DIV" | "MOD" | "POW" => {
                format!("R{} {} {}", i.a, rk(i.b), rk(i.c))
            }
            "CONCAT" => format!("R{} R{} R{}", i.a, i.b, i.c),
            "JMP" => format!("-> {}", pc as i64 + 1 + i.sbx()),
            "EQ" | "LT" | "LE" => format!("{} {} {}", i.a, rk(i.b), rk(i.c)),
            "TEST" => format!("R{} {}", i.a, i.c),
            "TESTSET" => format!("R{} R{} {}", i.a, i.b, i.c),
            "CALL" | "TAILCALL" => format!("R{} {} {}", i.a, i.b, i.c),
            "RETURN" => format!("R{} {}", i.a, i.b),
            "FORLOOP" | "FORPREP" => format!("R{} -> {}", i.a, pc as i64 + 1 + i.sbx()),
            "TFORLOOP" => format!("R{} {}", i.a, i.c),
            "SETLIST" => format!("R{} {} {}", i.a, i.b, i.c),
            "CLOSE" => format!("R{}", i.a),
            "CLOSURE" => format!("R{} function {}", i.a, i.bx()),
            "VARARG" => format!("R{} {}", i.a, i.b),
            _ => format!("{} {} {}", i.a, i.b, i.c),
        };
        out.push_str(&format!("  {pc:>4} [{line:>4}] {:<9} {args}\n", i.name()));
    }
    for (n, p) in f.protos.iter().enumerate() {
        list(p, &format!("function {n}"), names, out);
    }
}

trait PipeQuote {
    fn pipe_quote(self) -> String;
}

impl PipeQuote for String {
    fn pipe_quote(self) -> String {
        format!("\"{self}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `luac` output of `x = 1`, made by hand: LOADK 0 0; SETGLOBAL 0 1; RETURN 0 1.
    fn chunk() -> Vec<u8> {
        let mut b = b"\x1bLuaQ\x00\x01\x04\x04\x04\x08\x00".to_vec();
        let s = |v: &mut Vec<u8>, t: &str| {
            v.extend(((t.len() + 1) as u32).to_le_bytes());
            v.extend(t.as_bytes());
            v.push(0);
        };
        s(&mut b, "@t.lua");
        for v in [0u32, 0] {
            b.extend(v.to_le_bytes());
        }
        b.extend([0, 0, 2, 2]); // upvalues, params, vararg, stack
        b.extend(3u32.to_le_bytes());
        let ins = |op: u32, a: u32, bx: u32| op | (a << 6) | (bx << 14);
        for i in [ins(1, 0, 0), ins(7, 0, 1), 30 | (1 << 23)] {
            b.extend(i.to_le_bytes());
        }
        b.extend(2u32.to_le_bytes()); // constants
        b.push(3);
        b.extend(1.0f64.to_le_bytes());
        b.push(4);
        s(&mut b, "x");
        b.extend(0u32.to_le_bytes()); // protos
        for _ in 0..3 {
            b.extend(0u32.to_le_bytes()); // lines, locals, upvalue names
        }
        b
    }

    #[test]
    fn reads_a_chunk_and_lists_it() {
        let c = parse(&chunk()).unwrap();
        assert_eq!(c.main.code.len(), 3);
        assert_eq!(
            c.main.constants,
            [Constant::Number(1.0), Constant::Str(b"x".to_vec())]
        );
        let text = disassemble(&c, &|b| String::from_utf8_lossy(b).into_owned());
        assert!(text.contains("LOADK"), "{text}");
        assert!(text.contains("SETGLOBAL"), "{text}");
        assert!(text.contains("\"x\""), "{text}");
    }

    #[test]
    fn dump_gives_empty_strings_a_terminator() {
        // Lua loads a size-0 string as NULL and crashes on it in a local's name; only the source
        // may be NULL (it then takes the parent's name).
        let mut c = parse(&chunk()).unwrap();
        c.main.locals.push(Local {
            name: Vec::new(),
            start: 0,
            end: 1,
        });
        let d = dump(&c);
        assert_eq!(&d[..12], b"\x1bLua\x51\x00\x01\x04\x08\x04\x08\x00");
        let tail = &d[d.len() - 25..];
        let mut expected = 1u32.to_le_bytes().to_vec(); // one local
        expected.extend(1u64.to_le_bytes()); // size 1: just the terminator
        expected.push(0);
        expected.extend(0u32.to_le_bytes());
        expected.extend(1u32.to_le_bytes());
        expected.extend(0u32.to_le_bytes()); // no upvalue names
        assert_eq!(tail, &expected[..]);
    }

    #[test]
    fn bad_input_is_an_error() {
        assert_eq!(parse(b"print(1)"), Err(LuacError::NotBytecode));
        let good = chunk();
        assert_eq!(parse(&good[..good.len() - 5]), Err(LuacError::Truncated));
        let mut v52 = good.clone();
        v52[4] = 0x52;
        assert!(matches!(parse(&v52), Err(LuacError::Unsupported(_))));
        let mut huge = good;
        huge[35..39].copy_from_slice(&u32::MAX.to_le_bytes()); // the instruction count
        assert!(parse(&huge).is_err());
    }
}
