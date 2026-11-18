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
use rgrf_formats::{luac, luac50, luadec};

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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(grf) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: rgrf-lua-verify <data.grf> [--parse-only | --dump-only]");
        std::process::exit(2);
    };
    let files = compiled_files(grf);
    let mut bad = parse_check(&files);
    if args.iter().any(|a| a == "--dump-only") {
        std::process::exit(i32::from(dump_roundtrip(&files) > 0));
    }
    if !args.iter().any(|a| a == "--parse-only") {
        bad += behaviour_check(&files);
    }
    std::process::exit(i32::from(bad > 0));
}
