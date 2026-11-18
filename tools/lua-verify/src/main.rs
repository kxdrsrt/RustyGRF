//! Checks the Lua decompiler of `rgrf-formats` against a real Lua 5.1 (mlua builds the C sources):
//! 1. the decompiled text of every compiled `.lub` (Lua 5.0 and 5.1) must be accepted by the real
//!    Lua parser;
//! 2. running the original 5.1 bytecode and the decompiled source in a sandbox must give the same
//!    outcome (what the script defined, and what its functions return for a few argument sets).
//!
//! usage: rgrf-lua-verify <data.grf> [--parse-only]   (exit code 1 when something differs)
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
use rgrf_formats::{luac, luadec};

const DRIVER: &str = r##"
local type, pairs, tostring, pcall, setmetatable, getmetatable, rawget, sort, concat, fmt =
  type, pairs, tostring, pcall, setmetatable, getmetatable, rawget, table.sort, table.concat, string.format
local sethook, error, remove, ipairs = debug.sethook, error, table.remove, ipairs
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
  local keys = {}
  for k in pairs(v) do keys[#keys + 1] = k end
  local function kstr(k) return k == P and "<P>" or (type(k) == "table" or type(k) == "function") and "<" .. type(k) .. ">" or type(k) .. ":" .. tostring(k) end
  sort(keys, function(a, b) return kstr(a) < kstr(b) end)
  local parts = {}
  for i = 1, #keys do parts[i] = kstr(keys[i]) .. "=" .. ser(v[keys[i]], seen, depth + 1) end
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
  local out = {}
  local ok, err = pcall(chunk)
  sethook()
  out[#out + 1] = ok and "ran" or (tostring(err):find("timeout", 1, true) and "timeout" or "error")
  local names = {}
  for k, v in pairs(env) do
    if not std[k] then names[#names + 1] = k end
  end
  sort(names, function(a, b) return tostring(a) < tostring(b) end)
  for _, k in ipairs(names) do
    out[#out + 1] = tostring(k) .. " = " .. ser(rawget(env, k), {}, 0)
  end
  if noprobe then sethook(); return concat(out, "\n") end
  local patterns = { {}, { P, P, P, P }, { 1, 2, 3, 4 }, { "a", "b", "c", "d" }, { false, false, false, false } }
  for _, k in ipairs(names) do
    local f = rawget(env, k)
    if type(f) == "function" then
      for n, args in ipairs(patterns) do
        if not nohook then sethook(function() error("timeout", 0) end, "", 2000000) end
        local r = { pcall(f, args[1], args[2], args[3], args[4]) }
        sethook()
        local ok2 = remove(r, 1)
        local text
        if ok2 then
          local parts = {}
          for i = 1, #r do parts[i] = ser(r[i], {}, 0) end
          text = "ok " .. concat(parts, ";")
        else
          text = tostring(r[1]):find("timeout", 1, true) and "timeout" or "error"
        end
        out[#out + 1] = tostring(k) .. "#" .. n .. " -> " .. text
      end
    end
  end
  return concat(out, "\n")
end
"##;

enum Code<'a> {
    Binary(&'a [u8]),
    Text(&'a str),
}

/// Runs `code` in a fresh sandboxed Lua and returns the serialised outcome.
fn run(code: &Code) -> Result<String, String> {
    // SAFETY: a test-only Lua with the debug library (for the instruction-count hook) and binary
    // chunk loading; the sandbox neutralises everything that could touch the machine.
    let lua = unsafe { Lua::unsafe_new() };
    lua.load(DRIVER).exec().map_err(|e| e.to_string())?;
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
        .map(|s| {
            let bytes = s.as_bytes();
            match code {
                // the client's scripts hold CP949 bytes; the decompiled text holds the same characters as UTF-8
                Code::Binary(_) => decode_segments(&bytes),
                Code::Text(_) => String::from_utf8_lossy(&bytes).into_owned(),
            }
        })
        .map_err(|e| e.to_string())
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

/// The decompiled source of every compiled file (5.0 and 5.1) is accepted by the real Lua parser.
fn parse_check(files: &[Entry]) -> usize {
    let lua = unsafe { Lua::unsafe_new() };
    let mut bad = 0;
    for f in files {
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
    println!(
        "parse check: {} of {} decompiled files accepted by real Lua",
        files.len() - bad,
        files.len()
    );
    bad
}

/// Running the original 5.1 bytecode and the decompiled source gives the same outcome.
fn behaviour_check(files: &[Entry]) -> usize {
    let results: Vec<(String, Result<(), String>)> = files
        .par_iter()
        .filter(|f| f.data.get(4) == Some(&0x51))
        .filter(|f| {
            std::env::var("LV_ONLY").is_ok_and(|o| f.name.contains(&o))
                || std::env::var_os("LV_ONLY").is_none()
        })
        .map(|f| {
            if std::env::var_os("LV_TRACE").is_some() {
                eprintln!("running {}", f.name);
            }
            let verdict = (|| {
                let chunk = luac::parse(&f.data).map_err(|e| e.to_string())?;
                let original = luac::dump(&chunk);
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
                let a = run(&Code::Binary(&original))?;
                if trace {
                    eprintln!("  decompiled {}", f.name);
                }
                let b = run(&Code::Text(&text))?;
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
            (f.name.clone(), verdict)
        })
        .collect();
    let bad = results.iter().filter(|(_, r)| r.is_err()).count();
    println!(
        "behaviour check: {} 5.1 files, {} behave the same, {bad} differ",
        results.len(),
        results.len() - bad
    );
    for (n, r) in results.iter().filter(|(_, r)| r.is_err()) {
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
    bad
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(grf) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("usage: rgrf-lua-verify <data.grf> [--parse-only]");
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
