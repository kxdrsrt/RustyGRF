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
        /// 0x200 (32-bit offsets, max 4 GiB) or 0x300 (default: same as the source)
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
    /// CRC32 and MD5 of the decoded content per entry
    Hash {
        grf: PathBuf,
        pattern: Option<String>,
    },
}

fn parse_format(s: &str) -> Result<Format, String> {
    Format::parse(s).ok_or_else(|| format!("unknown format '{s}' (0x200 or 0x300)"))
}

#[derive(Clone, Copy, ValueEnum)]
enum MethodArg {
    Zlib,
    Zopfli,
    Lzma,
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
    // F compressed file, R file stored uncompressed (flags 0 with data)
    s.push(if e.is_file() {
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
    let kind = if e.is_marker() {
        "marker"
    } else if e.is_raw() {
        "raw"
    } else if e.is_encrypted() {
        "encrypted"
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
            let method = match method {
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
            if let Method::Zlib { level } | Method::Lzma { preset: level } = method
                && level > 9
            {
                bail!("--level must be 0-9 for zlib and lzma");
            }
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
    let raw = es.iter().filter(|e| e.is_raw()).count();
    let encrypted = es.iter().filter(|e| e.is_encrypted()).count();
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
            "entries": es.len(), "files": es.len() - markers, "markers": markers, "raw": raw,
            "encrypted": encrypted, "shared_data_entries": shared,
            "packed_bytes": packed, "unpacked_bytes": unpacked,
            "table": {"offset": t.abs_offset, "packed": t.zsize, "unpacked": t.size, "key_block": t.key_block},
            "open_ms": open_ms,
        });
        if let Some((z, l)) = counts {
            v["codecs"] = json!({"zlib": z, "lzma": l});
        }
        print_json(&v)?;
    } else {
        println!("File:       {} ({} bytes)", grf.display(), a.file_len());
        println!("Version:    0x{:03x}", a.header().version);
        println!(
            "Entries:    {} ({} files, {} empty markers; {} files stored uncompressed)",
            es.len(),
            es.len() - markers,
            markers,
            raw
        );
        println!("Content:    {packed} bytes packed, {unpacked} bytes unpacked (sum over entries)");
        println!(
            "Table:      at {}, {} bytes packed, {} unpacked",
            t.abs_offset, t.zsize, t.size
        );
        if t.key_block > 0 {
            println!("Key block:  {} bytes (protected archive)", t.key_block);
        }
        println!("Encrypted:  {encrypted}");
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
