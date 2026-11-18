//! A decompiler for Lua 5.1 bytecode of the client's `.lub` files (straight-line code: data tables,
//! assignments, calls, function definitions). A function with jumps or loops is not decompiled;
//! its bytecode listing is written as a comment instead, so nothing is lost and nothing is invented.

use crate::luac::{Chunk, Constant, Function, Instr};

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Nil,
    Bool(bool),
    Num(f64),
    Str(Vec<u8>),
    Global(Vec<u8>),
    Upvalue(String),
    Param(String),
    Vararg,
    Index(Box<Expr>, Box<Expr>),
    Method(Box<Expr>, Vec<u8>),
    Call(Box<Expr>, Vec<Expr>),
    Table(Vec<Entry>),
    Func(usize),
    Bin(&'static str, Box<Expr>, Box<Expr>),
    Un(&'static str, Box<Expr>),
    Concat(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
enum Entry {
    Item(Expr),
    Keyed(Expr, Expr),
}

/// The registers of a function while its code is walked.
struct State {
    reg: Vec<Option<Expr>>,
    top: usize,
}

/// Why a function is not decompiled.
struct Unsupported(String);

struct Ctx<'a> {
    names: &'a dyn Fn(&[u8]) -> String,
}

fn is_ident(s: &str) -> bool {
    const KEYWORDS: [&str; 21] = [
        "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in",
        "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
    ];
    let mut c = s.chars();
    matches!(c.next(), Some(f) if f.is_ascii_alphabetic() || f == '_')
        && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
        && !KEYWORDS.contains(&s)
}

fn quote(ctx: &Ctx<'_>, s: &[u8]) -> String {
    let text = (ctx.names)(s);
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\{:03}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn needs_parens(e: &Expr) -> bool {
    matches!(
        e,
        Expr::Bin(..)
            | Expr::Un(..)
            | Expr::Concat(_)
            | Expr::Func(_)
            | Expr::Nil
            | Expr::Num(_)
            | Expr::Bool(_)
            | Expr::Str(_)
            | Expr::Table(_)
    )
}

impl Ctx<'_> {
    fn expr(&self, f: &Function, e: &Expr, indent: usize) -> String {
        match e {
            Expr::Nil => "nil".into(),
            Expr::Bool(b) => b.to_string(),
            Expr::Num(v) => number(*v),
            Expr::Str(s) => quote(self, s),
            Expr::Global(n) => (self.names)(n),
            Expr::Upvalue(n) | Expr::Param(n) => n.clone(),
            Expr::Vararg => "...".into(),
            Expr::Index(t, k) => {
                let base = self.prefix(f, t, indent);
                match &**k {
                    Expr::Str(s) if is_ident(&(self.names)(s)) => {
                        format!("{base}.{}", (self.names)(s))
                    }
                    k => format!("{base}[{}]", self.expr(f, k, indent)),
                }
            }
            Expr::Method(o, n) => format!("{}:{}", self.prefix(f, o, indent), (self.names)(n)),
            Expr::Call(func, args) => {
                let list: Vec<String> = args.iter().map(|a| self.expr(f, a, indent)).collect();
                let callee = match &**func {
                    Expr::Method(..) => self.expr(f, func, indent),
                    other => self.prefix(f, other, indent),
                };
                format!("{callee}({})", list.join(", "))
            }
            Expr::Table(entries) => self.table(f, entries, indent),
            Expr::Func(i) => self.function(f, *i, indent),
            Expr::Bin(op, l, r) => format!(
                "{} {op} {}",
                self.operand(f, l, indent),
                self.operand(f, r, indent)
            ),
            Expr::Un(op, x) => format!("{op}{}", self.operand(f, x, indent)),
            Expr::Concat(parts) => parts
                .iter()
                .map(|p| self.operand(f, p, indent))
                .collect::<Vec<_>>()
                .join(" .. "),
        }
    }

    /// An expression as the left part of an index or call: parenthesised when it is not a name or a call.
    fn prefix(&self, f: &Function, e: &Expr, indent: usize) -> String {
        let s = self.expr(f, e, indent);
        if needs_parens(e) { format!("({s})") } else { s }
    }

    fn operand(&self, f: &Function, e: &Expr, indent: usize) -> String {
        let s = self.expr(f, e, indent);
        if matches!(e, Expr::Bin(..) | Expr::Un(..) | Expr::Concat(_)) {
            format!("({s})")
        } else {
            s
        }
    }

    fn table(&self, f: &Function, entries: &[Entry], indent: usize) -> String {
        if entries.is_empty() {
            return "{}".into();
        }
        let pad = "\t".repeat(indent + 1);
        let mut out = String::from("{\n");
        for (i, e) in entries.iter().enumerate() {
            let line = match e {
                Entry::Item(v) => self.expr(f, v, indent + 1),
                Entry::Keyed(k, v) => {
                    let key = match k {
                        Expr::Str(s) if is_ident(&(self.names)(s)) => (self.names)(s),
                        k => format!("[{}]", self.expr(f, k, indent + 1)),
                    };
                    format!("{key} = {}", self.expr(f, v, indent + 1))
                }
            };
            out.push_str(&pad);
            out.push_str(&line);
            out.push_str(if i + 1 < entries.len() { ",\n" } else { "\n" });
        }
        out.push_str(&"\t".repeat(indent));
        out.push('}');
        out
    }

    fn function(&self, parent: &Function, index: usize, indent: usize) -> String {
        let Some(p) = parent.protos.get(index) else {
            return "function() end".into();
        };
        let params: Vec<String> = (0..p.params).map(|i| param_name(p, i)).collect();
        let mut head = params;
        if p.is_vararg != 0 {
            head.push("...".into());
        }
        let body = self.body(p, indent + 1);
        let pad = "\t".repeat(indent);
        format!("function({})\n{body}{pad}end", head.join(", "))
    }

    /// The statements of a function, each on its own lines, indented by `indent` tabs.
    fn body(&self, f: &Function, indent: usize) -> String {
        match self.statements(f, indent) {
            Ok(s) => s,
            Err(Unsupported(why)) => {
                let pad = "\t".repeat(indent);
                let mut out = format!("{pad}-- not decompiled ({why}); bytecode:\n");
                let chunk = Chunk { main: f.clone() };
                for line in crate::luac::disassemble(&chunk, self.names).lines().skip(1) {
                    out.push_str(&format!("{pad}-- {line}\n"));
                }
                out
            }
        }
    }

    fn statements(&self, f: &Function, indent: usize) -> Result<String, Unsupported> {
        let slots = usize::from(f.max_stack).max(1) + 2;
        let mut reg: Vec<Option<Expr>> = vec![None; slots];
        for (i, r) in reg.iter_mut().enumerate().take(usize::from(f.params)) {
            *r = Some(Expr::Param(param_name(f, i as u8)));
        }
        let mut st = State { reg, top: 0 };
        self.block(f, &mut st, 0, f.code.len(), indent)
    }

    /// The instructions `start..end` as statements. A comparison followed by a jump becomes an
    /// `if` (with `else` when the body ends in a jump over more code); anything that does not fit
    /// that shape makes the whole function fall back to the listing.
    fn block(
        &self,
        f: &Function,
        st: &mut State,
        start: usize,
        end: usize,
        indent: usize,
    ) -> Result<String, Unsupported> {
        let pad = "\t".repeat(indent);
        let State { reg, top } = st;
        let mut out = String::new();
        let rk = |reg: &[Option<Expr>], v: u32| -> Result<Expr, Unsupported> {
            if v >= 256 {
                konst(f, (v - 256) as usize)
            } else {
                get(reg, v as usize)
            }
        };
        let mut pc = start;
        while pc < end {
            let i = Instr::decode(f.code[pc]);
            pc += 1;
            let (a, b, c) = (i.a as usize, i.b as usize, i.c as usize);
            if a >= reg.len() && !matches!(i.name(), "JMP") {
                return Err(Unsupported("register out of range".into()));
            }
            match i.name() {
                "MOVE" => reg[a] = Some(get(reg, b)?),
                "LOADK" => reg[a] = Some(konst(f, i.bx() as usize)?),
                "LOADBOOL" => {
                    if c != 0 {
                        return Err(Unsupported("conditional value".into()));
                    }
                    reg[a] = Some(Expr::Bool(b != 0));
                }
                "LOADNIL" => {
                    for r in a..=b.min(reg.len() - 1) {
                        reg[r] = Some(Expr::Nil);
                    }
                }
                "GETUPVAL" => {
                    let n = f
                        .upvalue_names
                        .get(b)
                        .map_or_else(|| format!("upvalue{b}"), |s| (self.names)(s));
                    reg[a] = Some(Expr::Upvalue(n));
                }
                "GETGLOBAL" => match konst(f, i.bx() as usize)? {
                    Expr::Str(s) => reg[a] = Some(Expr::Global(s)),
                    _ => return Err(Unsupported("global name".into())),
                },
                "GETTABLE" => {
                    reg[a] = Some(Expr::Index(Box::new(get(reg, b)?), Box::new(rk(reg, i.c)?)))
                }
                "SETGLOBAL" => {
                    let Expr::Str(name) = konst(f, i.bx() as usize)? else {
                        return Err(Unsupported("global name".into()));
                    };
                    let v = get(reg, a)?;
                    out.push_str(&format!(
                        "{pad}{} = {}\n",
                        (self.names)(&name),
                        self.expr(f, &v, indent)
                    ));
                }
                "SETUPVAL" => {
                    let n = f
                        .upvalue_names
                        .get(b)
                        .map_or_else(|| format!("upvalue{b}"), |s| (self.names)(s));
                    let v = get(reg, a)?;
                    out.push_str(&format!("{pad}{n} = {}\n", self.expr(f, &v, indent)));
                }
                "SETTABLE" => {
                    let key = rk(reg, i.b)?;
                    let val = rk(reg, i.c)?;
                    match reg[a].as_mut() {
                        Some(Expr::Table(entries)) => entries.push(Entry::Keyed(key, val)),
                        Some(target) => {
                            let target = target.clone();
                            let lhs =
                                self.expr(f, &Expr::Index(Box::new(target), Box::new(key)), indent);
                            out.push_str(&format!("{pad}{lhs} = {}\n", self.expr(f, &val, indent)));
                        }
                        None => return Err(Unsupported("assignment to an unknown value".into())),
                    }
                }
                "NEWTABLE" => reg[a] = Some(Expr::Table(Vec::new())),
                "SELF" => {
                    let obj = get(reg, b)?;
                    let Expr::Str(name) = rk(reg, i.c)? else {
                        return Err(Unsupported("method name".into()));
                    };
                    if a + 1 < reg.len() {
                        reg[a + 1] = Some(obj.clone());
                    }
                    reg[a] = Some(Expr::Method(Box::new(obj), name));
                }
                "ADD" | "SUB" | "MUL" | "DIV" | "MOD" | "POW" => {
                    let op = match i.name() {
                        "ADD" => "+",
                        "SUB" => "-",
                        "MUL" => "*",
                        "DIV" => "/",
                        "MOD" => "%",
                        _ => "^",
                    };
                    reg[a] = Some(Expr::Bin(
                        op,
                        Box::new(rk(reg, i.b)?),
                        Box::new(rk(reg, i.c)?),
                    ));
                }
                "UNM" => reg[a] = Some(Expr::Un("-", Box::new(get(reg, b)?))),
                "NOT" => reg[a] = Some(Expr::Un("not ", Box::new(get(reg, b)?))),
                "LEN" => reg[a] = Some(Expr::Un("#", Box::new(get(reg, b)?))),
                "CONCAT" => {
                    let mut parts = Vec::new();
                    for r in b..=c {
                        parts.push(get(reg, r)?);
                    }
                    reg[a] = Some(Expr::Concat(parts));
                }
                "CALL" | "TAILCALL" => {
                    let call_end = if b == 0 { *top } else { a + b };
                    if call_end < a + 1 {
                        return Err(Unsupported("call arguments".into()));
                    }
                    let mut args = Vec::new();
                    // for a method call the object is the first argument register and is implied
                    let method = matches!(reg[a], Some(Expr::Method(..)));
                    for r in a + 1 + usize::from(method)..call_end {
                        args.push(get(reg, r)?);
                    }
                    let call = Expr::Call(Box::new(get(reg, a)?), args);
                    if i.name() == "TAILCALL" {
                        out.push_str(&format!("{pad}return {}\n", self.expr(f, &call, indent)));
                        // a RETURN follows a TAILCALL; it carries no value of its own
                        pc += 1;
                    } else if c == 1 {
                        out.push_str(&format!("{pad}{}\n", self.expr(f, &call, indent)));
                    } else {
                        reg[a] = Some(call);
                        if c == 0 {
                            *top = a + 1;
                        }
                    }
                }
                "SETLIST" => {
                    let n = if b == 0 {
                        (*top).saturating_sub(a + 1)
                    } else {
                        b
                    };
                    let mut items = Vec::new();
                    for r in a + 1..=a + n {
                        items.push(get(reg, r)?);
                    }
                    match reg[a].as_mut() {
                        Some(Expr::Table(entries)) => {
                            entries.extend(items.into_iter().map(Entry::Item))
                        }
                        _ => return Err(Unsupported("list for an unknown table".into())),
                    }
                }
                "CLOSURE" => {
                    let idx = i.bx() as usize;
                    let ups = f
                        .protos
                        .get(idx)
                        .map_or(0, |p| usize::from(p.upvalue_count));
                    reg[a] = Some(Expr::Func(idx));
                    pc += ups; // the MOVE/GETUPVAL pseudo-instructions that bind the upvalues
                }
                "VARARG" => {
                    reg[a] = Some(Expr::Vararg);
                    if b == 0 {
                        *top = a + 1;
                    }
                }
                "RETURN" => {
                    let ret_end = if b == 0 { *top } else { a + b - 1 };
                    let last = pc >= f.code.len();
                    if b == 1 && last {
                        break;
                    }
                    let mut vals = Vec::new();
                    for r in a..ret_end {
                        vals.push(self.expr(f, &get(reg, r)?, indent));
                    }
                    if vals.is_empty() {
                        out.push_str(&format!("{pad}return\n"));
                    } else {
                        out.push_str(&format!("{pad}return {}\n", vals.join(", ")));
                    }
                }
                "EQ" | "LT" | "LE" | "TEST" => {
                    let jmp_pc = pc;
                    let j = f
                        .code
                        .get(jmp_pc)
                        .map(|r| Instr::decode(*r))
                        .filter(|j| j.name() == "JMP")
                        .ok_or_else(|| Unsupported("a comparison without a jump".into()))?;
                    let target = usize::try_from(jmp_pc as i64 + 1 + j.sbx())
                        .ok()
                        .filter(|&t| t > jmp_pc && t <= end)
                        .ok_or_else(|| Unsupported("a backward or far jump".into()))?;
                    let cond = if i.name() == "TEST" {
                        let v = get(reg, a)?;
                        let text = self.operand(f, &v, indent);
                        if c == 0 { text } else { format!("not {text}") }
                    } else {
                        let (l, r) = (rk(reg, i.b)?, rk(reg, i.c)?);
                        let op = match (i.name(), a) {
                            ("EQ", 0) => "==",
                            ("EQ", _) => "~=",
                            ("LT", 0) => "<",
                            ("LT", _) => ">=",
                            ("LE", 0) => "<=",
                            _ => ">",
                        };
                        format!(
                            "{} {op} {}",
                            self.operand(f, &l, indent),
                            self.operand(f, &r, indent)
                        )
                    };
                    // `if ... else ...`: the body ends in a jump over the else part
                    let else_jump = (target - 1 > jmp_pc)
                        .then(|| Instr::decode(f.code[target - 1]))
                        .filter(|e| e.name() == "JMP")
                        .and_then(|e| usize::try_from(target as i64 + e.sbx()).ok())
                        .filter(|&u| u > target && u <= end);
                    let then_end = if else_jump.is_some() {
                        target - 1
                    } else {
                        target
                    };
                    // every jump inside the body must stay inside it, or the shape is not a plain `if`
                    for (q, raw) in f.code.iter().enumerate().take(then_end).skip(jmp_pc + 1) {
                        let ins = Instr::decode(*raw);
                        if matches!(ins.name(), "JMP" | "FORLOOP" | "FORPREP") {
                            let t = q as i64 + 1 + ins.sbx();
                            if t < jmp_pc as i64 + 1 || t > then_end as i64 {
                                return Err(Unsupported("a complex condition or loop".into()));
                            }
                        }
                    }
                    let mut then_state = State {
                        reg: reg.clone(),
                        top: *top,
                    };
                    let then_text =
                        self.block(f, &mut then_state, jmp_pc + 1, then_end, indent + 1)?;
                    let mut states = vec![then_state];
                    out.push_str(&format!("{pad}if {cond} then\n{then_text}"));
                    if let Some(u) = else_jump {
                        let mut else_state = State {
                            reg: reg.clone(),
                            top: *top,
                        };
                        let else_text = self.block(f, &mut else_state, target, u, indent + 1)?;
                        out.push_str(&format!("{pad}else\n{else_text}"));
                        states.push(else_state);
                    }
                    out.push_str(&format!("{pad}end\n"));
                    // what a branch assigned to a register is unknown after the join
                    for r in 0..reg.len() {
                        if states.iter().any(|s| s.reg[r] != reg[r]) {
                            reg[r] = None;
                        }
                    }
                    pc = else_jump.unwrap_or(target);
                }
                "CLOSE" => {}
                other => return Err(Unsupported(format!("{other} (control flow)"))),
            }
        }
        Ok(out)
    }
}

fn param_name(f: &Function, i: u8) -> String {
    f.locals
        .get(usize::from(i))
        .filter(|l| !l.name.is_empty())
        .map_or_else(
            || format!("arg{i}"),
            |l| String::from_utf8_lossy(&l.name).into_owned(),
        )
}

fn get(reg: &[Option<Expr>], i: usize) -> Result<Expr, Unsupported> {
    reg.get(i)
        .and_then(Clone::clone)
        .ok_or_else(|| Unsupported(format!("register {i} has no known value")))
}

fn konst(f: &Function, i: usize) -> Result<Expr, Unsupported> {
    Ok(
        match f
            .constants
            .get(i)
            .ok_or_else(|| Unsupported("constant".into()))?
        {
            Constant::Nil => Expr::Nil,
            Constant::Bool(b) => Expr::Bool(*b),
            Constant::Number(v) => Expr::Num(*v),
            Constant::Str(s) => Expr::Str(s.clone()),
        },
    )
}

/// Lua source for the chunk. `names` turns raw bytes (names, strings) into text.
pub fn decompile(chunk: &Chunk, names: &dyn Fn(&[u8]) -> String) -> String {
    let ctx = Ctx { names };
    let mut out = String::from("-- decompiled by RustyGRF from Lua 5.1 bytecode\n");
    out.push_str(&ctx.body(&chunk.main, 0));
    out
}

/// True when every function of the chunk was decompiled (no "not decompiled" comment).
pub fn is_complete(text: &str) -> bool {
    !text.contains("-- not decompiled")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(b: &[u8]) -> String {
        String::from_utf8_lossy(b).into_owned()
    }

    fn s(v: &str) -> Constant {
        Constant::Str(v.as_bytes().to_vec())
    }

    fn ins(op: u32, a: u32, b: u32, c: u32) -> u32 {
        op | (a << 6) | (c << 14) | (b << 23)
    }

    fn insx(op: u32, a: u32, bx: u32) -> u32 {
        op | (a << 6) | (bx << 14)
    }

    fn func(code: Vec<u32>, constants: Vec<Constant>, protos: Vec<Function>) -> Function {
        Function {
            source: b"@t".to_vec(),
            line_defined: 0,
            last_line: 0,
            upvalue_count: 0,
            params: 0,
            is_vararg: 0,
            max_stack: 8,
            code,
            constants,
            protos,
            lines: vec![],
            locals: vec![],
            upvalue_names: vec![],
        }
    }

    // opcodes: LOADK 1, GETGLOBAL 5, SETGLOBAL 7, SETTABLE 9, NEWTABLE 10, CALL 28, RETURN 30, SETLIST 34, CLOSURE 36
    #[test]
    fn decompiles_a_data_table_with_nested_tables_and_a_call() {
        // Tbl = { [1] = "a", name = { 1, 2 } }; print("x")
        let code = vec![
            ins(10, 0, 0, 0),    // NEWTABLE R0
            ins(9, 0, 256, 257), // SETTABLE R0 K0 K1   [1] = "a"
            ins(10, 1, 0, 0),    // NEWTABLE R1
            insx(1, 2, 3),       // LOADK R2 K3 (1)
            insx(1, 3, 4),       // LOADK R3 K4 (2)
            ins(34, 1, 2, 1),    // SETLIST R1 2 1
            ins(9, 0, 258, 1),   // SETTABLE R0 K2 R1   name = R1
            insx(7, 0, 5),       // SETGLOBAL R0 "Tbl"
            insx(5, 0, 6),       // GETGLOBAL R0 "print"
            insx(1, 1, 7),       // LOADK R1 "x"
            ins(28, 0, 2, 1),    // CALL R0 2 1
            ins(30, 0, 1, 0),    // RETURN
        ];
        let consts = vec![
            Constant::Number(1.0),
            s("a"),
            s("name"),
            Constant::Number(1.0),
            Constant::Number(2.0),
            s("Tbl"),
            s("print"),
            s("x"),
        ];
        let chunk = Chunk {
            main: func(code, consts, vec![]),
        };
        let out = decompile(&chunk, &text);
        assert!(is_complete(&out), "{out}");
        assert!(out.contains("Tbl = {"), "{out}");
        assert!(out.contains("[1] = \"a\""), "{out}");
        assert!(out.contains("name = {\n\t\t1,\n\t\t2\n\t}"), "{out}");
        assert!(out.contains("print(\"x\")"), "{out}");
    }

    #[test]
    fn functions_with_control_flow_become_a_listing_comment() {
        let f = func(vec![ins(22, 0, 0, 0), ins(30, 0, 1, 0)], vec![], vec![]); // JMP, RETURN
        let out = decompile(&Chunk { main: f }, &text);
        assert!(!is_complete(&out));
        assert!(out.contains("-- not decompiled"), "{out}");
        assert!(out.contains("JMP"), "{out}");
    }

    #[test]
    fn a_nested_function_is_written_in_place() {
        // f = function(a) return a end
        let inner = {
            let mut p = func(vec![ins(30, 0, 2, 0), ins(30, 0, 1, 0)], vec![], vec![]);
            p.params = 1;
            p
        };
        let code = vec![insx(36, 0, 0), insx(7, 0, 0), ins(30, 0, 1, 0)];
        let chunk = Chunk {
            main: func(code, vec![s("f")], vec![inner]),
        };
        let out = decompile(&chunk, &text);
        assert!(out.contains("f = function(arg0)"), "{out}");
        assert!(out.contains("return arg0"), "{out}");
    }
}
