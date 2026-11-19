//! Checks the Lua decompiler of `rgrf-formats` against a real Lua 5.1 (mlua builds the C sources):
//! 1. the decompiled text of every compiled `.lub` (Lua 5.0 and 5.1) must be accepted by the real
//!    Lua parser;
//! 2. running the original 5.1 bytecode and the decompiled source in a sandbox must give the same
//!    outcome (what the script defined, and what its functions return for a few argument sets).
//!
//! Lua 5.0 files are checked the same way with a real Lua 5.0 (`fetch-lua50.sh` builds it into
//! ~/build/lua50; `$LUA50` points elsewhere); without it they only get the 5.1 parser check.
//!
//! usage: rgrf-lua-verify <data.grf> [--parse-only | --dump-only]   (exit code 1 when something differs)
//! env: LV_ONLY=<name part>, LV_TRACE, LV_SHOW (print outcomes), LV_MUTATE (self-test),
//!      LV_NOHOOK, LV_STEP=load|main
//!
//! The sandbox gives every undefined global a "proxy" value that can be indexed, called and used in
//! arithmetic (so scripts that expect other scripts' tables run on), neutralises io/os/package,
//! stops runaway loops after two million instructions, and serialises everything the script defined.
//! A development tool: it needs `unsafe` (debug library, binary chunks) and is not part of the
//! shipped programs.

use mlua::Lua;
use mlua::chunk::ChunkMode;
use rayon::prelude::*;
use rgrf_core::{Archive, NameMode};
use rgrf_formats::{luac, luac50, luacomp, luacomp50, luadec};

/// The sandbox, valid Lua 5.0 and 5.1 (no `#`, no string methods, no `...` in expressions:
/// `PACK` is filled in per version).
const DRIVER: &str = r##"
local pack = PACK
local find = string.find
local type, pairs, tostring, pcall, setmetatable, getmetatable, rawget, sort, concat, fmt =
  type, pairs, tostring, pcall, setmetatable, getmetatable, rawget, table.sort, table.concat, string.format
local sethook, error, ipairs = debug.sethook, error, ipairs
local P = {}
local function proxy() return P end
setmetatable(P, {
  __index = proxy, __call = proxy, __add = proxy, __sub = proxy, __mul = proxy, __div = proxy,
  __mod = proxy, __pow = proxy, __unm = proxy, __concat = proxy,
  __lt = function() return false end, __le = function() return true end,
})

local function ser(v, seen, depth)
  local t = type(v)
  if v == P then return "<P>" end
  if t == "number" then return fmt("%.14g", v) end
  if t == "string" then return fmt("%q", v) end
  if t == "function" then return "<fn>" end
  if t ~= "table" then return tostring(v) end
  if seen[v] or depth > 12 then return "<cycle>" end
  seen[v] = true
  local keys, n = {}, 0
  for k in pairs(v) do n = n + 1; keys[n] = k end
  local function kstr(k) return k == P and "<P>" or (type(k) == "table" or type(k) == "function") and "<" .. type(k) .. ">" or type(k) .. ":" .. tostring(k) end
  sort(keys, function(a, b) return kstr(a) < kstr(b) end)
  local parts = {}
  for i = 1, n do parts[i] = kstr(keys[i]) .. "=" .. ser(v[keys[i]], seen, depth + 1) end
  seen[v] = nil
  return "{" .. concat(parts, ",") .. "}"
end

function __rgrf_run(chunk)
  local nohook, noprobe = rawget(_G, "__rgrf_nohook"), rawget(_G, "__rgrf_noprobe")
  local std = {}
  for k in pairs(_G) do std[k] = true end
  std.__rgrf_run, std.__rgrf_nohook, std.__rgrf_noprobe = true, true, true
  local env = _G
  env.io, env.os, env.package, env.require, env.dofile, env.loadfile = P, P, P, P, P, P
  env.load, env.loadstring, env.print = P, P, function() end
  setmetatable(env, { __index = proxy })
  if not nohook then sethook(function() error("timeout", 0) end, "", 2000000) end
  local out, n = {}, 1
  local ok, err = pcall(chunk)
  sethook()
  out[1] = ok and "ran" or (find(tostring(err), "timeout", 1, true) and "timeout" or "error")
  local names, m = {}, 0
  for k, v in pairs(env) do
    if not std[k] then m = m + 1; names[m] = k end
  end
  sort(names, function(a, b) return tostring(a) < tostring(b) end)
  for _, k in ipairs(names) do
    n = n + 1
    out[n] = tostring(k) .. " = " .. ser(rawget(env, k), {}, 0)
  end
  if noprobe then return concat(out, "\n") end
  local patterns = { {}, { P, P, P, P }, { 1, 2, 3, 4 }, { "a", "b", "c", "d" }, { false, false, false, false } }
  for _, k in ipairs(names) do
    local f = rawget(env, k)
    if type(f) == "function" then
      for i, args in ipairs(patterns) do
        if not nohook then sethook(function() error("timeout", 0) end, "", 2000000) end
        local count, r = pack(pcall(f, args[1], args[2], args[3], args[4]))
        sethook()
        local text
        if r[1] then
          local parts = {}
          for i = 2, count do parts[i - 1] = ser(r[i], {}, 0) end
          text = "ok " .. concat(parts, ";")
        else
          text = find(tostring(r[2]), "timeout", 1, true) and "timeout" or "error"
        end
        n = n + 1
        out[n] = tostring(k) .. "#" .. i .. " -> " .. text
      end
    end
  end
  return concat(out, "\n")
end
"##;

const PACK51: &str = r##"function(...) return select("#", ...), { ... } end"##;
const PACK50: &str = "function(...) return arg.n, arg end";

/// Lua 5.0 runs as a separate program (a 64-bit `lua` built from lua.org's 5.0.3 sources, see
/// `fetch-lua50.sh`); its C symbols would clash with the 5.1 that mlua links in.
const DRIVER50_MAIN: &str = r#"
local write, stderr, exit, mode, path = io.write, io.stderr, os.exit, arg[1], arg[2]
local chunk, err = loadfile(path)
if not chunk then stderr:write(tostring(err)); exit(3) end
if mode == "parse" then exit(0) end
if mode == "dump" then write(string.dump(chunk)); exit(0) end
write(__rgrf_run(chunk))
"#;

enum Code<'a> {
    Binary(&'a [u8]),
    Text(&'a str),
}

/// Runs `code` in a fresh sandboxed Lua 5.1 and returns the serialised outcome.
fn run51(code: &Code) -> Result<String, String> {
    // SAFETY: a test-only Lua with the debug library (for the instruction-count hook) and binary
    // chunk loading; the sandbox neutralises everything that could touch the machine.
    let lua = unsafe { Lua::unsafe_new() };
    lua.load(DRIVER.replace("PACK", PACK51))
        .exec()
        .map_err(|e| e.to_string())?;
    let function = match code {
        Code::Binary(b) => lua.load(*b).set_mode(ChunkMode::Binary).into_function(),
        Code::Text(t) => lua
            .load(t.as_bytes())
            .set_mode(ChunkMode::Text)
            .into_function(),
    }
    .map_err(|e| e.to_string())?;
    if std::env::var_os("LV_NOHOOK").is_some() {
        lua.globals()
            .set("__rgrf_nohook", true)
            .map_err(|e| e.to_string())?;
    }
    match std::env::var("LV_STEP").as_deref() {
        Ok("load") => return Ok(String::new()),
        Ok("main") => lua
            .globals()
            .set("__rgrf_noprobe", true)
            .map_err(|e| e.to_string())?,
        _ => {}
    }
    let driver: mlua::Function = lua.globals().get("__rgrf_run").map_err(|e| e.to_string())?;
    driver
        .call::<mlua::LuaString>(function)
        .map(|s| outcome_text(code, &s.as_bytes()))
        .map_err(|e| e.to_string())
}

fn outcome_text(code: &Code, bytes: &[u8]) -> String {
    match code {
        // the client's scripts hold CP949 bytes; the decompiled text holds the same characters as UTF-8
        Code::Binary(_) => decode_segments(bytes),
        Code::Text(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// The Lua 5.0 program: `$LUA50`, else the build of `fetch-lua50.sh`.
fn lua50() -> Option<std::path::PathBuf> {
    let p = std::env::var_os("LUA50")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|h| std::path::Path::new(&h).join("build/lua50/lua-5.0.3/bin/lua"))
        })?;
    p.is_file().then_some(p)
}

/// Runs the 5.0 program on `code` (`mode`: "run", "parse" or "dump") and returns its output.
fn run50(lua: &std::path::Path, mode: &str, code: &Code) -> Result<Vec<u8>, String> {
    use std::io::Read;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("rgrf-lua-verify-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let (driver, chunk) = (dir.join(format!("d{n}.lua")), dir.join(format!("c{n}")));
    let mut prelude = DRIVER.replace("PACK", PACK50);
    if std::env::var_os("LV_NOHOOK").is_some() {
        prelude.insert_str(0, "__rgrf_nohook = true\n");
    }
    std::fs::write(&driver, prelude + DRIVER50_MAIN).map_err(|e| e.to_string())?;
    let bytes = match code {
        Code::Binary(b) => b.to_vec(),
        Code::Text(t) => t.as_bytes().to_vec(),
    };
    std::fs::write(&chunk, bytes).map_err(|e| e.to_string())?;
    let mut child = std::process::Command::new(lua)
        .arg(&driver)
        .arg(mode)
        .arg(&chunk)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let reader = std::thread::spawn(move || {
        let mut v = Vec::new();
        stdout.read_to_end(&mut v).map(|_| v)
    });
    let start = std::time::Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if start.elapsed() > std::time::Duration::from_secs(120) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Lua 5.0 did not finish within 120 s".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let out = reader
        .join()
        .map_err(|_| "reader thread failed")?
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&driver);
    let _ = std::fs::remove_file(&chunk);
    if status.success() {
        return Ok(out);
    }
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut err);
    }
    Err(format!("Lua 5.0 {status}: {}", err.trim()))
}

/// Runs `code` in the sandbox of the given version.
fn run(version: u8, code: &Code) -> Result<String, String> {
    if version == 0x50 {
        let lua = lua50().ok_or("no Lua 5.0 (see fetch-lua50.sh)")?;
        run50(&lua, "run", code).map(|b| outcome_text(code, &b))
    } else {
        run51(code)
    }
}

/// Decodes the output of the original bytecode the way the decompiler shows strings: text between
/// ASCII separators is UTF-8 when it is Hangul/CJK, else the archive's CP949/Latin-1 rule.
fn decode_segments(bytes: &[u8]) -> String {
    let names = |b: &[u8]| rgrf_core::GrfPath::new(b.to_vec()).display(NameMode::Cp949);
    let separator = |b: u8| b.is_ascii() && !b.is_ascii_alphanumeric() && b != b' ' && b != b'_';
    let mut out = String::new();
    for part in bytes.split_inclusive(|b| separator(*b)) {
        let (text, sep) = match part.split_last() {
            Some((last, rest)) if separator(*last) => (rest, Some(*last)),
            _ => (part, None),
        };
        out.push_str(&luadec::string_text(text, &names));
        out.extend(sep.map(char::from));
    }
    out
}

struct Entry {
    name: String,
    data: Vec<u8>,
}

fn compiled_files(grf: &str) -> Vec<Entry> {
    let arc = Archive::open(grf).unwrap_or_else(|e| {
        eprintln!("cannot open {grf}: {e}");
        std::process::exit(2);
    });
    let mut v = Vec::new();
    for i in arc.select(Some("*.lub"), NameMode::Cp1252) {
        let Ok(d) = arc.read(i) else { continue };
        if d.data.starts_with(b"\x1bLua") {
            v.push(Entry {
                name: arc.entries()[i].name.display(NameMode::Cp949),
                data: d.data,
            });
        }
    }
    v
}

fn decompile(data: &[u8]) -> Option<String> {
    let chunk = luac::parse_any(data).ok()?;
    let names = |b: &[u8]| rgrf_core::GrfPath::new(b.to_vec()).display(NameMode::Cp949);
    Some(luadec::decompile(&chunk, &names))
}

/// The decompiled source of every compiled file is accepted by the real parser of its version
/// (5.0 files by Lua 5.0 when it is there, else by 5.1).
fn parse_check(files: &[Entry]) -> usize {
    let lua = unsafe { Lua::unsafe_new() };
    let l50 = lua50();
    let rejected: Vec<String> = files
        .par_iter()
        .filter_map(|f| {
            let text = decompile(&f.data).unwrap_or_default();
            let result = match (&l50, f.data[4]) {
                (Some(l), 0x50) => run50(l, "parse", &Code::Text(&text)).map(drop),
                _ => Ok(()),
            };
            result.err().map(|e| format!("REJECTED {}: {e}", f.name))
        })
        .collect();
    let mut bad = rejected.len();
    for r in &rejected {
        println!("{r}");
    }
    for f in files.iter().filter(|f| f.data[4] != 0x50 || l50.is_none()) {
        let text = decompile(&f.data).unwrap_or_default();
        if let Err(e) = lua
            .load(text.as_bytes())
            .set_mode(ChunkMode::Text)
            .into_function()
        {
            bad += 1;
            println!("REJECTED {}: {e}", f.name);
        }
    }
    let n50 = files.iter().filter(|f| f.data[4] == 0x50).count();
    println!(
        "parse check: {} of {} decompiled files accepted by real Lua ({} 5.1 files by Lua 5.1, {n50} 5.0 files by Lua {})",
        files.len() - bad,
        files.len(),
        files.len() - n50,
        if l50.is_some() {
            "5.0"
        } else {
            "5.1 (no Lua 5.0 found)"
        }
    );
    bad
}

/// Running the original 5.1 bytecode and the decompiled source gives the same outcome.
fn behaviour_check(files: &[Entry]) -> usize {
    let results: Vec<(String, u8, Result<(), String>)> = files
        .par_iter()
        .filter(|f| f.data[4] == 0x51 || lua50().is_some())
        .filter(|f| {
            std::env::var("LV_ONLY").is_ok_and(|o| f.name.contains(&o))
                || std::env::var_os("LV_ONLY").is_none()
        })
        .map(|f| {
            if std::env::var_os("LV_TRACE").is_some() {
                eprintln!("running {}", f.name);
            }
            let verdict = (|| {
                let version = f.data[4];
                let original = if version == 0x50 {
                    luac50::dump(&luac50::parse(&f.data).map_err(|e| e.to_string())?)
                } else {
                    luac::dump(&luac::parse(&f.data).map_err(|e| e.to_string())?)
                };
                let text = decompile(&f.data).ok_or("cannot decompile")?;
                // self-test: a deliberately wrong decompilation must be noticed
                let text = if std::env::var_os("LV_MUTATE").is_some() {
                    text.replacen(" + ", " - ", 1)
                        .replacen(" == ", " ~= ", 1)
                        .replacen("return ", "return not ", 1)
                } else {
                    text
                };
                let trace = std::env::var_os("LV_TRACE").is_some();
                if trace {
                    eprintln!("  original {}", f.name);
                }
                let a = run(version, &Code::Binary(&original))?;
                if trace {
                    eprintln!("  decompiled {}", f.name);
                }
                let b = run(version, &Code::Text(&text))?;
                if std::env::var_os("LV_SHOW").is_some() {
                    eprintln!("{}:\n{}", f.name, a.chars().take(1500).collect::<String>());
                }
                if a == b {
                    return Ok(());
                }
                let (la, lb): (Vec<&str>, Vec<&str>) = (a.lines().collect(), b.lines().collect());
                let i = la
                    .iter()
                    .zip(&lb)
                    .position(|(x, y)| x != y)
                    .unwrap_or(la.len().min(lb.len()));
                // a window of text around the first differing character
                let at = match (la.get(i), lb.get(i)) {
                    (Some(x), Some(y)) => {
                        x.chars().zip(y.chars()).take_while(|(p, q)| p == q).count()
                    }
                    _ => 0,
                };
                let cut = |s: Option<&&str>| {
                    s.map_or(String::new(), |s| {
                        s.chars().skip(at.saturating_sub(40)).take(100).collect()
                    })
                };
                Err(format!(
                    "line {i}: original `{}` / decompiled `{}`",
                    cut(la.get(i)),
                    cut(lb.get(i))
                ))
            })();
            if std::env::var_os("LV_TRACE").is_some() {
                eprintln!("done {}", f.name);
            }
            (f.name.clone(), f.data[4], verdict)
        })
        .collect();
    let bad = results.iter().filter(|(_, _, r)| r.is_err()).count();
    for (label, v) in [("5.1", 0x51), ("5.0", 0x50)] {
        let of: Vec<_> = results.iter().filter(|(_, ver, _)| *ver == v).collect();
        let differ = of.iter().filter(|(_, _, r)| r.is_err()).count();
        println!(
            "behaviour check: {} {label} files, {} behave the same, {differ} differ",
            of.len(),
            of.len() - differ
        );
    }
    for (n, _, r) in results.iter().filter(|(_, _, r)| r.is_err()) {
        println!("DIFF {n}: {}", r.as_ref().unwrap_err());
    }
    bad
}

/// Loads our 64-bit dump of every 5.1 file into real Lua and has Lua write it out again
/// (`string.dump`): identical bytes mean Lua understood the chunk exactly as we wrote it.
fn dump_roundtrip(files: &[Entry]) -> usize {
    let lua = unsafe { Lua::unsafe_new() };
    let dump: mlua::Function = lua.load("return string.dump").eval().unwrap();
    let mut bad = 0;
    let mut total = 0;
    for f in files
        .iter()
        .filter(|f| f.data.get(4) == Some(&0x51))
        .filter(|f| std::env::var("LV_ONLY").map_or(true, |o| f.name == o))
    {
        total += 1;
        if std::env::var_os("LV_TRACE").is_some() {
            eprintln!("round trip {}", f.name);
        }
        let ours = luac::dump(&luac::parse(&f.data).unwrap());
        let loaded = lua
            .load(&ours[..])
            .set_mode(ChunkMode::Binary)
            .into_function();
        let again: Result<mlua::LuaString, _> = loaded.and_then(|func| dump.call(func));
        let _ = lua.gc_collect();
        match again {
            Ok(b) if b.as_bytes().as_ref() == &ours[..] => {}
            Ok(b) => {
                bad += 1;
                let at = b.as_bytes().iter().zip(&ours).position(|(x, y)| x != y);
                println!(
                    "DUMP DIFFERS {} (first difference at {at:?}, {} / {} bytes)",
                    f.name,
                    b.as_bytes().len(),
                    ours.len()
                );
            }
            Err(e) => {
                bad += 1;
                println!("DUMP REJECTED {}: {e}", f.name);
            }
        }
    }
    println!(
        "dump round trip: {} of {total} 5.1 files identical after Lua loads and writes them",
        total - bad
    );
    if let Some(l) = lua50() {
        let of: Vec<&Entry> = files.iter().filter(|f| f.data[4] == 0x50).collect();
        let differ: Vec<String> = of
            .par_iter()
            .filter_map(|f| {
                let ours = luac50::dump(&luac50::parse(&f.data).ok()?);
                match run50(&l, "dump", &Code::Binary(&ours)) {
                    Ok(b) if b == ours => None,
                    Ok(b) => Some(format!(
                        "DUMP DIFFERS {} (first difference at {:?}, {} / {} bytes)",
                        f.name,
                        b.iter().zip(&ours).position(|(x, y)| x != y),
                        b.len(),
                        ours.len()
                    )),
                    Err(e) => Some(format!("DUMP REJECTED {}: {e}", f.name)),
                }
            })
            .collect();
        for d in &differ {
            println!("{d}");
        }
        println!(
            "dump round trip: {} of {} 5.0 files identical after Lua 5.0 loads and writes them",
            of.len() - differ.len(),
            of.len()
        );
        bad += differ.len();
    }
    bad
}

/// The source bytes a decompiled text stands for: each run of non-ASCII characters becomes CP949
/// bytes, or Latin-1 bytes when it is not Korean (the inverse of the decompiler's string rule).
fn encode_text(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len());
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut Vec<u8>| -> Result<(), String> {
        if run.is_empty() {
            return Ok(());
        }
        let bytes = match rgrf_core::GrfPath::from_display(run, NameMode::Cp949) {
            Some(p) => p.as_bytes().to_vec(),
            None => run
                .chars()
                .map(|c| u8::try_from(u32::from(c)).map_err(|_| format!("cannot encode {c:?}")))
                .collect::<Result<_, _>>()?,
        };
        out.extend(bytes);
        run.clear();
        Ok(())
    };
    for c in text.chars() {
        if c.is_ascii() {
            flush(&mut run, &mut out)?;
            out.push(c as u8);
        } else {
            run.push(c);
        }
    }
    flush(&mut run, &mut out)?;
    Ok(out)
}

/// Compiles with the real Lua 5.1 of mlua (`string.dump`), 64-bit layout.
fn compile_real(source: &[u8]) -> Result<Vec<u8>, String> {
    let lua = unsafe { Lua::unsafe_new() };
    let f = lua
        .load(source)
        .set_name("=t")
        .set_mode(ChunkMode::Text)
        .into_function()
        .map_err(|e| e.to_string())?;
    let dump: mlua::Function = lua
        .load("return string.dump")
        .eval()
        .map_err(|e| e.to_string())?;
    let s: mlua::LuaString = dump.call(f).map_err(|e| e.to_string())?;
    Ok(s.as_bytes().to_vec())
}

/// First difference between the code of two functions (debug line numbers and source names are
/// ignored: the decompiled text has other line breaks than the original source).
fn compare_functions(a: &luac::Function, b: &luac::Function, path: &str) -> Option<String> {
    let flat = |f: &luac::Function| {
        (
            f.upvalue_count,
            f.params,
            f.is_vararg,
            f.max_stack,
            f.protos.len(),
        )
    };
    if flat(a) != flat(b) {
        return Some(format!("{path}: header {:?} / {:?}", flat(a), flat(b)));
    }
    if a.code != b.code {
        let i = a.code.iter().zip(&b.code).position(|(x, y)| x != y);
        return Some(format!(
            "{path}: code differs at {i:?} ({} / {} instructions)",
            a.code.len(),
            b.code.len()
        ));
    }
    if a.constants != b.constants {
        let i = a
            .constants
            .iter()
            .zip(&b.constants)
            .position(|(x, y)| x != y);
        return Some(format!(
            "{path}: constants differ at {i:?}: {:?} / {:?} ({} / {} constants)",
            i.map(|i| &a.constants[i]),
            i.map(|i| &b.constants[i]),
            a.constants.len(),
            b.constants.len()
        ));
    }
    let names = |f: &luac::Function| f.locals.iter().map(|l| l.name.clone()).collect::<Vec<_>>();
    if names(a) != names(b) {
        return Some(format!("{path}: local names differ"));
    }
    if a.upvalue_names != b.upvalue_names {
        return Some(format!("{path}: upvalue names differ"));
    }
    a.protos
        .iter()
        .zip(&b.protos)
        .enumerate()
        .find_map(|(i, (x, y))| compare_functions(x, y, &format!("{path}/{i}")))
}

/// Compiles the decompiled text of every 5.1 file with the compiler `$RILUAC` (a `luac`-style
/// program) and compares the result with the original bytecode.
fn recompile_check(files: &[Entry]) -> usize {
    let luac_bin = std::env::var_os("RILUAC");
    let dir = std::env::temp_dir().join(format!("rgrf-recompile-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let results: Vec<(String, Result<(), String>)> = files
        .par_iter()
        .enumerate()
        .filter(|(_, f)| f.data[4] == 0x51)
        .filter(|(_, f)| std::env::var("LV_ONLY").map_or(true, |o| f.name.contains(&o)))
        .map(|(n, f)| {
            let verdict = (|| {
                let original = luac::parse(&f.data).map_err(|e| e.to_string())?;
                let text = decompile(&f.data).ok_or("cannot decompile")?;
                let source = encode_text(&text)?;
                let bytes = if std::env::var_os("LV_OURS").is_some() {
                    // our compiler, fed with the Unicode text as a user would
                    let cp949 = |run: &str| {
                        rgrf_core::GrfPath::from_display(run, NameMode::Cp949)
                            .map(|p| p.as_bytes().to_vec())
                    };
                    let c =
                        luacomp::compile_text(&text, b"=t", luacomp::TextEncoding::Auto, &cp949)
                            .map_err(|e| e.to_string())?;
                    luac::dump32(&c)
                } else if let Some(luac_bin) = &luac_bin {
                    let (src, out) = (dir.join(format!("{n}.lua")), dir.join(format!("{n}.out")));
                    std::fs::write(&src, &source).map_err(|e| e.to_string())?;
                    let st = std::process::Command::new(luac_bin)
                        .arg("-o")
                        .arg(&out)
                        .arg(&src)
                        .output()
                        .map_err(|e| e.to_string())?;
                    if !st.status.success() {
                        return Err(format!(
                            "compiler failed: {}",
                            String::from_utf8_lossy(&st.stderr).trim()
                        ));
                    }
                    let bytes = std::fs::read(&out).map_err(|e| e.to_string())?;
                    let _ = std::fs::remove_file(&src);
                    let _ = std::fs::remove_file(&out);
                    bytes
                } else {
                    compile_real(&source)?
                };
                let again = luac::parse(&bytes).map_err(|e| e.to_string())?;
                match compare_functions(&original.main, &again.main, "main") {
                    None => Ok(()),
                    Some(d) => Err(d),
                }
            })();
            (f.name.clone(), verdict)
        })
        .collect();
    let bad = results.iter().filter(|(_, r)| r.is_err()).count();
    println!(
        "recompile check: {} 5.1 files, {} give the original code again, {bad} differ",
        results.len(),
        results.len() - bad
    );
    for (n, r) in results.iter().filter(|(_, r)| r.is_err()).take(40) {
        println!("DIFF {n}: {}", r.as_ref().unwrap_err());
    }
    let _ = std::fs::remove_dir_all(&dir);
    bad
}

/// Our compiler against the real Lua 5.1.5: the decompiled text of every 5.1 file compiles to
/// exactly the same bytes (code, constants, line numbers, local and upvalue names).
fn compile_equal_check(files: &[Entry]) -> usize {
    let results: Vec<(String, Result<(), String>)> = files
        .par_iter()
        .filter(|f| f.data[4] == 0x51)
        .filter(|f| std::env::var("LV_ONLY").map_or(true, |o| f.name.contains(&o)))
        .map(|f| {
            let verdict = (|| {
                let text = decompile(&f.data).ok_or("cannot decompile")?;
                let source = encode_text(&text)?;
                let real = compile_real(&source)?;
                let ours = luacomp::compile(&source, b"=t").map_err(|e| e.to_string())?;
                let ours = luac::dump(&ours);
                if real == ours {
                    return Ok(());
                }
                let at = real.iter().zip(&ours).position(|(a, b)| a != b);
                // find where the two chunks first differ structurally for a readable message
                let detail = match (luac::parse(&real), luac::parse(&ours)) {
                    (Ok(a), Ok(b)) => compare_functions(&a.main, &b.main, "main")
                        .unwrap_or_else(|| "same code, differs in debug info".into()),
                    _ => "cannot parse".into(),
                };
                Err(format!(
                    "bytes differ at {at:?} ({} / {} bytes): {detail}",
                    real.len(),
                    ours.len()
                ))
            })();
            (f.name.clone(), verdict)
        })
        .collect();
    let bad = results.iter().filter(|(_, r)| r.is_err()).count();
    println!(
        "compile check: {} 5.1 files, {} compile to exactly the bytes of real Lua 5.1.5, {bad} differ",
        results.len(),
        results.len() - bad
    );
    for (n, r) in results.iter().filter(|(_, r)| r.is_err()).take(40) {
        println!("DIFF {n}: {}", r.as_ref().unwrap_err());
    }
    bad
}

/// A small random generator of valid Lua 5.1 programs for the differential test of the compiler.
struct Gen {
    rng: u64,
    /// Local variable names visible, per function level (innermost last).
    scopes: Vec<Vec<String>>,
    vararg: Vec<bool>,
    loops: Vec<u32>,
    out: String,
    next_name: u32,
    /// Programs for Lua 5.0: no `#`, no `...` in expressions (a vararg function has the local
    /// `arg`), no hexadecimal numbers, nested long strings, calls with a string or table argument.
    v50: bool,
}

impl Gen {
    fn new(seed: u64) -> Self {
        Self {
            rng: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03,
            scopes: vec![Vec::new()],
            vararg: vec![true],
            loops: vec![0],
            out: String::new(),
            next_name: 0,
            v50: false,
        }
    }
    fn new50(seed: u64) -> Self {
        Self {
            vararg: vec![false],
            v50: true,
            ..Self::new(seed)
        }
    }
    fn r(&mut self, n: u32) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        ((self.rng >> 11) % u64::from(n)) as u32
    }
    fn fresh(&mut self) -> String {
        self.next_name += 1;
        format!("v{}", self.next_name)
    }
    fn any_var(&mut self) -> String {
        let visible: Vec<String> = self.scopes.iter().flatten().cloned().collect();
        if !visible.is_empty() && self.r(3) != 0 {
            let i = self.r(visible.len() as u32) as usize;
            visible[i].clone()
        } else {
            format!("g{}", self.r(5))
        }
    }
    fn number(&mut self) -> String {
        match self.r(8) {
            0 if self.r(4) == 0 => self.unique(),
            0 => "0".into(),
            1 => "1".into(),
            2 => format!("{}", self.r(300)),
            3 => format!("{}.{}", self.r(20), self.r(100)),
            4 => format!("{}e{}", self.r(9) + 1, self.r(5)),
            5 if self.v50 => match self.r(4) {
                0 => format!("{}.", self.r(50)),
                1 => format!("1e{}", 300 + self.r(40)),
                2 => format!("{}e-{}", self.r(9) + 1, 300 + self.r(40)),
                _ => format!("{}.e{}", self.r(9), self.r(5)),
            },
            5 => format!("0x{:x}", self.r(5000)),
            6 => format!("{}", 7),
            _ => format!(".{}", self.r(9) + 1),
        }
    }
    fn unique(&mut self) -> String {
        format!("{}.5", 1000 + self.r(100_000))
    }
    fn string(&mut self) -> String {
        match self.r(5) {
            0 => "\"a\"".into(),
            1 => "'b\\n'".into(),
            2 if self.v50 && self.r(2) == 0 => "[[a [[nested]] b\n]]".into(),
            2 => "[[long]]".into(),
            3 => format!("\"k{}\"", self.r(6)),
            _ => "\"\\65\\066\"".into(),
        }
    }
    fn expr(&mut self, d: u32) -> String {
        if self.v50 && d > 0 && self.r(14) == 0 {
            let v = self.any_var();
            return match self.r(3) {
                0 => format!("{v}{}", self.string()),
                1 => format!("{v}{}", self.table(d - 1)),
                _ => format!("{v}:m{}{}", self.r(3), self.string()),
            };
        }
        if d == 0 {
            return match self.r(6) {
                0 => self.number(),
                1 => self.string(),
                2 => ["nil", "true", "false"][self.r(3) as usize].into(),
                _ => self.any_var(),
            };
        }
        match self.r(18) {
            0 => self.number(),
            1 => self.string(),
            2 => ["nil", "true", "false"][self.r(3) as usize].into(),
            3 | 4 => self.any_var(),
            5..=7 => {
                // no `%`: Apple's compiler fuses the folded `a - floor(a/b)*b` of the reference Lua
                // into an FMA, so its constants differ from the x86 `luac` the client used
                let ops = [
                    "+", "-", "*", "/", "^", "..", "==", "~=", "<", "<=", ">", ">=", "and", "or",
                ];
                let o = ops[self.r(ops.len() as u32) as usize];
                format!("({} {o} {})", self.expr(d - 1), self.expr(d - 1))
            }
            8 => {
                let o = if self.v50 {
                    ["-", "not ", "- "][self.r(3) as usize]
                } else {
                    ["-", "not ", "#"][self.r(3) as usize]
                };
                format!("({o}{})", self.expr(d - 1))
            }
            9 => self.table(d - 1),
            10 => format!("{}({})", self.any_var(), self.args(d - 1)),
            11 => format!("{}:m{}({})", self.any_var(), self.r(3), self.args(d - 1)),
            12 => format!("{}.f{}", self.any_var(), self.r(4)),
            13 => {
                let (v, e) = (self.any_var(), self.expr(d - 1));
                // a long string right after `[` would read as `[[[`
                format!("{v}[ {e} ]")
            }
            14 => self.function(d - 1, false),
            15 if !self.v50 && *self.vararg.last().unwrap() => "...".into(),
            16 => format!("({})", self.expr(d - 1)),
            _ => self.any_var(),
        }
    }
    fn args(&mut self, d: u32) -> String {
        let n = self.r(4);
        let mut v: Vec<String> = (0..n).map(|_| self.expr(d)).collect();
        if !self.v50 && self.r(6) == 0 && *self.vararg.last().unwrap() {
            v.push("...".into());
        }
        v.join(", ")
    }
    fn table(&mut self, d: u32) -> String {
        let n = self.r(6);
        let mut v = Vec::new();
        for _ in 0..n {
            v.push(match self.r(4) {
                0 => format!("k{} = {}", self.r(5), self.expr(d)),
                1 => format!("[ {} ] = {}", self.expr(d), self.expr(d)),
                _ => self.expr(d),
            });
        }
        let sep = if self.r(2) == 0 { ", " } else { "; " };
        let tail = if self.r(5) == 0 { sep } else { "" };
        format!("{{{}{tail}}}", v.join(sep))
    }
    fn function(&mut self, d: u32, method: bool) -> String {
        let mut params: Vec<String> = (0..self.r(3)).map(|_| self.fresh()).collect();
        let vararg = self.r(3) == 0;
        let mut scope = params.clone();
        if method {
            scope.insert(0, "self".into());
        }
        if vararg {
            params.push("...".into());
            if self.v50 {
                scope.push("arg".into());
            }
        }
        let saved_out = std::mem::take(&mut self.out);
        self.scopes.push(scope);
        self.vararg.push(vararg);
        self.loops.push(0);
        let n = self.r(5);
        for _ in 0..n {
            self.stat(d);
        }
        if self.r(2) == 0 {
            let e = self.expr(d.min(1));
            self.out.push_str(&format!("return {e}\n"));
        }
        self.loops.pop();
        self.vararg.pop();
        self.scopes.pop();
        let body = std::mem::replace(&mut self.out, saved_out);
        format!("function({})\n{body}end", params.join(", "))
    }
    fn block(&mut self, d: u32) {
        let n = self.r(3);
        let mark = self.scopes.last().unwrap().len();
        for _ in 0..n {
            self.stat(d);
        }
        self.scopes.last_mut().unwrap().truncate(mark);
    }
    fn stat(&mut self, d: u32) {
        let d = d.max(1);
        match self.r(16) {
            0 | 1 => {
                let n = self.r(3) + 1;
                let names: Vec<String> = (0..n).map(|_| self.fresh()).collect();
                let m = self.r(4);
                let exprs: Vec<String> = (0..m).map(|_| self.expr(d - 1)).collect();
                let rhs = if exprs.is_empty() {
                    String::new()
                } else {
                    format!(" = {}", exprs.join(", "))
                };
                self.out
                    .push_str(&format!("local {}{rhs}\n", names.join(", ")));
                self.scopes.last_mut().unwrap().extend(names);
            }
            2 | 3 => {
                let n = self.r(3) + 1;
                let targets: Vec<String> = (0..n)
                    .map(|_| match self.r(4) {
                        0 => format!("{}.f{}", self.any_var(), self.r(3)),
                        1 => format!("{}[ {} ]", self.any_var(), self.expr(0)),
                        _ => self.any_var(),
                    })
                    .collect();
                let m = self.r(4) + 1;
                let exprs: Vec<String> = (0..m).map(|_| self.expr(d - 1)).collect();
                self.out
                    .push_str(&format!("{} = {}\n", targets.join(", "), exprs.join(", ")));
            }
            4 => {
                let c = format!("{}({})", self.any_var(), self.args(d - 1));
                self.out.push_str(&format!("{c}\n"));
            }
            5 => {
                let c = self.expr(d - 1);
                self.out.push_str(&format!("if {c} then\n"));
                self.block(d - 1);
                if self.r(2) == 0 {
                    let c = self.expr(d - 1);
                    self.out.push_str(&format!("elseif {c} then\n"));
                    self.block(d - 1);
                }
                if self.r(2) == 0 {
                    self.out.push_str("else\n");
                    self.block(d - 1);
                }
                self.out.push_str("end\n");
            }
            6 => {
                let c = self.expr(d - 1);
                self.out.push_str(&format!("while {c} do\n"));
                *self.loops.last_mut().unwrap() += 1;
                self.block(d - 1);
                *self.loops.last_mut().unwrap() -= 1;
                self.out.push_str("end\n");
            }
            7 => {
                self.out.push_str("repeat\n");
                *self.loops.last_mut().unwrap() += 1;
                let mark = self.scopes.last().unwrap().len();
                for _ in 0..self.r(3) {
                    self.stat(d - 1);
                }
                let c = self.expr(d - 1);
                self.scopes.last_mut().unwrap().truncate(mark);
                *self.loops.last_mut().unwrap() -= 1;
                self.out.push_str(&format!("until {c}\n"));
            }
            8 => {
                let v = self.fresh();
                let (a, b) = (self.expr(0), self.expr(0));
                let step = if self.r(2) == 0 {
                    format!(", {}", self.expr(0))
                } else {
                    String::new()
                };
                self.out.push_str(&format!("for {v} = {a}, {b}{step} do\n"));
                self.scopes.last_mut().unwrap().push(v);
                *self.loops.last_mut().unwrap() += 1;
                self.block(d - 1);
                *self.loops.last_mut().unwrap() -= 1;
                self.scopes.last_mut().unwrap().pop();
                self.out.push_str("end\n");
            }
            9 => {
                let (k, v) = (self.fresh(), self.fresh());
                let e = self.expr(d - 1);
                self.out
                    .push_str(&format!("for {k}, {v} in pairs({e}) do\n"));
                self.scopes.last_mut().unwrap().extend([k, v]);
                *self.loops.last_mut().unwrap() += 1;
                self.block(d - 1);
                *self.loops.last_mut().unwrap() -= 1;
                let n = self.scopes.last().unwrap().len();
                self.scopes.last_mut().unwrap().truncate(n - 2);
                self.out.push_str("end\n");
            }
            10 => {
                self.out.push_str("do\n");
                self.block(d - 1);
                self.out.push_str("end\n");
            }
            11 => {
                let f = self.function(d - 1, false);
                let g = self.r(4);
                self.out.push_str(&format!("function g{g}{}\n", &f[8..]));
            }
            12 => {
                let name = self.fresh();
                let f = self.function(d - 1, false);
                self.scopes.last_mut().unwrap().push(name.clone());
                self.out
                    .push_str(&format!("local function {name}{}\n", &f[8..]));
            }
            13 => {
                let f = self.function(d - 1, true);
                let (a, b) = (self.r(3), self.r(3));
                self.out
                    .push_str(&format!("function g{a}.m{b}:meth{}\n", &f[8..]));
            }
            14 if *self.loops.last().unwrap() > 0 => {
                let c = self.expr(d - 1);
                self.out.push_str(&format!("if {c} then break end\n"));
            }
            _ => {
                let v = self.any_var();
                let e = self.expr(d - 1);
                self.out.push_str(&format!("{v} = {e}\n"));
            }
        }
    }
    fn program(&mut self) -> String {
        // one in eight programs is large and made of constants only: past the 255 constants that
        // fit an operand, tables beyond one SETLIST flush, long argument lists
        if self.r(8) == 0 {
            for _ in 0..100 + self.r(300) {
                let (x, y) = (self.r(5), self.r(5));
                let line = match self.r(5) {
                    0 => {
                        let (a, b) = (self.unique(), self.unique());
                        format!("g{x} = {a} + g{y} * {b}\n")
                    }
                    1 => {
                        let (k, f) = (self.r(100_000), self.r(4));
                        format!("g{x}.f{f} = \"s{k}\" .. g1\n")
                    }
                    2 => {
                        let n = self.r(130);
                        let items: Vec<String> = (0..n).map(|_| self.unique()).collect();
                        format!("g{x} = {{{}}}\n", items.join(", "))
                    }
                    3 => {
                        let n = self.r(40);
                        let args: Vec<String> = (0..n).map(|_| self.unique()).collect();
                        format!("g{x}({})\n", args.join(", "))
                    }
                    _ => {
                        let (k, u) = (self.r(100_000), self.unique());
                        format!("if g{x} == \"c{k}\" then g1 = {u} end\n")
                    }
                };
                self.out.push_str(&line);
            }
            return std::mem::take(&mut self.out);
        }
        let n = self.r(10) + 2;
        for _ in 0..n {
            let depth = if self.v50 { 3 + self.r(2) } else { 3 };
            self.stat(depth);
        }
        if self.r(3) == 0 {
            let e = self.expr(2);
            self.out.push_str(&format!("return {e}\n"));
        }
        std::mem::take(&mut self.out)
    }
}

/// Differential test: random programs compile to the same bytes with our compiler and the real one.
fn fuzz_compile(count: u64, seed: u64) -> usize {
    let compiled = std::sync::atomic::AtomicU64::new(0);
    let bytes = std::sync::atomic::AtomicU64::new(0);
    let bad: Vec<(u64, String)> = (seed..seed + count)
        .into_par_iter()
        .filter_map(|i| {
            let src = Gen::new(i).program();
            let real = compile_real(src.as_bytes());
            let ours = luacomp::compile(src.as_bytes(), b"=t").map(|c| luac::dump(&c));
            match (real, ours) {
                (Ok(a), Ok(b)) if a == b => {
                    compiled.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    bytes.fetch_add(a.len() as u64, std::sync::atomic::Ordering::Relaxed);
                    None
                }
                (Ok(a), Ok(b)) => {
                    let at = a.iter().zip(&b).position(|(x, y)| x != y);
                    let detail = match (luac::parse(&a), luac::parse(&b)) {
                        (Ok(x), Ok(y)) => compare_functions(&x.main, &y.main, "main")
                            .unwrap_or_else(|| "same code, differs in debug info".into()),
                        _ => "cannot parse".into(),
                    };
                    Some((i, format!("bytes differ at {at:?}: {detail}\n{src}")))
                }
                (Err(_), Err(_)) => None,
                (a, b) => Some((
                    i,
                    format!(
                        "real {} / ours {}\n{src}",
                        a.map_or_else(|e| format!("error {e}"), |v| format!("{} bytes", v.len())),
                        b.map_or_else(|e| format!("error {e}"), |v| format!("{} bytes", v.len())),
                    ),
                )),
            }
        })
        .collect();
    let n = compiled.load(std::sync::atomic::Ordering::Relaxed);
    println!(
        "compile fuzz: {count} random programs (seeds {seed}..{}): {n} compiled to identical bytes (average {} bytes), {} differ",
        seed + count,
        bytes.load(std::sync::atomic::Ordering::Relaxed) / n.max(1),
        bad.len()
    );
    for (i, m) in bad.iter().take(3) {
        println!("--- seed {i}: {m}");
    }
    bad.len()
}

/// The driver of `compile_real50`: reads `<length>\n<source>` records from stdin and answers each
/// with `O<length>\n<bytecode>` or `E<length>\n<message>`.
const COMPILE_DRIVER50: &str = r#"
local read, write, len, dump, load = io.read, io.write, string.len, string.dump, loadstring
while 1 do
  local line = read("*l")
  if not line then break end
  local n = tonumber(line)
  local src = ""
  if n > 0 then src = read(n) end
  local f, err = load(src, "=t")
  if f then
    local d = dump(f)
    write("O", len(d), "\n", d)
  else
    write("E", len(err), "\n", err)
  end
end
"#;

/// Compiles every source with the real Lua 5.0.3 (`loadstring` + `string.dump`, chunk name `=t`;
/// its 64-bit layout, like `luac50::dump`) in one process, without temporary files: the sources
/// go in through stdin, the results come back through stdout.
fn compile_real50(
    lua: &std::path::Path,
    sources: &[Vec<u8>],
) -> Result<Vec<Result<Vec<u8>, String>>, String> {
    use std::io::{Read, Write};
    let mut child = std::process::Command::new(lua)
        .arg("-e")
        .arg(COMPILE_DRIVER50)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let mut input = Vec::new();
    for s in sources {
        input.extend(format!("{}\n", s.len()).into_bytes());
        input.extend(s);
    }
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let mut out = Vec::new();
    child
        .stdout
        .take()
        .ok_or("no stdout")?
        .read_to_end(&mut out)
        .map_err(|e| e.to_string())?;
    let _ = writer.join();
    let status = child.wait().map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    let mut at = 0;
    while at < out.len() {
        let nl = out[at..]
            .iter()
            .position(|&b| b == b'\n')
            .ok_or("truncated answer")?
            + at;
        let kind = out[at];
        let n: usize = std::str::from_utf8(&out[at + 1..nl])
            .map_err(|e| e.to_string())?
            .parse()
            .map_err(|_| "bad length")?;
        let body = out
            .get(nl + 1..nl + 1 + n)
            .ok_or("truncated answer")?
            .to_vec();
        results.push(if kind == b'O' {
            Ok(body)
        } else {
            Err(String::from_utf8_lossy(&body).into_owned())
        });
        at = nl + 1 + n;
    }
    if results.len() != sources.len() {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        return Err(format!(
            "Lua 5.0 {status} gave {} of {} answers: {}",
            results.len(),
            sources.len(),
            err.trim()
        ));
    }
    Ok(results)
}

/// The first difference between two 5.0 functions in what the code generator decides (header
/// flags, code, constants, names); line numbers, source names and variable ranges are ignored.
fn compare_functions50(a: &luac50::Function, b: &luac50::Function, path: &str) -> Option<String> {
    let flat = |f: &luac50::Function| {
        (
            f.upvalue_count,
            f.params,
            f.is_vararg,
            f.max_stack,
            f.protos.len(),
        )
    };
    if flat(a) != flat(b) {
        return Some(format!("{path}: header {:?} / {:?}", flat(a), flat(b)));
    }
    if a.code != b.code {
        let i = a.code.iter().zip(&b.code).position(|(x, y)| x != y);
        let show = |f: &luac50::Function| {
            i.and_then(|i| f.code.get(i)).map_or(String::new(), |&raw| {
                let d = luac50::Instr::decode(raw);
                format!("{} {} {} {}", d.name(), d.a, d.b, d.c)
            })
        };
        return Some(format!(
            "{path}: code differs at {i:?} ({} / {} instructions): [{}] / [{}]",
            a.code.len(),
            b.code.len(),
            show(a),
            show(b)
        ));
    }
    if a.constants != b.constants {
        let i = a
            .constants
            .iter()
            .zip(&b.constants)
            .position(|(x, y)| x != y);
        return Some(format!(
            "{path}: constants differ at {i:?}: {:?} / {:?} ({} / {})",
            i.map(|i| &a.constants[i]),
            i.map(|i| &b.constants[i]),
            a.constants.len(),
            b.constants.len()
        ));
    }
    let names = |f: &luac50::Function| f.locals.iter().map(|l| l.name.clone()).collect::<Vec<_>>();
    if names(a) != names(b) || a.upvalue_names != b.upvalue_names {
        return Some(format!("{path}: variable names differ"));
    }
    a.protos
        .iter()
        .zip(&b.protos)
        .enumerate()
        .find_map(|(i, (x, y))| compare_functions50(x, y, &format!("{path}/{i}")))
}

/// Puts the line numbers of `src` into `dst` where the code has the same length.
fn copy_lines(dst: &mut luac50::Function, src: &luac50::Function) {
    if dst.lines.len() == src.lines.len() {
        dst.lines.clone_from(&src.lines);
    }
    dst.line_defined = src.line_defined;
    for (d, s) in dst.protos.iter_mut().zip(&src.protos) {
        copy_lines(d, s);
    }
}

/// Our Lua 5.0 compiler against the real Lua 5.0.3: the decompiled text of every 5.0 file compiles
/// to exactly the same bytes (64-bit layout of the real Lua, `luac50::dump`). Also reports how many
/// files give the original code again (code, constants, names) and how many the original bytes in
/// the client's layout (`luac50::dump32`; the decompiled text has other line breaks than the
/// original source, so the line information usually differs).
fn compile_equal50_check(files: &[Entry]) -> usize {
    let Some(lua) = lua50() else {
        eprintln!("no Lua 5.0 (see fetch-lua50.sh)");
        return 1;
    };
    struct Outcome {
        same_as_real: Result<(), String>,
        same_code: bool,
        same_bytes: bool,
        same_but_lines: bool,
        why: String,
    }
    let results: Vec<(String, Outcome)> = files
        .par_iter()
        .filter(|f| f.data[4] == 0x50)
        .filter(|f| std::env::var("LV_ONLY").map_or(true, |o| f.name.contains(&o)))
        .map(|f| {
            let run = || -> Result<Outcome, String> {
                let text = decompile(&f.data).ok_or("cannot decompile")?;
                let source = encode_text(&text)?;
                let real = compile_real50(&lua, std::slice::from_ref(&source))?.remove(0)?;
                let ours = luacomp50::compile(&source, b"=t").map_err(|e| e.to_string())?;
                let same_as_real = if luac50::dump(&ours) == real {
                    Ok(())
                } else {
                    let at = luac50::dump(&ours)
                        .iter()
                        .zip(&real)
                        .position(|(a, b)| a != b);
                    Err(format!("bytes differ at {at:?}"))
                };
                let original = luac50::parse(&f.data).map_err(|e| e.to_string())?;
                let again = luacomp50::compile(&source, &original.main.source)
                    .map_err(|e| e.to_string())?;
                let why =
                    compare_functions50(&original.main, &again.main, "main").unwrap_or_default();
                Ok(Outcome {
                    same_as_real,
                    same_code: why.is_empty(),
                    same_bytes: luac50::dump32(&again) == f.data,
                    same_but_lines: {
                        let mut c = again.clone();
                        copy_lines(&mut c.main, &original.main);
                        luac50::dump32(&c) == f.data
                    },
                    why,
                })
            };
            let outcome = run().unwrap_or_else(|e| Outcome {
                same_as_real: Err(e),
                same_code: false,
                same_bytes: false,
                same_but_lines: false,
                why: String::new(),
            });
            (f.name.clone(), outcome)
        })
        .collect();
    let bad = results
        .iter()
        .filter(|(_, o)| o.same_as_real.is_err())
        .count();
    println!(
        "compile check 5.0: {} files, {} compile to exactly the bytes of real Lua 5.0.3, {bad} differ",
        results.len(),
        results.len() - bad
    );
    println!(
        "  against the original files: {} give the original code, constants and names again; {} are byte for byte the original (32-bit layout), {} when the line numbers (which the decompiled text cannot know) are taken from the original",
        results.iter().filter(|(_, o)| o.same_code).count(),
        results.iter().filter(|(_, o)| o.same_bytes).count(),
        results.iter().filter(|(_, o)| o.same_but_lines).count()
    );
    for (n, o) in results
        .iter()
        .filter(|(_, o)| o.same_as_real.is_err())
        .take(40)
    {
        println!("DIFF {n}: {}", o.same_as_real.as_ref().unwrap_err());
    }
    for (n, o) in results.iter().filter(|(_, o)| !o.same_code).take(10) {
        println!("not the original code: {n}: {}", o.why);
    }
    bad
}

/// A random syntax error: the program with a few characters or words changed, so that both
/// compilers must agree on the error message, not only on valid programs.
fn mutate(src: &str, g: &mut Gen) -> String {
    let mut b = src.as_bytes().to_vec();
    let words: [&[u8]; 14] = [
        b"end", b"do", b"then", b"(", b")", b"{", b"}", b"[[", b"]]", b"=", b",", b"--[[", b"'",
        b"...",
    ];
    for _ in 0..1 + g.r(3) {
        if b.is_empty() {
            break;
        }
        let at = g.r(b.len() as u32) as usize;
        match g.r(4) {
            0 => {
                b.remove(at);
            }
            1 => {
                let w = words[g.r(words.len() as u32) as usize];
                b.splice(at..at, w.iter().copied());
            }
            2 => b[at] = b" \n.;:~#%\\\"'"[g.r(11) as usize],
            _ => b.truncate(at),
        }
    }
    String::from_utf8_lossy(&b).into_owned()
}

/// Differential test: random programs (one in eight with a syntax error) compile to the same bytes
/// or give the same error message with our compiler and the real Lua 5.0.3.
fn fuzz_compile50(count: u64, seed: u64) -> usize {
    let Some(lua) = lua50() else {
        eprintln!("no Lua 5.0 (see fetch-lua50.sh)");
        return 1;
    };
    const BATCH: u64 = 400;
    let kinds = std::sync::Mutex::new(std::collections::BTreeMap::<String, u64>::new());
    let (valid, invalid, bytes) = (
        std::sync::atomic::AtomicU64::new(0),
        std::sync::atomic::AtomicU64::new(0),
        std::sync::atomic::AtomicU64::new(0),
    );
    let batches: Vec<u64> = (0..count.div_ceil(BATCH)).collect();
    let bad: Vec<(u64, String)> = batches
        .into_par_iter()
        .flat_map(|b| {
            let first = seed + b * BATCH;
            let last = (first + BATCH).min(seed + count);
            let sources: Vec<(u64, String)> = (first..last)
                .map(|i| {
                    let mut g = Gen::new50(i);
                    let p = g.program();
                    if g.r(8) == 0 && std::env::var_os("LV_NOMUT").is_none() {
                        (i, mutate(&p, &mut g))
                    } else {
                        (i, p)
                    }
                })
                .collect();
            let inputs: Vec<Vec<u8>> = sources
                .iter()
                .map(|(_, s)| s.clone().into_bytes())
                .collect();
            let real = match compile_real50(&lua, &inputs) {
                Ok(r) => r,
                Err(e) => return vec![(first, format!("real Lua failed: {e}"))],
            };
            let mut bad = Vec::new();
            for ((i, src), real) in sources.iter().zip(real) {
                let mut ours = luacomp50::compile(src.as_bytes(), b"=t");
                if std::env::var_os("LV_MUTATE").is_some() {
                    // self-test: a deliberately wrong compiler must be caught
                    if let Ok(c) = &mut ours {
                        c.main.max_stack = c.main.max_stack.wrapping_add(1);
                    }
                }
                match (real, ours) {
                    (Ok(a), Ok(c)) => {
                        let b = luac50::dump(&c);
                        if a == b {
                            valid.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            bytes.fetch_add(a.len() as u64, std::sync::atomic::Ordering::Relaxed);
                        } else {
                            let at = a.iter().zip(&b).position(|(x, y)| x != y);
                            let detail = match (luac50::parse(&a), luac50::parse(&b)) {
                                (Ok(x), Ok(y)) => compare_functions50(&x.main, &y.main, "main")
                                    .unwrap_or_else(|| "same code, differs in debug info".into()),
                                _ => "cannot parse".into(),
                            };
                            bad.push((*i, format!("bytes differ at {at:?}: {detail}\n{src}")));
                        }
                    }
                    (Err(a), Err(b)) => {
                        invalid.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if std::env::var("LV_SHOW").is_ok_and(|v| v == "2") && *i % 97 == 0 {
                            println!("--- seed {i}: {a}\n{src}");
                        }
                        if std::env::var_os("LV_SHOW").is_some() {
                            let kind = a.split_once(": ").map_or(a.clone(), |(_, m)| m.to_string());
                            let kind = kind.split(" near ").next().unwrap_or("").to_string();
                            kinds
                                .lock()
                                .unwrap()
                                .entry(kind)
                                .and_modify(|n| *n += 1)
                                .or_insert(1u64);
                        }
                        if a != b.message {
                            bad.push((
                                *i,
                                format!(
                                    "error messages differ:\n  real: {a}\n  ours: {}\n{src}",
                                    b.message
                                ),
                            ));
                        }
                    }
                    (a, b) => bad.push((
                        *i,
                        format!(
                            "real {} / ours {}\n{src}",
                            a.map_or_else(
                                |e| format!("error {e}"),
                                |v| format!("{} bytes", v.len())
                            ),
                            b.map_or_else(|e| format!("error {}", e.message), |_| "a chunk".into()),
                        ),
                    )),
                }
            }
            bad
        })
        .collect();
    let n = valid.load(std::sync::atomic::Ordering::Relaxed);
    println!(
        "compile fuzz 5.0: {count} random programs (seeds {seed}..{}): {n} compiled to identical bytes (average {} bytes), {} with the same syntax error message, {} differ",
        seed + count,
        bytes.load(std::sync::atomic::Ordering::Relaxed) / n.max(1),
        invalid.load(std::sync::atomic::Ordering::Relaxed),
        bad.len()
    );
    for (k, n) in kinds.lock().unwrap().iter() {
        println!("  {n:>6} x {k}");
    }
    let mut bad = bad;
    bad.sort_by_key(|(i, _)| *i);
    for (i, m) in bad.iter().take(3) {
        println!("--- seed {i}: {m}");
    }
    bad.len()
}

/// Garbage test: programs damaged by many edits (random bytes, cut-offs, inserted words) and pure
/// random bytes. Our compiler must never panic, and must agree with the real Lua 5.0.3 on the
/// outcome: the same bytes, or the same error message.
fn garbage_compile50(count: u64, seed: u64) -> usize {
    let Some(lua) = lua50() else {
        eprintln!("no Lua 5.0 (see fetch-lua50.sh)");
        return 1;
    };
    const BATCH: u64 = 400;
    let words: [&[u8]; 18] = [
        b"end",
        b"do",
        b"then",
        b"(",
        b")",
        b"{",
        b"}",
        b"[[",
        b"]]",
        b"=",
        b",",
        b"--[[",
        b"'",
        b"...",
        b"function",
        b"local ",
        b"for ",
        b"\\",
    ];
    let (same_ok, same_err) = (
        std::sync::atomic::AtomicU64::new(0),
        std::sync::atomic::AtomicU64::new(0),
    );
    let bad: Vec<(u64, String)> = (0..count.div_ceil(BATCH))
        .collect::<Vec<_>>()
        .into_par_iter()
        .flat_map(|b| {
            let first = seed + b * BATCH;
            let last = (first + BATCH).min(seed + count);
            let sources: Vec<(u64, Vec<u8>)> = (first..last)
                .map(|i| {
                    let mut g = Gen::new50(i);
                    let mut bytes = if g.r(4) == 0 {
                        (0..g.r(200)).map(|_| g.r(256) as u8).collect()
                    } else {
                        g.program().into_bytes()
                    };
                    for _ in 0..g.r(12) {
                        if bytes.is_empty() {
                            break;
                        }
                        let at = g.r(bytes.len() as u32) as usize;
                        match g.r(5) {
                            0 => {
                                bytes.remove(at);
                            }
                            1 => {
                                let w = words[g.r(words.len() as u32) as usize];
                                bytes.splice(at..at, w.iter().copied());
                            }
                            2 => bytes[at] = g.r(256) as u8,
                            3 => bytes.insert(at, g.r(256) as u8),
                            _ => bytes.truncate(at),
                        }
                    }
                    // a leading ESC would be read as a binary chunk
                    if bytes.first() == Some(&0x1b) {
                        bytes[0] = b' ';
                    }
                    (i, bytes)
                })
                .collect();
            let inputs: Vec<Vec<u8>> = sources.iter().map(|(_, s)| s.clone()).collect();
            let real = match compile_real50(&lua, &inputs) {
                Ok(r) => r,
                Err(e) => return vec![(first, format!("real Lua failed: {e}"))],
            };
            let mut bad = Vec::new();
            for ((i, src), real) in sources.iter().zip(real) {
                let ours = std::panic::catch_unwind(|| luacomp50::compile(src, b"=t"));
                let shown = String::from_utf8_lossy(src);
                match (real, ours) {
                    (_, Err(_)) => bad.push((*i, format!("PANIC\n{shown}"))),
                    (Ok(a), Ok(Ok(c))) if a == luac50::dump(&c) => {
                        same_ok.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    (Err(a), Ok(Err(b))) if a == b.message => {
                        same_err.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    (a, b) => bad.push((
                        *i,
                        format!(
                            "real {:?} / ours {:?}\n{shown}",
                            a.map(|v| v.len()),
                            b.map(|r| r.map(|c| luac50::dump(&c).len()).map_err(|e| e.message))
                                .map_err(|_| "panic")
                        ),
                    )),
                }
            }
            bad
        })
        .collect();
    println!(
        "garbage fuzz 5.0: {count} damaged programs and random bytes (seeds {seed}..{}): {} compiled to identical bytes, {} gave the same error message, {} differ or panic",
        seed + count,
        same_ok.load(std::sync::atomic::Ordering::Relaxed),
        same_err.load(std::sync::atomic::Ordering::Relaxed),
        bad.len()
    );
    let mut bad = bad;
    bad.sort_by_key(|(i, _)| *i);
    for (i, m) in bad.iter().take(3) {
        println!("--- seed {i}: {m}");
    }
    bad.len()
}

/// The limits of Lua 5.0 (syntax levels, locals, parameters, upvalues, registers, operands that
/// are constants, `while` conditions, jumps) on both sides of each edge: same outcome as the real
/// Lua 5.0.3, byte for byte or the same error message.
fn limits_compile50() -> usize {
    let Some(lua) = lua50() else {
        eprintln!("no Lua 5.0 (see fetch-lua50.sh)");
        return 1;
    };
    let names = |p: &str, n: usize| (1..=n).map(|i| format!("{p}{i}")).collect::<Vec<_>>();
    let mut cases: Vec<(String, String)> = Vec::new();
    for n in 190..=210 {
        cases.push((
            format!("{n} parentheses"),
            format!("x = {}1{}", "(".repeat(n), ")".repeat(n)),
        ));
        cases.push((
            format!("{n} nested blocks"),
            format!("{}{}", "do ".repeat(n), "end ".repeat(n)),
        ));
        cases.push((
            format!("{n} nested tables"),
            format!("x = {}{}", "{".repeat(n), "}".repeat(n)),
        ));
        cases.push((
            format!("{n} nested functions"),
            format!("x = {}{}", "function() return ".repeat(n), "end ".repeat(n)),
        ));
    }
    for n in 196..=204 {
        cases.push((
            format!("{n} locals"),
            format!("local {}", names("a", n).join(", ")),
        ));
        let each: String = (1..=n).map(|i| format!("local a{i} = {i}\n")).collect();
        cases.push((format!("{n} locals one by one"), each));
        let inner: String = (1..n).map(|i| format!("local a{i}\n")).collect();
        cases.push((
            format!("{n} locals and a for"),
            format!("{inner}for i = 1, 2 do end"),
        ));
    }
    for n in 96..=104 {
        cases.push((
            format!("{n} parameters"),
            format!("function f({}) end", names("p", n).join(", ")),
        ));
        cases.push((
            format!("{n} parameters and ..."),
            format!("function f({}, ...) end", names("p", n - 1).join(", ")),
        ));
    }
    for n in 28..=36 {
        let outer = names("u", n);
        cases.push((
            format!("{n} upvalues"),
            format!(
                "local {}\nfunction f() return {} end",
                outer.join(", "),
                outer.join(" + ")
            ),
        ));
    }
    for n in 240..=256 {
        cases.push((
            format!("call with {n} arguments"),
            format!("f({})", names("g", n).join(", ")),
        ));
        cases.push((
            format!("{n} targets"),
            format!("{} = 1", names("g", n).join(", ")),
        ));
        cases.push((
            format!("{n} values"),
            format!(
                "local x = {{{}}}\nf({})",
                names("g", n).join(", "),
                names("g", n).join(", ")
            ),
        ));
        cases.push((
            format!("{n} concatenated"),
            format!("x = {}", names("g", n).join(" .. ")),
        ));
        cases.push((
            format!("{n} right nested sums"),
            format!(
                "x = {}{}",
                names("g", n)
                    .iter()
                    .map(|g| format!("{g} + ("))
                    .collect::<String>(),
                ")".repeat(n)
            ),
        ));
    }
    for n in 20..=40 {
        cases.push((
            format!("while with {n} terms"),
            format!("while {} do x = 1 end", names("g", n).join(" and ")),
        ));
        cases.push((
            format!("while with {n} comparisons"),
            format!(
                "while {} do x = 1 end",
                (1..=n)
                    .map(|i| format!("g{i} == {i}"))
                    .collect::<Vec<_>>()
                    .join(" or ")
            ),
        ));
    }
    for n in 250..=270 {
        let body: String = (1..=n).map(|i| format!("g = g + {i}.5\n")).collect();
        cases.push((format!("{n} number constants"), body));
        let body: String = (1..=n)
            .map(|i| format!("t[\"k{i}\"] = \"v{i}\"\n"))
            .collect();
        cases.push((format!("{n} string constants in assignments"), body));
        cases.push((
            format!("{n} constants in one table"),
            format!(
                "t = {{{}}}",
                (1..=n)
                    .map(|i| format!("k{i} = {i}.5"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    for n in [31, 32, 33, 63, 64, 65, 66, 500, 1000] {
        cases.push((
            format!("table with {n} items"),
            format!(
                "t = {{{}}}",
                (1..=n)
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
        cases.push((
            format!("table with {n} items and a call"),
            format!(
                "t = {{{}, f()}}",
                (1..=n)
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
        cases.push((
            format!("table with {n} fields and items"),
            format!(
                "t = {{{}}}",
                (1..=n)
                    .map(|i| if i % 3 == 0 {
                        format!("k{i} = {i}")
                    } else {
                        format!("[{i}] = {i}")
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    // a function that is long: jumps of thousands of instructions and many lines
    for n in [1000, 20_000, 70_000] {
        let body: String = (1..=n).map(|_| "g = g + 1\n").collect();
        cases.push((
            format!("if over {n} statements"),
            format!("if g then\n{body}end"),
        ));
        cases.push((
            format!("while over {n} statements"),
            format!("while g do\n{body}end"),
        ));
    }
    let inputs: Vec<Vec<u8>> = cases.iter().map(|(_, s)| s.clone().into_bytes()).collect();
    let real = match compile_real50(&lua, &inputs) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("real Lua failed: {e}");
            return 1;
        }
    };
    let (mut bad, mut errors) = (0, 0);
    for ((name, src), real) in cases.iter().zip(real) {
        let ours = std::panic::catch_unwind(|| luacomp50::compile(src.as_bytes(), b"=t"));
        let verdict = match (&real, &ours) {
            (Ok(a), Ok(Ok(c))) => (*a == luac50::dump(c))
                .then_some(())
                .ok_or("different bytes".to_string()),
            (Err(a), Ok(Err(b))) => {
                errors += 1;
                (*a == b.message)
                    .then_some(())
                    .ok_or(format!("real: {a}\n    ours: {}", b.message))
            }
            (a, Ok(b)) => Err(format!(
                "real {} / ours {}",
                a.as_ref().map_or_else(|e| e.clone(), |_| "ok".into()),
                b.as_ref()
                    .map_or_else(|e| e.message.clone(), |_| "ok".into())
            )),
            (_, Err(_)) => Err("panic".to_string()),
        };
        if let Err(why) = verdict {
            bad += 1;
            println!("DIFF {name}: {why}");
        }
    }
    println!(
        "limits 5.0: {} programs at the edges of the limits: {} compile to identical bytes, {errors} give the same error message, {bad} differ",
        cases.len(),
        cases.len() - errors - bad
    );
    bad
}

/// Which text encodings the string constants of every compiled file use (for the decompiler's
/// round trip): UTF-8 with Hangul/CJK, other non-ASCII, or ASCII only.
fn encodings(files: &[Entry]) {
    fn walk(f: &luac::Function, utf8: &mut usize, other: &mut usize) {
        for k in &f.constants {
            if let luac::Constant::Str(s) = k {
                if s.is_ascii() {
                    continue;
                }
                match std::str::from_utf8(s) {
                    Ok(t) if t.chars().all(|c| c.is_ascii() || c as u32 >= 0x800) => *utf8 += 1,
                    _ => *other += 1,
                }
            }
        }
        for p in &f.protos {
            walk(p, utf8, other);
        }
    }
    let (mut only_utf8, mut mixed, mut only_other, mut ascii) = (0, 0, 0, 0);
    for f in files {
        let Ok(c) = luac::parse_any(&f.data) else {
            continue;
        };
        let (mut u, mut o) = (0, 0);
        walk(&c.main, &mut u, &mut o);
        match (u > 0, o > 0) {
            (true, false) => only_utf8 += 1,
            (true, true) => {
                mixed += 1;
                println!("MIXED {}: {u} UTF-8 strings, {o} other", f.name);
            }
            (false, true) => only_other += 1,
            _ => ascii += 1,
        }
    }
    println!(
        "encodings: {} files: {ascii} ASCII only, {only_other} only CP949/Latin-1, {only_utf8} only UTF-8, {mixed} both",
        files.len()
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--limits50") {
        std::process::exit(i32::from(limits_compile50() > 0));
    }
    if let Some(pos) = args.iter().position(|a| a == "--garbage50") {
        let count = args
            .get(pos + 1)
            .and_then(|n| n.parse().ok())
            .unwrap_or(1000);
        let seed = args.get(pos + 2).and_then(|n| n.parse().ok()).unwrap_or(0);
        std::process::exit(i32::from(garbage_compile50(count, seed) > 0));
    }
    if let Some(pos) = args.iter().position(|a| a == "--fuzz50") {
        let count = args
            .get(pos + 1)
            .and_then(|n| n.parse().ok())
            .unwrap_or(1000);
        let seed = args.get(pos + 2).and_then(|n| n.parse().ok()).unwrap_or(0);
        std::process::exit(i32::from(fuzz_compile50(count, seed) > 0));
    }
    if let Some(pos) = args.iter().position(|a| a == "--fuzz") {
        let count = args
            .get(pos + 1)
            .and_then(|n| n.parse().ok())
            .unwrap_or(1000);
        let seed = args.get(pos + 2).and_then(|n| n.parse().ok()).unwrap_or(0);
        std::process::exit(i32::from(fuzz_compile(count, seed) > 0));
    }
    let Some(grf) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: rgrf-lua-verify <data.grf> [--parse-only | --dump-only]");
        std::process::exit(2);
    };
    let files = compiled_files(grf);
    let mut bad = parse_check(&files);
    if args.iter().any(|a| a == "--encodings") {
        encodings(&files);
        return;
    }
    if args.iter().any(|a| a == "--compile-equal50") {
        std::process::exit(i32::from(compile_equal50_check(&files) > 0));
    }
    if args.iter().any(|a| a == "--compile-equal") {
        std::process::exit(i32::from(compile_equal_check(&files) > 0));
    }
    if args.iter().any(|a| a == "--recompile") {
        std::process::exit(i32::from(recompile_check(&files) > 0));
    }
    if args.iter().any(|a| a == "--dump-only") {
        std::process::exit(i32::from(dump_roundtrip(&files) > 0));
    }
    if !args.iter().any(|a| a == "--parse-only") {
        bad += behaviour_check(&files);
    }
    std::process::exit(i32::from(bad > 0));
}
