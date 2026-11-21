//! The tools of the server. Every tool is one row: name, description, parameter table, handler.
//! Results are JSON objects; errors are plain messages (reported as `isError` to the client).

use std::collections::{HashMap, HashSet};
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
        description: "Reads the format of an entry and returns its structure as JSON: SPR (image sizes), ACT (actions, frames, intervals, sounds), PAL, BMP and TGA (size, bit depth, colours, transparent pixels), GAT (cell types), GND (size, textures), RSW (models, sounds, water), RSM/RSM2 (nodes, triangles, animation), STR (layers, textures) and compiled Lua (function count). Use it instead of reading binary bytes.",
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
        name: "grf_map_files",
        title: "List the files a map needs",
        description: "Every file one map needs, starting from data\\<map>.rsw: world, ground and its textures, altitude, models and their textures, sounds, water pictures, minimap, effect script and its textures; each with `found` and the file that names it. To copy a map, give the found paths to `grf_extract`.",
        params: &[
            req("path", Ty::Str, "Path of the .grf file."),
            req(
                "map",
                Ty::Str,
                "The map: `prontera`, `prontera.rsw` or `data\\prontera.rsw`.",
            ),
            NAMES,
            LIMIT,
        ],
        writes: false,
        run: map_files_tool,
    },
    Tool {
        name: "grf_client_refs",
        title: "Check what animations and effects name",
        description: "Checks every animation (.act) and effect (.str) of the archives against the files that exist: the .spr next to an animation, the images its layers use (a layer asking for an image the sprite does not have), the sounds its frames play (`data\\wav\\<name>`, from ACT 2.1; \"atk\" marks the frame that hits and is no file) and the textures of an effect. Give ALL archives of the client, highest priority first (as in DATA.INI): with only one, sprites that live in another archive are reported missing. Names inside Lua tables are not checked. Missing sprites are counted per folder; they are often harmless leftovers (the kRO robe animations), so look at the other kinds first.",
        params: &[
            req(
                "paths",
                Ty::StrList,
                "The archives of the client, highest priority first.",
            ),
            opt(
                "only",
                Ty::Enum(&["act", "str"]),
                "Only animations (act) or only effects (str).",
            ),
            NAMES,
            LIMIT,
        ],
        writes: false,
        run: client_refs_tool,
    },
    Tool {
        name: "grf_client_unused",
        title: "Find files nothing names",
        description: "Lists the textures, sprites, sounds and models of the archives that no map, animation, effect, text, Lua table or XML names, per folder with the stored size: candidates to leave out of a smaller client. Give ALL archives of the client, highest priority first. Names the client puts together while it runs cannot be seen, so the result is only \"probably unused\": folders of maps (`names_are_composed` false: ground textures, models) are reliable, sprites, effects, interface and sounds are mostly false alarms (e.g. the robe sprites). Programs next to the archives are not scanned here (the CLI does that).",
        params: &[
            req(
                "paths",
                Ty::StrList,
                "The archives of the client, highest priority first.",
            ),
            NAMES,
            LIMIT,
        ],
        writes: false,
        run: client_unused_tool,
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
        name: "grf_image_reduce",
        title: "Shrink the true-colour BMPs of an archive",
        description: "Writes a COPY of an archive whose 16/24/32-bit BMPs are stored as 8-bit BMPs (256 colours) where the picture keeps at least `min_psnr` dB and the stored file gets smaller. Magenta, the transparent colour, stays exact and no other colour becomes transparent; pictures with at most 256 colours lose nothing. The client loads 8-bit BMPs (21,461 of kRO's own are). Use `dry_run` first to see the savings.",
        params: &[
            req("path", Ty::Str, "The archive (it stays unchanged)."),
            opt(
                "min_psnr",
                Ty::Int,
                "Lowest picture quality in dB (default 40, hard to tell from the original; 35 shows bands on smooth gradients).",
            ),
            opt(
                "only",
                Ty::Str,
                "Which entries, as a glob (default \"*.bmp\").",
            ),
            opt(
                "recompress",
                Ty::Bool,
                "Also compress every unchanged entry again with `method` (needs `method`), e.g. a zlib archive to lzma (about 20 % smaller; needs the LZMA-capable cps.dll). Default: unchanged entries are copied as they are.",
            ),
            NAMES,
            OUTPUT,
            METHOD_OR_COPY,
            LEVEL,
            DEDUPE,
            OVERWRITE,
            DRY_RUN,
        ],
        writes: true,
        run: image_reduce_tool,
    },
    Tool {
        name: "grf_flat_maps",
        title: "Make maps flat (patch archive)",
        description: "GRF Editor's flat maps maker: writes a small archive with only the changed world, ground and altitude files of the maps (all, or `maps`). Options: `flatten` (ground and walk grid at height 0, no walls, objects or water), `remove_light` (coloured light of the lightmaps), `remove_shadow`, `remove_color` (tile colours white), `gat_shadows` (new shadows from the walk grid; blocked cells dark), `reset_light` (neutral sun), `remove_objects` (all) or `remove_models`/`remove_sounds`/`remove_effects`, `remove_water`. Without any option: flatten, remove_light and remove_shadow. Where players can walk does not change. List the output first in DATA.INI.",
        params: &[
            req(
                "path",
                Ty::Str,
                "The archive with the maps (it stays unchanged).",
            ),
            opt(
                "maps",
                Ty::StrList,
                "Only these maps: `prontera` or `data\\prontera.rsw` (default: every .rsw).",
            ),
            opt(
                "flatten",
                Ty::Bool,
                "Flat ground, no walls, objects or water.",
            ),
            opt(
                "remove_light",
                Ty::Bool,
                "Remove the coloured light of the lightmaps.",
            ),
            opt(
                "remove_shadow",
                Ty::Bool,
                "Remove the shadows of the lightmaps.",
            ),
            opt("remove_color", Ty::Bool, "Tile colours become white."),
            opt(
                "gat_shadows",
                Ty::Bool,
                "Shadows from the walk grid (replaces light and shadows).",
            ),
            opt("reset_light", Ty::Bool, "Neutral sun and ambient light."),
            opt(
                "remove_objects",
                Ty::Bool,
                "Remove every object (models, lights, sounds, effects).",
            ),
            opt("remove_models", Ty::Bool, "Remove the models only."),
            opt("remove_sounds", Ty::Bool, "Remove the sounds only."),
            opt("remove_effects", Ty::Bool, "Remove the effects only."),
            opt("remove_water", Ty::Bool, "Move the water below the ground."),
            NAMES,
            OUTPUT,
            OVERWRITE,
            DRY_RUN,
        ],
        writes: true,
        run: flat_maps_tool,
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
    Tool {
        name: "grf_downgrade_maps",
        title: "Downgrade maps for old clients (patch archive)",
        description: "GRF Editor's Downgrade on maps: writes a small archive with the world file (version 2.1, the water of the ground file), ground file (1.7) and the models of the maps, whose RSM2 models (2.2 and 2.3) become RSM 1.4 (`.rsm`) with the objects moved to fit. Models whose nodes turn keep that animation; position, scale and texture animations are lost. All maps that use RSM2 models, or only `maps`. List the output first in DATA.INI. Not tried in an old client.",
        params: &[
            req(
                "path",
                Ty::Str,
                "The archive with the maps and their models (it stays unchanged).",
            ),
            opt(
                "maps",
                Ty::StrList,
                "Only these maps: `prontera` or `data\\prontera.rsw` (default: every map with RSM2 models).",
            ),
            NAMES,
            OUTPUT,
            OVERWRITE,
            DRY_RUN,
        ],
        writes: true,
        run: downgrade_maps_tool,
    },
    Tool {
        name: "grf_crypt",
        title: "Encrypt or decrypt single files or the file table (archive)",
        description: "GRF Editor's Encrypt and Decrypt on single files, and Encrypt File Table: writes a copy in which the files matching `encrypt_entries` are encrypted, those matching `decrypt_entries` are plain, and every other file keeps its state (0x200 only). The key is the server's RGRF_PASSWORD or RGRF_KEY_FILE; it also opens an encrypted source. Blocks are not shared in the result, so it can grow.",
        params: &[
            req("path", Ty::Str, "The archive (it stays unchanged)."),
            opt(
                "encrypt_entries",
                Ty::StrList,
                "Encrypt the files matching these patterns (substring, or glob with * or ?; `/` equals `\\`).",
            ),
            opt(
                "decrypt_entries",
                Ty::StrList,
                "Decrypt the files matching these patterns.",
            ),
            opt(
                "encrypt_table",
                Ty::Bool,
                "Encrypt the file table (GRF Editor's Encrypt File Table); the archive then needs the key to open. Without it the table keeps its state.",
            ),
            opt("decrypt_table", Ty::Bool, "Write the file table plain."),
            NAMES,
            OUTPUT,
            METHOD_OR_COPY,
            LEVEL,
            OVERWRITE,
            DRY_RUN,
        ],
        writes: true,
        run: crypt_tool,
    },
    Tool {
        name: "grf_hashlist_create",
        title: "Hash list of a folder (GRF Editor's .hash file)",
        description: "Hashes every file below `folder` and the entries of the GRF, GPF, Thor, RGZ and GSF archives in it (after decoding) and writes them as a hash list that GRF Editor and GrfCL read (`archive.grf?data\\path` for archive entries). Compare two lists with grf_hashlist_compare.",
        params: &[
            req("folder", Ty::Str, "The client folder."),
            req("output", Ty::Str, "The hash list file to create."),
            opt(
                "pattern",
                Ty::Str,
                "Only files whose name matches (* and ?; default all).",
            ),
            opt(
                "method",
                Ty::Enum(&["quick", "md5", "crc32", "size"]),
                "quick (default): MD5 of the first 4096 bytes, as GRF Editor does.",
            ),
            OVERWRITE,
        ],
        writes: true,
        run: hashlist_create_tool,
    },
    Tool {
        name: "grf_hashlist_compare",
        title: "Compare two hash lists",
        description: "What the user's hash list lacks (`missing`), changes (`different`), overrides by another path (`path override`) or adds (`extra`, `unreadable`) against the master list (GrfCL hashComp). Unless `exact`, files of the master's archives are also looked for loose and in the user's other archives.",
        params: &[
            req("master", Ty::Str, "The master hash list."),
            req("user", Ty::Str, "The hash list to check."),
            opt("exact", Ty::Bool, "Compare the exact paths only."),
            opt(
                "limit",
                Ty::Int,
                "How many differences to list (default 200; the count is complete).",
            ),
        ],
        writes: false,
        run: hashlist_compare_tool,
    },
    Tool {
        name: "grf_thor_pack",
        title: "Pack config files into a Thor Patcher",
        description: "Packs config.ini (stored as `config`), LanguageMap.ini and the given subfolders of `config_dir` behind the Thor Patcher program `program` (GrfCL thorPack) and writes the result to `output`.",
        params: &[
            req("program", Ty::Str, "The Thor Patcher without packed files."),
            req("output", Ty::Str, "The Thor Patcher with them."),
            req(
                "config_dir",
                Ty::Str,
                "Folder with config.ini and LanguageMap.ini.",
            ),
            opt(
                "folders",
                Ty::StrList,
                "Subfolders of config_dir to pack too (images, BGM, ...).",
            ),
            OVERWRITE,
        ],
        writes: true,
        run: thor_pack_tool,
    },
    Tool {
        name: "grf_thor_unpack",
        title: "Unpack the files of a Thor Patcher",
        description: "Writes the files packed into a Thor Patcher into `dest` (`config` becomes config.ini). Existing files are not replaced.",
        params: &[
            req("packed", Ty::Str, "The Thor Patcher with packed files."),
            req("dest", Ty::Str, "Folder to write into."),
        ],
        writes: true,
        run: thor_unpack_tool,
    },
    Tool {
        name: "grf_rare_info",
        title: "Read a seldom used client file",
        description: "Reads BSON (as JSON text), IMF, FNA, ICO/CUR or Thumbs.db: a loose file, or `entry` of an archive. `kind` says which. For ico and thumbs, `output_dir` writes the pictures (PNG or JPEG) there.",
        params: &[
            req(
                "path",
                Ty::Str,
                "The file, or an archive when `entry` is given.",
            ),
            opt("entry", Ty::Str, "The file inside the archive `path`."),
            req(
                "kind",
                Ty::Enum(&["bson", "imf", "fna", "ico", "thumbs"]),
                "The kind of file.",
            ),
            opt(
                "output_dir",
                Ty::Str,
                "Folder for the pictures of ico and thumbs.",
            ),
            NAMES,
        ],
        writes: false,
        run: rare_info_tool,
    },
    Tool {
        name: "grf_effect_str",
        title: "Look up an effect number or STR file",
        description: "GRF Editor's table of effect numbers and the STR files they play (118 entries). With `effect` the STR file of that number; with `str_file` (a name or path) the numbers that play it; without both, the whole table.",
        params: &[
            opt("effect", Ty::Int, "An effect number."),
            opt("str_file", Ty::Str, "The name of an STR file."),
        ],
        writes: false,
        run: effect_str_tool,
    },
    Tool {
        name: "grf_rare_convert",
        title: "Convert a seldom used client file",
        description: "bson_to_json and json_to_bson (the bytes come back the same), ebm_to_bmp and bmp_to_ebm (guild emblems are zlib-compressed BMPs). A loose file, or `entry` of an archive for the first of each pair; writes a new file. Put the result into an archive with grf_edit.",
        params: &[
            req(
                "path",
                Ty::Str,
                "The input file, or an archive when `entry` is given.",
            ),
            opt("entry", Ty::Str, "The file inside the archive `path`."),
            req(
                "kind",
                Ty::Enum(&["bson_to_json", "json_to_bson", "ebm_to_bmp", "bmp_to_ebm"]),
                "The conversion.",
            ),
            req("output", Ty::Str, "The file to create."),
            NAMES,
            OVERWRITE,
        ],
        writes: true,
        run: rare_convert_tool,
    },
    Tool {
        name: "grf_audio_info",
        title: "Sound information",
        description: "Format (wav, mp3, ogg), sample rate, channels, length and peak level of a sound: a loose file, or `entry` of an archive. Reads PCM and ADPCM WAV, MP3 and Ogg Vorbis.",
        params: &[
            req(
                "path",
                Ty::Str,
                "A sound file, or an archive when `entry` is given.",
            ),
            opt("entry", Ty::Str, "The sound inside the archive `path`."),
            NAMES,
        ],
        writes: false,
        run: audio_info_tool,
    },
    Tool {
        name: "grf_audio_edit",
        title: "Edit a sound (new file)",
        description: "Applies the operations `ops` in order to a sound (a loose file, or `entry` of an archive) and writes the result as a new file, by default in the sound's own format (WAV as 16-bit PCM; MP3 and Ogg are encoded again with about the same bit rate). Operations: trim:A-B, cut:A-B, silence:A-B (times in ms), gain:DB[@A-B], normalize:DB, fadein:MS, fadeout:MS, reverse, mono, resample:HZ, speed:FACTOR, echo:DELAY,FEEDBACK,MIX, reverb:ROOM,MIX, lowpass:HZ, highpass:HZ. Put the result into an archive with grf_edit.",
        params: &[
            req(
                "path",
                Ty::Str,
                "A sound file, or an archive when `entry` is given.",
            ),
            opt("entry", Ty::Str, "The sound inside the archive `path`."),
            req(
                "ops",
                Ty::StrList,
                "The operations, e.g. [\"trim:200-900\", \"fadeout:100\"].",
            ),
            opt(
                "to",
                Ty::Enum(&["wav", "mp3", "ogg"]),
                "Write this format instead of the sound's own.",
            ),
            opt(
                "kbps",
                Ty::Int,
                "Bit rate of an MP3 or Ogg result (default: about that of the source).",
            ),
            NAMES,
            OUTPUT,
            OVERWRITE,
        ],
        writes: true,
        run: audio_edit_tool,
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
    } else if e.is_protected() {
        "protected"
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
        "gravity_protected": es.iter().filter(|e| e.is_protected()).count(),
        "archive_keys": arc.protected_keys().ok(),
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
            "protected_decoded": r.unprotected,
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

fn image_reduce_tool(ctx: &Ctx, a: &Args) -> Out {
    use rayon::prelude::*;
    use rgrf_core::edit::{Edit, apply};
    use rgrf_formats::bmp::reduce_for_storage;
    let src = ctx.policy.read(a.str("path"))?;
    let out = output_path(ctx, a, std::slice::from_ref(&src))?;
    let min_psnr = a.opt_u64("min_psnr").unwrap_or(40) as f64;
    let only = a.opt_str("only").unwrap_or("*.bmp");
    let arc = open(ctx, a.str("path"))?;
    let mut opts = repack_options(a, true)?;
    if a.flag("recompress") {
        if a.opt_str("method").is_none() {
            return Err("`recompress` needs `method`".into());
        }
        opts.copy_streams = false;
    }
    let idx = arc.select(Some(only), names_mode(a));
    let results: Vec<_> = idx
        .par_iter()
        .map(|&i| {
            ctx.control.check().map_err(|_| "cancelled")?;
            let e = &arc.entries()[i];
            let d = arc.read(i).map_err(|_| "unreadable")?;
            reduce_for_storage(&d.data, min_psnr, u64::from(e.zsize), |b| {
                rgrf_core::encode::encode(opts.method, b).len() as u64
            })
            .map(|s| (e.name.clone(), s))
            .map_err(|k| k.reason())
        })
        .collect();
    ctx.control.check().map_err(err)?;
    let mut kept = std::collections::BTreeMap::<&str, usize>::new();
    let (mut before, mut after, mut lossless) = (0u64, 0u64, 0usize);
    let mut worst: Option<f64> = None;
    let mut edits = Vec::new();
    for r in results {
        match r {
            Ok((name, st)) => {
                before += st.stored_before;
                after += st.stored_after;
                if st.psnr.is_finite() {
                    worst = Some(worst.map_or(st.psnr, |w| w.min(st.psnr)));
                } else {
                    lossless += 1;
                }
                edits.push(Edit::Put {
                    name,
                    data: st.bytes,
                });
            }
            Err(why) => *kept.entry(why).or_default() += 1,
        }
    }
    let reduced = edits.len();
    let mut v = if reduced == 0 || opts.dry_run {
        json!({ "output": out.display().to_string(), "dry_run": opts.dry_run, "written": false })
    } else {
        let r = apply(&arc, &src, &out, &edits, &opts, &ctx.control).map_err(err)?;
        let mut v = repack_json(&r.merge.repack, &out);
        v["written"] = true.into();
        v
    };
    v["matched"] = idx.len().into();
    v["reduced"] = reduced.into();
    v["lossless"] = lossless.into();
    v["kept"] = json!(kept);
    v["stored_bytes_before"] = before.into();
    v["stored_bytes_after"] = after.into();
    v["worst_psnr_db"] = json!(worst);
    v["min_psnr_db"] = json!(min_psnr);
    Ok(v)
}

fn flat_maps_tool(ctx: &Ctx, a: &Args) -> Out {
    use rgrf_formats::flat_map::{FlatOptions, apply_map};
    let mut o = FlatOptions {
        flatten: a.flag("flatten"),
        remove_light: a.flag("remove_light"),
        remove_shadow: a.flag("remove_shadow"),
        remove_color: a.flag("remove_color"),
        gat_shadows: a.flag("gat_shadows"),
        reset_light: a.flag("reset_light"),
        remove_objects: a.flag("remove_objects"),
        remove_models: a.flag("remove_models"),
        remove_sounds: a.flag("remove_sounds"),
        remove_effects: a.flag("remove_effects"),
        remove_water: a.flag("remove_water"),
    };
    if !o.any() {
        (o.flatten, o.remove_light, o.remove_shadow) = (true, true, true);
    }
    map_patch_tool(ctx, a, |base, read| apply_map(base, read, &o))
}

/// What a map tool returns for one map: the files to write with their paths, or why it was left out.
type MapFiles = Result<Result<Vec<(Vec<u8>, Vec<u8>)>, String>, String>;

/// Runs `per_map` on the maps of the archive `path` (all, or `maps`) and writes what it returns
/// as one small archive: the shared part of the tools that change maps.
fn map_patch_tool(
    ctx: &Ctx,
    a: &Args,
    per_map: impl Fn(&[u8], &dyn Fn(&[u8]) -> Result<Option<Vec<u8>>, String>) -> MapFiles + Sync,
) -> Out {
    let src = ctx.policy.read(a.str("path"))?;
    let out = output_path(ctx, a, std::slice::from_ref(&src))?;
    let arc = open(ctx, a.str("path"))?;
    let mode = names_mode(a);
    let keys = arc.index_by_key();
    let wanted: Vec<String> = a
        .list("maps")
        .iter()
        .map(|m| {
            let m = m.to_ascii_lowercase();
            let m = m.rsplit('\\').next().unwrap_or(&m).to_string();
            m.trim_end_matches(".rsw").to_string()
        })
        .collect();
    let mut bases: Vec<Vec<u8>> = arc
        .entries()
        .iter()
        .filter(|e| e.is_file())
        .filter_map(|e| {
            let n = e.name.as_bytes();
            let low = n.to_ascii_lowercase();
            let stem = low.strip_suffix(b".rsw")?;
            let short = stem.rsplit(|&b| b == b'\\').next().unwrap_or(stem);
            (wanted.is_empty() || wanted.iter().any(|w| w.as_bytes() == short))
                .then(|| n[..n.len() - 4].to_vec())
        })
        .collect();
    bases.sort_by_key(|b| b.to_ascii_lowercase());
    bases.dedup_by_key(|b| b.to_ascii_lowercase());
    if bases.is_empty() {
        return Err("no matching .rsw file found".into());
    }
    let read = |p: &[u8]| -> Result<Option<Vec<u8>>, String> {
        keys.get(&GrfPath::new(p.to_vec()).key())
            .and_then(|v| v.last())
            .map(|&i| arc.read(i).map(|d| d.data).map_err(err))
            .transpose()
    };
    type One = (Vec<rgrf_core::edit::Packed>, u64, Option<String>);
    let results: Vec<Result<One, String>> = bases
        .par_iter()
        .map(|base| {
            ctx.control.check().map_err(err)?;
            let shown = GrfPath::new(base.clone()).display(mode);
            let changed = match per_map(base, &read)? {
                Ok(c) => c,
                Err(why) => return Ok((vec![], 0, Some(format!("{shown}: {why}")))),
            };
            let mut packed = Vec::new();
            let mut bytes = 0;
            for (path, d) in changed {
                bytes += d.len() as u64;
                packed.push(
                    rgrf_core::edit::pack(GrfPath::new(path), &d, Method::DEFAULT).map_err(err)?,
                );
            }
            Ok((packed, bytes, None))
        })
        .collect();
    ctx.control.check().map_err(err)?;
    let (mut files, mut problems) = (Vec::new(), Vec::new());
    let (mut changed_maps, mut bytes) = (0usize, 0u64);
    for r in results {
        let (f, b, p) = r?;
        changed_maps += usize::from(!f.is_empty());
        bytes += b;
        files.extend(f);
        problems.extend(p);
    }
    let dry_run = a.flag("dry_run");
    let written = if files.is_empty() || dry_run {
        0
    } else {
        rgrf_core::edit::write_packed(&files, &out, a.flag("overwrite")).map_err(err)?
    };
    Ok(json!({
        "output": out.display().to_string(),
        "written": written > 0,
        "dry_run": dry_run,
        "maps": bases.len(),
        "changed_maps": changed_maps,
        "files": if dry_run { files.len() } else { written },
        "bytes": bytes,
        "file_bytes": (written > 0).then(|| std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0)),
        "problems": problems.iter().take(50).collect::<Vec<_>>(),
        "problem_count": problems.len(),
    }))
}

fn downgrade_maps_tool(ctx: &Ctx, a: &Args) -> Out {
    let named = !a.list("maps").is_empty();
    map_patch_tool(ctx, a, |base, read| {
        // without names, only the maps that have RSM2 models
        Ok(
            rgrf_formats::downgrade::apply_map(base, read)?.map(|(files, d)| {
                if !named && d.instances == 0 {
                    Vec::new()
                } else {
                    files
                }
            }),
        )
    })
}

fn map_files_tool(ctx: &Ctx, a: &Args) -> Out {
    use rgrf_formats::map_files::{MapSource, resolve};
    struct Src<'a>(&'a Archive, HashMap<Vec<u8>, Vec<usize>>);
    impl MapSource for Src<'_> {
        fn read(&self, path: &[u8]) -> Option<Vec<u8>> {
            let &i = self.1.get(&GrfPath::new(path.to_vec()).key())?.last()?;
            let d = self.0.read(i).ok()?;
            d.status.is_ok().then_some(d.data)
        }
        fn exists(&self, path: &[u8]) -> bool {
            self.1.contains_key(&GrfPath::new(path.to_vec()).key())
        }
    }
    let arc = open(ctx, a.str("path"))?;
    let mode = names_mode(a);
    let files = resolve(a.str("map"), &Src(&arc, arc.index_by_key()))?;
    let limit = a.opt_u64("limit").map_or(usize::MAX, |n| n as usize);
    let missing = files.iter().filter(|f| !f.found).count();
    let show = |b: &[u8]| GrfPath::new(b.to_vec()).display(mode);
    Ok(json!({
        "map": rgrf_formats::map_files::map_base(a.str("map")),
        "count": files.len(),
        "missing": missing,
        "truncated": files.len() > limit,
        "files": files.iter().take(limit).map(|f| json!({
            "path": show(&f.path), "kind": f.kind.name(), "found": f.found,
            "named_by": f.named_by.as_deref().map(show), "problem": f.problem,
        })).collect::<Vec<_>>(),
    }))
}

fn client_refs_tool(ctx: &Ctx, a: &Args) -> Out {
    use rayon::prelude::*;
    use rgrf_formats::map_files::MapSource;
    use rgrf_formats::refs;
    type Keyed<'a> = (&'a Archive, HashMap<Vec<u8>, Vec<usize>>);
    struct Src<'a>(Vec<Keyed<'a>>);
    impl Src<'_> {
        fn find(&self, path: &[u8]) -> Option<(usize, usize)> {
            let key = GrfPath::new(path.to_vec()).key();
            self.0
                .iter()
                .enumerate()
                .find_map(|(n, (_, m))| m.get(&key).and_then(|v| v.last()).map(|&i| (n, i)))
        }
    }
    impl MapSource for Src<'_> {
        fn read(&self, path: &[u8]) -> Option<Vec<u8>> {
            let (n, i) = self.find(path)?;
            let d = self.0[n].0.read(i).ok()?;
            d.status.is_ok().then_some(d.data)
        }
        fn exists(&self, path: &[u8]) -> bool {
            self.find(path).is_some()
        }
    }
    let paths = a.list("paths");
    if paths.is_empty() {
        return Err("give at least one archive in `paths`".into());
    }
    let archives = paths
        .iter()
        .map(|p| open(ctx, p))
        .collect::<Result<Vec<_>, _>>()?;
    let src = Src(archives.iter().map(|x| (x, x.index_by_key())).collect());
    let only = a.opt_str("only");
    let mode = names_mode(a);
    let limit = a.opt_u64("limit").map_or(100, |n| n as usize);
    // the files that win: the first archive that has the path
    let mut work = Vec::new();
    let mut seen = HashSet::new();
    for (n, (arc, keys)) in src.0.iter().enumerate() {
        for (key, idxs) in keys {
            let Some(&i) = idxs.last() else { continue };
            let e = &arc.entries()[i];
            let ext = e
                .name
                .as_bytes()
                .rsplit(|&b| b == b'.')
                .next()
                .map(|x| x.to_ascii_lowercase());
            let is_act = match ext.as_deref() {
                Some(b"act") if only.is_none_or(|o| o == "act") => true,
                Some(b"str") if only.is_none_or(|o| o == "str") => false,
                _ => continue,
            };
            if e.is_file() && seen.insert(key.clone()) {
                work.push((n, i, is_act));
            }
        }
    }
    let found: Vec<(String, Vec<refs::Problem>)> = work
        .par_iter()
        .filter_map(|&(n, i, is_act)| {
            ctx.control.check().ok()?;
            let e = &src.0[n].0.entries()[i];
            let data = src.0[n].0.read(i).ok()?.data;
            let raw = e.name.as_bytes();
            let p = if is_act {
                refs::check_act(&src, raw, &data)
            } else {
                refs::check_str(&src, raw, &data)
            };
            (!p.is_empty()).then(|| (e.name.display(mode), p))
        })
        .collect();
    ctx.control.check().map_err(err)?;
    let acts = work.iter().filter(|w| w.2).count();
    let mut by_kind = std::collections::BTreeMap::<&str, usize>::new();
    let mut missing_sprites = std::collections::BTreeMap::<String, usize>::new();
    let mut list = Vec::new();
    let mut found = found;
    found.sort_by(|x, y| x.0.cmp(&y.0));
    for (file, problems) in &found {
        for p in problems {
            *by_kind.entry(p.kind).or_default() += 1;
            if p.kind == "missing_sprite" {
                let dir = file.rsplit_once('\\').map_or("", |d| d.0);
                *missing_sprites.entry(dir.to_string()).or_default() += 1;
            } else {
                list.push(json!({
                    "kind": p.kind, "file": file,
                    "missing": (!p.path.is_empty()).then(|| GrfPath::new(p.path.clone()).display(mode)),
                    "note": p.note,
                }));
            }
        }
    }
    let mut folders: Vec<_> = missing_sprites.into_iter().collect();
    folders.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(&y.0)));
    Ok(json!({
        "animations_checked": acts, "effects_checked": work.len() - acts,
        "files_with_problems": found.len(),
        "by_kind": by_kind,
        "problems": list.iter().take(limit).collect::<Vec<_>>(),
        "problems_truncated": list.len() > limit,
        "animations_without_sprite_by_folder": folders.iter().take(limit)
            .map(|(d, n)| json!({"folder": d, "count": n})).collect::<Vec<_>>(),
    }))
}

fn client_unused_tool(ctx: &Ctx, a: &Args) -> Out {
    use rgrf_formats::map_files::MapSource;
    use rgrf_formats::unused::{Client, FileInfo, analyze, names_are_composed};
    type Keyed = (Archive, HashMap<Vec<u8>, Vec<usize>>);
    struct View {
        archives: Vec<Keyed>,
        files: Vec<FileInfo>,
        at: Vec<(usize, usize)>,
    }
    impl View {
        fn find(&self, path: &[u8]) -> Option<(usize, usize)> {
            let key = GrfPath::new(path.to_vec()).key();
            self.archives
                .iter()
                .enumerate()
                .find_map(|(n, (_, m))| m.get(&key).and_then(|v| v.last()).map(|&i| (n, i)))
        }
    }
    impl MapSource for View {
        fn read(&self, path: &[u8]) -> Option<Vec<u8>> {
            let (n, i) = self.find(path)?;
            let d = self.archives[n].0.read(i).ok()?;
            d.status.is_ok().then_some(d.data)
        }
        fn exists(&self, path: &[u8]) -> bool {
            self.find(path).is_some()
        }
    }
    impl Client for View {
        fn files(&self) -> &[FileInfo] {
            &self.files
        }
        fn read_file(&self, i: usize) -> Option<Vec<u8>> {
            let (n, e) = self.at[i];
            Some(self.archives[n].0.read(e).ok()?.data)
        }
    }
    let paths = a.list("paths");
    if paths.is_empty() {
        return Err("give at least one archive in `paths`".into());
    }
    let mut v = View {
        archives: Vec::new(),
        files: Vec::new(),
        at: Vec::new(),
    };
    for p in &paths {
        let arc = open(ctx, p)?;
        let keys = arc.index_by_key();
        v.archives.push((arc, keys));
    }
    for n in 0..v.archives.len() {
        let wins: Vec<(usize, Vec<u8>, u64)> = v.archives[n]
            .1
            .values()
            .filter_map(|idxs| {
                let &i = idxs.last()?;
                let e = &v.archives[n].0.entries()[i];
                e.is_file()
                    .then(|| (i, e.name.as_bytes().to_vec(), u64::from(e.zsize)))
            })
            .collect();
        for (i, path, zsize) in wins {
            if v.find(&path) == Some((n, i)) {
                v.files.push(FileInfo { path, zsize });
                v.at.push((n, i));
            }
        }
    }
    ctx.control.check().map_err(err)?;
    let r = analyze(&v, &[]);
    let mode = names_mode(a);
    let limit = a.opt_u64("limit").map_or(100, |n| n as usize);
    let show = |b: &[u8]| GrfPath::new(b.to_vec()).display(mode);
    Ok(json!({
        "files_looked_at": r.files_looked_at,
        "unnamed_files": r.unnamed, "unnamed_stored_bytes": r.unnamed_bytes,
        "unnamed_in_map_folders": r.unnamed_reliable,
        "unnamed_in_map_folders_stored_bytes": r.unnamed_reliable_bytes,
        "names_found": r.names_found,
        "folders": r.folders.iter().take(limit).map(|f| json!({
            "folder": show(&f.name), "names_are_composed": names_are_composed(&f.name),
            "files": f.files, "unnamed": f.unnamed, "unnamed_stored_bytes": f.unnamed_bytes,
            "examples": f.examples.iter().map(|e| show(e)).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "folders_truncated": r.folders.len() > limit,
    }))
}

fn crypt_tool(ctx: &Ctx, a: &Args) -> Out {
    let (on, off) = (a.list("encrypt_entries"), a.list("decrypt_entries"));
    let (table_on, table_off) = (a.flag("encrypt_table"), a.flag("decrypt_table"));
    if on.is_empty() && off.is_empty() && !table_on && !table_off {
        return Err("nothing to do: give `encrypt_entries`, `decrypt_entries`, `encrypt_table` or `decrypt_table`".into());
    }
    if table_on && table_off {
        return Err("`encrypt_table` and `decrypt_table` exclude each other".into());
    }
    let key = env_key()?.ok_or_else(|| {
        "crypt needs RGRF_PASSWORD or RGRF_KEY_FILE in the server's environment".to_string()
    })?;
    let src = ctx.policy.read(a.str("path"))?;
    let out = output_path(ctx, a, std::slice::from_ref(&src))?;
    let arc = open(ctx, a.str("path"))?;
    let mode = names_mode(a);
    let pick = |patterns: &[&str]| -> Result<Vec<usize>, String> {
        let mut found = Vec::new();
        for p in patterns {
            let hits: Vec<usize> = arc
                .select(Some(p), mode)
                .into_iter()
                .filter(|&i| arc.entries()[i].is_file())
                .collect();
            if hits.is_empty() {
                return Err(format!("no file matches '{p}'"));
            }
            found.extend(hits);
        }
        Ok(found)
    };
    let (on, off) = (pick(&on)?, pick(&off)?);
    let mut opts = repack_options(a, true)?;
    opts.encrypt_selected(&arc, key, &on, &off);
    opts.encrypt_table = (table_on || (!table_off && arc.table_encrypted())).then_some(key);
    let r = repack(&arc, &out, &opts, &ctx.control).map_err(err)?;
    let mut v = repack_json(&r, &out);
    v["encrypted_entries"] = opts.encrypt_only.as_ref().map_or(0, |s| s.len()).into();
    v["file_table_encrypted"] = opts.encrypt_table.is_some().into();
    Ok(v)
}

/// A sound from `entry` of the archive `path`, or from the loose file `path`.
fn audio_bytes(ctx: &Ctx, a: &Args) -> Result<(Vec<u8>, String), String> {
    if a.opt_str("entry").is_some() {
        let (name, data) = entry_data(ctx, a)?;
        return Ok((data, name));
    }
    let p = ctx.policy.read(a.str("path"))?;
    let len = std::fs::metadata(&p).map_err(err)?.len();
    if len > MAX_ENTRY_BYTES {
        return Err(format!("larger than {MAX_ENTRY_BYTES} bytes"));
    }
    Ok((
        std::fs::read(&p).map_err(|e| format!("{}: {e}", a.str("path")))?,
        a.str("path").to_string(),
    ))
}

fn audio_info_tool(ctx: &Ctx, a: &Args) -> Out {
    use rgrf_formats::audio::Audio;
    let (data, name) = audio_bytes(ctx, a)?;
    let (audio, source) = Audio::decode(&data).map_err(|e| format!("{name}: {e}"))?;
    let peak = audio.peak();
    Ok(json!({
        "name": name, "format": source.format.extension(), "kbps": source.kbps,
        "sample_rate": audio.rate, "channels": audio.channels.len(),
        "samples": audio.len(), "duration_ms": audio.duration_ms(),
        "peak": peak, "peak_db": (peak > 0.0).then(|| 20.0 * peak.log10()),
    }))
}

fn audio_edit_tool(ctx: &Ctx, a: &Args) -> Out {
    use rgrf_formats::audio::{Audio, Format, Op};
    let ops: Vec<Op> = a
        .list("ops")
        .iter()
        .map(|t| Op::parse(t).map_err(err))
        .collect::<Result<_, _>>()?;
    if ops.is_empty() {
        return Err("give at least one operation in `ops`".into());
    }
    let out = ctx
        .policy
        .write_file(a.str("output"), a.flag("overwrite"))?;
    if a.opt_str("entry").is_none() && ctx.policy.read(a.str("path")).is_ok_and(|p| p == out) {
        return Err("the output must not be the input file".into());
    }
    let (data, name) = audio_bytes(ctx, a)?;
    let (mut audio, mut source) = Audio::decode(&data).map_err(|e| format!("{name}: {e}"))?;
    if let Some(f) = a.opt_str("to") {
        let f = match f {
            "mp3" => Format::Mp3,
            "ogg" => Format::Ogg,
            _ => Format::Wav,
        };
        if f != source.format {
            source.kbps = None;
        }
        source.format = f;
    }
    if let Some(k) = a.opt_u64("kbps") {
        source.kbps = u32::try_from(k).ok();
    }
    audio.apply_all(&ops).map_err(|e| format!("{name}: {e}"))?;
    let bytes = audio.encode(&source).map_err(|e| format!("{name}: {e}"))?;
    let tmp = out.with_extension("rgrf-tmp");
    std::fs::write(&tmp, &bytes).map_err(err)?;
    std::fs::rename(&tmp, &out).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        err(e)
    })?;
    Ok(json!({
        "output": out.display().to_string(), "format": source.format.extension(),
        "operations": ops.iter().map(Op::text).collect::<Vec<_>>(),
        "sample_rate": audio.rate, "channels": audio.channels.len(),
        "duration_ms": audio.duration_ms(), "bytes": bytes.len(),
    }))
}

fn hashlist_create_tool(ctx: &Ctx, a: &Args) -> Out {
    use rgrf_core::hashlist::{HashMethod, hash_folder};
    let folder = ctx.policy.read(a.str("folder"))?;
    let out = ctx
        .policy
        .write_file(a.str("output"), a.flag("overwrite"))?;
    let method = match a.opt_str("method").unwrap_or("quick") {
        "md5" => HashMethod::Md5,
        "crc32" => HashMethod::Crc32,
        "size" => HashMethod::FileSize,
        _ => HashMethod::QuickMd5,
    };
    let list = hash_folder(
        &folder,
        a.opt_str("pattern").unwrap_or("*"),
        method,
        &ctx.control,
    )
    .map_err(err)?;
    list.write(&out).map_err(err)?;
    Ok(json!({
        "output": out.display().to_string(), "files": list.hashes.len(),
        "method": list.method.name(),
    }))
}

fn hashlist_compare_tool(ctx: &Ctx, a: &Args) -> Out {
    use rgrf_core::hashlist::{HashList, compare};
    let m = HashList::read(&ctx.policy.read(a.str("master"))?).map_err(err)?;
    let u = HashList::read(&ctx.policy.read(a.str("user"))?).map_err(err)?;
    if m.method.len() != u.method.len() {
        return Err("the lists use different hash methods".into());
    }
    let diffs = compare(&m, &u, a.flag("exact"));
    let limit = a.opt_u64("limit").unwrap_or(200) as usize;
    let items: Vec<_> = diffs
        .iter()
        .take(limit)
        .map(|d| json!({"issue": d.issue.name(), "path": d.path, "user_path": d.user_path}))
        .collect();
    Ok(json!({"issues": diffs.len(), "listed": items.len(), "differences": items}))
}

fn thor_pack_tool(ctx: &Ctx, a: &Args) -> Out {
    let program = ctx.policy.read(a.str("program"))?;
    let cfg = ctx.policy.read(a.str("config_dir"))?;
    let out = ctx
        .policy
        .write_file(a.str("output"), a.flag("overwrite"))?;
    let prog = std::fs::read(&program).map_err(|e| format!("{}: {e}", program.display()))?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (stored, name) in [
        ("config", "config.ini"),
        ("LanguageMap.ini", "LanguageMap.ini"),
    ] {
        let p = cfg.join(name);
        files.push((
            stored.into(),
            std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?,
        ));
    }
    for folder in a.list("folders") {
        // a folder name never leaves the config folder
        if folder
            .split(['/', '\\'])
            .any(|p| p == ".." || p.contains(':'))
        {
            return Err(format!("'{folder}' is not a folder of config_dir"));
        }
        let root = cfg.join(folder);
        if !root.is_dir() {
            return Err(format!("{} is not a folder", root.display()));
        }
        let mut stack = vec![root.clone()];
        let mut found = Vec::new();
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).map_err(err)?.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    found.push(p);
                }
            }
        }
        found.sort();
        let parent = root.parent().unwrap_or(&cfg).to_path_buf();
        for f in found {
            let rel: Vec<String> = f
                .strip_prefix(&parent)
                .unwrap_or(&f)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            let sep = if rel.first().is_some_and(|f| f == "Languages") {
                "\\"
            } else {
                "/"
            };
            files.push((
                rel.join(sep),
                std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?,
            ));
        }
    }
    let packed = rgrf_core::thorpack::pack(&prog, &files).map_err(err)?;
    std::fs::write(&out, &packed).map_err(err)?;
    Ok(json!({"output": out.display().to_string(), "files": files.len(), "bytes": packed.len()}))
}

fn thor_unpack_tool(ctx: &Ctx, a: &Args) -> Out {
    let packed = ctx.policy.read(a.str("packed"))?;
    let dest = ctx.policy.write_dir(a.str("dest"))?;
    let exe = std::fs::read(&packed).map_err(|e| format!("{}: {e}", packed.display()))?;
    let (program_bytes, files) = rgrf_core::thorpack::unpack(&exe).map_err(err)?;
    let mut written = Vec::new();
    for (name, data) in &files {
        let name = if name == "config" {
            "config.ini"
        } else {
            name.as_str()
        };
        let mut target = dest.clone();
        for part in name.split(['/', '\\']) {
            if part.is_empty() || part == ".." || part.contains(':') {
                return Err(format!("the packed name '{name}' is not a safe path"));
            }
            target.push(part);
        }
        if target.exists() {
            return Err(format!("{} exists", target.display()));
        }
        if let Some(d) = target.parent() {
            std::fs::create_dir_all(d).map_err(err)?;
        }
        std::fs::write(&target, data).map_err(err)?;
        written.push(name.to_string());
    }
    Ok(json!({"program_bytes": program_bytes, "files": written}))
}

fn rare_bytes(ctx: &Ctx, a: &Args) -> Result<(Vec<u8>, String), String> {
    audio_bytes(ctx, a)
}

fn effect_str_tool(_: &Ctx, a: &Args) -> Out {
    use rgrf_formats::effects2str::{EFFECTS, ids_of, str_of};
    let rows: Vec<(i32, &str)> = if let Some(id) = a.opt_u64("effect") {
        i32::try_from(id)
            .ok()
            .and_then(|i| str_of(i).map(|n| (i, n)))
            .into_iter()
            .collect()
    } else if let Some(n) = a.opt_str("str_file") {
        ids_of(n)
            .into_iter()
            .filter_map(|i| str_of(i).map(|s| (i, s)))
            .collect()
    } else {
        EFFECTS.to_vec()
    };
    Ok(
        json!({"effects": rows.iter().map(|(i, n)| json!({"effect": i, "str": n})).collect::<Vec<_>>()}),
    )
}

fn rare_info_tool(ctx: &Ctx, a: &Args) -> Out {
    use rgrf_formats::{bson, fna, ico, imf, thumbsdb};
    let (data, name) = rare_bytes(ctx, a)?;
    let dir = a
        .opt_str("output_dir")
        .map(|d| ctx.policy.write_dir(d))
        .transpose()?;
    let mode = names_mode(a);
    let write = |dir: &std::path::Path, file: String, bytes: &[u8]| -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(err)?;
        let p = dir.join(file);
        if p.exists() {
            return Err(format!("{} exists", p.display()));
        }
        std::fs::write(p, bytes).map_err(err)
    };
    match a.str("kind") {
        "bson" => {
            let doc = bson::parse(&data).map_err(|e| format!("{name}: {e}"))?;
            Ok(json!({"name": name, "json": bson::to_json(&doc)}))
        }
        "imf" => {
            let f = imf::Imf::parse(&data).map_err(|e| format!("{name}: {e}"))?;
            Ok(json!({"name": name, "checksum": f.checksum, "text": f.to_text()}))
        }
        "fna" => {
            let f = fna::Fna::parse(&data).map_err(|e| format!("{name}: {e}"))?;
            let names: Vec<String> = f
                .files
                .iter()
                .map(|n| GrfPath::new(n.clone()).display(mode))
                .collect();
            Ok(json!({"name": name, "files": names}))
        }
        "ico" => {
            let pics = ico::parse(&data).map_err(|e| format!("{name}: {e}"))?;
            let mut written = Vec::new();
            for (i, p) in pics.iter().enumerate() {
                if let Some(dir) = &dir {
                    let png = match &p.picture {
                        ico::IcoPicture::Png(b) => b.clone(),
                        ico::IcoPicture::Rgba(img) => {
                            rgrf_formats::png_io::rgba_to_png(img).map_err(err)?
                        }
                    };
                    let file = format!("{i}_{}x{}.png", p.width, p.height);
                    write(dir, file.clone(), &png)?;
                    written.push(file);
                }
            }
            let items: Vec<_> = pics
                .iter()
                .map(|p| json!({"width": p.width, "height": p.height, "bits": p.bits}))
                .collect();
            Ok(json!({"name": name, "pictures": items, "written": written}))
        }
        _ => {
            let db = thumbsdb::parse(&data).map_err(|e| format!("{name}: {e}"))?;
            let mut written = 0;
            for t in &db.thumbs {
                if let (Some(dir), Some(j)) = (&dir, &t.jpeg) {
                    let base = t.name.rsplit(['\\', '/']).next().unwrap_or(&t.name);
                    let safe: String = base
                        .chars()
                        .map(|c| {
                            if c.is_alphanumeric() || "._- ".contains(c) {
                                c
                            } else {
                                '_'
                            }
                        })
                        .collect();
                    write(dir, format!("{}_{safe}.jpg", t.id), j)?;
                    written += 1;
                }
            }
            let items: Vec<_> = db
                .thumbs
                .iter()
                .map(|t| json!({"id": t.id, "name": t.name, "jpeg_bytes": t.jpeg.as_ref().map(Vec::len)}))
                .collect();
            Ok(
                json!({"name": name, "width": db.width, "height": db.height, "thumbnails": items, "written": written}),
            )
        }
    }
}

fn rare_convert_tool(ctx: &Ctx, a: &Args) -> Out {
    use rgrf_formats::{bson, ebm};
    let out = ctx
        .policy
        .write_file(a.str("output"), a.flag("overwrite"))?;
    let (data, name) = rare_bytes(ctx, a)?;
    let bytes = match a.str("kind") {
        "bson_to_json" => {
            bson::to_json(&bson::parse(&data).map_err(|e| format!("{name}: {e}"))?).into_bytes()
        }
        "json_to_bson" => {
            let text = String::from_utf8(data).map_err(|_| format!("{name} is not UTF-8 text"))?;
            bson::to_bytes(&bson::from_json(&text).map_err(|e| format!("{name}: {e}"))?)
        }
        "ebm_to_bmp" => ebm::to_bmp(&data).map_err(|e| format!("{name}: {e}"))?,
        _ => {
            if !data.starts_with(b"BM") {
                return Err(format!("{name} is not a BMP"));
            }
            ebm::from_bmp(&data)
        }
    };
    std::fs::write(&out, &bytes).map_err(err)?;
    Ok(json!({"output": out.display().to_string(), "bytes": bytes.len()}))
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
