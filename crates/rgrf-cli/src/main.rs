//! `rgrf`: inspect, check and extract GRF archives (milestone M1, read-only commands).
//!
//! Exit codes: 0 ok, 1 error, 2 warnings (e.g. a damaged entry was tolerated), 3 verification failed.

use std::collections::HashSet;
use std::io::{self, BufWriter, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand, ValueEnum};
use rayon::prelude::*;
use rgrf_core::build::{BuildOptions, build};
use rgrf_core::codec::Codec;
use rgrf_core::encode::Method;
use rgrf_core::extract::{ExtractOptions, OnCollision, extract};
use rgrf_core::hash::Digest;
use rgrf_core::name::hex;
use rgrf_core::progress::{Control, Progress};
use rgrf_core::repack::{Dedupe, RepackOptions, repack};
use rgrf_core::verify::{Severity, VerifyOptions, verify};
use rgrf_core::writer::Format;
use rgrf_core::{Archive, DecodeStatus, Entry, GrfPath, NameMode};
use serde_json::{Value, json};

mod act_cmd;
mod client_cmd;
mod edit_cmd;
mod lua_cmd;
mod map_cmd;
mod patch_cmd;
mod spr_cmd;

const SCHEMA_VERSION: u32 = 1;

#[derive(Parser)]
#[command(
    name = "rgrf",
    version,
    about = "Inspect, check and extract Ragnarok Online GRF archives"
)]
struct Cli {
    /// Machine-readable output (stable schema, field `schema_version`)
    #[arg(long, global = true)]
    json: bool,
    /// How archive names are shown and written: cp1252 (as GRF Editor), cp949 (Korean), utf8, raw
    #[arg(long, global = true, default_value = "cp1252", value_parser = parse_names)]
    names: NameMode,
    /// No progress output
    #[arg(long, short, global = true)]
    quiet: bool,
    /// Worker threads (default: all cores)
    #[arg(long, global = true)]
    threads: Option<usize>,
    #[command(subcommand)]
    command: Cmd,
}

fn parse_names(s: &str) -> Result<NameMode, String> {
    NameMode::parse(s).ok_or_else(|| format!("unknown name mode '{s}' (cp1252, cp949, utf8, raw)"))
}

#[derive(Subcommand)]
enum Cmd {
    /// Version, size, entry counts
    Info {
        grf: PathBuf,
        /// Also count zlib/LZMA entries (reads one byte per entry)
        #[arg(long)]
        codecs: bool,
    },
    /// List entries (glob with * and ?, or a substring)
    Ls {
        grf: PathBuf,
        pattern: Option<String>,
        /// Sizes, flags and offsets
        #[arg(long, short)]
        long: bool,
    },
    /// Search entries by name and/or extension
    Find {
        grf: PathBuf,
        pattern: Option<String>,
        /// Extensions without dot, comma separated (e.g. spr,act)
        #[arg(long, value_delimiter = ',')]
        ext: Vec<String>,
    },
    /// Write one entry to stdout
    Cat { grf: PathBuf, path: String },
    /// Extract entries into a directory (never overwrites without --overwrite)
    Extract {
        grf: PathBuf,
        dest: PathBuf,
        pattern: Option<String>,
        #[arg(long, value_enum, default_value = "abort")]
        on_collision: Collision,
        #[arg(long)]
        overwrite: bool,
    },
    /// Check table, offsets, overlaps, duplicates and decode every entry
    Verify {
        grf: PathBuf,
        /// Only check the table, do not decode entries
        #[arg(long)]
        no_decode: bool,
        /// Print every issue (default: first 50 per severity)
        #[arg(long)]
        all: bool,
    },
    /// Write a new archive with another compression, format or deduplication (content unchanged)
    Repack {
        grf: PathBuf,
        /// Output archive (written to a temporary file first, never over the source)
        #[arg(short, long)]
        output: PathBuf,
        /// zlib (default, every client), zopfli (zlib-compatible, smaller, slow) or lzma (only
        /// with the LZMA-capable cps.dll in the client)
        #[arg(long, value_enum, default_value = "zlib")]
        method: MethodArg,
        /// zlib: 0-9 (default 9); zopfli: iterations (default 10); lzma: preset 0-9 (default 9)
        #[arg(long)]
        level: Option<u32>,
        /// 0x200 (32-bit offsets, max 4 GiB), 0x300, or the old 0x102/0x103 (default: 0x200 or 0x300 as the source)
        #[arg(long, value_parser = parse_format)]
        format: Option<Format>,
        /// Pad streams to a multiple of 8 (default: yes for 0x200, no for 0x300, as in real archives)
        #[arg(long, conflicts_with = "no_align")]
        align: bool,
        #[arg(long)]
        no_align: bool,
        /// none: no sharing; source: keep the sharing of the source (default); content: share
        /// blocks of identical content
        #[arg(long, value_enum, default_value = "source")]
        dedupe: DedupeArg,
        /// Always use the new stream, never keep the original stream of the same codec
        #[arg(long)]
        recompress_all: bool,
        /// Skip decoding the written archive for comparison (not recommended)
        #[arg(long)]
        no_verify: bool,
        /// Replace an existing output file
        #[arg(long)]
        overwrite: bool,
        /// Only plan, write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Compare two archives by path and content (exit 0 identical, 2 differences)
    Diff {
        a: PathBuf,
        b: PathBuf,
        /// Print every path (default: the first 50 per group)
        #[arg(long)]
        all: bool,
    },
    /// Merge archives into a new one. The FIRST archive wins for a path (like the lowest index in
    /// DATA.INI); give your custom GRFs first and data.grf last
    Merge {
        #[arg(required = true, num_args = 1..)]
        grfs: Vec<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
        /// The LAST archive is the base: earlier archives replace its files but add no new paths
        /// (update a GRF with a newer translation without pulling in everything else)
        #[arg(long)]
        replace_only: bool,
        #[command(flatten)]
        write: WriteArgs,
    },
    /// Write an archive with only what is new or different in NEW compared with OLD
    Patch {
        old: PathBuf,
        new: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[command(flatten)]
        write: WriteArgs,
    },
    /// Build an archive from a folder (the folder is the archive root, e.g. it contains `data/`)
    Build {
        folder: PathBuf,
        /// Output archive (written to a temporary file first)
        #[arg(short, long)]
        output: PathBuf,
        /// zlib (default, every client), zopfli (slow, smaller) or lzma (needs the LZMA cps.dll)
        #[arg(long, value_enum, default_value = "zlib")]
        method: MethodArg,
        /// zlib: 0-9 (default 9); zopfli: iterations (default 10); lzma: preset 0-9 (default 9)
        #[arg(long)]
        level: Option<u32>,
        /// 0x200 (default, 32-bit offsets, max 4 GiB), 0x300, or the old 0x102/0x103
        #[arg(long, value_parser = parse_format, default_value = "0x200")]
        format: Format,
        /// Pad streams to a multiple of 8 (default: yes for 0x200, no for 0x300)
        #[arg(long, conflicts_with = "no_align")]
        align: bool,
        #[arg(long)]
        no_align: bool,
        /// Let files with identical content share one data block
        #[arg(long)]
        dedupe: bool,
        /// Skip decoding the written archive for comparison (not recommended)
        #[arg(long)]
        no_verify: bool,
        /// Replace an existing output file
        #[arg(long)]
        overwrite: bool,
        /// Only scan and plan, write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// CRC32 and MD5 of the decoded content per entry
    Hash {
        grf: PathBuf,
        pattern: Option<String>,
    },
    /// Sprites (.spr): show, export frames as PNG, build from PNGs
    Spr {
        #[command(subcommand)]
        action: SprCmd,
    },
    /// Animations (.act): show, render frames as PNG
    Act {
        #[command(subcommand)]
        action: ActCmd,
    },
    /// Lua files (.lub/.lua of the client)
    Lua {
        #[command(subcommand)]
        action: LuaCmd,
    },
    /// A client folder: DATA.INI, archives, hidden files, server address, Lua
    Client {
        #[command(subcommand)]
        action: ClientCmd,
    },
    /// Maps (.gat/.gnd/.rsw): info, picture of the walkability grid, check the files a map needs
    Map {
        #[command(subcommand)]
        action: MapCmd,
    },
    /// Thor patches (.thor): create; every other command opens them like a GRF
    Thor {
        #[command(subcommand)]
        action: ThorCmd,
    },
    /// RGZ patches (.rgz): create; every other command opens them like a GRF
    Rgz {
        #[command(subcommand)]
        action: RgzCmd,
    },
}

#[derive(Subcommand)]
enum ThorCmd {
    /// Create a Thor patch from a folder (the folder is the patch root) or from an archive
    Create {
        /// Folder or archive (GRF, GPF, Thor, RGZ) whose files go into the patch
        source: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// Merge the files into a GRF (paths inside `data\`) instead of the client folder
        #[arg(long)]
        merge_into_grf: bool,
        /// Name of the target GRF (default: the client's own), only with --merge-into-grf
        #[arg(long)]
        target: Option<String>,
        /// Delete this path in the target (repeatable)
        #[arg(long = "remove")]
        remove: Vec<String>,
        /// Add the data.integrity list of CRC32 hashes of the file contents, as real Thor patches have it
        #[arg(long)]
        integrity: bool,
        /// zlib (default, every client), zopfli (slow, smaller) or lzma (needs the LZMA cps.dll)
        #[arg(long, value_enum, default_value = "zlib")]
        method: MethodArg,
        /// zlib: 0-9 (default 9); zopfli: iterations (default 10); lzma: preset 0-9 (default 9)
        #[arg(long)]
        level: Option<u32>,
        /// Replace an existing output file
        #[arg(long)]
        overwrite: bool,
    },
}

#[derive(Subcommand)]
enum RgzCmd {
    /// Create an RGZ patch from a folder (the folder is the patch root) or from an archive
    Create {
        source: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// gzip level 0-9 (default 9)
        #[arg(long, default_value_t = 9)]
        level: u32,
        /// Replace an existing output file
        #[arg(long)]
        overwrite: bool,
    },
}

#[derive(Subcommand)]
enum MapCmd {
    /// Version, size, cell types (GAT), textures (GND) or objects (RSW) of a map file
    Info {
        input: PathBuf,
        entry: Option<String>,
    },
    /// Picture of a GAT: green walkable, blue water, grey not walkable; north at the top
    Png {
        input: PathBuf,
        entry: Option<String>,
        /// Enlarge every cell to scale x scale pixels
        #[arg(long, default_value_t = 1)]
        scale: u32,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Change the type and/or height of GAT cells in a rectangle (writes a new file)
    GatSet {
        input: PathBuf,
        entry: Option<String>,
        /// One corner of the rectangle `x,y` (x east, y north, origin south-west)
        #[arg(long, value_parser = parse_cell)]
        from: (u32, u32),
        /// The opposite corner (default: the same cell)
        #[arg(long, value_parser = parse_cell)]
        to: Option<(u32, u32)>,
        /// Cell type: 0 walkable, 1 not walkable, 2 not walkable and not snipable, 3 water, 5 not
        /// walkable but snipable, 6 walkable
        #[arg(long = "type")]
        kind: Option<i32>,
        /// Height for all four corners of every cell
        #[arg(long)]
        height: Option<f32>,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Check that every file the maps name exists: ground, altitude, textures, models, sounds
    Check {
        /// A client folder (with DATA.INI) or a .grf
        input: PathBuf,
        /// Only these maps (names without .rsw); default: all
        maps: Vec<String>,
    },
}

#[derive(Subcommand)]
enum ClientCmd {
    /// Report what could go wrong before the client is started (exit 0 ok, 2 warnings, 3 errors)
    Check {
        /// The client folder (with DATA.INI)
        folder: PathBuf,
        /// Skip the Lua syntax check
        #[arg(long)]
        no_lua: bool,
    },
}

#[derive(Subcommand)]
enum LuaCmd {
    /// Lua 5.1 syntax check of a file, a folder (recursive) or the Lua files of a GRF
    Check {
        /// A .lub/.lua file, a folder, or a .grf archive
        input: PathBuf,
        /// For a GRF: only entries matching this pattern (glob or substring)
        pattern: Option<String>,
    },
    /// Decompile a compiled Lua 5.0 or 5.1 file (.lub) to source; code of a shape that is not understood
    /// stays as a bytecode listing in comments, so the output may be partial (exit code 2 then)
    Decompile {
        /// A .lub file, or a .grf archive together with --entry
        input: PathBuf,
        /// The entry inside the GRF (e.g. data\\luafiles514\\lua files\\deletefiles.lub)
        #[arg(long)]
        entry: Option<String>,
        /// Write the source here instead of printing it
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Print the bytecode listing (like luac -l) instead of source
        #[arg(long)]
        listing: bool,
    },
    /// Compile Lua 5.1 (or, with --lua-version 5.0, 5.0) source (.lua, e.g. an edited decompiled
    /// file) to a client-ready .lub: the same bytes as the real `luac` makes, in the 32-bit layout
    /// the Ragnarok Online client loads
    Compile {
        /// The source file
        input: PathBuf,
        /// The .lub to write
        #[arg(short, long)]
        output: PathBuf,
        /// Drop line numbers and local names (luac -s); smaller, but error messages lose their lines
        #[arg(long)]
        strip: bool,
        /// How non-ASCII characters become bytes: auto (UTF-8 if the first line says
        /// `strings: UTF-8`, else CP949), cp949 or utf8
        #[arg(long, value_enum, default_value = "auto")]
        encoding: EncodingArg,
        /// Name stored in the file for error messages (default: `@` and the file name)
        #[arg(long)]
        name: Option<String>,
        /// Replace an existing output file
        #[arg(long)]
        overwrite: bool,
        /// Lua version of the source and of the file: 5.1, or 5.0 (62 of the kRO archive's files)
        #[arg(long, value_enum, default_value = "5.1")]
        lua_version: LuaVersionArg,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum LuaVersionArg {
    #[value(name = "5.1")]
    V51,
    #[value(name = "5.0")]
    V50,
}

#[derive(Clone, Copy, ValueEnum)]
enum EncodingArg {
    Auto,
    Cp949,
    Utf8,
}

#[derive(Subcommand)]
enum ActCmd {
    /// Version, actions, frames, layers, sounds (a loose .act file, or a GRF and the entry path)
    Info {
        input: PathBuf,
        entry: Option<String>,
    },
    /// Draw frames with the sprite next to the .act (or the same path with .spr inside the GRF)
    Render {
        input: PathBuf,
        entry: Option<String>,
        /// Sprite to use instead of the one with the same name
        #[arg(long)]
        spr: Option<PathBuf>,
        /// Action (direction/animation) number
        #[arg(long, default_value_t = 0)]
        action: usize,
        /// One frame (then -o is a file); without it every frame of the action (then -o is a folder)
        #[arg(long)]
        frame: Option<usize>,
        /// More ACTs drawn on top, e.g. headgear (loose files with their .spr beside them, or
        /// entries of the same GRF); repeatable
        #[arg(long = "with")]
        with: Vec<String>,
        /// Do not bind the --with parts to the base by their anchors
        #[arg(long)]
        no_anchor: bool,
        /// Write one animated GIF (-o is the file) instead of PNGs
        #[arg(long)]
        gif: bool,
        /// Enlarge every pixel to scale x scale pixels
        #[arg(long, default_value_t = 1)]
        scale: u32,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Move all layers and anchors (writes a new file)
    Translate {
        input: PathBuf,
        entry: Option<String>,
        /// Only this action (default: all)
        #[arg(long)]
        action: Option<usize>,
        #[arg(long, allow_hyphen_values = true, default_value_t = 0.0)]
        dx: f32,
        #[arg(long, allow_hyphen_values = true, default_value_t = 0.0)]
        dy: f32,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Set the frame time of an action (units of 25 ms: 6 = 150 ms)
    Interval {
        input: PathBuf,
        entry: Option<String>,
        #[arg(long)]
        action: usize,
        #[arg(long)]
        value: f32,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Copy an action to a new position
    CopyAction {
        input: PathBuf,
        entry: Option<String>,
        #[arg(long)]
        from: usize,
        /// Position of the copy (0 up to the number of actions)
        #[arg(long)]
        to: usize,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Remove an action
    RemoveAction {
        input: PathBuf,
        entry: Option<String>,
        #[arg(long)]
        action: usize,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
}

#[derive(Subcommand)]
enum SprCmd {
    /// Version, frames and sizes of a sprite (a loose .spr file, or a GRF and the entry path)
    Info {
        input: PathBuf,
        entry: Option<String>,
    },
    /// Write every frame as PNG (palette images as indexed PNG, true-colour as RGBA)
    Export {
        input: PathBuf,
        entry: Option<String>,
        /// Output folder (frames are numbered 000.png, 001.png, ...)
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Build an .spr from the PNGs of a folder (sorted by name)
    Build {
        folder: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        /// SPR version: 2.1 (default, RLE as in the client files), 2.0 (raw), 1.1 or 1.0 (palette images only)
        #[arg(long, default_value = "2.1")]
        version: rgrf_formats::spr::Version,
        #[arg(long)]
        overwrite: bool,
    },
    /// Replace one image with a PNG (writes a new file)
    Replace {
        input: PathBuf,
        entry: Option<String>,
        /// Image kind: indexed (palette) or rgba (true colour)
        #[arg(long, value_enum, default_value = "indexed")]
        kind: edit_cmd::KindArg,
        /// Index among the images of that kind (see `spr info`)
        #[arg(long)]
        frame: usize,
        #[arg(long)]
        png: PathBuf,
        /// Map PNG colours to the sprite's palette (nearest colour, alpha below 128 is transparent)
        #[arg(long)]
        nearest: bool,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Append an image from a PNG (writes a new file, prints the index)
    Add {
        input: PathBuf,
        entry: Option<String>,
        #[arg(long, value_enum, default_value = "indexed")]
        kind: edit_cmd::KindArg,
        #[arg(long)]
        png: PathBuf,
        #[arg(long)]
        nearest: bool,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Remove one image; with --act/--act-output the layers of that ACT follow
    Remove {
        input: PathBuf,
        entry: Option<String>,
        #[arg(long, value_enum, default_value = "indexed")]
        kind: edit_cmd::KindArg,
        #[arg(long)]
        frame: usize,
        /// The .act that belongs to the sprite (layers using the image become empty, later ones move down)
        #[arg(long, requires = "act_output")]
        act: Option<PathBuf>,
        #[arg(long, requires = "act")]
        act_output: Option<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Convert between palette images and true-colour images (writes a new file; with --act/--act-output
    /// the layers of that ACT follow)
    Convert {
        input: PathBuf,
        entry: Option<String>,
        /// `rgba`: palette images become true-colour (the palette is dropped). `indexed`: true-colour
        /// images become palette images (lossless when they use at most 255 colours, else reduced)
        #[arg(long, value_enum)]
        to: edit_cmd::ConvertTo,
        #[arg(long, requires = "act_output")]
        act: Option<PathBuf>,
        #[arg(long, requires = "act")]
        act_output: Option<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
    /// Use the colours of a .pal file (the images keep their indices)
    SetPalette {
        input: PathBuf,
        entry: Option<String>,
        #[arg(long)]
        pal: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
    },
}

fn parse_cell(s: &str) -> Result<(u32, u32), String> {
    let (x, y) = s
        .split_once(',')
        .ok_or_else(|| format!("'{s}': expected x,y"))?;
    let n = |v: &str| v.trim().parse::<u32>().map_err(|e| format!("'{v}': {e}"));
    Ok((n(x)?, n(y)?))
}

fn parse_format(s: &str) -> Result<Format, String> {
    Format::parse(s).ok_or_else(|| format!("unknown format '{s}' (0x102, 0x103, 0x200 or 0x300)"))
}

#[derive(Clone, Copy, ValueEnum)]
enum MethodArg {
    Zlib,
    Zopfli,
    Lzma,
}

#[derive(clap::Args)]
struct WriteArgs {
    /// Without it every stream is copied unchanged (fast, nothing is compressed again). With it
    /// entries are compressed again and kept only where smaller: zlib (every client), zopfli
    /// (zlib-compatible, smaller, slow) or lzma (only with the LZMA-capable cps.dll)
    #[arg(long, value_enum)]
    method: Option<MethodArg>,
    /// zlib: 0-9 (default 9); zopfli: iterations (default 10); lzma: preset 0-9 (default 9)
    #[arg(long, requires = "method")]
    level: Option<u32>,
    /// 0x200 (32-bit offsets, max 4 GiB) or 0x300 (default: 0x300 if any input is 0x300)
    #[arg(long, value_parser = parse_format)]
    format: Option<Format>,
    /// Pad streams to a multiple of 8 (default: yes for 0x200, no for 0x300)
    #[arg(long, conflicts_with = "no_align")]
    align: bool,
    #[arg(long)]
    no_align: bool,
    /// none: no sharing; source: keep the sharing of the inputs (default); content: share blocks
    /// of identical content, also across inputs
    #[arg(long, value_enum, default_value = "source")]
    dedupe: DedupeArg,
    /// Always use the new stream, never keep the original stream of the same codec
    #[arg(long, requires = "method")]
    recompress_all: bool,
    /// Skip decoding the written archive for comparison (not recommended)
    #[arg(long)]
    no_verify: bool,
    /// Replace an existing output file
    #[arg(long)]
    overwrite: bool,
    /// Only plan, write nothing
    #[arg(long)]
    dry_run: bool,
}

impl WriteArgs {
    fn options(&self) -> Result<RepackOptions> {
        Ok(RepackOptions {
            method: match self.method {
                Some(m) => method_from(m, self.level)?,
                None => Method::DEFAULT,
            },
            copy_streams: self.method.is_none(),
            format: self.format,
            align: if self.align {
                Some(true)
            } else if self.no_align {
                Some(false)
            } else {
                None
            },
            dedupe: match self.dedupe {
                DedupeArg::None => Dedupe::None,
                DedupeArg::Source => Dedupe::Source,
                DedupeArg::Content => Dedupe::Content,
            },
            recompress_all: self.recompress_all,
            verify: !self.no_verify,
            overwrite: self.overwrite,
            dry_run: self.dry_run,
            max_file_len: None,
        })
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum DedupeArg {
    None,
    Source,
    Content,
}

#[derive(Clone, Copy, ValueEnum)]
enum Collision {
    Abort,
    Skip,
    Rename,
}

fn main() {
    let cli = Cli::parse();
    if let Some(n) = cli.threads
        && let Err(e) = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
    {
        eprintln!("rgrf: error: cannot set threads: {e}");
        std::process::exit(1);
    }
    let code = match run(&cli) {
        Ok(code) => code,
        Err(e) => {
            let broken_pipe = e.chain().any(|c| {
                c.downcast_ref::<io::Error>()
                    .is_some_and(|i| i.kind() == io::ErrorKind::BrokenPipe)
            });
            if broken_pipe {
                std::process::exit(0);
            }
            eprintln!("rgrf: error: {e:#}");
            1
        }
    };
    std::process::exit(code);
}

fn open(path: &Path) -> Result<(Archive, u128)> {
    let t = Instant::now();
    let a = Archive::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    Ok((a, t.elapsed().as_millis()))
}

fn flags_text(e: &Entry) -> String {
    let mut s = String::new();
    // F compressed file, R file stored uncompressed (flags 0 with data), X delete directive
    s.push(if e.is_removal() {
        'X'
    } else if e.is_file() {
        'F'
    } else if e.is_raw() {
        'R'
    } else {
        '-'
    });
    // H header crypted (0x02), D data crypted (0x04), G Gravity protected (0x80)
    s.push(if e.flags & 0x02 != 0 { 'H' } else { '-' });
    s.push(if e.flags & 0x04 != 0 {
        'D'
    } else if e.flags & 0x80 != 0 {
        'G'
    } else {
        '-'
    });
    s
}

fn entry_json(i: usize, e: &Entry, mode: NameMode) -> Value {
    let kind = if e.is_removal() {
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
    };
    json!({
        "index": i,
        "name": e.name.display(mode),
        "name_hex": hex(e.name.as_bytes()),
        "size": e.size,
        "zsize": e.zsize,
        "zsize_aligned": e.zsize_aligned,
        "flags": e.flags,
        "offset": e.abs_offset(),
        "kind": kind,
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

fn status_text(s: &DecodeStatus) -> String {
    match s {
        DecodeStatus::Ok => "ok".into(),
        DecodeStatus::Truncated { got, expected } => {
            format!("truncated ({got} of {expected} bytes)")
        }
        DecodeStatus::SizeMismatch { got, expected } => {
            format!("size mismatch ({got}, table says {expected})")
        }
        DecodeStatus::Corrupt(m) => format!("corrupt ({m})"),
        DecodeStatus::Encrypted => "encrypted".into(),
    }
}

fn print_json(v: &Value) -> Result<()> {
    let mut out = BufWriter::new(io::stdout().lock());
    serde_json::to_writer(&mut out, v)?;
    writeln!(out)?;
    out.flush()?;
    Ok(())
}

/// Progress on stderr, only when a human is watching.
struct Meter {
    show: bool,
}

impl Meter {
    fn new(cli: &Cli) -> Self {
        Self {
            show: !cli.quiet && !cli.json && io::stderr().is_terminal(),
        }
    }

    fn callback(&self) -> impl Fn(Progress) + Sync + use<> {
        let show = self.show;
        move |p: Progress| {
            if show {
                eprint!("\r  {}/{}  ", p.done, p.total);
            }
        }
    }

    fn done(&self) {
        if self.show {
            eprint!("\r{:30}\r", "");
        }
    }
}

fn run(cli: &Cli) -> Result<i32> {
    match &cli.command {
        Cmd::Info { grf, codecs } => info(cli, grf, *codecs),
        Cmd::Ls { grf, pattern, long } => list(cli, grf, pattern.as_deref(), &[], *long),
        Cmd::Find { grf, pattern, ext } => list(cli, grf, pattern.as_deref(), ext, true),
        Cmd::Cat { grf, path } => cat(cli, grf, path),
        Cmd::Extract {
            grf,
            dest,
            pattern,
            on_collision,
            overwrite,
        } => extract_cmd(
            cli,
            grf,
            dest,
            pattern.as_deref(),
            *on_collision,
            *overwrite,
        ),
        Cmd::Verify {
            grf,
            no_decode,
            all,
        } => verify_cmd(cli, grf, *no_decode, *all),
        Cmd::Hash { grf, pattern } => hash_cmd(cli, grf, pattern.as_deref()),
        Cmd::Thor { action } => match action {
            ThorCmd::Create {
                source,
                output,
                merge_into_grf,
                target,
                remove,
                integrity,
                method,
                level,
                overwrite,
            } => patch_cmd::thor_create(
                &patch_cmd::ThorArgs {
                    source,
                    output,
                    merge_into_grf: *merge_into_grf,
                    target: target.as_deref(),
                    remove,
                    integrity: *integrity,
                    method: method_from(*method, *level)?,
                    overwrite: *overwrite,
                },
                cli.names,
                cli.json,
            ),
        },
        Cmd::Rgz { action } => match action {
            RgzCmd::Create {
                source,
                output,
                level,
                overwrite,
            } => patch_cmd::rgz_create(source, output, *level, *overwrite, cli.names, cli.json),
        },
        Cmd::Map { action } => match action {
            MapCmd::Info { input, entry } => {
                map_cmd::info(input, entry.as_deref(), cli.names, cli.json)
            }
            MapCmd::Png {
                input,
                entry,
                scale,
                output,
                overwrite,
            } => map_cmd::png(
                input,
                entry.as_deref(),
                output,
                *scale,
                *overwrite,
                cli.names,
                cli.json,
            ),
            MapCmd::GatSet {
                input,
                entry,
                from,
                to,
                kind,
                height,
                output,
                overwrite,
            } => map_cmd::gat_set(
                input,
                entry.as_deref(),
                (*from, to.unwrap_or(*from)),
                (*kind, *height),
                output,
                *overwrite,
                cli.names,
                cli.json,
            ),
            MapCmd::Check { input, maps } => map_cmd::check(input, maps, cli.names, cli.json),
        },
        Cmd::Client { action } => match action {
            ClientCmd::Check { folder, no_lua } => {
                client_cmd::check(folder, cli.names, cli.json, !*no_lua)
            }
        },
        Cmd::Lua { action } => match action {
            LuaCmd::Compile {
                input,
                output,
                strip,
                encoding,
                name,
                overwrite,
                lua_version,
            } => lua_cmd::compile(
                input,
                output,
                *strip,
                match encoding {
                    EncodingArg::Auto => rgrf_formats::luacomp::TextEncoding::Auto,
                    EncodingArg::Cp949 => rgrf_formats::luacomp::TextEncoding::Cp949,
                    EncodingArg::Utf8 => rgrf_formats::luacomp::TextEncoding::Utf8,
                },
                name.as_deref(),
                *overwrite,
                cli.json,
                matches!(lua_version, LuaVersionArg::V50),
            ),
            LuaCmd::Check { input, pattern } => {
                lua_cmd::check(input, pattern.as_deref(), cli.names, cli.json)
            }
            LuaCmd::Decompile {
                input,
                entry,
                output,
                listing,
            } => lua_cmd::decompile(
                input,
                entry.as_deref(),
                output.as_deref(),
                *listing,
                cli.names,
            ),
        },
        Cmd::Act { action } => match action {
            ActCmd::Info { input, entry } => {
                act_cmd::info(input, entry.as_deref(), cli.names, cli.json)
            }
            ActCmd::Translate {
                input,
                entry,
                action,
                dx,
                dy,
                output,
                overwrite,
            } => edit_cmd::act_translate(
                input,
                entry.as_deref(),
                *action,
                *dx,
                *dy,
                cli.names,
                &edit_cmd::ActOut {
                    output,
                    overwrite: *overwrite,
                    json: cli.json,
                },
            ),
            ActCmd::Interval {
                input,
                entry,
                action,
                value,
                output,
                overwrite,
            } => edit_cmd::act_interval(
                input,
                entry.as_deref(),
                *action,
                *value,
                cli.names,
                &edit_cmd::ActOut {
                    output,
                    overwrite: *overwrite,
                    json: cli.json,
                },
            ),
            ActCmd::CopyAction {
                input,
                entry,
                from,
                to,
                output,
                overwrite,
            } => edit_cmd::act_copy_action(
                input,
                entry.as_deref(),
                *from,
                *to,
                cli.names,
                &edit_cmd::ActOut {
                    output,
                    overwrite: *overwrite,
                    json: cli.json,
                },
            ),
            ActCmd::RemoveAction {
                input,
                entry,
                action,
                output,
                overwrite,
            } => edit_cmd::act_remove_action(
                input,
                entry.as_deref(),
                *action,
                cli.names,
                &edit_cmd::ActOut {
                    output,
                    overwrite: *overwrite,
                    json: cli.json,
                },
            ),
            ActCmd::Render {
                input,
                entry,
                spr,
                action,
                frame,
                with,
                no_anchor,
                gif,
                scale,
                output,
                overwrite,
            } => act_cmd::render(&act_cmd::RenderArgs {
                input,
                entry: entry.as_deref(),
                spr: spr.as_deref(),
                with,
                no_anchor: *no_anchor,
                action: *action,
                frame: *frame,
                gif: *gif,
                scale: *scale,
                output,
                overwrite: *overwrite,
                names: cli.names,
                json: cli.json,
            }),
        },
        Cmd::Spr { action } => match action {
            SprCmd::Info { input, entry } => {
                spr_cmd::info(input, entry.as_deref(), cli.names, cli.json)
            }
            SprCmd::Export {
                input,
                entry,
                output,
                overwrite,
            } => spr_cmd::export(
                input,
                entry.as_deref(),
                output,
                *overwrite,
                cli.names,
                cli.json,
            ),
            SprCmd::Replace {
                input,
                entry,
                kind,
                frame,
                png,
                nearest,
                output,
                overwrite,
            } => edit_cmd::spr_replace(
                input,
                entry.as_deref(),
                *kind,
                *frame,
                png,
                *nearest,
                output,
                *overwrite,
                cli.names,
                cli.json,
            ),
            SprCmd::Add {
                input,
                entry,
                kind,
                png,
                nearest,
                output,
                overwrite,
            } => edit_cmd::spr_add(
                input,
                entry.as_deref(),
                *kind,
                png,
                *nearest,
                output,
                *overwrite,
                cli.names,
                cli.json,
            ),
            SprCmd::Remove {
                input,
                entry,
                kind,
                frame,
                act,
                act_output,
                output,
                overwrite,
            } => edit_cmd::spr_remove(
                input,
                entry.as_deref(),
                *kind,
                *frame,
                act.as_deref(),
                act_output.as_deref(),
                output,
                *overwrite,
                cli.names,
                cli.json,
            ),
            SprCmd::Convert {
                input,
                entry,
                to,
                act,
                act_output,
                output,
                overwrite,
            } => edit_cmd::spr_convert(
                input,
                entry.as_deref(),
                *to,
                act.as_deref(),
                act_output.as_deref(),
                output,
                *overwrite,
                cli.names,
                cli.json,
            ),
            SprCmd::SetPalette {
                input,
                entry,
                pal,
                output,
                overwrite,
            } => edit_cmd::spr_set_palette(
                input,
                entry.as_deref(),
                pal,
                output,
                *overwrite,
                cli.names,
                cli.json,
            ),
            SprCmd::Build {
                folder,
                output,
                version,
                overwrite,
            } => spr_cmd::build(folder, output, *version, *overwrite, cli.json),
        },
        Cmd::Build {
            folder,
            output,
            method,
            level,
            format,
            align,
            no_align,
            dedupe,
            no_verify,
            overwrite,
            dry_run,
        } => {
            let opts = BuildOptions {
                method: method_from(*method, *level)?,
                format: *format,
                align: if *align {
                    Some(true)
                } else if *no_align {
                    Some(false)
                } else {
                    None
                },
                dedupe: *dedupe,
                names: cli.names,
                verify: !*no_verify,
                overwrite: *overwrite,
                dry_run: *dry_run,
                max_file_len: None,
            };
            build_cmd(cli, folder, output, &opts)
        }
        Cmd::Diff { a, b, all } => diff_cmd(cli, a, b, *all),
        Cmd::Merge {
            grfs,
            output,
            replace_only,
            write,
        } => merge_cmd(cli, grfs, output, *replace_only, &write.options()?),
        Cmd::Patch {
            old,
            new,
            output,
            write,
        } => patch_cmd(cli, old, new, output, &write.options()?),
        Cmd::Repack {
            grf,
            output,
            method,
            level,
            format,
            align,
            no_align,
            dedupe,
            recompress_all,
            no_verify,
            overwrite,
            dry_run,
        } => {
            let method = method_from(*method, *level)?;
            let opts = RepackOptions {
                method,
                format: *format,
                align: if *align {
                    Some(true)
                } else if *no_align {
                    Some(false)
                } else {
                    None
                },
                dedupe: match dedupe {
                    DedupeArg::None => Dedupe::None,
                    DedupeArg::Source => Dedupe::Source,
                    DedupeArg::Content => Dedupe::Content,
                },
                recompress_all: *recompress_all,
                verify: !*no_verify,
                overwrite: *overwrite,
                dry_run: *dry_run,
                copy_streams: false,
                max_file_len: None,
            };
            repack_cmd(cli, grf, output, &opts)
        }
    }
}

fn info(cli: &Cli, grf: &Path, codecs: bool) -> Result<i32> {
    let (a, open_ms) = open(grf)?;
    let es = a.entries();
    let markers = es.iter().filter(|e| e.is_marker()).count();
    let removals = es.iter().filter(|e| e.is_removal()).count();
    let raw = es.iter().filter(|e| e.is_raw()).count();
    let encrypted = es.iter().filter(|e| e.needs_key()).count();
    let des = es.iter().filter(|e| e.is_des()).count();
    let packed: u64 = es.iter().map(|e| u64::from(e.zsize)).sum();
    let unpacked: u64 = es.iter().map(|e| u64::from(e.size)).sum();
    let mut seen = HashSet::new();
    let shared = es
        .iter()
        .filter(|e| e.zsize > 0 && !seen.insert((e.offset, e.zsize)))
        .count();
    let counts = if codecs {
        let c: Vec<Option<Codec>> = (0..a.len())
            .into_par_iter()
            .map(|i| a.sniff_codec(i))
            .collect::<std::result::Result<_, _>>()?;
        Some((
            c.iter().filter(|c| **c == Some(Codec::Zlib)).count(),
            c.iter().filter(|c| **c == Some(Codec::Lzma)).count(),
        ))
    } else {
        None
    };
    let t = a.table_info();
    if cli.json {
        let mut v = json!({
            "schema_version": SCHEMA_VERSION,
            "path": grf.display().to_string(),
            "file_size": a.file_len(),
            "version": format!("0x{:03x}", a.header().version),
            "kind": a.header().kind.name(),
            "entries": es.len(), "files": es.len() - markers, "markers": markers, "raw": raw,
            "removals": removals,
            "encrypted": encrypted, "des_scrambled": des, "shared_data_entries": shared,
            "packed_bytes": packed, "unpacked_bytes": unpacked,
            "table": {"offset": t.abs_offset, "packed": t.zsize, "unpacked": t.size, "key_block": t.key_block},
            "open_ms": open_ms,
        });
        if let Some((z, l)) = counts {
            v["codecs"] = json!({"zlib": z, "lzma": l});
        }
        if let Some(p) = &a.header().patch {
            v["patch"] = json!({
                "merge_into_grf": p.merge_into_grf,
                "target_grf": GrfPath::new(p.target_grf.clone()).display(cli.names),
            });
        }
        print_json(&v)?;
    } else {
        println!("File:       {} ({} bytes)", grf.display(), a.file_len());
        if a.header().kind == rgrf_core::container::Kind::Grf {
            println!("Version:    0x{:03x}", a.header().version);
        } else {
            println!("Format:     {}", a.header().format_name());
        }
        if let Some(p) = &a.header().patch {
            let target = GrfPath::new(p.target_grf.clone()).display(cli.names);
            println!(
                "Applies to: {}",
                match (p.merge_into_grf, target.is_empty()) {
                    (true, true) => "merged into the default GRF".to_owned(),
                    (true, false) => format!("merged into {target}"),
                    (false, _) => "the client folder".to_owned(),
                }
            );
        }
        println!(
            "Entries:    {} ({} files, {} empty markers; {} files stored uncompressed)",
            es.len(),
            es.len() - markers,
            markers - removals,
            raw
        );
        if removals > 0 {
            println!("Removals:   {removals} delete directives");
        }
        println!("Content:    {packed} bytes packed, {unpacked} bytes unpacked (sum over entries)");
        println!(
            "Table:      at {}, {} bytes packed, {} unpacked",
            t.abs_offset, t.zsize, t.size
        );
        if t.key_block > 0 {
            println!("Key block:  {} bytes (protected archive)", t.key_block);
        }
        println!("Encrypted:  {encrypted}");
        if des > 0 {
            println!("DES:        {des} entries scrambled with the old DES scheme (readable)");
        }
        println!("Shared:     {shared} entries share data with another entry");
        if let Some((z, l)) = counts {
            println!("Codecs:     zlib {z}, LZMA {l}");
        }
        println!("Opened in:  {open_ms} ms");
    }
    Ok(0)
}

fn list(cli: &Cli, grf: &Path, pattern: Option<&str>, ext: &[String], long: bool) -> Result<i32> {
    let (a, _) = open(grf)?;
    let mode = cli.names;
    let wanted: Vec<Vec<u8>> = ext
        .iter()
        .map(|e| e.trim_start_matches('.').to_ascii_lowercase().into_bytes())
        .collect();
    let indices: Vec<usize> = a
        .select(pattern, mode)
        .into_iter()
        .filter(|&i| {
            wanted.is_empty()
                || a.entries()[i]
                    .name
                    .extension_lowercase()
                    .is_some_and(|x| wanted.contains(&x))
        })
        .collect();
    let mut out = BufWriter::new(io::stdout().lock());
    if cli.json {
        write!(
            out,
            "{{\"schema_version\":{SCHEMA_VERSION},\"count\":{},\"entries\":[",
            indices.len()
        )?;
        for (n, &i) in indices.iter().enumerate() {
            if n > 0 {
                out.write_all(b",")?;
            }
            serde_json::to_writer(&mut out, &entry_json(i, &a.entries()[i], mode))?;
        }
        writeln!(out, "]}}")?;
    } else {
        for &i in &indices {
            let e = &a.entries()[i];
            if long {
                writeln!(
                    out,
                    "{:>10} {:>10} {} {:>11}  {}",
                    e.size,
                    e.zsize,
                    flags_text(e),
                    e.abs_offset(),
                    e.name.display(mode)
                )?;
            } else {
                writeln!(out, "{}", e.name.display(mode))?;
            }
        }
    }
    out.flush()?;
    Ok(0)
}

fn cat(cli: &Cli, grf: &Path, path: &str) -> Result<i32> {
    let (a, _) = open(grf)?;
    let wanted = GrfPath::from_display(path, cli.names).ok_or_else(|| {
        anyhow!(
            "'{path}' cannot be written in the name mode {:?}",
            cli.names
        )
    })?;
    let Some(i) = a.find(&wanted) else {
        bail!("no entry named '{path}' (names are compared case-insensitively, / equals \\)");
    };
    let d = a.read(i)?;
    if d.status == DecodeStatus::Encrypted {
        bail!("entry is encrypted; decryption is not supported yet");
    }
    let mut out = io::stdout().lock();
    out.write_all(&d.data)?;
    out.flush()?;
    if d.status.is_ok() {
        Ok(0)
    } else {
        eprintln!("rgrf: warning: {}: {}", path, status_text(&d.status));
        Ok(2)
    }
}

fn extract_cmd(
    cli: &Cli,
    grf: &Path,
    dest: &Path,
    pattern: Option<&str>,
    on_collision: Collision,
    overwrite: bool,
) -> Result<i32> {
    let (a, _) = open(grf)?;
    let mode = cli.names;
    let indices = a.select(pattern, mode);
    let meter = Meter::new(cli);
    let cb = meter.callback();
    let ctl = Control {
        cancel: None,
        progress: Some(&cb),
    };
    let opts = ExtractOptions {
        names: mode,
        on_collision: match on_collision {
            Collision::Abort => OnCollision::Abort,
            Collision::Skip => OnCollision::Skip,
            Collision::Rename => OnCollision::Rename,
        },
        overwrite,
    };
    let started = Instant::now();
    let r = extract(&a, &indices, dest, &opts, &ctl);
    meter.done();
    let r = r?;
    let warn = !r.damaged.is_empty() || r.skipped_encrypted > 0 || r.skipped_collisions > 0;
    if cli.json {
        let damaged: Vec<Value> = r
            .damaged
            .iter()
            .map(|d| {
                let mut v = entry_json(d.index, &a.entries()[d.index], mode);
                v["status"] = status_json(&d.status);
                v
            })
            .collect();
        print_json(&json!({
            "schema_version": SCHEMA_VERSION,
            "selected": r.selected, "written": r.written, "bytes": r.bytes,
            "skipped_markers": r.skipped_markers, "skipped_encrypted": r.skipped_encrypted,
            "skipped_collisions": r.skipped_collisions, "renamed": r.renamed,
            "damaged": damaged, "seconds": started.elapsed().as_secs_f64(),
        }))?;
    } else {
        println!(
            "{} files written ({:.1} MiB) to {} in {:.1}s",
            r.written,
            r.bytes as f64 / 1_048_576.0,
            dest.display(),
            started.elapsed().as_secs_f64()
        );
        println!(
            "skipped: {} markers, {} encrypted, {} collisions; renamed: {}",
            r.skipped_markers, r.skipped_encrypted, r.skipped_collisions, r.renamed
        );
        for d in &r.damaged {
            eprintln!(
                "warning: {}: {}",
                a.entries()[d.index].name.display(mode),
                status_text(&d.status)
            );
        }
    }
    Ok(if warn { 2 } else { 0 })
}

fn verify_cmd(cli: &Cli, grf: &Path, no_decode: bool, all: bool) -> Result<i32> {
    let (a, _) = open(grf)?;
    let meter = Meter::new(cli);
    let cb = meter.callback();
    let ctl = Control {
        cancel: None,
        progress: Some(&cb),
    };
    let started = Instant::now();
    let r = verify(&a, VerifyOptions { decode: !no_decode }, &ctl);
    meter.done();
    let r = r?;
    let mode = cli.names;
    let label = |s: Severity| -> &'static str { s.into() };
    if cli.json {
        let issues: Vec<Value> = r
            .issues
            .iter()
            .map(|i| {
                let mut v = json!({
                    "severity": label(i.severity),
                    "kind": format!("{:?}", i.kind),
                    "message": i.message,
                });
                if let Some(e) = i.entry {
                    v["entry"] = entry_json(e, &a.entries()[e], mode);
                }
                v
            })
            .collect();
        print_json(&json!({
            "schema_version": SCHEMA_VERSION,
            "entries": r.entries, "files": r.files, "markers": r.markers, "raw": r.raw, "decoded": r.decoded,
            "errors": r.count(Severity::Error), "warnings": r.count(Severity::Warning),
            "infos": r.count(Severity::Info), "issues": issues,
            "seconds": started.elapsed().as_secs_f64(),
        }))?;
    } else {
        for sev in [Severity::Error, Severity::Warning, Severity::Info] {
            let list: Vec<_> = r.issues.iter().filter(|i| i.severity == sev).collect();
            let shown = if all { list.len() } else { list.len().min(50) };
            for i in &list[..shown] {
                let who = i
                    .entry
                    .map(|e| format!("{}: ", a.entries()[e].name.display(mode)))
                    .unwrap_or_default();
                println!("{:7} {who}{}", label(sev), i.message);
            }
            if shown < list.len() {
                println!(
                    "{:7} ... and {} more (use --all or --json)",
                    label(sev),
                    list.len() - shown
                );
            }
        }
        println!(
            "{} entries ({} files, {} markers), {} decoded: {} errors, {} warnings, {} infos in {:.1}s",
            r.entries,
            r.files,
            r.markers,
            r.decoded,
            r.count(Severity::Error),
            r.count(Severity::Warning),
            r.count(Severity::Info),
            started.elapsed().as_secs_f64()
        );
    }
    Ok(r.exit_code())
}

type HashRow = (usize, Option<Digest>, DecodeStatus, usize);

fn hash_cmd(cli: &Cli, grf: &Path, pattern: Option<&str>) -> Result<i32> {
    let (a, _) = open(grf)?;
    let mode = cli.names;
    let indices = a.select(pattern, mode);
    let meter = Meter::new(cli);
    let cb = meter.callback();
    let total = indices.len() as u64;
    let done = AtomicU64::new(0);
    let rows: Vec<Result<HashRow>> = indices
        .par_iter()
        .map(|&i| {
            let d = a.read(i)?;
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n.is_multiple_of(512) || n == total {
                cb(Progress { done: n, total });
            }
            let digest = (d.status != DecodeStatus::Encrypted && !a.entries()[i].is_marker())
                .then(|| Digest::of(&d.data));
            Ok((i, digest, d.status, d.data.len()))
        })
        .collect();
    meter.done();
    let mut out = BufWriter::new(io::stdout().lock());
    let mut warnings = false;
    if cli.json {
        write!(out, "{{\"schema_version\":{SCHEMA_VERSION},\"entries\":[")?;
    }
    for (n, r) in rows.into_iter().enumerate() {
        let (i, digest, status, len) = r?;
        let e = &a.entries()[i];
        warnings |= !status.is_ok();
        if cli.json {
            if n > 0 {
                out.write_all(b",")?;
            }
            let mut v = entry_json(i, e, mode);
            v["length"] = json!(len);
            v["crc32"] = digest.map_or(Value::Null, |d| json!(d.crc32_hex()));
            v["md5"] = digest.map_or(Value::Null, |d| json!(d.md5_hex()));
            v["status"] = status_json(&status);
            serde_json::to_writer(&mut out, &v)?;
        } else {
            let (crc, md5) =
                digest.map_or(("-".into(), "-".into()), |d| (d.crc32_hex(), d.md5_hex()));
            let note = if status.is_ok() {
                String::new()
            } else {
                format!("  [{}]", status_text(&status))
            };
            writeln!(out, "{crc} {md5} {len:>10} {}{note}", e.name.display(mode))?;
        }
    }
    if cli.json {
        writeln!(out, "]}}")?;
    }
    out.flush()?;
    Ok(if warnings { 2 } else { 0 })
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn repack_cmd(cli: &Cli, grf: &Path, output: &Path, opts: &RepackOptions) -> Result<i32> {
    if same_file(grf, output) {
        bail!("the output must not be the source archive");
    }
    let (a, _) = open(grf)?;
    let meter = Meter::new(cli);
    let cb = meter.callback();
    let ctl = Control {
        cancel: None,
        progress: Some(&cb),
    };
    let started = Instant::now();
    let r = repack(&a, output, opts, &ctl);
    meter.done();
    let r = r?;
    let secs = started.elapsed().as_secs_f64();
    let warn = !r.damaged.is_empty() || r.encrypted_blocks > 0;
    let pct = |from: u64, to: u64| {
        if from == 0 {
            0.0
        } else {
            (to as f64 / from as f64 - 1.0) * 100.0
        }
    };
    if cli.json {
        print_json(&json!({
            "schema_version": SCHEMA_VERSION,
            "dry_run": r.dry_run, "output": output.display().to_string(),
            "format": format!("0x{:03x}", r.format),
            "entries": r.entries, "files": r.files, "markers": r.markers,
            "blocks": r.blocks, "shared_files": r.shared_files,
            "encrypted_blocks": r.encrypted_blocks, "damaged_blocks": r.damaged.len(),
            "stored_blocks": r.stored_blocks,
            "kept_original": r.kept_original, "recompressed": r.recompressed,
            "source_file_bytes": r.source_file_len, "source_stream_bytes": r.source_stream_bytes,
            "output_file_bytes": r.output_file_len, "output_stream_bytes": r.output_stream_bytes,
            "verified": r.verified, "seconds": secs,
        }))?;
    } else {
        println!(
            "{} {} -> {} (0x{:03x}) in {:.1}s",
            if r.dry_run { "Plan for" } else { "Repacked" },
            grf.display(),
            output.display(),
            r.format,
            secs
        );
        println!(
            "Entries:  {} ({} files, {} markers); data blocks {}, files sharing a block {}",
            r.entries, r.files, r.markers, r.blocks, r.shared_files
        );
        if !r.dry_run {
            println!(
                "Streams:  kept original {}, re-encoded {}, copied unchanged: {} stored uncompressed, {} encrypted, {} damaged",
                r.kept_original,
                r.recompressed,
                r.stored_blocks,
                r.encrypted_blocks,
                r.damaged.len()
            );
            println!(
                "Size:     file {} -> {} bytes ({:+.1}%), streams {} -> {} bytes ({:+.1}%)",
                r.source_file_len,
                r.output_file_len,
                pct(r.source_file_len, r.output_file_len),
                r.source_stream_bytes,
                r.output_stream_bytes,
                pct(r.source_stream_bytes, r.output_stream_bytes)
            );
            println!(
                "Checked:  {}",
                if r.verified {
                    "every entry decoded and compared with the source"
                } else {
                    "no (skipped)"
                }
            );
        } else {
            println!(
                "Source streams to carry over: {} bytes in {} blocks",
                r.source_stream_bytes, r.blocks
            );
        }
        for &i in &r.damaged {
            eprintln!(
                "warning: {}: damaged stream copied unchanged",
                a.entries()[i].name.display(cli.names)
            );
        }
    }
    Ok(if warn { 2 } else { 0 })
}

fn show_names(cli: &Cli, title: &str, names: &[GrfPath], all: bool) {
    if names.is_empty() {
        return;
    }
    println!("{title} ({}):", names.len());
    let n = if all {
        names.len()
    } else {
        names.len().min(50)
    };
    for p in &names[..n] {
        println!("  {}", p.display(cli.names));
    }
    if n < names.len() {
        println!("  ... and {} more (--all)", names.len() - n);
    }
}

fn diff_cmd(cli: &Cli, a: &Path, b: &Path, all: bool) -> Result<i32> {
    let (x, _) = open(a)?;
    let (y, _) = open(b)?;
    let started = Instant::now();
    let r = rgrf_core::merge::diff(&x, &y, &Control::default())?;
    let secs = started.elapsed().as_secs_f64();
    let names = |v: &[GrfPath]| -> Vec<String> { v.iter().map(|p| p.display(cli.names)).collect() };
    if cli.json {
        print_json(&json!({
            "schema_version": SCHEMA_VERSION,
            "identical": r.is_identical(),
            "same": r.same,
            "only_a": names(&r.only_a), "only_b": names(&r.only_b), "changed": names(&r.changed),
            "shadowed_a": r.shadowed_a, "shadowed_b": r.shadowed_b, "seconds": secs,
        }))?;
    } else {
        println!("A: {}\nB: {}", a.display(), b.display());
        show_names(cli, "Only in A", &r.only_a, all);
        show_names(cli, "Only in B", &r.only_b, all);
        show_names(cli, "Different content", &r.changed, all);
        println!(
            "{} identical, {} only in A, {} only in B, {} different ({:.1}s){}",
            r.same,
            r.only_a.len(),
            r.only_b.len(),
            r.changed.len(),
            secs,
            if r.shadowed_a + r.shadowed_b > 0 {
                format!(
                    "; ignored {} + {} entries hidden by a later entry with the same path",
                    r.shadowed_a, r.shadowed_b
                )
            } else {
                String::new()
            }
        );
    }
    Ok(if r.is_identical() { 0 } else { 2 })
}

fn write_summary(r: &rgrf_core::repack::RepackReport, output: &Path, secs: f64) {
    println!(
        "{} {} (0x{:03x}) in {:.1}s",
        if r.dry_run { "Plan for" } else { "Wrote" },
        output.display(),
        r.format,
        secs
    );
    println!(
        "Files:    {}; data blocks {}, files sharing a block {}",
        r.files, r.blocks, r.shared_files
    );
    if !r.dry_run {
        println!(
            "Streams:  kept original {}, re-encoded {}, copied unchanged: {} stored uncompressed, {} encrypted, {} damaged",
            r.kept_original,
            r.recompressed,
            r.stored_blocks,
            r.encrypted_blocks,
            r.damaged.len()
        );
        println!(
            "Size:     {} bytes (inputs together {} bytes)",
            r.output_file_len, r.source_file_len
        );
        println!(
            "Checked:  {}",
            if r.verified {
                "every entry checked against its input (decoded, or the copied stream compared)"
            } else {
                "no (skipped)"
            }
        );
    }
}

fn write_json(r: &rgrf_core::repack::RepackReport, output: &Path, secs: f64) -> Value {
    json!({
        "dry_run": r.dry_run, "output": output.display().to_string(),
        "format": format!("0x{:03x}", r.format),
        "files": r.files, "blocks": r.blocks, "shared_files": r.shared_files,
        "encrypted_blocks": r.encrypted_blocks, "damaged_blocks": r.damaged.len(),
        "stored_blocks": r.stored_blocks,
        "kept_original": r.kept_original, "recompressed": r.recompressed,
        "input_file_bytes": r.source_file_len, "output_file_bytes": r.output_file_len,
        "output_stream_bytes": r.output_stream_bytes, "verified": r.verified, "seconds": secs,
    })
}

fn open_all(paths: &[PathBuf], output: &Path) -> Result<Vec<Archive>> {
    let mut archives = Vec::new();
    for p in paths {
        if same_file(p, output) {
            bail!("the output must not be one of the input archives");
        }
        archives.push(open(p)?.0);
    }
    Ok(archives)
}

fn merge_cmd(
    cli: &Cli,
    grfs: &[PathBuf],
    output: &Path,
    replace_only: bool,
    opts: &RepackOptions,
) -> Result<i32> {
    let archives = open_all(grfs, output)?;
    let refs: Vec<&Archive> = archives.iter().collect();
    let meter = Meter::new(cli);
    let cb = meter.callback();
    let ctl = Control {
        cancel: None,
        progress: Some(&cb),
    };
    let started = Instant::now();
    let r = rgrf_core::merge::merge_with(&refs, output, opts, replace_only, &ctl);
    meter.done();
    let r = r?;
    let secs = started.elapsed().as_secs_f64();
    let warn = !r.repack.damaged.is_empty() || r.repack.encrypted_blocks > 0;
    if cli.json {
        let mut v = write_json(&r.repack, output, secs);
        v["schema_version"] = SCHEMA_VERSION.into();
        v["sources"] = grfs
            .iter()
            .zip(&r.sources)
            .map(|(p, s)| {
                json!({
                    "path": p.display().to_string(), "files": s.files, "used": s.used,
                    "overridden": s.overridden, "shadowed": s.shadowed, "ignored_new": s.ignored_new,
                    "markers_skipped": s.markers_skipped,
                })
            })
            .collect::<Vec<_>>()
            .into();
        print_json(&v)?;
    } else {
        write_summary(&r.repack, output, secs);
        println!(
            "Inputs (highest priority first): files / used in the result / replaced by a higher-priority input"
        );
        for (p, s) in grfs.iter().zip(&r.sources) {
            println!(
                "  {:>9} {:>9} {:>9}  {}{}",
                s.files,
                s.used,
                s.overridden,
                p.display(),
                {
                    let mut notes = Vec::new();
                    if s.markers_skipped > 0 {
                        notes.push(format!("{} empty markers not copied", s.markers_skipped));
                    }
                    if s.ignored_new > 0 {
                        notes.push(format!(
                            "{} new paths ignored (--replace-only)",
                            s.ignored_new
                        ));
                    }
                    if notes.is_empty() {
                        String::new()
                    } else {
                        format!("  ({})", notes.join(", "))
                    }
                }
            );
        }
    }
    Ok(if warn { 2 } else { 0 })
}

fn patch_cmd(
    cli: &Cli,
    old: &Path,
    new: &Path,
    output: &Path,
    opts: &RepackOptions,
) -> Result<i32> {
    let archives = open_all(&[old.to_path_buf(), new.to_path_buf()], output)?;
    let meter = Meter::new(cli);
    let cb = meter.callback();
    let ctl = Control {
        cancel: None,
        progress: Some(&cb),
    };
    let started = Instant::now();
    let r = rgrf_core::merge::patch(&archives[0], &archives[1], output, opts, &ctl);
    meter.done();
    let r = r?;
    let secs = started.elapsed().as_secs_f64();
    if cli.json {
        let mut v = write_json(&r.repack, output, secs);
        v["schema_version"] = SCHEMA_VERSION.into();
        v["added"] = r.added.into();
        v["changed"] = r.changed.into();
        v["removed"] = r.removed.into();
        print_json(&v)?;
    } else {
        write_summary(&r.repack, output, secs);
        println!(
            "Patch:    {} new, {} changed; {} removed in NEW (a GRF cannot delete, they stay visible from OLD)",
            r.added, r.changed, r.removed
        );
        println!("Use:      list the patch before OLD in DATA.INI");
    }
    Ok(0)
}

fn method_from(method: MethodArg, level: Option<u32>) -> Result<Method> {
    let m = match method {
        MethodArg::Zlib => Method::Zlib {
            level: level.unwrap_or(9),
        },
        MethodArg::Zopfli => Method::Zopfli {
            iterations: u64::from(level.unwrap_or(10)),
        },
        MethodArg::Lzma => Method::Lzma {
            preset: level.unwrap_or(9),
        },
    };
    if let Method::Zlib { level } | Method::Lzma { preset: level } = m
        && level > 9
    {
        bail!("--level must be 0-9 for zlib and lzma");
    }
    Ok(m)
}

fn build_cmd(cli: &Cli, folder: &Path, output: &Path, opts: &BuildOptions) -> Result<i32> {
    let meter = Meter::new(cli);
    let cb = meter.callback();
    let ctl = Control {
        cancel: None,
        progress: Some(&cb),
    };
    let started = Instant::now();
    let r = build(folder, output, opts, &ctl);
    meter.done();
    let r = r?;
    let secs = started.elapsed().as_secs_f64();
    if cli.json {
        let skipped: Vec<Value> = r
            .skipped
            .iter()
            .map(|s| json!({"path": s.path.display().to_string(), "reason": s.reason}))
            .collect();
        print_json(&json!({
            "schema_version": SCHEMA_VERSION,
            "dry_run": r.dry_run, "output": output.display().to_string(),
            "format": format!("0x{:03x}", r.format),
            "files": r.files, "blocks": r.blocks, "shared_files": r.shared_files,
            "unpacked_bytes": r.unpacked_bytes, "output_stream_bytes": r.output_stream_bytes,
            "output_file_bytes": r.output_file_len, "verified": r.verified,
            "skipped": skipped, "seconds": secs,
        }))?;
    } else {
        println!(
            "{} {} -> {} (0x{:03x}) in {:.1}s",
            if r.dry_run { "Plan for" } else { "Built" },
            folder.display(),
            output.display(),
            r.format,
            secs
        );
        println!(
            "Files:    {} ({} bytes), data blocks {}, files sharing a block {}",
            r.files, r.unpacked_bytes, r.blocks, r.shared_files
        );
        if !r.dry_run {
            println!(
                "Size:     streams {} bytes, file {} bytes ({:.1}% of the unpacked size)",
                r.output_stream_bytes,
                r.output_file_len,
                r.output_file_len as f64 * 100.0 / r.unpacked_bytes.max(1) as f64
            );
            println!(
                "Checked:  {}",
                if r.verified {
                    "every file decoded and compared with the folder"
                } else {
                    "no (skipped)"
                }
            );
        }
        for s in r.skipped.iter().take(10) {
            eprintln!("skipped: {} ({})", s.path.display(), s.reason);
        }
        if r.skipped.len() > 10 {
            eprintln!("skipped: ... and {} more", r.skipped.len() - 10);
        }
    }
    Ok(0)
}
