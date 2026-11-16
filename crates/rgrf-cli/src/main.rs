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
use rgrf_core::extract::{ExtractOptions, OnCollision, extract};
use rgrf_core::hash::Digest;
use rgrf_core::name::hex;
use rgrf_core::progress::{Control, Progress};
use rgrf_core::verify::{Severity, VerifyOptions, verify};
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
    /// CRC32 and MD5 of the decoded content per entry
    Hash {
        grf: PathBuf,
        pattern: Option<String>,
    },
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
    s.push(if e.is_file() { 'F' } else { '-' });
    s.push(if e.flags & 0x02 != 0 { 'M' } else { '-' });
    s.push(if e.flags & 0x04 != 0 { 'H' } else { '-' });
    s
}

fn entry_json(i: usize, e: &Entry, mode: NameMode) -> Value {
    let kind = if e.is_marker() {
        "marker"
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
    }
}

fn info(cli: &Cli, grf: &Path, codecs: bool) -> Result<i32> {
    let (a, open_ms) = open(grf)?;
    let es = a.entries();
    let markers = es.iter().filter(|e| e.is_marker()).count();
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
            "entries": es.len(), "files": es.len() - markers, "markers": markers,
            "encrypted": encrypted, "shared_data_entries": shared,
            "packed_bytes": packed, "unpacked_bytes": unpacked,
            "table": {"offset": t.abs_offset, "packed": t.zsize, "unpacked": t.size},
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
            "Entries:    {} ({} files, {} markers)",
            es.len(),
            es.len() - markers,
            markers
        );
        println!("Content:    {packed} bytes packed, {unpacked} bytes unpacked (sum over entries)");
        println!(
            "Table:      at {}, {} bytes packed, {} unpacked",
            t.abs_offset, t.zsize, t.size
        );
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
            "entries": r.entries, "files": r.files, "markers": r.markers, "decoded": r.decoded,
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
