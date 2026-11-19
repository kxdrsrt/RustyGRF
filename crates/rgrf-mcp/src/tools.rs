//! The tools of the server. Every tool is one row: name, description, parameter table, handler.
//! Results are JSON objects; errors are plain messages (reported as `isError` to the client).

use std::collections::HashSet;
use std::path::PathBuf;

use rayon::prelude::*;
use rgrf_core::build::{BuildOptions, build};
use rgrf_core::codec::Codec;
use rgrf_core::encode::Method;
use rgrf_core::extract::{ExtractOptions, OnCollision, extract};
use rgrf_core::hash::Digest;
use rgrf_core::name::hex;
use rgrf_core::progress::Control;
use rgrf_core::repack::{Dedupe, RepackOptions, RepackReport, repack};
use rgrf_core::verify::{Severity, VerifyOptions, verify};
use rgrf_core::writer::Format;
use rgrf_core::{Archive, DecodeStatus, Entry, GrfPath, NameMode};
use serde_json::{Value, json};

use crate::params::{Args, Param, Ty, opt, req};
use crate::policy::Policy;

/// Entries larger than this are not decoded (reported as damaged); untrusted archives must not
/// be able to make the server allocate gigabytes.
const MAX_ENTRY_BYTES: u64 = 256 << 20;

pub struct Ctx<'a> {
    pub policy: &'a Policy,
    pub control: Control<'a>,
}

type Out = Result<Value, String>;

pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub params: &'static [Param],
    /// Creates files (only offered with `--allow-write`).
    pub writes: bool,
    pub run: fn(&Ctx, &Args) -> Out,
}

const NAMES: Param = opt(
    "names",
    Ty::Enum(&["cp1252", "cp949", "utf8", "raw"]),
    "How archive names are shown and typed. Default cp1252 (as GRF Editor shows them); cp949 shows Korean names.",
);
const LIMIT: Param = opt("limit", Ty::Int, "Maximum number of items to return.");
const OVERWRITE: Param = opt(
    "overwrite",
    Ty::Bool,
    "Replace the output file if it exists (default false: stop instead).",
);
const DRY_RUN: Param = opt(
    "dry_run",
    Ty::Bool,
    "Only plan and report sizes, write nothing.",
);
const METHOD: Param = opt(
    "method",
    Ty::Enum(&["zlib", "zopfli", "lzma"]),
    "Compression: zlib (default, every client), zopfli (smaller zlib, very slow: about 8 CPU seconds per MB) or lzma (smallest, only for clients with the LZMA-capable cps.dll).",
);
const METHOD_OR_COPY: Param = opt(
    "method",
    Ty::Enum(&["zlib", "zopfli", "lzma"]),
    "Omit it to copy every stream unchanged (fast, nothing is compressed again, recommended). Set it to compress again and keep the new stream where it is smaller: zlib (every client), zopfli (smaller, very slow) or lzma (smallest, only for clients with the LZMA-capable cps.dll).",
);
const LEVEL: Param = opt(
    "level",
    Ty::Int,
    "zlib 0-9 (default 9), zopfli iterations (default 10), lzma preset 0-9 (default 9).",
);
const ENCRYPT: Param = opt(
    "encrypt",
    Ty::Bool,
    "Encrypt the files with GRF Editor's scheme (0x200 only), using the password of the server's RGRF_PASSWORD (or the key file RGRF_KEY_FILE). Without it, an archive that is opened with a key is written decrypted.",
);
const FORMAT: Param = opt(
    "format",
    Ty::Enum(&["0x200", "0x300", "0x102", "0x103"]),
    "Archive version: 0x200 (max 4 GiB, all clients), 0x300 (newer clients), or the old 0x102/0x103 (scrambled, for very old clients and tools).",
);
const DEDUPE: Param = opt(
    "dedupe",
    Ty::Enum(&["none", "source", "content"]),
    "none: every file its own block; source (default): keep the sharing of the inputs; content: identical files share one block (smallest).",
);
const OUTPUT: Param = req(
    "output",
    Ty::Str,
    "Archive file to create (inside an allowed folder).",
);

/// The decoded bytes of `entry` in the archive `path` and the entry's name as typed in `names`.
pub(crate) fn entry_data(ctx: &Ctx, a: &Args) -> Result<(String, Vec<u8>), String> {
    let arc = open(ctx, a.str("path"))?;
    let mode = names_mode(a);
    let i = entry_index(&arc, a.str("entry"), mode)?;
    let d = arc.read(i).map_err(err)?;
    if d.status == DecodeStatus::Encrypted {
        return Err("entry is encrypted; decryption is not supported yet".into());
    }
    Ok((arc.entries()[i].name.display(mode), d.data))
}

pub const TOOLS: &[Tool] = &[
    Tool {
        name: "grf_info",
        title: "Archive summary",
        description: "Version, size and entry counts of a GRF archive (opens in milliseconds, even for multi-GB files).",
        params: &[
            req("path", Ty::Str, "Path of the .grf file."),
            opt(
                "codecs",
                Ty::Bool,
                "Also count zlib and LZMA entries (reads one byte per entry).",
            ),
        ],
        writes: false,
        run: info,
    },
    Tool {
        name: "grf_list",
        title: "List or search entries",
        description: "Entries of an archive, paged. `pattern` is a case-insensitive substring, or a glob if it contains * or ?; `/` equals `\\`. Use it to search by name or extension.",
        params: &[
            req("path", Ty::Str, "Path of the .grf file."),
            opt(
                "pattern",
                Ty::Str,
                "Substring or glob, e.g. `job4` or `data\\luafiles514\\*.lub`.",
            ),
            opt(
                "ext",
                Ty::StrList,
                "Only these extensions, e.g. [\"spr\", \"act\"].",
            ),
            opt("offset", Ty::Int, "Skip this many matches (paging)."),
            LIMIT,
            NAMES,
        ],
        writes: false,
        run: list,
    },
    Tool {
        name: "grf_read_text",
        title: "Read a text entry",
        description: "Decoded content of a text entry (Lua, TXT, XML, ...), cut at max_bytes. Refuses binary data; use grf_read_summary for those.",
        params: &[
            req("path", Ty::Str, "Path of the .grf file."),
            req(
                "entry",
                Ty::Str,
                "Entry path inside the archive, e.g. `data\\sclientinfo.xml` (case-insensitive, / equals \\).",
            ),
            opt(
                "max_bytes",
                Ty::Int,
                "Maximum bytes to return (default 65536, at most 1048576).",
            ),
            opt(
                "encoding",
                Ty::Enum(&["auto", "utf8", "cp949", "cp1252"]),
                "Text encoding (default auto: UTF-8, else Korean cp949, else cp1252).",
            ),
            NAMES,
        ],
        writes: false,
        run: read_text,
    },
    Tool {
        name: "grf_read_summary",
        title: "Describe an entry",
        description: "Size, CRC32, MD5, guessed file type and the first 64 bytes (hex) of any entry, including binary ones.",
        params: &[
            req("path", Ty::Str, "Path of the .grf file."),
            req("entry", Ty::Str, "Entry path inside the archive."),
            NAMES,
        ],
        writes: false,
        run: read_summary,
    },
    Tool {
        name: "grf_inspect",
        title: "Look inside a client file",
        description: "Reads the format of an entry and returns its structure as JSON: SPR (image sizes), ACT (actions, frames, intervals, sounds), PAL, GAT (cell types), GND (size, textures), RSW (models, sounds, water), RSM/RSM2 (nodes, triangles, animation), STR (layers, textures) and compiled Lua (function count). Use it instead of reading binary bytes.",
        params: &[
            req("path", Ty::Str, "Path of the .grf file."),
            req(
                "entry",
                Ty::Str,
                "Entry path inside the archive, e.g. `data\\prontera.rsw`.",
            ),
            NAMES,
        ],
        writes: false,
        run: crate::inspect::inspect,
    },
    Tool {
        name: "grf_lua_decompile",
        title: "Decompile a compiled Lua file",
        description: "Turns a compiled Lua 5.0 or 5.1 .lub entry back into source text. `complete` is false when some functions could only be shown as a commented bytecode listing (a few control-flow shapes are not understood yet); Lua 5.0 (62 files in the kRO client) is decompiled too. Set `listing` to get the plain bytecode listing instead.",
        params: &[
            req("path", Ty::Str, "Path of the .grf file."),
            req(
                "entry",
                Ty::Str,
                "Entry path inside the archive, e.g. `data\\luafiles514\\lua files\\datainfo\\accessoryid.lub`.",
            ),
            opt(
                "listing",
                Ty::Bool,
                "Return the bytecode listing instead of source (default false).",
            ),
            opt(
                "max_bytes",
                Ty::Int,
                "Maximum bytes of text to return (default 65536, at most 1048576).",
            ),
            NAMES,
        ],
        writes: false,
        run: crate::inspect::lua_decompile,
    },
    Tool {
        name: "grf_verify",
        title: "Check an archive",
        description: "Checks the file table, offsets, overlaps and decodes every entry. Reports errors, warnings and infos (a truncated stream is a warning: the client reads it too).",
        params: &[
            req("path", Ty::Str, "Path of the .grf file."),
            opt(
                "decode",
                Ty::Bool,
                "Decode every entry (default true). false only checks the table.",
            ),
            opt(
                "max_issues",
                Ty::Int,
                "Maximum issues to list (default 50).",
            ),
            NAMES,
        ],
        writes: false,
        run: verify_tool,
    },
    Tool {
        name: "grf_diff",
        title: "Compare two archives",
        description: "Paths only in A, only in B, and with different content (compared by SHA-256 of the decoded data; case and / vs \\ do not matter).",
        params: &[
            req("a", Ty::Str, "First archive."),
            req("b", Ty::Str, "Second archive."),
            opt(
                "limit",
                Ty::Int,
                "Maximum paths to list per group (default 50).",
            ),
            NAMES,
        ],
        writes: false,
        run: diff_tool,
    },
    Tool {
        name: "grf_extract",
        title: "Extract files",
        description: "Extracts entries to a folder. Never replaces existing files unless overwrite is true.",
        params: &[
            req("path", Ty::Str, "Path of the .grf file."),
            req(
                "dest",
                Ty::Str,
                "Folder to write to (inside an allowed folder).",
            ),
            opt(
                "pattern",
                Ty::Str,
                "Only entries matching this substring or glob.",
            ),
            opt(
                "on_collision",
                Ty::Enum(&["abort", "skip", "rename"]),
                "What to do when a target exists (default abort).",
            ),
            OVERWRITE,
            NAMES,
        ],
        writes: true,
        run: extract_tool,
    },
    Tool {
        name: "grf_build",
        title: "Build an archive from a folder",
        description: "Packs a folder into a new GRF; the folder is the archive root (it contains `data/`). Skips .DS_Store and similar, refuses unrepresentable names and case collisions. The result is decoded and compared with the folder.",
        params: &[
            req("source_dir", Ty::Str, "Folder to pack."),
            OUTPUT,
            METHOD,
            LEVEL,
            FORMAT,
            ENCRYPT,
            opt(
                "dedupe",
                Ty::Bool,
                "Let files with identical content share one data block.",
            ),
            OVERWRITE,
            DRY_RUN,
            NAMES,
        ],
        writes: true,
        run: build_tool,
    },
    Tool {
        name: "grf_repack",
        title: "Recompress an archive",
        description: "Writes a new archive with the same files but another compression, version or sharing. Content never changes; every entry of the result is decoded and compared with the source.",
        params: &[
            req("path", Ty::Str, "Source archive."),
            OUTPUT,
            METHOD,
            LEVEL,
            FORMAT,
            ENCRYPT,
            DEDUPE,
            OVERWRITE,
            DRY_RUN,
        ],
        writes: true,
        run: repack_tool,
    },
    Tool {
        name: "grf_merge",
        title: "Merge archives",
        description: "Merges archives into one. For every path the FIRST archive of `paths` wins (like the lowest index in DATA.INI): list custom GRFs first and data.grf last.",
        params: &[
            req("paths", Ty::StrList, "Archives, highest priority first."),
            opt(
                "replace_only",
                Ty::Bool,
                "The LAST archive is the base: earlier archives replace its files but add no new paths (update a GRF with a newer translation without pulling in everything else).",
            ),
            OUTPUT,
            METHOD_OR_COPY,
            LEVEL,
            FORMAT,
            ENCRYPT,
            DEDUPE,
            OVERWRITE,
            DRY_RUN,
        ],
        writes: true,
        run: merge_tool,
    },
    Tool {
        name: "grf_edit",
        title: "Edit an archive (replace, add, remove files)",
        description: "Writes a changed COPY of an archive: files from disk or text replace or add paths, other paths are left out. The original is never modified and unchanged entries keep their streams (no recompression), so editing a few files of a large archive is fast. Give at least one of `put`, `put_text`, `remove`.",
        params: &[
            req("path", Ty::Str, "The archive to edit (it stays unchanged)."),
            opt(
                "put",
                Ty::StrList,
                "Files to replace or add, each \"ARCHIVE_PATH=FILE\", e.g. \"data/clientinfo.xml=/work/clientinfo.xml\" (the file must be in a readable folder).",
            ),
            opt(
                "put_text",
                Ty::StrList,
                "Text files to replace or add, each \"ARCHIVE_PATH=TEXT\" (use `\\n` in JSON for line breaks, no conversion is made); written in `text_encoding`.",
            ),
            opt(
                "text_encoding",
                Ty::Enum(&["utf8", "cp949", "cp1252"]),
                "Encoding of `put_text` (default utf8).",
            ),
            opt(
                "remove",
                Ty::StrList,
                "Archive paths to leave out of the result.",
            ),
            NAMES,
            OUTPUT,
            METHOD_OR_COPY,
            LEVEL,
            FORMAT,
            ENCRYPT,
            DEDUPE,
            OVERWRITE,
            DRY_RUN,
        ],
        writes: true,
        run: edit_tool,
    },
    Tool {
        name: "grf_patch",
        title: "Create a patch archive",
        description: "Writes an archive with only the files that are new or different in `new` compared with `old`. List it before `old` in DATA.INI. Paths removed in `new` cannot be expressed in a GRF.",
        params: &[
            req("old", Ty::Str, "The older archive."),
            req("new", Ty::Str, "The newer archive."),
            OUTPUT,
            METHOD_OR_COPY,
            LEVEL,
            FORMAT,
            ENCRYPT,
            DEDUPE,
            OVERWRITE,
            DRY_RUN,
        ],
        writes: true,
        run: patch_tool,
    },
];

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn names_mode(a: &Args) -> NameMode {
    a.opt_str("names")
        .and_then(NameMode::parse)
        .unwrap_or_default()
}

/// The key of GRF Editor's encryption from the server's environment: `RGRF_PASSWORD` or the key
/// file `RGRF_KEY_FILE`. The password is not a tool argument so that it stays out of the chat.
fn env_key() -> Result<Option<rgrf_core::grfcrypt::Key>, String> {
    use rgrf_core::grfcrypt;
    if let Some(p) = std::env::var("RGRF_PASSWORD")
        .ok()
        .filter(|p| !p.is_empty())
    {
        return grfcrypt::key_from_password(&p)
            .map(Some)
            .map_err(|e| e.to_string());
    }
    if let Some(f) = std::env::var("RGRF_KEY_FILE")
        .ok()
        .filter(|f| !f.is_empty())
    {
        let bytes = std::fs::read(&f).map_err(|e| format!("RGRF_KEY_FILE {f}: {e}"))?;
        return grfcrypt::key_from_file(&bytes)
            .map(Some)
            .map_err(|e| e.to_string());
    }
    Ok(None)
}

fn open(ctx: &Ctx, path: &str) -> Result<Archive, String> {
    let p = ctx.policy.read(path)?;
    let mut a = match env_key()? {
        Some(k) => Archive::open_with_key(&p, &k),
        None => Archive::open(&p),
    }
    .map_err(|e| format!("{path}: {e}"))?;
    a.set_max_entry_size(MAX_ENTRY_BYTES);
    Ok(a)
}

fn entry_index(a: &Archive, text: &str, mode: NameMode) -> Result<usize, String> {
    let p = GrfPath::from_display(text, mode)
        .ok_or_else(|| format!("'{text}' cannot be written in the name mode {mode:?}"))?;
    a.find(&p).ok_or_else(|| {
        format!("no entry named '{text}' (names are compared case-insensitively, / equals \\)")
    })
}

fn kind(e: &Entry) -> &'static str {
    if e.is_removal() {
        "remove"
    } else if e.is_marker() {
        "marker"
    } else if e.is_raw() {
        "raw"
    } else if e.needs_key() {
        "encrypted"
    } else if e.is_des() {
        "des"
    } else {
        "file"
    }
}

fn entry_json(i: usize, e: &Entry, mode: NameMode) -> Value {
    json!({
        "index": i, "name": e.name.display(mode), "size": e.size, "packed_size": e.zsize,
        "kind": kind(e),
    })
}

fn status_json(s: &DecodeStatus) -> Value {
    match s {
        DecodeStatus::Ok => json!({"state": "ok"}),
        DecodeStatus::Truncated { got, expected } => {
            json!({"state": "truncated", "got": got, "expected": expected})
        }
        DecodeStatus::SizeMismatch { got, expected } => {
            json!({"state": "size_mismatch", "got": got, "expected": expected})
        }
        DecodeStatus::Corrupt(m) => json!({"state": "corrupt", "message": m}),
        DecodeStatus::Encrypted => json!({"state": "encrypted"}),
    }
}

fn limit(a: &Args, default: u64, max: u64) -> usize {
    a.opt_u64("limit").unwrap_or(default).min(max) as usize
}

/// First `n` names, and whether there were more.
fn names_json(v: &[GrfPath], n: usize, mode: NameMode) -> Value {
    json!({
        "count": v.len(), "truncated": v.len() > n,
        "paths": v.iter().take(n).map(|p| p.display(mode)).collect::<Vec<_>>(),
    })
}

fn info(ctx: &Ctx, a: &Args) -> Out {
    let arc = open(ctx, a.str("path"))?;
    let es = arc.entries();
    let markers = es.iter().filter(|e| e.is_marker()).count();
    let mut seen = HashSet::new();
    let shared = es
        .iter()
        .filter(|e| e.zsize > 0 && !seen.insert((e.offset, e.zsize)))
        .count();
    let t = arc.table_info();
    let mut v = json!({
        "path": a.str("path"),
        "file_size": arc.file_len(),
        "version": format!("0x{:03x}", arc.header().version),
        "entries": es.len(), "files": es.len() - markers, "markers": markers,
        "stored_uncompressed": es.iter().filter(|e| e.is_raw()).count(),
        "encrypted": es.iter().filter(|e| e.needs_key()).count(),
        "des_scrambled": es.iter().filter(|e| e.is_des()).count(),
        "entries_sharing_data": shared,
        "packed_bytes": es.iter().map(|e| u64::from(e.zsize)).sum::<u64>(),
        "unpacked_bytes": es.iter().map(|e| u64::from(e.size)).sum::<u64>(),
        "table": {"offset": t.abs_offset, "packed": t.zsize, "unpacked": t.size, "key_block": t.key_block},
    });
    if a.flag("codecs") {
        let c: Vec<Option<Codec>> = (0..arc.len())
            .into_par_iter()
            .map(|i| arc.sniff_codec(i))
            .collect::<Result<_, _>>()
            .map_err(err)?;
        v["codecs"] = json!({
            "zlib": c.iter().filter(|c| **c == Some(Codec::Zlib)).count(),
            "lzma": c.iter().filter(|c| **c == Some(Codec::Lzma)).count(),
        });
    }
    Ok(v)
}

fn list(ctx: &Ctx, a: &Args) -> Out {
    let arc = open(ctx, a.str("path"))?;
    let mode = names_mode(a);
    let wanted: Vec<Vec<u8>> = a
        .list("ext")
        .iter()
        .map(|e| e.trim_start_matches('.').to_ascii_lowercase().into_bytes())
        .collect();
    let all: Vec<usize> = arc
        .select(a.opt_str("pattern"), mode)
        .into_iter()
        .filter(|&i| {
            wanted.is_empty()
                || arc.entries()[i]
                    .name
                    .extension_lowercase()
                    .is_some_and(|x| wanted.contains(&x))
        })
        .collect();
    let offset = (a.opt_u64("offset").unwrap_or(0) as usize).min(all.len());
    let n = limit(a, 100, 1000);
    let page = &all[offset..all.len().min(offset + n)];
    let end = offset + page.len();
    Ok(json!({
        "total": all.len(), "offset": offset, "returned": page.len(),
        "truncated": end < all.len(),
        "next_offset": (end < all.len()).then_some(end),
        "entries": page.iter().map(|&i| entry_json(i, &arc.entries()[i], mode)).collect::<Vec<_>>(),
    }))
}

fn looks_binary(data: &[u8]) -> bool {
    data.iter().take(4096).any(|&b| b == 0)
}

/// UTF-8 prefix; an incomplete character at a cut end is dropped when the data was truncated.
fn utf8_prefix(data: &[u8], truncated: bool) -> Option<&str> {
    match std::str::from_utf8(data) {
        Ok(s) => Some(s),
        Err(e) if truncated && data.len() - e.valid_up_to() < 4 && e.error_len().is_none() => {
            std::str::from_utf8(&data[..e.valid_up_to()]).ok()
        }
        Err(_) => None,
    }
}

fn decode_text(
    data: &[u8],
    encoding: &str,
    truncated: bool,
) -> Result<(String, &'static str), String> {
    let data = data.strip_prefix(b"\xef\xbb\xbf").unwrap_or(data);
    let legacy = |enc: &'static encoding_rs::Encoding, label| {
        let (s, _) = enc.decode_without_bom_handling(data);
        (s.into_owned(), label)
    };
    match encoding {
        "utf8" => utf8_prefix(data, truncated)
            .map(|s| (s.to_owned(), "utf8"))
            .ok_or_else(|| "not valid UTF-8 (try encoding=cp949 or cp1252)".to_string()),
        "cp949" => Ok(legacy(encoding_rs::EUC_KR, "cp949")),
        "cp1252" => Ok(legacy(encoding_rs::WINDOWS_1252, "cp1252")),
        _ => {
            if let Some(s) = utf8_prefix(data, truncated) {
                return Ok((s.to_owned(), "utf8"));
            }
            let (s, _, had_errors) = encoding_rs::EUC_KR.decode(data);
            if !had_errors {
                return Ok((s.into_owned(), "cp949"));
            }
            Ok(legacy(encoding_rs::WINDOWS_1252, "cp1252"))
        }
    }
}

fn read_text(ctx: &Ctx, a: &Args) -> Out {
    let arc = open(ctx, a.str("path"))?;
    let mode = names_mode(a);
    let i = entry_index(&arc, a.str("entry"), mode)?;
    let d = arc.read(i).map_err(err)?;
    if d.status == DecodeStatus::Encrypted {
        return Err("entry is encrypted; decryption is not supported yet".into());
    }
    if looks_binary(&d.data) {
        return Err("entry looks binary; use grf_read_summary".into());
    }
    let max = a.opt_u64("max_bytes").unwrap_or(65_536).clamp(1, 1 << 20) as usize;
    let truncated = d.data.len() > max;
    let (text, used) = decode_text(
        &d.data[..d.data.len().min(max)],
        a.opt_str("encoding").unwrap_or("auto"),
        truncated,
    )?;
    Ok(json!({
        "entry": arc.entries()[i].name.display(mode), "size": d.data.len(),
        "status": status_json(&d.status), "encoding": used, "truncated": truncated, "text": text,
    }))
}

fn guess_type(d: &[u8]) -> &'static str {
    let starts = |m: &[u8]| d.starts_with(m);
    if starts(b"SP") {
        "SPR sprite"
    } else if starts(b"AC") {
        "ACT animation"
    } else if starts(b"GRSW") {
        "RSW map resources"
    } else if starts(b"GRGN") {
        "GND ground"
    } else if starts(b"GRAT") {
        "GAT altitude"
    } else if starts(b"GRSM") {
        "RSM model"
    } else if starts(b"BM") {
        "BMP image"
    } else if starts(b"\x89PNG") {
        "PNG image"
    } else if starts(&[0xFF, 0xD8, 0xFF]) {
        "JPEG image"
    } else if starts(b"RIFF") && d.get(8..12) == Some(b"WAVE") {
        "WAV audio"
    } else if starts(b"OggS") {
        "Ogg audio"
    } else if starts(b"\x1bLua") {
        "compiled Lua"
    } else if starts(b"<?xml") {
        "XML text"
    } else if !d.is_empty()
        && !looks_binary(d)
        && std::str::from_utf8(&d[..d.len().min(512)]).is_ok()
    {
        "text"
    } else {
        "unknown binary"
    }
}

fn read_summary(ctx: &Ctx, a: &Args) -> Out {
    let arc = open(ctx, a.str("path"))?;
    let mode = names_mode(a);
    let i = entry_index(&arc, a.str("entry"), mode)?;
    let e = &arc.entries()[i];
    let mut v = entry_json(i, e, mode);
    if e.is_marker() {
        return Ok(v);
    }
    let d = arc.read(i).map_err(err)?;
    v["status"] = status_json(&d.status);
    if d.status != DecodeStatus::Encrypted {
        let dg = Digest::of(&d.data);
        v["decoded_size"] = d.data.len().into();
        v["crc32"] = dg.crc32_hex().into();
        v["md5"] = dg.md5_hex().into();
        v["type"] = guess_type(&d.data).into();
        v["preview_hex"] = hex(&d.data[..d.data.len().min(64)]).into();
    }
    Ok(v)
}

fn verify_tool(ctx: &Ctx, a: &Args) -> Out {
    let arc = open(ctx, a.str("path"))?;
    let mode = names_mode(a);
    let decode = a.opt_bool("decode").unwrap_or(true);
    let r = verify(&arc, VerifyOptions { decode }, &ctx.control).map_err(err)?;
    let max = a.opt_u64("max_issues").unwrap_or(50).min(1000) as usize;
    let label = |s: Severity| -> &'static str { s.into() };
    let issues: Vec<Value> = r
        .issues
        .iter()
        .take(max)
        .map(|i| {
            let mut v = json!({
                "severity": label(i.severity), "kind": format!("{:?}", i.kind),
                "message": i.message,
            });
            if let Some(e) = i.entry {
                v["entry"] = entry_json(e, &arc.entries()[e], mode);
            }
            v
        })
        .collect();
    Ok(json!({
        "entries": r.entries, "files": r.files, "markers": r.markers, "raw": r.raw,
        "decoded": r.decoded,
        "errors": r.count(Severity::Error), "warnings": r.count(Severity::Warning),
        "infos": r.count(Severity::Info),
        "ok": r.count(Severity::Error) == 0,
        "issues": issues, "issues_truncated": r.issues.len() > max,
    }))
}

fn diff_tool(ctx: &Ctx, a: &Args) -> Out {
    let (x, y) = (open(ctx, a.str("a"))?, open(ctx, a.str("b"))?);
    let mode = names_mode(a);
    let n = limit(a, 50, 1000);
    let r = rgrf_core::merge::diff(&x, &y, &ctx.control).map_err(err)?;
    Ok(json!({
        "identical": r.is_identical(), "same": r.same,
        "only_in_a": names_json(&r.only_a, n, mode),
        "only_in_b": names_json(&r.only_b, n, mode),
        "different_content": names_json(&r.changed, n, mode),
        "hidden_by_later_duplicate": {"a": r.shadowed_a, "b": r.shadowed_b},
    }))
}

fn extract_tool(ctx: &Ctx, a: &Args) -> Out {
    let arc = open(ctx, a.str("path"))?;
    let mode = names_mode(a);
    let dest = ctx.policy.write_dir(a.str("dest"))?;
    let idx = arc.select(a.opt_str("pattern"), mode);
    let opts = ExtractOptions {
        names: mode,
        on_collision: match a.opt_str("on_collision") {
            Some("skip") => OnCollision::Skip,
            Some("rename") => OnCollision::Rename,
            _ => OnCollision::Abort,
        },
        overwrite: a.flag("overwrite"),
    };
    let r = extract(&arc, &idx, &dest, &opts, &ctx.control).map_err(err)?;
    Ok(json!({
        "dest": dest.display().to_string(), "selected": r.selected, "written": r.written,
        "bytes": r.bytes, "skipped_markers": r.skipped_markers,
        "skipped_encrypted": r.skipped_encrypted, "skipped_collisions": r.skipped_collisions,
        "renamed": r.renamed,
        "damaged": r.damaged.iter().take(20).map(|d| json!({
            "name": arc.entries()[d.index].name.display(mode), "status": status_json(&d.status),
        })).collect::<Vec<_>>(),
    }))
}

fn method(a: &Args) -> Result<Method, String> {
    let level = a
        .opt_u64("level")
        .map(|l| l.min(u64::from(u32::MAX)) as u32);
    let m = match a.opt_str("method").unwrap_or("zlib") {
        "zopfli" => Method::Zopfli {
            iterations: u64::from(level.unwrap_or(10)),
        },
        "lzma" => Method::Lzma {
            preset: level.unwrap_or(9),
        },
        _ => Method::Zlib {
            level: level.unwrap_or(9),
        },
    };
    if let Method::Zlib { level } | Method::Lzma { preset: level } = m
        && level > 9
    {
        return Err("level must be 0-9 for zlib and lzma".into());
    }
    Ok(m)
}

fn format(a: &Args) -> Option<Format> {
    a.opt_str("format").and_then(|f| match f {
        "0x200" => Some(Format::V200),
        "0x300" => Some(Format::V300),
        "0x102" => Some(Format::V102),
        "0x103" => Some(Format::V103),
        _ => None,
    })
}

/// The key for `encrypt: true`.
fn encrypt_key(a: &Args) -> Result<Option<rgrf_core::grfcrypt::Key>, String> {
    if !a.flag("encrypt") {
        return Ok(None);
    }
    env_key()?.map(Some).ok_or_else(|| {
        "encrypt needs RGRF_PASSWORD or RGRF_KEY_FILE in the server's environment".to_string()
    })
}

/// `copy_by_default`: without `method` the streams are copied unchanged (merge, patch).
fn repack_options(a: &Args, copy_by_default: bool) -> Result<RepackOptions, String> {
    Ok(RepackOptions {
        method: method(a)?,
        copy_streams: copy_by_default && a.opt_str("method").is_none(),
        format: format(a),
        dedupe: match a.opt_str("dedupe") {
            Some("none") => Dedupe::None,
            Some("content") => Dedupe::Content,
            _ => Dedupe::Source,
        },
        overwrite: a.flag("overwrite"),
        dry_run: a.flag("dry_run"),
        encrypt: encrypt_key(a)?,
        ..RepackOptions::default()
    })
}

fn repack_json(r: &RepackReport, output: &std::path::Path) -> Value {
    json!({
        "output": output.display().to_string(), "dry_run": r.dry_run,
        "format": format!("0x{:03x}", r.format),
        "files": r.files, "data_blocks": r.blocks, "files_sharing_a_block": r.shared_files,
        "kept_original_streams": r.kept_original, "re_encoded": r.recompressed,
        "copied_unchanged": {
            "stored_uncompressed": r.stored_blocks, "encrypted": r.encrypted_blocks,
            "damaged": r.damaged.len(),
        },
        "input_file_bytes": r.source_file_len, "output_file_bytes": r.output_file_len,
        "output_stream_bytes": r.output_stream_bytes, "verified": r.verified,
    })
}

/// Output path for a tool that writes an archive; never one of the inputs.
fn output_path(ctx: &Ctx, a: &Args, inputs: &[PathBuf]) -> Result<PathBuf, String> {
    let out = ctx
        .policy
        .write_file(a.str("output"), a.flag("overwrite"))?;
    if inputs.contains(&out) {
        return Err("the output must not be one of the input archives".into());
    }
    Ok(out)
}

fn build_tool(ctx: &Ctx, a: &Args) -> Out {
    let src = ctx.policy.read(a.str("source_dir"))?;
    let out = output_path(ctx, a, &[])?;
    let opts = BuildOptions {
        method: method(a)?,
        format: format(a).unwrap_or(Format::V200),
        dedupe: a.flag("dedupe"),
        names: names_mode(a),
        overwrite: a.flag("overwrite"),
        dry_run: a.flag("dry_run"),
        encrypt: encrypt_key(a)?,
        ..BuildOptions::default()
    };
    let r = build(&src, &out, &opts, &ctx.control).map_err(err)?;
    Ok(json!({
        "output": out.display().to_string(), "dry_run": r.dry_run,
        "format": format!("0x{:03x}", r.format),
        "files": r.files, "data_blocks": r.blocks, "files_sharing_a_block": r.shared_files,
        "unpacked_bytes": r.unpacked_bytes, "output_stream_bytes": r.output_stream_bytes,
        "output_file_bytes": r.output_file_len, "verified": r.verified,
        "skipped": {
            "count": r.skipped.len(),
            "items": r.skipped.iter().take(20).map(|s| json!({
                "path": s.path.display().to_string(), "reason": s.reason,
            })).collect::<Vec<_>>(),
        },
    }))
}

fn repack_tool(ctx: &Ctx, a: &Args) -> Out {
    let src = ctx.policy.read(a.str("path"))?;
    let out = output_path(ctx, a, std::slice::from_ref(&src))?;
    let arc = open(ctx, a.str("path"))?;
    let r = repack(&arc, &out, &repack_options(a, false)?, &ctx.control).map_err(err)?;
    Ok(repack_json(&r, &out))
}

fn merge_tool(ctx: &Ctx, a: &Args) -> Out {
    let paths = a.list("paths");
    let inputs: Vec<PathBuf> = paths
        .iter()
        .map(|p| ctx.policy.read(p))
        .collect::<Result<_, _>>()?;
    let out = output_path(ctx, a, &inputs)?;
    let archives: Vec<Archive> = paths
        .iter()
        .map(|p| open(ctx, p))
        .collect::<Result<_, _>>()?;
    let refs: Vec<&Archive> = archives.iter().collect();
    let r = rgrf_core::merge::merge_with(
        &refs,
        &out,
        &repack_options(a, true)?,
        a.flag("replace_only"),
        &ctx.control,
    )
    .map_err(err)?;
    let mut v = repack_json(&r.repack, &out);
    v["inputs"] = paths
        .iter()
        .zip(&r.sources)
        .map(|(p, s)| {
            json!({
                "path": p, "files": s.files, "used_in_result": s.used,
                "replaced_by_higher_priority": s.overridden, "hidden_by_later_duplicate": s.shadowed,
                "new_paths_ignored": s.ignored_new,
                "empty_markers_not_copied": s.markers_skipped,
            })
        })
        .collect::<Vec<_>>()
        .into();
    Ok(v)
}

fn edit_tool(ctx: &Ctx, a: &Args) -> Out {
    use rgrf_core::edit::{Edit, apply, archive_path, split_put};
    let (put, put_text, remove) = (a.list("put"), a.list("put_text"), a.list("remove"));
    if put.is_empty() && put_text.is_empty() && remove.is_empty() {
        return Err("nothing to do: give `put`, `put_text` or `remove`".into());
    }
    let src = ctx.policy.read(a.str("path"))?;
    let out = output_path(ctx, a, std::slice::from_ref(&src))?;
    let mode = names_mode(a);
    let mut edits = Vec::new();
    for spec in put {
        let (name, file) = split_put(spec).map_err(err)?;
        let f = ctx.policy.read(file)?;
        let len = std::fs::metadata(&f).map_err(err)?.len();
        if len > MAX_ENTRY_BYTES {
            return Err(format!("{file}: larger than {MAX_ENTRY_BYTES} bytes"));
        }
        edits.push(Edit::Put {
            name: archive_path(name, mode).map_err(err)?,
            data: std::fs::read(&f).map_err(|e| format!("{file}: {e}"))?,
        });
    }
    let enc = match a.opt_str("text_encoding") {
        Some("cp949") => Some(encoding_rs::EUC_KR),
        Some("cp1252") => Some(encoding_rs::WINDOWS_1252),
        _ => None,
    };
    for spec in put_text {
        let (name, text) = split_put(spec).map_err(err)?;
        let data = match enc {
            None => text.as_bytes().to_vec(),
            Some(e) => {
                let (bytes, _, bad) = e.encode(text);
                if bad {
                    return Err(format!(
                        "{name}: the text cannot be written in {}",
                        e.name()
                    ));
                }
                bytes.into_owned()
            }
        };
        edits.push(Edit::Put {
            name: archive_path(name, mode).map_err(err)?,
            data,
        });
    }
    for name in remove {
        edits.push(Edit::Remove {
            name: archive_path(name, mode).map_err(err)?,
        });
    }
    let arc = open(ctx, a.str("path"))?;
    let r = apply(
        &arc,
        &src,
        &out,
        &edits,
        &repack_options(a, true)?,
        &ctx.control,
    )
    .map_err(err)?;
    let mut v = repack_json(&r.merge.repack, &out);
    v["added"] = r.added.into();
    v["replaced"] = r.replaced.into();
    v["removed"] = r.removed.into();
    Ok(v)
}

fn patch_tool(ctx: &Ctx, a: &Args) -> Out {
    let inputs = [
        ctx.policy.read(a.str("old"))?,
        ctx.policy.read(a.str("new"))?,
    ];
    let out = output_path(ctx, a, &inputs)?;
    let (old, new) = (open(ctx, a.str("old"))?, open(ctx, a.str("new"))?);
    let r = rgrf_core::merge::patch(&old, &new, &out, &repack_options(a, true)?, &ctx.control)
        .map_err(err)?;
    let mut v = repack_json(&r.repack, &out);
    v["added"] = r.added.into();
    v["changed"] = r.changed.into();
    v["removed_in_new"] = r.removed.into();
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_are_unique_and_prefixed() {
        let mut seen = HashSet::new();
        for t in TOOLS {
            assert!(
                t.name.starts_with("grf_") && seen.insert(t.name),
                "{}",
                t.name
            );
            assert!(!t.description.is_empty());
        }
    }

    #[test]
    fn text_decoding_handles_utf8_korean_and_cut_characters() {
        let (s, e) = decode_text("héllo".as_bytes(), "auto", false).unwrap();
        assert_eq!((s.as_str(), e), ("héllo", "utf8"));
        // "한글" in cp949
        let (s, e) = decode_text(&[0xC7, 0xD1, 0xB1, 0xDB], "auto", false).unwrap();
        assert_eq!((s.as_str(), e), ("한글", "cp949"));
        // a UTF-8 character cut in the middle by max_bytes is dropped, not an error
        let cut = &"aé".as_bytes()[..2];
        assert_eq!(decode_text(cut, "utf8", true).unwrap().0, "a");
        assert!(decode_text(cut, "utf8", false).is_err());
        assert_eq!(
            decode_text(b"\xef\xbb\xbfhi", "utf8", false).unwrap().0,
            "hi"
        );
    }

    #[test]
    fn file_types_are_recognised_by_their_first_bytes() {
        assert_eq!(guess_type(b"SP\x01\x02"), "SPR sprite");
        assert_eq!(guess_type(b"RIFF\0\0\0\0WAVEfmt"), "WAV audio");
        assert_eq!(guess_type(b"hello world"), "text");
        assert_eq!(guess_type(&[0, 1, 2, 3]), "unknown binary");
    }
}
