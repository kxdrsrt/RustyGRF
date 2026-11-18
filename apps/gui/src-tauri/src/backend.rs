//! Everything the app does with an archive, without any Tauri types, so it can be tested.
//! `lib.rs` only wraps these functions as commands.

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use rgrf_core::build::{BuildOptions, build};
use rgrf_core::encode::Method;
use rgrf_core::extract::{ExtractOptions, OnCollision, extract};
use rgrf_core::merge::merge_with;
use rgrf_core::progress::Control;
use rgrf_core::repack::RepackOptions;
use rgrf_core::verify::{Severity, VerifyOptions, verify};
use rgrf_core::writer::Format;
use rgrf_core::{Archive, DecodeStatus, GrfPath, NameMode};
use rgrf_formats::act::Act;
use rgrf_formats::gat::{self, Gat};
use rgrf_formats::gif_io::{delay_cs, encode_gif, upscale};
use rgrf_formats::gnd::GndSummary;
use rgrf_formats::png_io::rgba_to_png;
use rgrf_formats::render::{bounds_of, render_frame};
use rgrf_formats::rsm::Rsm;
use rgrf_formats::rsw::{self, Rsw};
use rgrf_formats::spr::{RgbaImage, Spr};
use serde::{Deserialize, Serialize};

/// Largest entry the preview reads (bigger ones show only their details).
const MAX_PREVIEW_BYTES: u32 = 48 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 200_000;
const MAX_SPR_FRAMES: usize = 96;

pub struct Session {
    pub path: PathBuf,
    arc: Archive,
    mode: NameMode,
    names: Vec<String>,
    filtered: Vec<usize>,
}

#[derive(Serialize, Debug)]
pub struct ArchiveInfo {
    pub path: String,
    pub file_name: String,
    pub format: String,
    pub entries: usize,
    pub files: usize,
    pub file_size: u64,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct Row {
    pub index: usize,
    pub name: String,
    pub size: u32,
    pub kind: &'static str,
}

#[derive(Serialize, Debug)]
pub struct Details {
    pub name: String,
    pub size: u32,
    pub stored: u32,
    pub offset: u64,
    pub codec: &'static str,
    pub flags: u8,
}

#[derive(Serialize, Debug)]
pub struct Frame {
    pub png: String,
    pub width: u32,
    pub height: u32,
    pub label: String,
}

#[derive(Serialize, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Preview {
    Image {
        mime: String,
        data: String,
    },
    Audio {
        mime: String,
        data: String,
    },
    Text {
        text: String,
        encoding: String,
        truncated: bool,
    },
    Frames {
        frames: Vec<Frame>,
        info: Vec<String>,
    },
    Animation {
        gif: String,
        width: u32,
        height: u32,
        action: usize,
        actions: usize,
        frames: usize,
        interval_ms: u32,
        info: Vec<String>,
        note: Option<String>,
    },
    Map {
        png: String,
        width: u32,
        height: u32,
        info: Vec<String>,
    },
    Info {
        title: String,
        lines: Vec<String>,
    },
    Hex {
        dump: String,
        size: u32,
    },
    Message {
        text: String,
    },
}

#[derive(Serialize, Debug)]
pub struct ExtractSummary {
    pub files: usize,
    pub damaged: usize,
    pub dest: String,
}

#[derive(Deserialize, Debug)]
pub struct BuildRequest {
    pub src: PathBuf,
    pub dst: PathBuf,
    /// `zlib` (default, every client), `zopfli` or `lzma` (needs a client with LZMA support).
    pub method: String,
    /// `0x200` (every client) or `0x300`.
    pub format: String,
    pub dedupe: bool,
    pub overwrite: bool,
}

#[derive(Serialize, Debug)]
pub struct BuildSummary {
    pub files: usize,
    pub blocks: usize,
    pub shared: usize,
    pub unpacked_bytes: u64,
    pub file_bytes: u64,
    pub verified: bool,
    pub skipped: Vec<String>,
}

/// Packs a folder into a GRF and verifies the result against the files on disk.
pub fn build_archive(req: &BuildRequest, ctl: &Control) -> Result<BuildSummary, String> {
    let method = match req.method.as_str() {
        "zlib" => Method::DEFAULT,
        "zopfli" => Method::Zopfli { iterations: 10 },
        "lzma" => Method::Lzma { preset: 9 },
        other => return Err(format!("unknown method '{other}'")),
    };
    let format = Format::parse(&req.format)
        .ok_or_else(|| format!("unknown format '{}' (0x200 or 0x300)", req.format))?;
    let opts = BuildOptions {
        method,
        format,
        dedupe: req.dedupe,
        overwrite: req.overwrite,
        ..BuildOptions::default()
    };
    let rep = build(&req.src, &req.dst, &opts, ctl).map_err(|e| e.to_string())?;
    Ok(BuildSummary {
        files: rep.files,
        blocks: rep.blocks,
        shared: rep.shared_files,
        unpacked_bytes: rep.unpacked_bytes,
        file_bytes: rep.output_file_len,
        verified: rep.verified,
        skipped: rep
            .skipped
            .iter()
            .map(|s| format!("{}: {}", s.path.display(), s.reason))
            .collect(),
    })
}

#[derive(Deserialize, Debug)]
pub struct MergeRequest {
    /// Highest priority first: for every path the first archive that has it wins.
    pub sources: Vec<PathBuf>,
    pub dst: PathBuf,
    /// The last archive is the base; the others only replace files it already has.
    pub replace_only: bool,
    pub overwrite: bool,
}

#[derive(Serialize, Debug)]
pub struct MergeSourceSummary {
    pub path: String,
    pub files: usize,
    pub used: usize,
    pub overridden: usize,
    pub ignored_new: usize,
}

#[derive(Serialize, Debug)]
pub struct MergeSummary {
    pub files: usize,
    pub file_bytes: u64,
    pub verified: bool,
    pub damaged: usize,
    pub sources: Vec<MergeSourceSummary>,
}

/// Merges archives into a new one (the inputs are never changed) and verifies the result.
pub fn merge_archives(req: &MergeRequest, ctl: &Control) -> Result<MergeSummary, String> {
    if req.sources.len() < 2 {
        return Err("choose at least two archives".into());
    }
    if req.sources.iter().any(|p| same_file(p, &req.dst)) {
        return Err("the target must not be one of the input archives".into());
    }
    let archives = req
        .sources
        .iter()
        .map(|p| Archive::open(p).map_err(|e| format!("{}: {e}", p.display())))
        .collect::<Result<Vec<_>, _>>()?;
    let refs: Vec<&Archive> = archives.iter().collect();
    let opts = RepackOptions {
        overwrite: req.overwrite,
        ..RepackOptions::default()
    };
    let rep =
        merge_with(&refs, &req.dst, &opts, req.replace_only, ctl).map_err(|e| e.to_string())?;
    Ok(MergeSummary {
        files: rep.repack.files,
        file_bytes: rep.repack.output_file_len,
        verified: rep.repack.verified,
        damaged: rep.repack.damaged.len(),
        sources: req
            .sources
            .iter()
            .zip(&rep.sources)
            .map(|(p, s)| MergeSourceSummary {
                path: p.display().to_string(),
                files: s.files,
                used: s.used,
                overridden: s.overridden,
                ignored_new: s.ignored_new,
            })
            .collect(),
    })
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

#[derive(Serialize, Debug)]
pub struct VerifySummary {
    pub entries: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub issues: Vec<String>,
    pub decoded: bool,
}

fn kind_of(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "bmp" | "jpg" | "jpeg" | "png" | "gif" | "tga" => "image",
        "spr" => "sprite",
        "act" => "animation",
        "pal" => "palette",
        "gat" | "gnd" | "rsw" => "map",
        "rsm" | "rsm2" => "model",
        "wav" | "mp3" | "ogg" => "audio",
        "txt" | "xml" | "lua" | "lub" | "ini" | "csv" | "json" | "log" => "text",
        "str" => "effect",
        "imf" => "imf",
        _ => "file",
    }
}

fn ext(name: &str) -> String {
    name.rsplit('.').next().unwrap_or("").to_ascii_lowercase()
}

fn b64(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

fn png_frame(img: &RgbaImage, label: String) -> Option<Frame> {
    Some(Frame {
        png: b64(&rgba_to_png(img).ok()?),
        width: u32::from(img.width),
        height: u32::from(img.height),
        label,
    })
}

/// Text of a file whose encoding is unknown: UTF-8 if it is, else Korean (CP949) if that decodes
/// without errors, else Windows-1252.
fn decode_text(bytes: &[u8]) -> (String, &'static str) {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return (s.to_owned(), "UTF-8");
    }
    let (s, _, bad) = encoding_rs::EUC_KR.decode(bytes);
    if !bad {
        return (s.into_owned(), "CP949");
    }
    let (s, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
    (s.into_owned(), "Windows-1252")
}

fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (i, chunk) in bytes.chunks(16).take(32).enumerate() {
        out.push_str(&format!("{:08x}  ", i * 16));
        for j in 0..16 {
            match chunk.get(j) {
                Some(b) => out.push_str(&format!("{b:02x} ")),
                None => out.push_str("   "),
            }
        }
        out.push(' ');
        out.extend(chunk.iter().map(|&b| {
            if (0x20..0x7f).contains(&b) {
                char::from(b)
            } else {
                '.'
            }
        }));
        out.push('\n');
    }
    out
}

impl Session {
    pub fn open(path: &Path) -> Result<Self, String> {
        let arc = Archive::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        // Korean names are readable; `set_names` switches to Latin-1 (as GRF Editor shows them) or UTF-8.
        let mode = NameMode::Cp949;
        let names = Self::display_names(&arc, mode);
        let filtered = (0..names.len()).collect();
        Ok(Self {
            path: path.to_path_buf(),
            arc,
            mode,
            names,
            filtered,
        })
    }

    fn display_names(arc: &Archive, mode: NameMode) -> Vec<String> {
        arc.entries().iter().map(|e| e.name.display(mode)).collect()
    }

    /// How names are shown and searched: `cp949`, `cp1252` or `utf8`. Extracting always writes
    /// Latin-1 names, so the result round-trips with `rgrf build`.
    pub fn set_names(&mut self, mode: &str) -> Result<(), String> {
        let m = NameMode::parse(mode).ok_or_else(|| format!("unknown name mode '{mode}'"))?;
        self.mode = m;
        self.names = Self::display_names(&self.arc, m);
        self.filtered = (0..self.names.len()).collect();
        Ok(())
    }

    pub fn info(&self) -> ArchiveInfo {
        ArchiveInfo {
            path: self.path.display().to_string(),
            file_name: self
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            format: format!("0x{:03x}", self.arc.header().version),
            entries: self.arc.len(),
            files: self.arc.entries().iter().filter(|e| e.is_file()).count(),
            file_size: self.arc.file_len(),
        }
    }

    /// Sets the filter (empty = everything; `*` and `?` make a glob, else a substring) and returns
    /// the number of matches.
    pub fn search(&mut self, query: &str) -> usize {
        self.filtered = self.arc.select(Some(query), self.mode);
        self.filtered.len()
    }

    pub fn page(&self, offset: usize, limit: usize) -> Vec<Row> {
        self.filtered
            .iter()
            .skip(offset)
            .take(limit)
            .map(|&i| Row {
                index: i,
                name: self.names[i].clone(),
                size: self.arc.entries()[i].size,
                kind: kind_of(&self.names[i]),
            })
            .collect()
    }

    pub fn details(&self, index: usize) -> Result<Details, String> {
        let e = self.arc.entry(index).map_err(|e| e.to_string())?;
        let codec = match self.arc.sniff_codec(index) {
            Ok(Some(rgrf_core::codec::Codec::Zlib)) => "zlib",
            Ok(Some(rgrf_core::codec::Codec::Lzma)) => "LZMA",
            _ => "none",
        };
        Ok(Details {
            name: self.names[index].clone(),
            size: e.size,
            stored: e.zsize,
            offset: e.abs_offset(),
            codec,
            flags: e.flags,
        })
    }

    fn read(&self, index: usize) -> Result<Vec<u8>, String> {
        let e = self.arc.entry(index).map_err(|e| e.to_string())?;
        if e.size > MAX_PREVIEW_BYTES {
            return Err(format!(
                "{} MB is too large to preview",
                e.size / 1024 / 1024
            ));
        }
        let d = self.arc.read(index).map_err(|e| e.to_string())?;
        if d.status == DecodeStatus::Encrypted {
            return Err("the entry is encrypted (decryption is not supported yet)".into());
        }
        Ok(d.data)
    }

    /// The entry with the same path but another extension (SPR belonging to an ACT).
    fn sibling(&self, index: usize, new_ext: &str) -> Option<usize> {
        let name = &self.arc.entries()[index].name;
        let bytes = name.as_bytes();
        let dot = bytes.iter().rposition(|&b| b == b'.')?;
        let mut b = bytes[..=dot].to_vec();
        b.extend(new_ext.as_bytes());
        self.arc.find(&GrfPath::new(b))
    }

    pub fn preview(&self, index: usize, action: Option<usize>) -> Result<Preview, String> {
        let name = self.names.get(index).ok_or("no such entry")?.clone();
        let entry = self.arc.entry(index).map_err(|e| e.to_string())?;
        if !entry.is_file() {
            return Ok(Preview::Message {
                text: "Empty marker entry".into(),
            });
        }
        let data = match self.read(index) {
            Ok(d) => d,
            Err(e) => return Ok(Preview::Message { text: e }),
        };
        let kind = ext(&name);
        Ok(match kind.as_str() {
            "bmp" | "jpg" | "jpeg" | "png" | "gif" => {
                let mime = match kind.as_str() {
                    "bmp" => "image/bmp",
                    "png" => "image/png",
                    "gif" => "image/gif",
                    _ => "image/jpeg",
                };
                Preview::Image {
                    mime: mime.into(),
                    data: b64(&data),
                }
            }
            "wav" => Preview::Audio {
                mime: "audio/wav".into(),
                data: b64(&data),
            },
            "mp3" => Preview::Audio {
                mime: "audio/mpeg".into(),
                data: b64(&data),
            },
            "spr" => self.preview_spr(&data)?,
            "act" => self.preview_act(index, &data, action.unwrap_or(0))?,
            "pal" => {
                let p = rgrf_formats::pal::Pal::parse(&data).map_err(|e| e.to_string())?;
                // 16 x 16 swatches of 12 pixels
                let mut px = Vec::new();
                for y in 0..192usize {
                    for x in 0..192usize {
                        px.extend(p.rgba(((y / 12) * 16 + x / 12) as u8));
                    }
                }
                let img = RgbaImage {
                    width: 192,
                    height: 192,
                    pixels: px,
                };
                Preview::Frames {
                    frames: png_frame(&img, "256 colours (index 0 is transparent)".into())
                        .into_iter()
                        .collect(),
                    info: vec!["Palette: 256 colours".into()],
                }
            }
            "gat" => {
                let g = Gat::parse(&data).map_err(|e| e.to_string())?;
                let (lo, hi) = g.height_range();
                let mut info = vec![format!(
                    "GAT {} - {} x {} cells, heights {lo:.1} to {hi:.1}",
                    g.version, g.width, g.height
                )];
                for (k, n) in g.type_counts() {
                    info.push(format!("type {k} ({}): {n} cells", gat::type_name(k)));
                }
                let img = g.to_image().ok_or("map too large")?;
                let s = if img.width < 200 {
                    4
                } else if img.width < 420 {
                    2
                } else {
                    1
                };
                let (w, h) = (usize::from(img.width) * s, usize::from(img.height) * s);
                let px = if s == 1 {
                    img.pixels
                } else {
                    upscale(
                        &img.pixels,
                        usize::from(img.width),
                        usize::from(img.height),
                        s,
                    )
                };
                let big = RgbaImage {
                    width: u16::try_from(w).map_err(|e| e.to_string())?,
                    height: u16::try_from(h).map_err(|e| e.to_string())?,
                    pixels: px,
                };
                Preview::Map {
                    png: b64(&rgba_to_png(&big).map_err(|e| e.to_string())?),
                    width: w as u32,
                    height: h as u32,
                    info,
                }
            }
            "gnd" => {
                let g = GndSummary::parse(&data).map_err(|e| e.to_string())?;
                let mut lines = vec![format!(
                    "GND {} - {} x {} ground cells, {} textures",
                    g.version,
                    g.width,
                    g.height,
                    g.textures.len()
                )];
                lines.extend(g.texture_names());
                Preview::Info {
                    title: "Ground".into(),
                    lines,
                }
            }
            "rsw" => {
                let r = Rsw::parse(&data).map_err(|e| e.to_string())?;
                let (m, l, s, e) = r.object_counts();
                let mut lines = vec![
                    format!(
                        "RSW {} - ground {}, altitude {}",
                        r.version,
                        rsw::text(&r.ground),
                        rsw::text(&r.altitude)
                    ),
                    format!(
                        "{m} models ({} files), {l} lights, {s} sounds ({} files), {e} effects",
                        r.model_files().len(),
                        r.sound_files().len()
                    ),
                ];
                lines.push("Models:".into());
                lines.extend(r.model_files());
                if !r.sound_files().is_empty() {
                    lines.push("Sounds:".into());
                    lines.extend(r.sound_files());
                }
                Preview::Info {
                    title: "World".into(),
                    lines,
                }
            }
            "rsm" | "rsm2" => {
                let m = Rsm::parse(&data).map_err(|e| e.to_string())?;
                let mut lines = vec![format!("RSM {} - {} meshes", m.version, m.meshes.len())];
                for x in &m.meshes {
                    lines.push(format!(
                        "mesh {} ({} vertices, {} faces)",
                        x.name.iter().map(|&b| char::from(b)).collect::<String>(),
                        x.vertices,
                        x.faces
                    ));
                }
                lines.push("Textures:".into());
                lines.extend(
                    m.all_textures()
                        .iter()
                        .map(|t| t.iter().map(|&b| char::from(b)).collect::<String>()),
                );
                Preview::Info {
                    title: "Model".into(),
                    lines,
                }
            }
            "txt" | "xml" | "lua" | "lub" | "ini" | "csv" | "json" | "log" => {
                if data.starts_with(b"\x1bLua") {
                    Preview::Message {
                        text: "Compiled Lua (bytecode), no text to show".into(),
                    }
                } else {
                    let (mut text, encoding) = decode_text(&data);
                    let truncated = text.chars().count() > MAX_TEXT_CHARS;
                    if truncated {
                        text = text.chars().take(MAX_TEXT_CHARS).collect();
                    }
                    Preview::Text {
                        text,
                        encoding: encoding.into(),
                        truncated,
                    }
                }
            }
            _ => Preview::Hex {
                dump: hex_dump(&data),
                size: data.len() as u32,
            },
        })
    }

    fn preview_spr(&self, data: &[u8]) -> Result<Preview, String> {
        let spr = Spr::parse(data).map_err(|e| e.to_string())?;
        let frames = spr.frames_rgba();
        let mut info = vec![format!(
            "SPR {} - {} palette images, {} true-colour images",
            spr.version,
            spr.indexed.len(),
            spr.rgba.len()
        )];
        info.extend(spr.warnings.iter().map(|w| format!("warning: {w}")));
        if frames.len() > MAX_SPR_FRAMES {
            info.push(format!(
                "showing the first {MAX_SPR_FRAMES} of {} images",
                frames.len()
            ));
        }
        let out = frames
            .iter()
            .take(MAX_SPR_FRAMES)
            .enumerate()
            .filter_map(|(i, f)| png_frame(f, format!("{i}: {} x {}", f.width, f.height)))
            .collect();
        Ok(Preview::Frames { frames: out, info })
    }

    fn preview_act(&self, index: usize, data: &[u8], action: usize) -> Result<Preview, String> {
        let act = Act::parse(data).map_err(|e| e.to_string())?;
        let mut info = vec![format!(
            "ACT {} - {} actions, {} frames, {} layers",
            act.version,
            act.actions.len(),
            act.actions.iter().map(|a| a.frames.len()).sum::<usize>(),
            act.layers().count()
        )];
        let Some(si) = self.sibling(index, "spr") else {
            return Ok(Preview::Info {
                title: "Animation".into(),
                lines: {
                    info.push(
                        "The matching .spr is not in this archive, so there is nothing to draw."
                            .into(),
                    );
                    info
                },
            });
        };
        let spr = Spr::parse(&self.read(si)?).map_err(|e| e.to_string())?;
        let actions = act.actions.len();
        let action = action.min(actions.saturating_sub(1));
        let Some(a) = act.actions.get(action) else {
            return Ok(Preview::Message {
                text: "The animation has no actions".into(),
            });
        };
        let Some(bounds) =
            bounds_of(&act, &spr, action, 0..a.frames.len()).map_err(|e| e.to_string())?
        else {
            return Ok(Preview::Message {
                text: format!("Action {action} draws nothing"),
            });
        };
        let scale = if bounds.width() <= 80 && bounds.height() <= 80 {
            4
        } else if bounds.width() <= 200 {
            2
        } else {
            1
        };
        let canvases = (0..a.frames.len())
            .map(|f| render_frame(&act, &spr, action, f, bounds).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let delay = delay_cs(a.interval);
        let gif = encode_gif(&canvases, &vec![delay; canvases.len()], scale)
            .map_err(|e| e.to_string())?;
        Ok(Preview::Animation {
            gif: b64(&gif),
            width: bounds.width() * scale,
            height: bounds.height() * scale,
            action,
            actions,
            frames: a.frames.len(),
            interval_ms: (a.interval * 25.0).round() as u32,
            info,
            note: act.warnings.first().cloned(),
        })
    }

    /// With `rename` a file that exists already gets a new name (`name~1.ext`); without it the
    /// extraction stops before writing anything. Existing files are never overwritten.
    pub fn extract(
        &self,
        indices: &[usize],
        dest: &Path,
        rename: bool,
    ) -> Result<ExtractSummary, String> {
        let opts = ExtractOptions {
            names: NameMode::Cp1252,
            on_collision: if rename {
                OnCollision::Rename
            } else {
                OnCollision::Abort
            },
            overwrite: false,
        };
        let ctl = Control {
            cancel: None,
            progress: None,
        };
        let rep = extract(&self.arc, indices, dest, &opts, &ctl).map_err(|e| e.to_string())?;
        Ok(ExtractSummary {
            files: indices.len(),
            damaged: rep.damaged.len(),
            dest: dest.display().to_string(),
        })
    }

    /// Indices of the entries in the current filter.
    pub fn filtered(&self) -> &[usize] {
        &self.filtered
    }

    pub fn verify(&self, decode: bool) -> Result<VerifySummary, String> {
        let ctl = Control {
            cancel: None,
            progress: None,
        };
        let rep = verify(&self.arc, VerifyOptions { decode }, &ctl).map_err(|e| e.to_string())?;
        let count = |s: Severity| rep.issues.iter().filter(|i| i.severity == s).count();
        let issues = rep
            .issues
            .iter()
            .filter(|i| i.severity != Severity::Info)
            .take(100)
            .map(|i| match i.entry {
                Some(e) => format!(
                    "{}: {}",
                    self.names.get(e).map(String::as_str).unwrap_or("?"),
                    i.message
                ),
                None => i.message.clone(),
            })
            .collect();
        Ok(VerifySummary {
            entries: rep.entries,
            errors: count(Severity::Error),
            warnings: count(Severity::Warning),
            infos: count(Severity::Info),
            issues,
            decoded: decode,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgrf_core::build::{BuildOptions, build};
    use std::fs;

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("rustygrf-gui-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    /// An archive with text, an image, a sprite with its animation, a map grid and a blob.
    fn sample(tag: &str) -> (PathBuf, PathBuf) {
        use rgrf_formats::act::{Action, Frame as AFrame, Layer, V2_5};
        use rgrf_formats::gat::Cell;
        use rgrf_formats::spr::{IndexedImage, PALETTE_LEN, Version};

        let dir = tmp(tag);
        let src = dir.join("src/data");
        fs::create_dir_all(src.join("sprite")).unwrap();
        fs::write(src.join("readme.txt"), "hello \u{e9}\n").unwrap();
        fs::write(src.join("korean.txt"), [0xb0u8, 0xa1, 0x0a]).unwrap(); // CP949 "가"
        fs::write(src.join("blob.bin"), (0..100u8).collect::<Vec<_>>()).unwrap();
        fs::write(src.join("pic.bmp"), b"BM0000").unwrap();
        let mut pal = Box::new([0u8; PALETTE_LEN]);
        pal[4..8].copy_from_slice(&[255, 0, 0, 0]);
        let spr = Spr {
            version: Version::V2_1,
            indexed: vec![IndexedImage {
                width: 2,
                height: 2,
                pixels: vec![1; 4],
            }],
            rgba: vec![],
            palette: Some(pal),
            warnings: vec![],
        };
        fs::write(
            src.join("sprite/a.spr"),
            spr.to_bytes(Version::V2_1).unwrap(),
        )
        .unwrap();
        let frame = |dx: f32| AFrame {
            layers: vec![Layer {
                sprite_index: 0,
                offset_x: dx,
                ..Layer::default()
            }],
            ..AFrame::default()
        };
        let act = Act {
            version: V2_5,
            reserved: [0; 10],
            actions: vec![
                Action {
                    frames: vec![frame(0.0), frame(3.0)],
                    interval: 4.0,
                },
                Action {
                    frames: vec![frame(1.0)],
                    interval: 4.0,
                },
            ],
            sounds: vec![],
            trailing: vec![],
            warnings: vec![],
        };
        fs::write(src.join("sprite/a.act"), act.to_bytes(V2_5).unwrap()).unwrap();
        let cell = |k| Cell {
            heights: [0.0; 4],
            kind: k,
        };
        let gat = Gat {
            version: Version::new(1, 2),
            width: 2,
            height: 2,
            cells: vec![cell(0), cell(1), cell(3), cell(5)],
        };
        fs::write(src.join("map.gat"), gat.to_bytes()).unwrap();
        let out = dir.join("t.grf");
        build(
            &dir.join("src"),
            &out,
            &BuildOptions::default(),
            &Control {
                cancel: None,
                progress: None,
            },
        )
        .unwrap();
        (dir, out)
    }

    fn find(s: &Session, name: &str) -> usize {
        s.names
            .iter()
            .position(|n| n.to_ascii_lowercase().ends_with(name))
            .unwrap()
    }

    #[test]
    fn open_search_and_page() {
        let (dir, grf) = sample("open");
        let mut s = Session::open(&grf).unwrap();
        let info = s.info();
        assert_eq!(
            (info.format.as_str(), info.entries, info.files),
            ("0x200", 7, 7)
        );
        assert_eq!(info.file_name, "t.grf");
        assert_eq!(s.search(""), 7);
        assert_eq!(s.search("sprite"), 2);
        assert_eq!(s.search("*.txt"), 2);
        let rows = s.page(0, 10);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.kind == "text"));
        assert_eq!(s.page(1, 10).len(), 1);
        assert_eq!(s.page(5, 10).len(), 0);
        let d = s.details(rows[0].index).unwrap();
        assert!(d.name.ends_with(".txt") && d.size > 0 && d.codec == "zlib");
        assert!(s.details(99).is_err());
        assert!(Session::open(&dir.join("missing.grf")).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn previews_by_kind() {
        let (dir, grf) = sample("prev");
        let s = Session::open(&grf).unwrap();
        match s.preview(find(&s, "readme.txt"), None).unwrap() {
            Preview::Text {
                text,
                encoding,
                truncated,
            } => {
                assert_eq!(text, "hello \u{e9}\n");
                assert_eq!(encoding, "UTF-8");
                assert!(!truncated);
            }
            other => panic!("{other:?}"),
        }
        match s.preview(find(&s, "korean.txt"), None).unwrap() {
            Preview::Text { text, encoding, .. } => {
                assert_eq!((text.trim(), encoding.as_str()), ("\u{ac00}", "CP949"))
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            s.preview(find(&s, "blob.bin"), None).unwrap(),
            Preview::Hex { size: 100, .. }
        ));
        match s.preview(find(&s, "pic.bmp"), None).unwrap() {
            Preview::Image { mime, .. } => assert_eq!(mime, "image/bmp"),
            other => panic!("{other:?}"),
        }
        match s.preview(find(&s, "a.spr"), None).unwrap() {
            Preview::Frames { frames, info } => {
                assert_eq!(frames.len(), 1);
                assert_eq!((frames[0].width, frames[0].height), (2, 2));
                assert!(info[0].starts_with("SPR 2.1"));
            }
            other => panic!("{other:?}"),
        }
        match s.preview(find(&s, "map.gat"), None).unwrap() {
            Preview::Map {
                width,
                height,
                info,
                ..
            } => {
                assert_eq!((width, height), (8, 8), "small maps are enlarged");
                assert!(info.iter().any(|l| l.contains("not walkable")));
            }
            other => panic!("{other:?}"),
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn animation_preview_picks_the_action_and_clamps() {
        let (dir, grf) = sample("anim");
        let s = Session::open(&grf).unwrap();
        let act = find(&s, "a.act");
        let get = |a| match s.preview(act, Some(a)).unwrap() {
            Preview::Animation {
                gif,
                action,
                actions,
                frames,
                interval_ms,
                ..
            } => (gif, action, actions, frames, interval_ms),
            other => panic!("{other:?}"),
        };
        let (gif, action, actions, frames, ms) = get(0);
        assert_eq!((action, actions, frames, ms), (0, 2, 2, 100));
        assert!(B64.decode(&gif).unwrap().starts_with(b"GIF89a"));
        assert_eq!(get(1).3, 1);
        assert_eq!(
            get(99).1,
            1,
            "an action past the end is clamped to the last"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn extract_and_verify() {
        let (dir, grf) = sample("ext");
        let mut s = Session::open(&grf).unwrap();
        s.search("*.txt");
        let out = dir.join("out");
        let idx: Vec<usize> = s.filtered().to_vec();
        let r = s.extract(&idx, &out, false).unwrap();
        assert_eq!((r.files, r.damaged), (2, 0));
        assert!(out.join("data/readme.txt").exists());
        // a second run into the same folder refuses to overwrite
        assert!(s.extract(&idx, &out, false).is_err());
        let v = s.verify(true).unwrap();
        assert_eq!((v.entries, v.errors, v.warnings), (7, 0, 0));
        assert!(v.decoded);
        assert!(!s.verify(false).unwrap().decoded);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn merges_archives_by_priority() {
        let dir = tmp("merge");
        let mk = |name: &str, files: &[(&str, &str)]| {
            let root = dir.join(format!("src-{name}"));
            for (f, c) in files {
                let p = root.join(f);
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(p, c).unwrap();
            }
            let out = dir.join(format!("{name}.grf"));
            let none = Control::default();
            let req = BuildRequest {
                src: root,
                dst: out.clone(),
                method: "zlib".into(),
                format: "0x200".into(),
                dedupe: false,
                overwrite: false,
            };
            build_archive(&req, &none).unwrap();
            out
        };
        let base = mk("base", &[("data/a.txt", "old a"), ("data/b.txt", "b")]);
        let patch = mk("patch", &[("data/a.txt", "new a"), ("data/c.txt", "c")]);
        let req = |replace_only: bool, dst: &str, sources: Vec<PathBuf>| MergeRequest {
            sources,
            dst: dir.join(dst),
            replace_only,
            overwrite: false,
        };
        let none = Control::default();
        let r = merge_archives(
            &req(false, "m.grf", vec![patch.clone(), base.clone()]),
            &none,
        )
        .unwrap();
        assert_eq!((r.files, r.verified), (3, true));
        assert_eq!(r.sources[0].used, 2);
        assert_eq!(r.sources[1].overridden, 1);
        let r = merge_archives(
            &req(true, "r.grf", vec![patch.clone(), base.clone()]),
            &none,
        )
        .unwrap();
        assert_eq!((r.files, r.sources[0].ignored_new), (2, 1));
        assert!(merge_archives(&req(false, "x.grf", vec![base.clone()]), &none).is_err());
        assert!(merge_archives(&req(false, "base.grf", vec![patch, base]), &none).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn builds_a_folder_and_can_be_cancelled() {
        use rgrf_core::progress::CancelToken;
        let dir = tmp("build");
        let src = dir.join("src/data/texture");
        fs::create_dir_all(&src).unwrap();
        for i in 0..20 {
            fs::write(
                src.join(format!("f{i}.txt")),
                format!("file {i}").repeat(50),
            )
            .unwrap();
        }
        let req = |dst: &str, method: &str| BuildRequest {
            src: dir.join("src"),
            dst: dir.join(dst),
            method: method.into(),
            format: "0x200".into(),
            dedupe: false,
            overwrite: false,
        };
        let none = Control {
            cancel: None,
            progress: None,
        };
        let rep = build_archive(&req("a.grf", "zlib"), &none).unwrap();
        assert_eq!((rep.files, rep.verified), (20, true));
        assert_eq!(
            Session::open(&dir.join("a.grf")).unwrap().info().entries,
            20
        );
        // an existing archive is not overwritten
        assert!(build_archive(&req("a.grf", "zlib"), &none).is_err());
        assert!(build_archive(&req("b.grf", "bogus"), &none).is_err());

        let token = CancelToken::new();
        token.cancel();
        let ctl = Control {
            cancel: Some(&token),
            progress: None,
        };
        let err = build_archive(&req("c.grf", "zlib"), &ctl).unwrap_err();
        assert!(err.to_lowercase().contains("cancel"), "{err}");
        assert!(!dir.join("c.grf").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn names_can_be_shown_as_korean_or_latin1() {
        let dir = tmp("names");
        // the bytes C0 AF C0 FA are the Korean "유저"; written as Latin-1 characters they build that archive path
        let folder = dir.join("src/data/\u{c0}\u{af}\u{c0}\u{fa}");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("a.txt"), "x").unwrap();
        let out = dir.join("n.grf");
        build(
            &dir.join("src"),
            &out,
            &BuildOptions::default(),
            &Control {
                cancel: None,
                progress: None,
            },
        )
        .unwrap();
        let mut s = Session::open(&out).unwrap();
        assert_eq!(s.page(0, 1)[0].name, "data\\\u{c720}\u{c800}\\a.txt");
        assert_eq!(s.search("\u{c720}\u{c800}"), 1);
        s.set_names("cp1252").unwrap();
        assert_eq!(
            s.page(0, 1)[0].name,
            "data\\\u{c0}\u{af}\u{c0}\u{fa}\\a.txt"
        );
        assert_eq!(s.search("\u{c720}\u{c800}"), 0);
        assert_eq!(s.search("\u{c0}\u{af}\u{c0}\u{fa}"), 1);
        assert!(s.set_names("nonsense").is_err());
        // extracting writes the Latin-1 folder name whatever the display mode
        let idx = s.filtered().to_vec();
        s.extract(&idx, &dir.join("out"), false).unwrap();
        assert!(dir.join("out/data/\u{c0}\u{af}\u{c0}\u{fa}/a.txt").exists());
        let _ = fs::remove_dir_all(dir);
    }
}
