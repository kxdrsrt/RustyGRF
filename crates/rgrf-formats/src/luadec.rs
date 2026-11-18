//! A decompiler for Lua 5.1 bytecode of the client's `.lub` files: data tables, assignments, calls,
//! function definitions, local variables (names from the debug information), `if`/`else` with `and`
//! / `or`, `while`, numeric and generic `for`, and `break`. A function whose code has a shape this
//! does not understand is not guessed; its bytecode listing is written as a comment instead, so
//! nothing is lost and nothing is invented. The output is checked for valid syntax and for keeping
//! every string constant, not for the same meaning (no Lua interpreter is available to run both).

use crate::luac::{Chunk, Constant, Function, Instr, Local};

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
    /// A comparison used as a value (`x = a == b`), already written out.
    Text(String),
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
    /// How many functions deep the function being written is nested.
    depth: std::cell::Cell<usize>,
    /// How many blocks (`if`, loops, condition code) deep the code being walked is.
    block_depth: std::cell::Cell<usize>,
}

/// Decrements the block depth when a block is left, however it is left.
struct BlockGuard<'a>(&'a std::cell::Cell<usize>);

impl Drop for BlockGuard<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

/// Nested functions and expressions deeper than this are not decompiled: the output is built
/// recursively, so a damaged or hostile file must not be able to exhaust the stack.
const MAX_NESTING: usize = 40;
const MAX_EXPR_DEPTH: usize = 100;
const MAX_BLOCK_DEPTH: usize = 60;

/// True when `e` is nested deeper than [`MAX_EXPR_DEPTH`]. At most a few thousand nodes are looked
/// at, so checking every instruction stays cheap even for huge data tables.
fn too_deep(e: &Expr) -> bool {
    fn go(e: &Expr, depth: usize, budget: &mut usize) -> bool {
        if depth > MAX_EXPR_DEPTH {
            return true;
        }
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        match e {
            Expr::Index(a, b) | Expr::Bin(_, a, b) => {
                go(a, depth + 1, budget) || go(b, depth + 1, budget)
            }
            Expr::Method(o, _) | Expr::Un(_, o) => go(o, depth + 1, budget),
            Expr::Call(g, args) => {
                go(g, depth + 1, budget) || args.iter().any(|a| go(a, depth + 1, budget))
            }
            Expr::Table(entries) => entries.iter().any(|en| match en {
                Entry::Item(v) => go(v, depth + 1, budget),
                Entry::Keyed(k, v) => go(k, depth + 1, budget) || go(v, depth + 1, budget),
            }),
            Expr::Concat(parts) => parts.iter().any(|p| go(p, depth + 1, budget)),
            _ => false,
        }
    }
    go(e, 0, &mut 4000)
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

/// The text of a string constant. Some scripts hold UTF-8 text between the usual CP949 ones: bytes
/// that are valid UTF-8 and whose non-ASCII characters are all Hangul or CJK (U+0800 and up) are
/// shown as UTF-8; short CP949 strings can be valid UTF-8 by accident, but not with such characters.
pub fn string_text(s: &[u8], names: &dyn Fn(&[u8]) -> String) -> String {
    match std::str::from_utf8(s) {
        Ok(t) if !t.is_ascii() && t.chars().all(|c| c.is_ascii() || c as u32 >= 0x800) => {
            t.to_string()
        }
        _ => names(s),
    }
}

fn quote(ctx: &Ctx<'_>, s: &[u8]) -> String {
    let text = string_text(s, ctx.names);
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
            | Expr::Text(_)
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
            Expr::Bin(op, l, r) => {
                // `a or b or c` is written without parentheses (the same operator on the right)
                let chain =
                    matches!(&**r, Expr::Bin(o, ..) if o == op && (*op == "or" || *op == "and"));
                let right = if chain {
                    self.expr(f, r, indent)
                } else {
                    self.operand(f, r, indent)
                };
                format!("{} {op} {right}", self.operand(f, l, indent))
            }
            Expr::Un(op, x) => format!("{op}{}", self.operand(f, x, indent)),
            Expr::Text(t) => t.clone(),
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
        if matches!(
            e,
            Expr::Bin(..) | Expr::Un(..) | Expr::Concat(_) | Expr::Text(_)
        ) {
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
        self.depth.set(self.depth.get() + 1);
        let body = self.body(p, indent + 1);
        self.depth.set(self.depth.get() - 1);
        let pad = "\t".repeat(indent);
        format!("function({})\n{body}{pad}end", head.join(", "))
    }

    /// The statements of a function, each on its own lines, indented by `indent` tabs.
    fn body(&self, f: &Function, indent: usize) -> String {
        let result = if self.depth.get() > MAX_NESTING {
            Err(Unsupported("functions nested too deeply".into()))
        } else {
            self.statements(f, indent)
        };
        match result {
            Ok(s) => s,
            Err(Unsupported(why)) => {
                let pad = "\t".repeat(indent);
                let mut out = format!("{pad}-- not decompiled ({why}); bytecode:\n");
                let listing = match crate::luac50::from_lua51(f) {
                    Some(f50) => crate::luac50::list_function(&f50, self.names),
                    None => crate::luac::disassemble(&Chunk { main: f.clone() }, self.names),
                };
                for line in listing.lines().skip(1) {
                    out.push_str(&format!("{pad}-- {line}\n"));
                }
                out
            }
        }
    }

    fn statements(&self, f: &Function, indent: usize) -> Result<String, Unsupported> {
        let slots = usize::from(f.max_stack).max(1) + 2;
        // the frame starts out nil, and the compiler leaves out a LOADNIL at the very start of a
        // function (`x = nil` is then a SETGLOBAL of an untouched register)
        let mut reg: Vec<Option<Expr>> = vec![Some(Expr::Nil); slots];
        for (i, r) in reg.iter_mut().enumerate().take(usize::from(f.params)) {
            *r = Some(Expr::Param(param_name(f, i as u8)));
        }
        let mut st = State { reg, top: 0 };
        self.block(f, &mut st, 0, f.code.len(), indent, Exits::default())
    }

    /// Declares the locals that become visible at `pc` with the value their register holds.
    fn declare_locals(
        &self,
        f: &Function,
        reg: &mut [Option<Expr>],
        pc: usize,
        indent: usize,
        out: &mut String,
    ) -> Result<(), Unsupported> {
        let starting: Vec<usize> = f
            .locals
            .iter()
            .enumerate()
            .filter(|(_, l)| l.start as usize == pc && !hidden(l))
            .map(|(i, _)| i)
            .collect();
        if starting.is_empty() {
            return Ok(());
        }
        let active = active_at(f, pc);
        let (mut names, mut vals, mut regs) = (Vec::new(), Vec::new(), Vec::new());
        for li in starting {
            let Some(r) = active.iter().position(|&x| x == li) else {
                continue;
            };
            if r < usize::from(f.params) || r >= reg.len() {
                continue;
            }
            let name = (self.names)(&f.locals[li].name);
            // the implicit `arg` table of a vararg function is not declared in the source
            if pc == 0 && name == "arg" && f.is_vararg != 0 {
                reg[r] = Some(Expr::Param(name));
                continue;
            }
            // a loop variable is already set by the loop
            if matches!(&reg[r], Some(Expr::Param(n)) if *n == name) {
                continue;
            }
            names.push(name);
            vals.push(reg[r].clone());
            regs.push(r);
        }
        if names.is_empty() {
            return Ok(());
        }
        let pad = "\t".repeat(indent);
        let shown = |e: &Option<Expr>| e.as_ref().map(|e| self.expr(f, e, indent));
        let line = if let [Some(Expr::Func(_))] = vals.as_slice() {
            let text = shown(&vals[0]).unwrap_or_default();
            match text.strip_prefix("function(") {
                Some(rest) => format!("local function {}({rest}", names[0]),
                None => format!("local {} = {text}", names[0]),
            }
        } else if vals.iter().all(Option::is_some) {
            let v: Vec<String> = vals.iter().filter_map(shown).collect();
            format!("local {} = {}", names.join(", "), v.join(", "))
        } else if matches!(vals[0], Some(Expr::Call(..))) && vals[1..].iter().all(Option::is_none) {
            format!(
                "local {} = {}",
                names.join(", "),
                shown(&vals[0]).unwrap_or_default()
            )
        } else {
            return Err(Unsupported(format!(
                "a local without a known value: {} at {pc}",
                names.join(", ")
            )));
        };
        out.push_str(&format!("{pad}{line}\n"));
        for (r, n) in regs.into_iter().zip(names) {
            reg[r] = Some(Expr::Param(n));
        }
        Ok(())
    }

    /// The name of the declared local that instruction `i` at `at` writes, when it writes one.
    fn assigned_local(
        &self,
        f: &Function,
        reg: &[Option<Expr>],
        at: usize,
        i: &Instr,
    ) -> Result<Option<String>, Unsupported> {
        let a = i.a as usize;
        if i.name() == "LOADNIL" {
            let active = active_at(f, at);
            if (a..=i.b as usize).any(|r| active.get(r).is_some_and(|&li| !hidden(&f.locals[li]))) {
                return Err(Unsupported("several locals set at once".into()));
            }
            return Ok(None);
        }
        if !writes_value(i) {
            return Ok(None);
        }
        let active = active_at(f, at);
        let Some(&li) = active.get(a) else {
            return Ok(None);
        };
        if hidden(&f.locals[li]) {
            return Ok(None);
        }
        let name = (self.names)(&f.locals[li].name);
        if reg.get(a) != Some(&Some(Expr::Param(name.clone()))) {
            return Err(Unsupported("a local that was not declared".into()));
        }
        Ok(Some(name))
    }

    /// `name = value` for a declared local that was just written (its register holds the value).
    fn assign_local(
        &self,
        f: &Function,
        reg: &mut [Option<Expr>],
        a: usize,
        name: String,
        indent: usize,
        out: &mut String,
    ) -> Result<(), Unsupported> {
        let Some(v) = reg[a].clone() else {
            return Err(Unsupported("a local assigned an unknown value".into()));
        };
        if v == Expr::Param(name.clone()) {
            return Ok(());
        }
        // a register that still holds a value naming the variable would read the new value if it
        // were used later: it becomes unknown. Left-over temporaries are never read again; a real
        // swap (`a, b = b, a`) reads one, and the function then falls back to the listing.
        for (r, e) in reg.iter_mut().enumerate() {
            if r != a && e.as_ref().is_some_and(|e| *e != v && mentions(e, &name)) {
                *e = None;
            }
        }
        let pad = "\t".repeat(indent);
        out.push_str(&format!("{pad}{name} = {}\n", self.expr(f, &v, indent)));
        reg[a] = Some(Expr::Param(name));
        Ok(())
    }

    /// A comparison (or test) at `q` followed by its jump: the text of the condition that enters the
    /// body, the text of the condition under which the jump is taken, and the jump's target.
    fn cond_pair(
        &self,
        f: &Function,
        reg: &[Option<Expr>],
        q: usize,
        indent: usize,
    ) -> Result<Option<Pair>, Unsupported> {
        let (Some(&raw), Some(&jraw)) = (f.code.get(q), f.code.get(q + 1)) else {
            return Ok(None);
        };
        let i = Instr::decode(raw);
        let j = Instr::decode(jraw);
        if !matches!(i.name(), "EQ" | "LT" | "LE" | "TEST") || j.name() != "JMP" {
            return Ok(None);
        }
        let target = usize::try_from(q as i64 + 2 + j.sbx())
            .map_err(|_| Unsupported("a backward or far jump".into()))?;
        let (enter, taken) = if i.name() == "TEST" {
            let v = get(reg, i.a as usize)?;
            let text = self.operand(f, &v, indent);
            let (yes, no) = (text.clone(), format!("not {text}"));
            if i.c == 0 { (yes, no) } else { (no, yes) }
        } else {
            let (l, r) = (rk_of(f, reg, i.b)?, rk_of(f, reg, i.c)?);
            let (enter, taken) = match (i.name(), i.a) {
                ("EQ", 0) => ("==", "~="),
                ("EQ", _) => ("~=", "=="),
                ("LT", 0) => ("<", ">="),
                ("LT", _) => (">=", "<"),
                ("LE", 0) => ("<=", ">"),
                _ => (">", "<="),
            };
            let (l, r) = (self.operand(f, &l, indent), self.operand(f, &r, indent));
            (format!("{l} {enter} {r}"), format!("{l} {taken} {r}"))
        };
        Ok(Some(Pair {
            enter,
            taken,
            target,
            jmp: q + 1,
        }))
    }

    /// The targets of `x, y, z = f()`: after a CALL with `c - 1` results come `c - 1` stores, last
    /// result first, each into a global, an upvalue or a local variable. None when they do not
    /// have that shape.
    fn multi_targets(&self, f: &Function, pc: usize, a: usize, c: usize) -> Option<Vec<String>> {
        let live = active_at(f, pc).len();
        let mut targets = Vec::new();
        for k in 0..c - 1 {
            let n = Instr::decode(*f.code.get(pc + k)?);
            let src = a + (c - 2 - k);
            let name = match n.name() {
                "SETGLOBAL" if n.a as usize == src => match f.constants.get(n.bx() as usize)? {
                    Constant::Str(s) => (self.names)(s),
                    _ => return None,
                },
                "SETUPVAL" if n.a as usize == src => {
                    (self.names)(f.upvalue_names.get(n.b as usize)?)
                }
                "MOVE" if n.b as usize == src && (n.a as usize) < live => {
                    reg_name(self, f, pc + k, n.a as usize)?
                }
                _ => return None,
            };
            targets.push(name);
        }
        Some(targets)
    }

    /// A comparison as a value: `CMP; JMP -> L; LOADBOOL r 0 1; L: LOADBOOL r 1 0` is `r = (a == b)`.
    fn compare_value(
        &self,
        f: &Function,
        reg: &mut [Option<Expr>],
        at: usize,
        end: usize,
        indent: usize,
    ) -> Result<Option<usize>, Unsupported> {
        let at_ins = |q: usize| f.code.get(q).map(|r| Instr::decode(*r));
        let (Some(j), Some(no), Some(yes)) = (at_ins(at + 1), at_ins(at + 2), at_ins(at + 3))
        else {
            return Ok(None);
        };
        let shape = j.name() == "JMP"
            && at as i64 + 2 + j.sbx() == at as i64 + 3
            && no.name() == "LOADBOOL"
            && (no.b, no.c) == (0, 1)
            && yes.name() == "LOADBOOL"
            && (yes.b, yes.c) == (1, 0)
            && yes.a == no.a;
        if !shape || at + 4 > end || (yes.a as usize) >= reg.len() {
            return Ok(None);
        }
        let Some(p) = self.cond_pair(f, reg, at, indent)? else {
            return Ok(None);
        };
        reg[yes.a as usize] = Some(Expr::Text(p.taken));
        Ok(Some(at + 4))
    }

    /// `x or y` / `x and y` as a value: `TEST a; JMP over; <y into a>`. The value of `y` is
    /// computed by temporaries only (no statements); the register then holds the combined value.
    /// Returns where the code goes on, or None when this test is not that shape.
    fn value_test(
        &self,
        f: &Function,
        reg: &mut [Option<Expr>],
        top: usize,
        at: usize,
        end: usize,
        indent: usize,
    ) -> Result<Option<usize>, Unsupported> {
        let i = Instr::decode(f.code[at]);
        let Some(j) = f
            .code
            .get(at + 1)
            .map(|r| Instr::decode(*r))
            .filter(|j| i.name() == "TEST" && j.name() == "JMP")
        else {
            return Ok(None);
        };
        let Ok(t) = usize::try_from(at as i64 + 2 + j.sbx()) else {
            return Ok(None);
        };
        let a = i.a as usize;
        if t <= at + 2 || t > end || a >= reg.len() || !(at + 2..t).all(|q| temp_write(f, q)) {
            return Ok(None);
        }
        if Instr::decode(f.code[t - 1]).a as usize != a || jumped_into(f, at + 2, t) {
            return Ok(None);
        }
        let mut cs = State {
            reg: reg.to_vec(),
            top,
        };
        if !self
            .block(f, &mut cs, at + 2, t, indent, Exits::default())?
            .is_empty()
        {
            return Ok(None);
        }
        let (Some(first), Some(second)) = (reg[a].clone(), cs.reg[a].clone()) else {
            return Ok(None);
        };
        let op = if i.c == 1 { "or" } else { "and" };
        reg[a] = Some(Expr::Bin(op, Box::new(first), Box::new(second)));
        let n = reg.len();
        reg[a + 1..].clone_from_slice(&cs.reg[a + 1..n]);
        Ok(Some(t))
    }

    /// Lua 5.0's `while`: a jump over the body to the condition, which sits after the body and
    /// jumps back to the body while it holds. `at` is the first jump, `t` the condition.
    #[allow(clippy::too_many_arguments)]
    fn inverted_while(
        &self,
        f: &Function,
        reg: &mut [Option<Expr>],
        top: usize,
        at: usize,
        t: usize,
        end: usize,
        indent: usize,
    ) -> Result<(String, usize), Unsupported> {
        let body_start = at + 1;
        let mut cs = State {
            reg: reg.to_vec(),
            top,
        };
        let mut pairs: Vec<Pair> = Vec::new();
        let mut q = t;
        loop {
            let mut r = q;
            while r < end && temp_write(f, r) {
                r += 1;
            }
            if r + 1 >= end || pairs.len() > 8 {
                return Err(Unsupported("a loop condition of an unknown shape".into()));
            }
            let pre = self.block(f, &mut cs, q, r, indent, Exits::default())?;
            let p = self
                .cond_pair(f, &cs.reg, r, indent)?
                .ok_or_else(|| Unsupported("a loop condition of an unknown shape".into()))?;
            if !pre.is_empty() {
                return Err(Unsupported("a loop condition of an unknown shape".into()));
            }
            q = p.jmp + 1;
            let last = p.target == body_start;
            pairs.push(p);
            if last {
                break;
            }
        }
        let exit = q;
        let n = pairs.len();
        if exit > end
            || !pairs[..n - 1]
                .iter()
                .all(|p| p.target == exit || p.target == body_start)
        {
            return Err(Unsupported("a loop condition of an unknown shape".into()));
        }
        let mut cond = pairs[n - 1].taken.clone();
        let mut bare_or = false;
        for p in pairs[..n - 1].iter().rev() {
            if p.target == exit {
                let rest = if bare_or { format!("({cond})") } else { cond };
                cond = format!("{} and {rest}", p.enter);
                bare_or = false;
            } else {
                cond = format!("{} or {cond}", p.taken);
                bare_or = true;
            }
        }
        let mut inner = State {
            reg: reg.to_vec(),
            top,
        };
        let body = self.block(
            f,
            &mut inner,
            body_start,
            t,
            indent + 1,
            Exits {
                brk: Some(exit),
                tail: None,
            },
        )?;
        let pad = "\t".repeat(indent);
        forget_changed(reg, &[&inner, &cs], 0..0);
        Ok((format!("{pad}while {cond} do\n{body}{pad}end\n"), exit))
    }

    /// The instructions `start..end` as statements. Comparisons become `if` (with `else`, `and`,
    /// `or`) or `while`; loops become `for`; locals come from the debug information. A shape that
    /// is not understood makes the whole function fall back to the listing.
    fn block(
        &self,
        f: &Function,
        st: &mut State,
        start: usize,
        end: usize,
        indent: usize,
        exits: Exits,
    ) -> Result<String, Unsupported> {
        if self.block_depth.get() >= MAX_BLOCK_DEPTH {
            return Err(Unsupported("blocks nested too deeply".into()));
        }
        self.block_depth.set(self.block_depth.get() + 1);
        let _guard = BlockGuard(&self.block_depth);
        let Exits { brk, tail } = exits;
        let pad = "\t".repeat(indent);
        let State { reg, top } = st;
        let mut out = String::new();
        let mut pc = start;
        while pc < end {
            self.declare_locals(f, reg, pc, indent, &mut out)?;
            let at = pc;
            let i = Instr::decode(f.code[pc]);
            pc += 1;
            let (a, b, c) = (i.a as usize, i.b as usize, i.c as usize);
            if a >= reg.len() && !matches!(i.name(), "JMP") {
                return Err(Unsupported("register out of range".into()));
            }
            let assigned = self.assigned_local(f, reg, at, &i)?;
            let rk = |reg: &[Option<Expr>], v: u32| rk_of(f, reg, v);
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
                    if a >= active_at(f, at).len() {
                        reg[a] = None;
                    }
                }
                "SETUPVAL" => {
                    let n = f
                        .upvalue_names
                        .get(b)
                        .map_or_else(|| format!("upvalue{b}"), |s| (self.names)(s));
                    let v = get(reg, a)?;
                    out.push_str(&format!("{pad}{n} = {}\n", self.expr(f, &v, indent)));
                    if a >= active_at(f, at).len() {
                        reg[a] = None;
                    }
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
                            // the operands were used up
                            let live = active_at(f, at).len();
                            for r in [b, c] {
                                if r < rk_base(f) as usize && r >= live {
                                    reg[r] = None;
                                }
                            }
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
                    let multi = if c > 2 && pc + c - 1 <= end {
                        self.multi_targets(f, pc, a, c)
                    } else {
                        None
                    };
                    if i.name() == "TAILCALL" {
                        out.push_str(&format!("{pad}return {}\n", self.expr(f, &call, indent)));
                        // a RETURN follows a TAILCALL; it carries no value of its own
                        pc += 1;
                    } else if c == 1 {
                        out.push_str(&format!("{pad}{}\n", self.expr(f, &call, indent)));
                        // the call's registers are free again
                        for r in reg.iter_mut().skip(a) {
                            *r = None;
                        }
                    } else if let Some(mut targets) = multi {
                        // `x, y = f()`: the results are stored last to first
                        targets.reverse();
                        out.push_str(&format!(
                            "{pad}{} = {}\n",
                            targets.join(", "),
                            self.expr(f, &call, indent)
                        ));
                        for r in reg.iter_mut().skip(a) {
                            *r = None;
                        }
                        pc += c - 1;
                    } else {
                        reg[a] = Some(call);
                        // the arguments are used up and further results are not known as expressions
                        for r in reg.iter_mut().skip(a + 1) {
                            *r = None;
                        }
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
                "JMP" => {
                    let target = usize::try_from(at as i64 + 1 + i.sbx()).ok();
                    if target == Some(pc) {
                        // a jump to the next instruction does nothing
                    } else if target == Some(end)
                        && (pc..end).all(|q| Instr::decode(f.code[q]).name() == "JMP")
                        && !jumped_into(f, pc, end)
                    {
                        // the jump to the block's end; only unreachable jumps (a threaded `else
                        // break`) lie between
                        pc = end;
                    } else if target.is_some_and(|t| is_break_target(f, brk, t)) {
                        out.push_str(&format!("{pad}break\n"));
                    } else if f.orig_code.is_some()
                        && target.is_some_and(|t| {
                            t > at && t < end && Instr::decode(f.code[t]).name() != "TFORLOOP"
                        })
                    {
                        // Lua 5.0 writes `while` with the condition at the bottom
                        let t = target.unwrap_or(end);
                        let (text, next) = self.inverted_while(f, reg, *top, at, t, end, indent)?;
                        out.push_str(&text);
                        pc = next;
                    } else if let Some(t) = target.filter(|&t| t > at && t + 2 <= end) {
                        // `for ... in ...`: a jump to the TFORLOOP, whose own jump comes back here
                        let tf = Instr::decode(f.code[t]);
                        let back = Instr::decode(f.code[t + 1]);
                        if tf.name() != "TFORLOOP"
                            || back.name() != "JMP"
                            || t as i64 + 2 + back.sbx() != pc as i64
                        {
                            return Err(Unsupported("JMP (control flow)".into()));
                        }
                        // 5.1: the loop variables follow a hidden control register (A + 3); in 5.0
                        // the first one is the control (A + 2) and C counts the others
                        let ga = tf.a as usize;
                        let (vbase, nv) = if f.orig_code.is_some() {
                            (ga + 2, tf.c as usize + 1)
                        } else {
                            (ga + 3, tf.c as usize)
                        };
                        if vbase + nv > reg.len() {
                            return Err(Unsupported("register out of range".into()));
                        }
                        let first = get(reg, ga)?;
                        let mut list = vec![self.expr(f, &first, indent)];
                        if reg[ga + 1].is_some() {
                            let mut more = vec![get(reg, ga + 1)?];
                            if let Some(e) = reg[ga + 2].clone() {
                                more.push(e);
                            }
                            while more.last() == Some(&Expr::Nil) {
                                more.pop();
                            }
                            list.extend(more.iter().map(|e| self.expr(f, e, indent)));
                        }
                        let mut inner = State {
                            reg: reg.clone(),
                            top: *top,
                        };
                        let mut vars = Vec::new();
                        for k in 0..nv {
                            let name = reg_name(self, f, pc, vbase + k).unwrap_or_else(|| {
                                ["k", "v"]
                                    .get(k)
                                    .map_or_else(|| format!("v{k}"), |s| (*s).into())
                            });
                            inner.reg[vbase + k] = Some(Expr::Param(name.clone()));
                            vars.push(name);
                        }
                        let body = self.block(
                            f,
                            &mut inner,
                            pc,
                            t,
                            indent + 1,
                            Exits {
                                brk: Some(t + 2),
                                tail: None,
                            },
                        )?;
                        out.push_str(&format!(
                            "{pad}for {} in {} do\n{body}{pad}end\n",
                            vars.join(", "),
                            list.join(", ")
                        ));
                        forget_changed(reg, &[&inner], ga..vbase + nv);
                        pc = t + 2;
                    } else {
                        return Err(Unsupported("JMP (control flow)".into()));
                    }
                }
                "FORPREP" => {
                    let fl = usize::try_from(at as i64 + 1 + i.sbx())
                        .ok()
                        .filter(|&t| t > at && t < end)
                        .ok_or_else(|| Unsupported("a loop that leaves its block".into()))?;
                    let tail = Instr::decode(f.code[fl]);
                    if tail.name() != "FORLOOP" || tail.a != i.a || a + 3 >= reg.len() {
                        return Err(Unsupported("a loop of an unknown shape".into()));
                    }
                    let (init, limit, step) = (get(reg, a)?, get(reg, a + 1)?, get(reg, a + 2)?);
                    let var = reg_name(self, f, pc, a + 3).unwrap_or_else(|| "i".into());
                    let mut head = format!(
                        "for {var} = {}, {}",
                        self.expr(f, &init, indent),
                        self.expr(f, &limit, indent)
                    );
                    if step != Expr::Num(1.0) {
                        head.push_str(&format!(", {}", self.expr(f, &step, indent)));
                    }
                    let mut inner = State {
                        reg: reg.clone(),
                        top: *top,
                    };
                    inner.reg[a + 3] = Some(Expr::Param(var));
                    let body = self.block(
                        f,
                        &mut inner,
                        pc,
                        fl,
                        indent + 1,
                        Exits {
                            brk: Some(fl + 1),
                            tail: None,
                        },
                    )?;
                    out.push_str(&format!("{pad}{head} do\n{body}{pad}end\n"));
                    forget_changed(reg, &[&inner], a..a + 4);
                    pc = fl + 1;
                }
                "EQ" | "LT" | "LE" | "TEST" => 'cmp: {
                    if let Some(next) = self.compare_value(f, reg, at, end, indent)? {
                        pc = next;
                        break 'cmp;
                    }
                    if let Some(next) = self.value_test(f, reg, *top, at, end, indent)? {
                        pc = next;
                        break 'cmp;
                    }
                    let first_cmp = at;
                    // a jump to the end of the whole `if ... else` leaves this block like its end does
                    let norm = |mut p: Pair| {
                        if Some(p.target) == tail && (p.target > end || p.target <= p.jmp) {
                            p.target = end;
                        }
                        p
                    };
                    let first = norm(
                        self.cond_pair(f, reg, first_cmp, indent)?
                            .ok_or_else(|| Unsupported("a comparison without a jump".into()))?,
                    );
                    // consecutive comparisons may be one condition joined by `and` / `or`
                    let mut pairs = vec![first];
                    let mut cs = State {
                        reg: reg.clone(),
                        top: *top,
                    };
                    let mut q = pairs[0].jmp + 1;
                    loop {
                        let mut r = q;
                        while r < end && temp_write(f, r) {
                            r += 1;
                        }
                        if r + 1 >= end || has_local_start(f, q, r + 1) {
                            break;
                        }
                        self.block(f, &mut cs, q, r, indent, Exits { brk, tail: None })?;
                        let Some(p) = self.cond_pair(f, &cs.reg, r, indent)?.map(norm) else {
                            break;
                        };
                        q = p.jmp + 1;
                        pairs.push(p);
                    }
                    // `if cond then break end` is one conditional jump straight to the loop's exit
                    if let Some(b) = brk.filter(|&b| is_break_target(f, Some(b), pairs[0].target)) {
                        let k = pairs
                            .iter()
                            .take_while(|p| is_break_target(f, Some(b), p.target))
                            .count();
                        let cond: Vec<&str> = pairs[..k].iter().map(|p| p.taken.as_str()).collect();
                        out.push_str(&format!(
                            "{pad}if {} then\n{pad}\tbreak\n{pad}end\n",
                            cond.join(" or ")
                        ));
                        pc = pairs[k - 1].jmp + 1;
                        break 'cmp;
                    }
                    let mut m = 1;
                    for k in (1..=pairs.len()).rev() {
                        let (end_k, body_k) = (pairs[k - 1].target, pairs[k - 1].jmp + 1);
                        if end_k > pairs[k - 1].jmp
                            && end_k <= end
                            && pairs[..k - 1]
                                .iter()
                                .all(|p| p.target == end_k || p.target == body_k)
                        {
                            m = k;
                            break;
                        }
                    }
                    pairs.truncate(m);
                    let last = &pairs[m - 1];
                    let (jmp_pc, target) = (last.jmp, last.target);
                    if target <= jmp_pc || target > end {
                        return Err(Unsupported("a backward or far jump".into()));
                    }
                    let mut cond = last.enter.clone();
                    let mut conj = false; // the text is a bare `or` chain
                    for p in pairs[..m - 1].iter().rev() {
                        if p.target == target {
                            let rest = if conj { format!("({cond})") } else { cond };
                            cond = format!("{} and {rest}", p.enter);
                            conj = false;
                        } else {
                            cond = format!("{} or {cond}", p.taken);
                            conj = true;
                        }
                    }
                    // `while`: the body ends in a jump back to where the condition is computed
                    let loop_head = (target - 1 > jmp_pc)
                        .then(|| Instr::decode(f.code[target - 1]))
                        .filter(|e| e.name() == "JMP")
                        .and_then(|e| usize::try_from(target as i64 + e.sbx()).ok())
                        .filter(|&tb| {
                            tb >= start
                                && tb <= first_cmp
                                && (tb..first_cmp).all(|q| temp_write(f, q))
                        });
                    let looped = loop_head.is_some();
                    // `if ... else ...`: the body ends in a jump over the else part
                    let else_jump = (!looped && target - 1 > jmp_pc)
                        .then(|| Instr::decode(f.code[target - 1]))
                        .filter(|e| e.name() == "JMP")
                        .and_then(|e| usize::try_from(target as i64 + e.sbx()).ok())
                        .filter(|&u| {
                            u > target
                                && (u <= end || Some(u) == tail)
                                && !is_break_target(f, brk, u)
                        });
                    let then_end = if looped || else_jump.is_some() {
                        target - 1
                    } else {
                        target
                    };
                    let inner_brk = if looped { Some(target) } else { brk };
                    // a jump to the end of the whole `if ... else` leaves the body like falling off it;
                    // a body that ends where this block ends also leaves the way this block does
                    let then_tail = if else_jump.is_some() {
                        else_jump
                    } else if looped {
                        // an `if` at the end of the body may jump straight back to the head
                        loop_head
                    } else if target == end {
                        tail
                    } else {
                        None
                    };
                    // every jump inside the body must stay inside it (or leave the loop), or the
                    // shape is not a plain `if`
                    for (q, raw) in f.code.iter().enumerate().take(then_end).skip(jmp_pc + 1) {
                        let ins = Instr::decode(*raw);
                        if matches!(ins.name(), "JMP" | "FORLOOP" | "FORPREP") {
                            let t = q as i64 + 1 + ins.sbx();
                            let leaves = ins.name() == "JMP"
                                && (is_break_target(f, inner_brk, t as usize)
                                    || Some(t as usize) == then_tail);
                            if !leaves && (t < jmp_pc as i64 + 1 || t > then_end as i64) {
                                return Err(Unsupported("a complex condition or loop".into()));
                            }
                        }
                    }
                    let mut then_state = State {
                        reg: reg.clone(),
                        top: *top,
                    };
                    let then_text = self.block(
                        f,
                        &mut then_state,
                        jmp_pc + 1,
                        then_end,
                        indent + 1,
                        Exits {
                            brk: inner_brk,
                            tail: then_tail,
                        },
                    )?;
                    let mut states = vec![then_state];
                    let head = if looped {
                        format!("while {cond} do")
                    } else {
                        format!("if {cond} then")
                    };
                    out.push_str(&format!("{pad}{head}\n{then_text}"));
                    if let Some(u) = else_jump {
                        let mut else_state = State {
                            reg: reg.clone(),
                            top: *top,
                        };
                        // an else part that reaches the end of this block is left the same way
                        let else_end = u.min(end);
                        let else_tail = (else_end == end).then_some(tail).flatten();
                        let else_text = self.block(
                            f,
                            &mut else_state,
                            target,
                            else_end,
                            indent + 1,
                            Exits {
                                brk,
                                tail: else_tail,
                            },
                        )?;
                        out.push_str(&format!("{pad}else\n{else_text}"));
                        states.push(else_state);
                    }
                    out.push_str(&format!("{pad}end\n"));
                    // what a branch assigned to a register is unknown after the join
                    let refs: Vec<&State> = states.iter().collect();
                    forget_changed(reg, &refs, 0..0);
                    pc = else_jump.map_or(target, |u| u.min(end));
                }
                "CLOSE" => {}
                other => return Err(Unsupported(format!("{other} (control flow)"))),
            }
            // operands that were temporaries are used up by the instruction that reads them
            let operands: Vec<usize> = match i.name() {
                "CONCAT" => (b..=c).collect(),
                "ADD" | "SUB" | "MUL" | "DIV" | "MOD" | "POW" | "GETTABLE" => [i.b, i.c]
                    .into_iter()
                    .filter(|&r| r < rk_base(f))
                    .map(|r| r as usize)
                    .collect(),
                "UNM" | "NOT" | "LEN" => vec![b],
                _ => Vec::new(),
            };
            if !operands.is_empty() {
                let live = active_at(f, at).len();
                for r in operands {
                    if r != a && r >= live && r < reg.len() {
                        reg[r] = None;
                    }
                }
            }
            if a < reg.len() && reg[a].as_ref().is_some_and(too_deep) {
                return Err(Unsupported("an expression nested too deeply".into()));
            }
            if let Some(name) = assigned {
                self.assign_local(f, reg, a, name, indent, &mut out)?;
            }
        }
        Ok(out)
    }
}

/// Where a jump out of a block can land: `brk` is the exit of the innermost loop (a `break`), `tail`
/// a place that is equivalent to falling off the end of the block (the compiler merges jump chains,
/// so the exit of an inner `if` can point past the end of the outer `if ... else`).
#[derive(Clone, Copy, Default)]
struct Exits {
    brk: Option<usize>,
    tail: Option<usize>,
}

/// True when a jump to `t` leaves the innermost loop: `t` is its exit, or the exit instruction is
/// itself a jump to `t` (the compiler sends a `break` straight to where the exit jump goes).
fn is_break_target(f: &Function, brk: Option<usize>, t: usize) -> bool {
    let Some(b) = brk else { return false };
    if b == t {
        return true;
    }
    f.code
        .get(b)
        .map(|r| Instr::decode(*r))
        .is_some_and(|j| j.name() == "JMP" && b as i64 + 1 + j.sbx() == t as i64)
}

/// One comparison with its jump, see [`Ctx::cond_pair`].
struct Pair {
    enter: String,
    taken: String,
    target: usize,
    jmp: usize,
}

fn hidden(l: &Local) -> bool {
    l.name.first() == Some(&b'(')
}

/// The locals that are in scope at `pc`, in declaration order (the register of each is its position).
fn active_at(f: &Function, pc: usize) -> Vec<usize> {
    f.locals
        .iter()
        .enumerate()
        .filter(|(_, l)| (l.start as usize) <= pc && pc < l.end as usize)
        .map(|(i, _)| i)
        .collect()
}

/// The name of the (visible) local that lives in register `r` at `pc`.
fn reg_name(ctx: &Ctx<'_>, f: &Function, pc: usize, r: usize) -> Option<String> {
    let li = *active_at(f, pc).get(r)?;
    (!hidden(&f.locals[li])).then(|| (ctx.names)(&f.locals[li].name))
}

fn has_local_start(f: &Function, from: usize, to: usize) -> bool {
    f.locals
        .iter()
        .any(|l| !hidden(l) && (from..=to).contains(&(l.start as usize)))
}

/// Instructions that put a value in register A.
fn writes_value(i: &Instr) -> bool {
    match i.name() {
        "MOVE" | "LOADK" | "LOADNIL" | "GETUPVAL" | "GETGLOBAL" | "GETTABLE" | "NEWTABLE"
        | "ADD" | "SUB" | "MUL" | "DIV" | "MOD" | "POW" | "UNM" | "NOT" | "LEN" | "CONCAT"
        | "CLOSURE" => true,
        "LOADBOOL" => i.c == 0,
        "CALL" => i.c == 2,
        _ => false,
    }
}

/// A value-producing instruction that writes a temporary register (not a local), so a run of them
/// between two comparisons is the second comparison's operand code.
fn temp_write(f: &Function, q: usize) -> bool {
    let i = Instr::decode(f.code[q]);
    let value = writes_value(&i) || (i.name() == "CALL" && i.c != 1);
    value && (i.a as usize) >= active_at(f, q).len()
}

/// True when some jump lands on an instruction in `from..to`.
fn jumped_into(f: &Function, from: usize, to: usize) -> bool {
    f.code.iter().enumerate().any(|(q, raw)| {
        let i = Instr::decode(*raw);
        matches!(i.name(), "JMP" | "FORLOOP" | "FORPREP") && {
            let t = q as i64 + 1 + i.sbx();
            t >= from as i64 && t < to as i64
        }
    })
}

fn mentions(e: &Expr, name: &str) -> bool {
    match e {
        Expr::Param(n) => n == name,
        Expr::Index(a, b) | Expr::Bin(_, a, b) => mentions(a, name) || mentions(b, name),
        Expr::Method(o, _) => mentions(o, name),
        Expr::Call(g, args) => mentions(g, name) || args.iter().any(|a| mentions(a, name)),
        Expr::Table(entries) => entries.iter().any(|en| match en {
            Entry::Item(v) => mentions(v, name),
            Entry::Keyed(k, v) => mentions(k, name) || mentions(v, name),
        }),
        Expr::Un(_, x) => mentions(x, name),
        Expr::Concat(parts) => parts.iter().any(|p| mentions(p, name)),
        _ => false,
    }
}

/// After a branch or loop: a register that any inner state changed (or one in `also`) is unknown.
fn forget_changed(reg: &mut [Option<Expr>], inner: &[&State], also: std::ops::Range<usize>) {
    for r in 0..reg.len() {
        if also.contains(&r) || inner.iter().any(|s| s.reg[r] != reg[r]) {
            reg[r] = None;
        }
    }
}

/// The first operand value that names a constant: 256 in Lua 5.1, 250 in 5.0.
pub fn rk_base(f: &Function) -> u32 {
    if f.orig_code.is_some() {
        crate::luac50::RK_BASE
    } else {
        256
    }
}

fn rk_of(f: &Function, reg: &[Option<Expr>], v: u32) -> Result<Expr, Unsupported> {
    if v >= rk_base(f) {
        konst(f, (v - rk_base(f)) as usize)
    } else {
        get(reg, v as usize)
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
    let ctx = Ctx {
        names,
        depth: std::cell::Cell::new(0),
        block_depth: std::cell::Cell::new(0),
    };
    let version = if chunk.main.orig_code.is_some() {
        "5.0"
    } else {
        "5.1"
    };
    // The compiler (`luacomp::compile_text`) reads this mark to write the strings back as UTF-8;
    // every other file is written back as CP949 (measured: no file of the real archives mixes both).
    let utf8 = if has_utf8_strings(&chunk.main) {
        "; strings: UTF-8"
    } else {
        ""
    };
    let mut out = format!("-- decompiled by RustyGRF from Lua {version} bytecode{utf8}\n");
    out.push_str(&ctx.body(&chunk.main, 0));
    out
}

/// Whether some string constant is shown as UTF-8 (see [`string_text`]).
fn has_utf8_strings(f: &Function) -> bool {
    f.constants.iter().any(|k| {
        matches!(k, Constant::Str(s) if !s.is_ascii() && std::str::from_utf8(s)
            .is_ok_and(|t| t.chars().all(|c| c.is_ascii() || c as u32 >= 0x800)))
    }) || f.protos.iter().any(has_utf8_strings)
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
            orig_code: None,
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

    fn jmp(sbx: i64) -> u32 {
        insx(22, 0, (sbx + 131_071) as u32)
    }

    fn sbx_op(op: u32, a: u32, sbx: i64) -> u32 {
        insx(op, a, (sbx + 131_071) as u32)
    }

    fn decompiled(code: Vec<u32>, consts: Vec<Constant>, locals: Vec<(&str, u32, u32)>) -> String {
        let mut f = func(code, consts, vec![]);
        f.locals = locals
            .into_iter()
            .map(|(n, start, end)| Local {
                name: n.as_bytes().to_vec(),
                start,
                end,
            })
            .collect();
        decompile(&Chunk { main: f }, &text)
    }

    // opcodes: MOVE 0, ADD 12, JMP 22, EQ 23, TEST 26, FORLOOP 31, FORPREP 32, TFORLOOP 33
    #[test]
    fn a_numeric_for_loop_and_a_generic_for_loop() {
        let consts = vec![Constant::Number(1.0), Constant::Number(3.0), s("print")];
        let out = decompiled(
            vec![
                insx(1, 0, 0),
                insx(1, 1, 1),
                insx(1, 2, 0),
                sbx_op(32, 0, 3), // FORPREP -> FORLOOP at 7
                insx(5, 4, 2),
                ins(0, 5, 3, 0),
                ins(28, 4, 2, 1),
                sbx_op(31, 0, -4),
                ins(30, 0, 1, 0),
            ],
            consts,
            vec![],
        );
        assert!(is_complete(&out), "{out}");
        assert!(out.contains("for i = 1, 3 do\n\tprint(i)\nend"), "{out}");

        let consts = vec![s("pairs"), s("t"), s("print")];
        let out = decompiled(
            vec![
                insx(5, 0, 0),
                insx(5, 1, 1),
                ins(28, 0, 2, 4), // R0, R1, R2 = pairs(t)
                jmp(4),           // to the TFORLOOP at 8
                insx(5, 5, 2),
                ins(0, 6, 3, 0),
                ins(0, 7, 4, 0),
                ins(28, 5, 3, 1),
                ins(33, 0, 0, 2), // TFORLOOP R0, two variables
                jmp(-6),          // back to the body at 4
                ins(30, 0, 1, 0),
            ],
            consts,
            vec![],
        );
        assert!(is_complete(&out), "{out}");
        assert!(
            out.contains("for k, v in pairs(t) do\n\tprint(k, v)\nend"),
            "{out}"
        );
    }

    #[test]
    fn conditions_joined_by_or_and_and() {
        let consts = vec![s("a"), s("b"), s("print"), Constant::Number(1.0)];
        let body = |end: usize| {
            let _ = end;
            vec![
                insx(5, 0, 2),
                insx(1, 1, 3),
                ins(28, 0, 2, 1),
                ins(30, 0, 1, 0),
            ]
        };
        // if a or b then print(1) end
        let mut code = vec![
            insx(5, 0, 0),
            ins(26, 0, 0, 1), // TEST R0, taken when a is true
            jmp(3),           // into the body
            insx(5, 0, 1),
            ins(26, 0, 0, 0), // TEST R0, taken when b is false
            jmp(3),           // past the body
        ];
        code.extend(body(9));
        let out = decompiled(code, consts.clone(), vec![]);
        assert!(is_complete(&out), "{out}");
        assert!(out.contains("if a or b then\n\tprint(1)\nend"), "{out}");
        // if a and b then print(1) end
        let mut code = vec![
            insx(5, 0, 0),
            ins(26, 0, 0, 0),
            jmp(6),
            insx(5, 0, 1),
            ins(26, 0, 0, 0),
            jmp(3),
        ];
        code.extend(body(9));
        let out = decompiled(code, consts, vec![]);
        assert!(is_complete(&out), "{out}");
        assert!(out.contains("if a and b then\n\tprint(1)\nend"), "{out}");
    }

    #[test]
    fn a_while_loop_with_break_and_named_locals() {
        // local n = 0; while n < 10 do n = n + 1; if n == 5 then break end end
        let consts = vec![
            Constant::Number(0.0),
            Constant::Number(10.0),
            Constant::Number(1.0),
            Constant::Number(5.0),
        ];
        let code = vec![
            insx(1, 0, 0),      // 0 LOADK R0 0 (the local n starts at 1)
            ins(24, 0, 0, 257), // 1 LT 0 R0 K1: the body runs while n < 10
            jmp(5),             // 2 -> 8 (exit)
            ins(12, 0, 0, 258), // 3 ADD R0 R0 K2
            ins(23, 0, 0, 259), // 4 EQ 0 R0 K3: the body runs when n == 5
            jmp(1),             // 5 -> 7 (skip the break)
            jmp(1),             // 6 break -> 8
            jmp(-7),            // 7 back to 1
            ins(30, 0, 1, 0),   // 8
        ];
        let out = decompiled(code, consts, vec![("n", 1, 8)]);
        assert!(is_complete(&out), "{out}");
        let expected =
            "local n = 0\nwhile n < 10 do\n\tn = n + 1\n\tif n == 5 then\n\t\tbreak\n\tend\nend\n";
        assert!(out.ends_with(expected), "{out}");
    }

    #[test]
    fn a_local_without_a_value_stays_a_local_and_a_function_start_is_nil() {
        // `local w; w = 1`: the compiler leaves out LOADNIL at the start of a function, so the
        // local starts at instruction 0 and must still be declared (else `w = 1` would be a global)
        let out = decompiled(
            vec![insx(1, 0, 0), ins(30, 0, 1, 0)],
            vec![Constant::Number(1.0)],
            vec![("w", 0, 2)],
        );
        assert!(is_complete(&out), "{out}");
        assert!(out.contains("local w = nil\nw = 1"), "{out}");
        // `x = nil` as the first statement: SETGLOBAL of an untouched register
        let out = decompiled(vec![insx(7, 0, 0), ins(30, 0, 1, 0)], vec![s("x")], vec![]);
        assert!(is_complete(&out), "{out}");
        assert!(out.contains("x = nil"), "{out}");
    }

    #[test]
    fn lua_50_while_has_its_condition_at_the_bottom() {
        // while x do print("hi") end, as Lua 5.0 writes it: a jump to the test, the body, the test
        // that jumps back while it holds (instructions are in the 5.1 form `to_lua51` makes)
        let code = vec![
            jmp(3),           // 0 -> 4
            insx(5, 0, 0),    // 1 print
            insx(1, 1, 1),    // 2 "hi"
            ins(28, 0, 2, 1), // 3 CALL
            insx(5, 0, 2),    // 4 x
            ins(26, 0, 0, 1), // 5 TEST R0, jump back when true
            jmp(-6),          // 6 -> 1
            ins(30, 0, 1, 0), // 7
        ];
        let mut f = func(code, vec![s("print"), s("hi"), s("x")], vec![]);
        f.orig_code = Some(vec![0; 8]);
        let out = decompile(&Chunk { main: f }, &text);
        assert!(is_complete(&out), "{out}");
        assert!(out.contains("Lua 5.0"), "{out}");
        assert!(out.ends_with("while x do\n\tprint(\"hi\")\nend\n"), "{out}");
    }

    #[test]
    fn or_and_comparisons_as_values() {
        // x = a or {}
        let out = decompiled(
            vec![
                insx(5, 0, 0),    // GETGLOBAL R0 "a"
                ins(26, 0, 0, 1), // TEST R0: keep it when true
                jmp(1),
                ins(10, 0, 0, 0), // NEWTABLE R0
                insx(7, 0, 1),    // SETGLOBAL R0 "x"
                ins(30, 0, 1, 0),
            ],
            vec![s("a"), s("x")],
            vec![],
        );
        assert!(is_complete(&out), "{out}");
        assert!(out.contains("x = a or {}"), "{out}");
        // x = (a == 1)
        let out = decompiled(
            vec![
                insx(5, 0, 0),
                ins(23, 1, 0, 257), // EQ 1 R0 K1
                jmp(1),
                ins(2, 1, 0, 1), // LOADBOOL R1 false, skip the next
                ins(2, 1, 1, 0), // LOADBOOL R1 true
                insx(7, 1, 2),
                ins(30, 0, 1, 0),
            ],
            vec![s("a"), Constant::Number(1.0), s("x")],
            vec![],
        );
        assert!(is_complete(&out), "{out}");
        assert!(out.contains("x = a == 1"), "{out}");
    }

    #[test]
    fn a_swap_of_locals_is_not_guessed() {
        // local a, b = 1, 2; a, b = b, a  (the temporary holds `b` while `b` is overwritten)
        let consts = vec![Constant::Number(1.0), Constant::Number(2.0)];
        let code = vec![
            insx(1, 0, 0),
            insx(1, 1, 1),
            ins(0, 2, 1, 0), // R2 = b
            ins(0, 1, 0, 0), // b = a   <- would make R2 read the new b
            ins(0, 0, 2, 0), // a = R2
            ins(30, 0, 1, 0),
        ];
        let out = decompiled(code, consts, vec![("a", 2, 6), ("b", 2, 6)]);
        assert!(!is_complete(&out), "{out}");
    }

    #[test]
    fn deeply_nested_functions_do_not_overflow_the_stack() {
        // f = function() f = function() ... end end, 190 levels (the reader allows 200)
        let mut inner = func(vec![ins(30, 0, 1, 0)], vec![], vec![]);
        for _ in 0..190 {
            inner = func(
                vec![insx(36, 0, 0), insx(7, 0, 0), ins(30, 0, 1, 0)],
                vec![s("f")],
                vec![inner],
            );
        }
        let out = std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(move || decompile(&Chunk { main: inner }, &text))
            .unwrap()
            .join()
            .expect("no stack overflow or panic");
        assert!(
            out.contains("f = function()"),
            "{}",
            &out[..200.min(out.len())]
        );
    }

    #[test]
    fn deeply_nested_blocks_and_or_chains_do_not_overflow_the_stack() {
        let run = |chunk: Chunk| {
            std::thread::Builder::new()
                .stack_size(2 << 20)
                .spawn(move || decompile(&chunk, &text))
                .unwrap()
                .join()
                .expect("no stack overflow or panic")
        };
        // 20000 `if a then ... end f()` inside each other (the statement after each inner `end`
        // keeps the ends apart): n x (GETGLOBAL, TEST, JMP), `f()`, then n x `f()`
        let n = 20_000usize;
        let mut code = Vec::new();
        for k in 0..n {
            let jmp_pc = 3 * k + 2;
            let target = 3 * n + 2 + 2 * (n - 1 - k);
            code.extend([
                insx(5, 0, 0),
                ins(26, 0, 0, 0),
                jmp(target as i64 - (jmp_pc as i64 + 1)),
            ]);
        }
        for _ in 0..=n {
            code.extend([insx(5, 0, 1), ins(28, 0, 1, 1)]);
        }
        code.push(ins(30, 0, 1, 0));
        let out = run(Chunk {
            main: func(code, vec![s("a"), s("f")], vec![]),
        });
        assert!(!is_complete(&out) || out.len() > 100);
        // `x = a or a or a ...` 20000 times: one TEST/JMP pair per link, all jumping to the end
        let mut code = vec![insx(5, 0, 0)];
        for k in 0..n {
            let to_end = (n - k - 1) * 3 + 1;
            code.extend([ins(26, 0, 0, 1), jmp(to_end as i64), insx(5, 0, 0)]);
        }
        code.extend([insx(7, 0, 0), ins(30, 0, 1, 0)]);
        let out = run(Chunk {
            main: func(code, vec![s("a")], vec![]),
        });
        assert!(!out.is_empty());
    }

    #[test]
    fn deeply_nested_expressions_fall_back_to_the_listing() {
        let run = |chunk: Chunk| {
            std::thread::Builder::new()
                .stack_size(2 << 20)
                .spawn(move || decompile(&chunk, &text))
                .unwrap()
                .join()
                .expect("no stack overflow or panic")
        };
        // x = 1 + 1 + 1 ... (5000 additions on one register)
        let mut code = vec![insx(1, 0, 0), insx(1, 1, 0)];
        code.extend((0..5000).map(|_| ins(12, 0, 0, 1)));
        code.extend([insx(7, 0, 1), ins(30, 0, 1, 0)]);
        let out = run(Chunk {
            main: func(code, vec![Constant::Number(1.0), s("x")], vec![]),
        });
        assert!(!is_complete(&out), "a chain this deep is not written");
        // tables wrapped into each other through two registers, 3000 levels
        let mut code = vec![ins(10, 0, 0, 0)];
        for _ in 0..1500 {
            code.extend([
                ins(10, 1, 0, 0),
                ins(9, 1, 256, 0),
                ins(10, 0, 0, 0),
                ins(9, 0, 256, 1),
            ]);
        }
        code.extend([insx(7, 0, 1), ins(30, 0, 1, 0)]);
        let out = run(Chunk {
            main: func(code, vec![Constant::Number(1.0), s("t")], vec![]),
        });
        assert!(!is_complete(&out));
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
