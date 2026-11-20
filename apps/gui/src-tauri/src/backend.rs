//! Everything the app does with an archive, without any Tauri types, so it can be tested.
//! `lib.rs` only wraps these functions as commands.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::act_editor::{self, ActEdit, ActFrameView};
use crate::gat_editor::{self, GatEdit};
use crate::pal_editor::{self, PalEdit};
use crate::spr_editor;
use crate::view::{self, Line, Meta, Tree, ViewSpec};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use rgrf_core::build::{BuildOptions, build};
use rgrf_core::edit::{self, Edit};
use rgrf_core::encode::Method;
use rgrf_core::extract::{ExtractOptions, OnCollision, extract};
use rgrf_core::merge::merge_with;
use rgrf_core::progress::Control;
use rgrf_core::repack::RepackOptions;
use rgrf_core::verify::{Severity, VerifyOptions, verify};
use rgrf_core::writer::Format;
use rgrf_core::{Archive, DecodeStatus, GrfPath, NameMode};
use rgrf_formats::act::Act;
use rgrf_formats::edit::{Kind, quantize};
use rgrf_formats::gat::{self, Gat};
use rgrf_formats::gif_io::{delay_cs, encode_gif, upscale};
use rgrf_formats::gnd_map;
use rgrf_formats::pal::Pal;
use rgrf_formats::png_io::{Frame as PngFrame, frame_from_png, rgba_to_png};
use rgrf_formats::render::{bounds_of, render_frame};
use rgrf_formats::rsm_model;
use rgrf_formats::rsw::{self, Rsw};
use rgrf_formats::spr::{IndexedImage, PALETTE_LEN, RgbaImage, Spr};
use rgrf_formats::str_effect;
use serde::{Deserialize, Serialize};

/// Largest entry the preview reads (bigger ones show only their details).
const MAX_PREVIEW_BYTES: u32 = 48 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 200_000;
const MAX_SPR_FRAMES: usize = 96;
/// Undo steps kept per open archive.
const MAX_UNDO: usize = 200;
/// An effect preview shows at most this many frames, at half the stage size.
const MAX_EFFECT_FRAMES: usize = 120;
const EFFECT_SCALE: f32 = 0.5;

/// Textures of a scene, each loaded once.
/// Number of moments sampled from an animation loop (about 30 per second, 360 at most).
fn sample_count(duration_ms: f32) -> usize {
    if duration_ms > 0.0 {
        ((duration_ms / 33.0).ceil() as usize).clamp(2, 360)
    } else {
        1
    }
}

#[derive(Default)]
struct TexturePool {
    out: Vec<TextureOut>,
    by_key: HashMap<Vec<u8>, usize>,
}

/// The texture-coordinate transforms over the loop for the part of `node` that uses `texture`,
/// when that texture moves; the node's texture number is returned with them.
fn uv_samples(
    model: &rsm_model::Model,
    node: usize,
    texture: &[u8],
    duration: f32,
    samples: usize,
) -> Option<(usize, Vec<f32>)> {
    let ti = model.nodes[node]
        .textures
        .iter()
        .position(|t| t == texture)?;
    if !rsm_model::texture_moves(model, node, ti) {
        return None;
    }
    let v = (0..samples)
        .flat_map(|k| {
            rsm_model::uv_transform_at(model, node, ti, duration * k as f32 / samples as f32)
        })
        .collect();
    Some((ti, v))
}

/// A moving model of a map: the triangles of all nodes in node space, merged by texture with a node
/// number per vertex, and the node matrices over the loop (already turned like the static models:
/// y up, standing on 0, then y negated for the map's instance matrices).
fn animated_def(
    s: &Session,
    model: &rsm_model::Model,
    duration: f32,
    pool: &mut TexturePool,
    keys: &HashMap<Vec<u8>, usize>,
    f32s: &dyn Fn(&[f32]) -> String,
) -> ModelDefOut {
    struct Merged {
        texture: usize,
        positions: Vec<f32>,
        normals: Vec<f32>,
        uvs: Vec<f32>,
        nodes: Vec<f32>,
        /// (node, texture number) and the transforms, for a texture that moves: such parts are
        /// not merged with others
        uv: Option<((usize, usize), Vec<f32>)>,
    }
    let samples = sample_count(duration);
    let mut merged: Vec<Merged> = Vec::new();
    for ni in 0..model.nodes.len() {
        for p in rsm_model::node_parts(model, ni) {
            let texture = pool.get(s, keys, b"data\\texture\\", &p.texture);
            let uv =
                uv_samples(model, ni, &p.texture, duration, samples).map(|(ti, v)| ((ni, ti), v));
            let at = merged
                .iter()
                .position(|m| {
                    m.texture == texture && m.uv.as_ref().map(|u| u.0) == uv.as_ref().map(|u| u.0)
                })
                .unwrap_or_else(|| {
                    merged.push(Merged {
                        texture,
                        positions: Vec::new(),
                        normals: Vec::new(),
                        uvs: Vec::new(),
                        nodes: Vec::new(),
                        uv,
                    });
                    merged.len() - 1
                });
            let m = &mut merged[at];
            m.nodes
                .extend(std::iter::repeat_n(ni as f32, p.positions.len() / 3));
            m.positions.extend(p.positions);
            m.normals.extend(p.normals);
            m.uvs.extend(p.uvs);
        }
    }
    let view = rsm_model::view_matrix(model);
    let flip_y = [
        1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let to_map = rsm_model::multiply(&flip_y, &view);
    let mut matrices: Vec<f32> = Vec::with_capacity(model.nodes.len() * samples * 16);
    let worlds: Vec<Vec<[f32; 16]>> = (0..samples)
        .map(|k| rsm_model::node_matrices_at(model, duration * k as f32 / samples as f32))
        .collect();
    for ni in 0..model.nodes.len() {
        for w in &worlds {
            matrices.extend(rsm_model::multiply(&to_map, &w[ni]));
        }
    }
    ModelDefOut {
        parts: merged
            .iter()
            .map(|m| ModelPartOut {
                texture: m.texture,
                positions: f32s(&m.positions),
                normals: f32s(&m.normals),
                uvs: f32s(&m.uvs),
                nodes: Some(f32s(&m.nodes)),
                uv_anim: m.uv.as_ref().map(|u| f32s(&u.1)),
            })
            .collect(),
        anim: Some(ModelAnimOut {
            duration_ms: duration,
            samples,
            node_count: model.nodes.len(),
            matrices: f32s(&matrices),
        }),
    }
}

impl TexturePool {
    fn get(
        &mut self,
        s: &Session,
        keys: &HashMap<Vec<u8>, usize>,
        base: &[u8],
        name: &[u8],
    ) -> usize {
        let full = [base, name].concat();
        let key = GrfPath::new(full).key();
        if let Some(&i) = self.by_key.get(&key) {
            return i;
        }
        let png = keys
            .get(&key)
            .and_then(|&ti| s.texture_image(ti))
            .and_then(|img| rgba_to_png(&img).ok())
            .map(|b| b64(&b));
        self.out.push(TextureOut {
            name: s.shown_bytes(name),
            png,
        });
        self.by_key.insert(key, self.out.len() - 1);
        self.out.len() - 1
    }
}

pub struct Session {
    pub path: PathBuf,
    arc: Archive,
    mode: NameMode,
    names: Vec<String>,
    /// The entries matching the main list (any layout), for "all in the list".
    filtered: Vec<usize>,
    /// The main list (0) and the folder pane (1).
    views: [ViewState; 2],
    pending: Vec<Change>,
    /// Earlier states of `pending` (newest last) for undo, and states undone for redo.
    undo: Vec<Vec<Change>>,
    redo: Vec<Vec<Change>>,
    keys: OnceLock<HashMap<Vec<u8>, usize>>,
    /// The key of GRF Editor's encryption, if the archive was opened with one.
    crypt_key: Option<rgrf_core::grfcrypt::Key>,
    /// Folder for drafts of pending changes (see [`crate::draft`]); none: no drafts.
    draft_root: Option<PathBuf>,
    /// A draft was found when the archive was opened and is not restored or discarded yet; it is
    /// not overwritten until then.
    draft_offered: bool,
}

/// One arranged list: its settings, and either the flat order or a folder tree with its open folders.
#[derive(Default)]
struct ViewState {
    spec: ViewSpec,
    flat: Vec<usize>,
    tree: Option<Tree>,
    lines: Vec<Line>,
    expanded: std::collections::HashSet<String>,
}

#[derive(Serialize, Debug)]
pub struct GatMeta {
    pub cells_x: u32,
    pub cells_y: u32,
    pub scale: u32,
    pub types: Vec<(i32, String)>,
}

/// One thumbnail for the gallery; `data` is missing when the entry has none (see `error`).
#[derive(Serialize, Debug)]
pub struct ThumbOut {
    pub index: usize,
    pub mime: String,
    pub data: Option<String>,
    /// Size of the original picture (0 for pictures passed on unchanged).
    pub width: u32,
    pub height: u32,
    pub error: Option<String>,
}

#[derive(Serialize, Debug, PartialEq, Eq, Default)]
pub struct Located {
    pub line: Option<usize>,
    pub rows: usize,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct ViewSummary {
    /// Lines the list shows (folders count in a tree).
    pub rows: usize,
    /// Entries that match.
    pub matches: usize,
}

/// A change waiting to be saved into a new archive (the open archive is never modified).
#[derive(Debug, Clone)]
enum Change {
    /// The file's content becomes the content of `name` (a replacement or a new path).
    Put {
        name: GrfPath,
        file: PathBuf,
    },
    /// Edited content of a sprite or animation, with what was changed in it.
    PutBytes {
        name: GrfPath,
        data: Arc<Vec<u8>>,
        what: Vec<String>,
    },
    Remove {
        name: GrfPath,
    },
}

impl Change {
    fn name(&self) -> &GrfPath {
        match self {
            Self::Put { name, .. } | Self::PutBytes { name, .. } | Self::Remove { name } => name,
        }
    }
}

/// A text or Lua entry as text for the editor.
#[derive(Serialize, Debug)]
pub struct TextForEdit {
    pub text: String,
    /// "Lua 5.1 bytecode", "text (UTF-8)", ...
    pub format: String,
    /// False when the decompiler left functions as a listing (they cannot be compiled again).
    pub complete: bool,
}

#[derive(Serialize, Debug)]
pub struct PendingRow {
    /// `replace`, `add` or `remove`
    pub kind: &'static str,
    pub name: String,
    pub file: Option<String>,
}

/// What a save needs, without holding the open session.
pub struct EditPlan {
    src: PathBuf,
    changes: Vec<Change>,
    /// The key of an encrypted source; the result is encrypted again with it.
    key: Option<rgrf_core::grfcrypt::Key>,
}

impl EditPlan {
    fn open(&self) -> Result<Archive, String> {
        match &self.key {
            Some(k) => Archive::open_with_key(&self.src, k),
            None => Archive::open(&self.src),
        }
        .map_err(|e| format!("{}: {e}", self.src.display()))
    }

    fn edits(&self) -> Result<Vec<Edit>, String> {
        self.changes
            .iter()
            .map(|c| {
                Ok(match c {
                    Change::Put { name, file } => Edit::Put {
                        name: name.clone(),
                        data: std::fs::read(file)
                            .map_err(|e| format!("{}: {e}", file.display()))?,
                    },
                    Change::PutBytes { name, data, .. } => Edit::Put {
                        name: name.clone(),
                        data: (**data).clone(),
                    },
                    Change::Remove { name } => Edit::Remove { name: name.clone() },
                })
            })
            .collect()
    }
}

#[derive(Serialize, Debug)]
pub struct ReduceSummary {
    pub matched: usize,
    pub reduced: usize,
    /// Reduced without any colour change (at most 256 colours before).
    pub lossless: usize,
    /// Left alone, by reason.
    pub kept: std::collections::BTreeMap<String, usize>,
    pub stored_before: u64,
    pub stored_after: u64,
    pub worst_psnr: Option<f64>,
    /// Whether an archive was written (not when nothing was reduced and nothing is pending).
    pub written: bool,
    pub files: usize,
    pub file_bytes: u64,
    pub verified: bool,
}

/// Writes `dst` as the open archive with its pending changes and every other true-colour BMP
/// stored as 8 bits where the picture keeps `min_psnr` dB and the stored file gets smaller
/// (`rgrf image reduce` does the same). The open archive is not touched.
pub fn reduce_images(
    plan: &EditPlan,
    dst: &Path,
    min_psnr: f64,
    ctl: &Control,
) -> Result<ReduceSummary, String> {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    if same_file(&plan.src, dst) {
        return Err(err!("err-target-open"));
    }
    let src = plan.open()?;
    let touched: std::collections::HashSet<Vec<u8>> =
        plan.changes.iter().map(|c| c.name().key()).collect();
    let idx: Vec<usize> = src
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            e.is_file()
                && !touched.contains(&e.name.key())
                && e.name.as_bytes().to_ascii_lowercase().ends_with(b".bmp")
        })
        .map(|(i, _)| i)
        .collect();
    let total = idx.len() as u64;
    let done = AtomicU64::new(0);
    let results: Vec<Result<(GrfPath, rgrf_formats::bmp::Stored), &'static str>> = idx
        .par_iter()
        .map(|&i| {
            ctl.check().map_err(|_| "cancelled")?;
            ctl.report(done.fetch_add(1, Ordering::Relaxed) + 1, total);
            let e = &src.entries()[i];
            let d = src.read(i).map_err(|_| "unreadable")?;
            let r =
                rgrf_formats::bmp::reduce_for_storage(&d.data, min_psnr, u64::from(e.zsize), |b| {
                    rgrf_core::encode::encode(Method::DEFAULT, b).len() as u64
                });
            r.map(|s| (e.name.clone(), s)).map_err(|k| k.reason())
        })
        .collect();
    ctl.check().map_err(|e| e.to_string())?;
    let mut sum = ReduceSummary {
        matched: idx.len(),
        reduced: 0,
        lossless: 0,
        kept: Default::default(),
        stored_before: 0,
        stored_after: 0,
        worst_psnr: None,
        written: false,
        files: 0,
        file_bytes: 0,
        verified: false,
    };
    let mut edits = plan.edits()?;
    for r in results {
        match r {
            Ok((name, st)) => {
                sum.reduced += 1;
                sum.lossless += usize::from(st.psnr.is_infinite());
                sum.stored_before += st.stored_before;
                sum.stored_after += st.stored_after;
                if st.psnr.is_finite() {
                    sum.worst_psnr = Some(sum.worst_psnr.map_or(st.psnr, |w: f64| w.min(st.psnr)));
                }
                edits.push(Edit::Put {
                    name,
                    data: st.bytes,
                });
            }
            Err(why) => *sum.kept.entry(why.to_owned()).or_default() += 1,
        }
    }
    if edits.is_empty() {
        return Ok(sum);
    }
    let opts = RepackOptions {
        overwrite: true,
        copy_streams: true,
        encrypt: plan.key,
        ..RepackOptions::default()
    };
    let r = edit::apply(&src, &plan.src, dst, &edits, &opts, ctl).map_err(|e| e.to_string())?;
    sum.written = true;
    sum.files = r.merge.repack.files;
    sum.file_bytes = r.merge.repack.output_file_len;
    sum.verified = r.merge.repack.verified;
    Ok(sum)
}

/// An animation of the archive shown with the one being edited (see `act_editor::Reference`).
#[derive(Deserialize, Debug, Clone)]
pub struct ReferenceSpec {
    pub path: String,
    #[serde(default)]
    pub under: bool,
}

/// Starts the program the system has for the file (not waiting for it).
pub fn open_with_system(path: &Path) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg(path);
        c
    } else if cfg!(target_os = "windows") {
        // `start` takes the first quoted argument as a window title
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]).arg(path);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("no program could be started for {}: {e}", path.display()))
}

/// Most bytes one hex page returns.
const HEX_PAGE_MAX: usize = 4096;

#[derive(Serialize, Debug)]
pub struct HexPage {
    pub offset: u64,
    pub size: u64,
    /// The bytes from `offset`, base64.
    pub bytes: String,
}

/// Width, height, frame count and frames per second from the header of a Bink 1 or 2 file.
fn bink_header(d: &[u8]) -> Option<(u32, u32, u32, f32)> {
    if d.len() < 44 || !(d.starts_with(b"BIK") || d.starts_with(b"KB2")) {
        return None;
    }
    let u = |at: usize| u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]);
    let (frames, width, height, num, den) = (u(8), u(20), u(24), u(28), u(32));
    if width == 0 || height == 0 || den == 0 {
        return None;
    }
    Some((width, height, frames, num as f32 / den as f32))
}

/// A sound named in an animation, as the client finds it: `data\wav\<name>`.
#[derive(Serialize, Debug)]
pub struct SoundOut {
    /// The path in the archive.
    pub path: String,
    /// Playable audio (base64 WAV), or `None` with the reason in `problem`.
    pub data: Option<String>,
    pub problem: Option<String>,
}

/// One BMP reduced to 8 bits, to compare before staging it.
#[derive(Serialize, Debug)]
pub struct ReducePreview {
    /// `None`: no colour changed.
    pub psnr: Option<f64>,
    pub colors: usize,
    pub bytes_before: usize,
    pub bytes_after: usize,
    /// Both pictures as BMP, base64.
    pub before: String,
    pub after: String,
}

fn reduce_one(data: &[u8]) -> Result<(Vec<u8>, f64, usize), String> {
    let info = rgrf_formats::bmp::info(data).map_err(|e| e.to_string())?;
    if info.bits <= 8 {
        return Err(err!("err-already-bits", "bits": info.bits));
    }
    let img = rgrf_formats::bmp::parse(data).map_err(|e| e.to_string())?;
    if img.pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255) {
        return Err(err!("err-alpha"));
    }
    rgrf_formats::bmp::write_reduced(&img, 256).map_err(|e| e.to_string())
}

#[derive(Serialize, Debug)]
pub struct EditSummary {
    pub added: usize,
    pub replaced: usize,
    pub removed: usize,
    pub files: usize,
    pub file_bytes: u64,
    pub verified: bool,
}

#[derive(Serialize, Debug)]
pub struct ArchiveInfo {
    pub path: String,
    pub file_name: String,
    pub format: String,
    pub entries: usize,
    pub files: usize,
    pub file_size: u64,
    /// Set for an archive with GRF Editor's encryption.
    pub encryption: Option<EncryptionOut>,
    /// Unsaved changes of an earlier session, kept as a draft, waiting to be restored or discarded.
    pub draft: Option<crate::draft::DraftInfo>,
}

#[derive(Serialize, Debug)]
pub struct MapExtractSummary {
    pub map: String,
    /// Files written.
    pub files: usize,
    pub damaged: usize,
    /// Files the map names that are not in the archive.
    pub missing: Vec<String>,
    /// Files that exist but could not be read to follow what they name.
    pub problems: Vec<String>,
    pub dest: String,
    /// Files left out on purpose (the parts given in `skip`).
    pub skipped: usize,
}

/// Which files of a map belong to a part that can be left out of an extraction.
fn map_part(kind: rgrf_formats::map_files::Kind) -> Option<&'static str> {
    use rgrf_formats::map_files::Kind;
    Some(match kind {
        Kind::Model | Kind::ModelTexture => "models",
        Kind::Water => "water",
        Kind::Sound => "sounds",
        Kind::Minimap => "minimap",
        Kind::EffectScript | Kind::EffectTexture => "effects",
        Kind::World | Kind::Ground | Kind::Altitude | Kind::GroundTexture => return None,
    })
}

/// Writes a picture (PNG) the page made, e.g. of the 3D view.
pub fn save_picture(path: &Path, png_base64: &str) -> Result<u64, String> {
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
    {
        return Err(err!("err-saved-png"));
    }
    let data = B64
        .decode(png_base64)
        .map_err(|e| format!("not a picture: {e}"))?;
    if !data.starts_with(b"\x89PNG") {
        return Err(err!("err-not-png"));
    }
    std::fs::write(path, &data).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(data.len() as u64)
}

#[derive(Serialize, Debug)]
pub struct DraftRestored {
    pub restored: usize,
    /// Changes left out because their file or stored content is gone.
    pub skipped: usize,
}

#[derive(Serialize, Debug)]
pub struct EncryptionOut {
    pub encrypted_entries: usize,
    /// No (right) key yet: the encrypted entries cannot be read.
    pub key_needed: bool,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct Row {
    /// The entry; `FOLDER_ROW` for a folder line.
    pub index: usize,
    pub name: String,
    pub size: u32,
    pub kind: &'static str,
    pub stored: u32,
    /// Tree layouts: indentation, and for folders the state and totals.
    pub depth: u16,
    pub folder: bool,
    pub expanded: bool,
    pub count: usize,
    pub bytes: u64,
}

/// `Row::index` of a folder line (no entry has this number).
pub const FOLDER_ROW: usize = u32::MAX as usize;

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
    pub label: Note,
}

/// A line of text for the UI. `Key` carries a catalogue key and its values, so the UI can
/// show it in its own language; `Plain` is shown as it is (names, messages from the libraries).
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum Note {
    Plain(String),
    Key {
        key: &'static str,
        args: serde_json::Value,
    },
}

impl From<String> for Note {
    fn from(s: String) -> Self {
        Note::Plain(s)
    }
}

impl From<&str> for Note {
    fn from(s: &str) -> Self {
        Note::Plain(s.into())
    }
}

macro_rules! note {
    ($key:literal) => {
        Note::Key { key: $key, args: serde_json::Value::Null }
    };
    ($key:expr, $($args:tt)+) => {
        Note::Key { key: $key, args: serde_json::json!({ $($args)+ }) }
    };
}

/// The ground of a map as the file stores it, packed: 36 bytes per tile (8 `f32` uv, `u16` texture
/// index into the scene's textures, `u16` lightmap) and 28 bytes per cube (4 `f32` heights, 3 `i32`
/// tile numbers); the page builds the mesh from it.
#[derive(Serialize, Debug)]
pub struct GroundOut {
    pub width: usize,
    pub height: usize,
    pub zoom: f32,
    pub tiles: String,
    pub cubes: String,
    pub lightmap_count: usize,
    pub lightmaps: String,
}

/// One node of a model: its triangles in the node's own space and, per sampled moment, the matrix
/// that takes them to the viewer (4 x 4 column-major `f32`, base64).
#[derive(Serialize, Debug)]
pub struct ModelNodeOut {
    pub parts: Vec<ModelPartOut>,
    pub matrices: String,
}

/// How a model of the map moves: the matrices of every node at `samples` moments of its loop.
#[derive(Serialize, Debug)]
pub struct ModelAnimOut {
    pub duration_ms: f32,
    pub samples: usize,
    pub node_count: usize,
    /// For each node and sample a column-major 4 x 4 (`f32`, base64): node-major, then sample.
    pub matrices: String,
}

#[derive(Serialize, Debug)]
pub struct ModelDefOut {
    pub parts: Vec<ModelPartOut>,
    /// Set for models that move: the parts are then in node space with a node number per vertex.
    pub anim: Option<ModelAnimOut>,
}

#[derive(Serialize, Debug)]
pub struct InstanceOut {
    pub model: usize,
    /// Column-major 4 x 4, already turned to the viewer's axes (y up, z towards the south).
    pub matrix: Vec<f32>,
    /// How fast the model's animation runs (0 = it stands still).
    pub speed: f32,
}

#[derive(Serialize, Debug)]
pub struct LightOut {
    pub longitude: i32,
    pub latitude: i32,
    pub diffuse: [f32; 3],
    pub ambient: [f32; 3],
    pub intensity: f32,
}

#[derive(Serialize, Debug)]
pub struct WaterOut {
    pub level: f32,
    /// The pictures of the water cycle (JPEG, base64), in order; empty when the archive has none.
    pub frames: Vec<String>,
    /// Time one picture stays, in milliseconds.
    pub frame_ms: f32,
    /// The waves of the surface: height (in ground units), speed (degrees per 1/60 s) and
    /// pitch (degrees per tile), from the world file.
    pub wave_height: f32,
    pub wave_speed: f32,
    pub wave_pitch: f32,
}

/// Triangles of one texture; the arrays are base64 of little-endian `f32`.
#[derive(Serialize, Debug)]
pub struct ModelPartOut {
    pub texture: usize,
    pub positions: String,
    pub normals: String,
    pub uvs: String,
    /// The node of every vertex (`f32`, base64), only for parts of moving models in a map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nodes: Option<String>,
    /// For a texture that moves (RSM 2.3): per animation sample six `f32` (`a b c d tx ty`,
    /// base64) with `u' = a u + b v + tx`, `v' = c u + d v + ty`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uv_anim: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct TextureOut {
    pub name: String,
    /// `None` when the archive has no such texture.
    pub png: Option<String>,
}

#[derive(Serialize, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Preview {
    Image {
        mime: String,
        data: String,
        /// Bits per pixel of a BMP (more than 8: it can be reduced to 8).
        #[serde(skip_serializing_if = "Option::is_none")]
        bits: Option<u16>,
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
        info: Vec<Note>,
    },
    Animation {
        gif: String,
        width: u32,
        height: u32,
        action: usize,
        actions: usize,
        frames: usize,
        interval_ms: u32,
        info: Vec<Note>,
        note: Option<String>,
    },
    Scene {
        textures: Vec<TextureOut>,
        ground: Option<GroundOut>,
        models: Vec<ModelDefOut>,
        instances: Vec<InstanceOut>,
        light: LightOut,
        water: Option<WaterOut>,
        info: Vec<Note>,
    },
    Model {
        nodes: Vec<ModelNodeOut>,
        textures: Vec<TextureOut>,
        /// Length of the animation loop in milliseconds, 0 when the model does not move.
        duration_ms: f32,
        /// Number of sampled moments in `matrices`.
        samples: usize,
        min: [f32; 3],
        max: [f32; 3],
        alpha: f32,
        triangles: usize,
        info: Vec<Note>,
    },
    Effect {
        gif: String,
        width: u32,
        height: u32,
        frames: usize,
        fps: u32,
        info: Vec<Note>,
    },
    Map {
        png: String,
        width: u32,
        height: u32,
        info: Vec<Note>,
        /// Set for a GAT: its size in cells, the pixels per cell in the picture and the cell types.
        gat: Option<GatMeta>,
    },
    Info {
        title: String,
        lines: Vec<Note>,
    },
    Hex {
        dump: String,
        size: u32,
    },
    /// A Bink video: the browser cannot play it, so the header is shown and the file can be
    /// opened with the system's player.
    Video {
        width: u32,
        height: u32,
        frames: u32,
        fps: f32,
        size: u32,
    },
    Message {
        text: Note,
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
    let format = Format::parse(&req.format).ok_or_else(|| {
        format!(
            "unknown format '{}' (0x102, 0x103, 0x200 or 0x300)",
            req.format
        )
    })?;
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

enum NewImage {
    Indexed(IndexedImage),
    Rgba(RgbaImage),
}

/// A PNG as an image for `spr`: for palette images its colours are mapped to the sprite's palette
/// (transparency becomes index 0), otherwise it becomes a true-colour image.
fn png_for(spr: &Spr, png: &[u8], indexed: bool) -> Result<NewImage, String> {
    let new = frame_from_png(png).map_err(|e| e.to_string())?;
    let to_rgba = |img: &IndexedImage, pal: &[u8; PALETTE_LEN]| RgbaImage {
        width: img.width,
        height: img.height,
        pixels: img
            .pixels
            .iter()
            .flat_map(|&i| {
                let i = usize::from(i);
                [
                    pal[4 * i],
                    pal[4 * i + 1],
                    pal[4 * i + 2],
                    if i == 0 { 0 } else { 255 },
                ]
            })
            .collect(),
    };
    if !indexed {
        return Ok(NewImage::Rgba(match new {
            PngFrame::Rgba(img) => img,
            PngFrame::Indexed { image, palette } => to_rgba(&image, &palette),
        }));
    }
    let pal = spr.palette.clone().ok_or_else(|| err!("err-no-palette"))?;
    Ok(NewImage::Indexed(match new {
        PngFrame::Indexed { image, palette }
            if (0..256).all(|i| palette[4 * i..4 * i + 3] == pal[4 * i..4 * i + 3]) =>
        {
            image
        }
        PngFrame::Indexed { image, palette } => quantize(&to_rgba(&image, &palette), &pal),
        PngFrame::Rgba(img) => quantize(&img, &pal),
    }))
}

/// `png` replaces image `frame` (palette images first, then true-colour ones, as in the preview).
pub fn replace_spr_image(data: &[u8], frame: usize, png: &[u8]) -> Result<Vec<u8>, String> {
    let mut spr = Spr::parse(data).map_err(|e| e.to_string())?;
    let n_indexed = spr.indexed.len();
    match png_for(&spr, png, frame < n_indexed)? {
        NewImage::Indexed(i) => spr.replace_indexed(frame, i),
        NewImage::Rgba(i) => spr.replace_rgba(frame.wrapping_sub(n_indexed), i),
    }
    .map_err(|e| e.to_string())?;
    spr.to_bytes(spr.version).map_err(|e| e.to_string())
}

/// Appends a PNG as a new image: a palette image if the sprite uses a palette.
pub fn add_spr_image(data: &[u8], png: &[u8]) -> Result<Vec<u8>, String> {
    let mut spr = Spr::parse(data).map_err(|e| e.to_string())?;
    let indexed = spr.palette.is_some() && !(spr.indexed.is_empty() && !spr.rgba.is_empty());
    match png_for(&spr, png, indexed)? {
        NewImage::Indexed(i) => {
            spr.push_indexed(i).map_err(|e| e.to_string())?;
        }
        NewImage::Rgba(i) => {
            spr.push_rgba(i);
        }
    }
    spr.to_bytes(spr.version).map_err(|e| e.to_string())
}

/// Removes image `frame`; the animation (when given) is changed so its layers follow.
pub fn remove_spr_image(
    data: &[u8],
    frame: usize,
    act: Option<&[u8]>,
) -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
    let mut spr = Spr::parse(data).map_err(|e| e.to_string())?;
    let mut act = act
        .map(|a| Act::parse(a).map_err(|e| e.to_string()))
        .transpose()?;
    let n = spr.indexed.len();
    let (kind, i) = if frame < n {
        (Kind::Indexed, frame)
    } else {
        (Kind::Rgba, frame - n)
    };
    spr.remove_image(kind, i, act.as_mut())
        .map_err(|e| e.to_string())?;
    let spr_out = spr.to_bytes(spr.version).map_err(|e| e.to_string())?;
    let act_out = act
        .map(|a| a.to_bytes(a.version).map_err(|e| e.to_string()))
        .transpose()?;
    Ok((spr_out, act_out))
}

/// Gives the sprite another palette (the images keep their indices, so the colours change).
pub fn set_spr_palette(data: &[u8], pal: &[u8]) -> Result<Vec<u8>, String> {
    let mut spr = Spr::parse(data).map_err(|e| e.to_string())?;
    spr.set_palette(&Pal::parse(pal).map_err(|e| e.to_string())?);
    spr.to_bytes(spr.version).map_err(|e| e.to_string())
}

/// The ACT stores the interval in units of 25 ms.
pub fn set_act_interval(data: &[u8], action: usize, ms: f32) -> Result<Vec<u8>, String> {
    if !(ms.is_finite() && ms > 0.0) {
        return Err(err!("err-interval-positive"));
    }
    let mut act = Act::parse(data).map_err(|e| e.to_string())?;
    act.set_interval(action, ms / 25.0)
        .map_err(|e| e.to_string())?;
    act.to_bytes(act.version).map_err(|e| e.to_string())
}

fn file_name_of(p: &Path) -> String {
    p.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

fn check_file(file: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(file).map_err(|e| format!("{}: {e}", file.display()))?;
    if !meta.is_file() {
        return Err(err!("err-not-file", "path": file.display().to_string()));
    }
    if meta.len() > u64::from(u32::MAX) {
        return Err(err!("err-file-4gib", "path": file.display().to_string()));
    }
    Ok(())
}

/// Writes the pending changes as a new archive and verifies it; the source stays as it is.
pub fn save_edited(
    plan: &EditPlan,
    dst: &Path,
    overwrite: bool,
    ctl: &Control,
) -> Result<EditSummary, String> {
    let src = plan.open()?;
    let edits = plan.edits()?;
    // unchanged entries keep their streams, so the archive does not grow or lose its compression
    let opts = RepackOptions {
        overwrite,
        copy_streams: true,
        encrypt: plan.key,
        ..RepackOptions::default()
    };
    let r = edit::apply(&src, &plan.src, dst, &edits, &opts, ctl).map_err(|e| e.to_string())?;
    Ok(EditSummary {
        added: r.added,
        replaced: r.replaced,
        removed: r.removed,
        files: r.merge.repack.files,
        file_bytes: r.merge.repack.output_file_len,
        verified: r.merge.repack.verified,
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
        return Err(err!("err-merge-two"));
    }
    if req.sources.iter().any(|p| same_file(p, &req.dst)) {
        return Err(err!("err-merge-target"));
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

fn png_frame(img: &RgbaImage, label: Note) -> Option<Frame> {
    Some(Frame {
        png: b64(&rgba_to_png(img).ok()?),
        width: u32::from(img.width),
        height: u32::from(img.height),
        label,
    })
}

/// Text of a file whose encoding is unknown: UTF-8 if it is, else Korean (CP949) if that decodes
/// without errors, else Windows-1252.
fn lua_version(chunk: &rgrf_formats::luac::Chunk) -> &'static str {
    if chunk.main.orig_code.is_some() {
        "5.0"
    } else {
        "5.1"
    }
}

/// Encodes text in a single-byte or Korean legacy encoding; characters it cannot hold are an error
/// (a replacement would silently change the file).
fn encode_legacy(enc: &'static encoding_rs::Encoding, text: &str) -> Result<Vec<u8>, String> {
    let (bytes, _, bad) = enc.encode(text);
    if bad {
        return Err(err!("err-text-chars", "encoding": enc.name()));
    }
    Ok(bytes.into_owned())
}

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
        Self::open_with(path, None)
    }

    /// Opens an archive, with the key of GRF Editor's encryption if there is one (this also opens
    /// a file table that is encrypted).
    pub fn open_with(path: &Path, key: Option<&rgrf_core::grfcrypt::Key>) -> Result<Self, String> {
        let arc = match key {
            Some(k) => Archive::open_with_key(path, k),
            None => Archive::open(path),
        }
        .map_err(|e| format!("{}: {e}", path.display()))?;
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
            views: Default::default(),
            pending: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            keys: OnceLock::new(),
            crypt_key: key.copied(),
            draft_root: None,
            draft_offered: false,
        })
    }

    /// Where drafts of pending changes are kept; a draft of this archive found there is offered
    /// through [`Session::info`] until it is restored or discarded.
    pub fn set_draft_root(&mut self, root: Option<PathBuf>) {
        self.draft_offered = root
            .as_deref()
            .is_some_and(|r| crate::draft::info(r, &self.path).is_some());
        self.draft_root = root;
    }

    /// Writes the pending changes as the draft (or removes it when there are none). A failure is
    /// returned to nobody: the changes are still in memory, only the safety copy is missing.
    fn persist_draft(&self) {
        let Some(root) = &self.draft_root else { return };
        if self.draft_offered {
            return;
        }
        let changes: Vec<crate::draft::DraftChange> = self
            .pending
            .iter()
            .map(|c| match c {
                Change::Put { name, file } => crate::draft::DraftChange::Put {
                    name: name.as_bytes().to_vec(),
                    file: file.clone(),
                },
                Change::PutBytes { name, data, what } => crate::draft::DraftChange::Bytes {
                    name: name.as_bytes().to_vec(),
                    data: Arc::clone(data),
                    what: what.clone(),
                },
                Change::Remove { name } => crate::draft::DraftChange::Remove {
                    name: name.as_bytes().to_vec(),
                },
            })
            .collect();
        if let Err(e) = crate::draft::save(root, &self.path, &changes) {
            eprintln!("draft of {} not written: {e}", self.path.display());
        }
    }

    /// Takes the changes of the offered draft as the pending changes (one step for undo).
    pub fn restore_draft(&mut self) -> Result<DraftRestored, String> {
        let root = self.draft_root.clone().ok_or("drafts are off")?;
        let (changes, skipped) =
            crate::draft::take(&root, &self.path).ok_or_else(|| err!("err-no-draft"))?;
        self.draft_offered = false;
        let restored = changes.len();
        self.transact(|s| {
            s.pending = changes
                .into_iter()
                .map(|c| match c {
                    crate::draft::DraftChange::Put { name, file } => Change::Put {
                        name: GrfPath::new(name),
                        file,
                    },
                    crate::draft::DraftChange::Bytes { name, data, what } => Change::PutBytes {
                        name: GrfPath::new(name),
                        data,
                        what,
                    },
                    crate::draft::DraftChange::Remove { name } => Change::Remove {
                        name: GrfPath::new(name),
                    },
                })
                .collect();
            Ok(())
        })?;
        Ok(DraftRestored { restored, skipped })
    }

    /// Keeps the offered draft for a later session and saves the changes of this one again.
    pub fn defer_draft(&mut self) {
        if let Some(root) = &self.draft_root
            && let Err(e) = crate::draft::set_aside(root, &self.path)
        {
            eprintln!("draft of {} not set aside: {e}", self.path.display());
            return;
        }
        self.draft_offered = false;
        self.persist_draft();
    }

    /// Deletes the offered draft; the pending changes from now on are saved as the new draft.
    pub fn discard_draft(&mut self) {
        if let Some(root) = &self.draft_root {
            crate::draft::discard(root, &self.path);
        }
        self.draft_offered = false;
        self.persist_draft();
    }

    pub fn stage_replace(&mut self, index: usize, file: &Path) -> Result<(), String> {
        self.transact(|s| s.stage_replace_impl(index, file))
    }

    pub fn stage_add(&mut self, path: &str, file: &Path) -> Result<(), String> {
        self.transact(|s| s.stage_add_impl(path, file))
    }

    pub fn stage_spr_image(
        &mut self,
        index: usize,
        frame: usize,
        png: &Path,
    ) -> Result<(), String> {
        self.transact(|s| s.stage_spr_image_impl(index, frame, png))
    }

    /// The sound of an animation frame, read from this archive (with staged changes).
    pub fn act_sound(&self, name: &str) -> Result<SoundOut, String> {
        let raw = rgrf_formats::act::sound_from_name(name)
            .ok_or_else(|| format!("'{name}' is not a sound name an animation can hold"))?;
        let len = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let path = [b"data\\wav\\".as_slice(), &raw[..len]].concat();
        let shown = GrfPath::new(path.clone()).display(self.mode);
        let out = |data: Option<String>, problem: Option<&str>| SoundOut {
            path: shown.clone(),
            data,
            problem: problem.map(str::to_owned),
        };
        // the client's marker for the frame that hits; there is no such file
        if name.eq_ignore_ascii_case("atk") {
            return Ok(out(
                None,
                Some("\"atk\" marks the frame that hits, it is not a sound"),
            ));
        }
        let Some(i) = self.arc.find(&GrfPath::new(path)) else {
            return Ok(out(None, Some("not in this archive")));
        };
        let data = self.read_working(i)?;
        Ok(match rgrf_formats::wav::playable(&data) {
            Ok(rgrf_formats::wav::Playable::AsIs) => out(Some(b64(&data)), None),
            Ok(rgrf_formats::wav::Playable::Decoded(pcm)) => out(Some(b64(&pcm)), None),
            Err(e) => out(None, Some(&format!("cannot be played: {e}"))),
        })
    }

    pub fn reduce_preview(&self, index: usize) -> Result<ReducePreview, String> {
        let (_, data, _) = self.working_bytes(index)?;
        let (out, psnr, colors) = reduce_one(&data)?;
        Ok(ReducePreview {
            psnr: psnr.is_finite().then_some(psnr),
            colors,
            bytes_before: data.len(),
            bytes_after: out.len(),
            before: b64(&data),
            after: b64(&out),
        })
    }

    /// Stores a true-colour BMP as an 8-bit one (256 colours, magenta kept exact).
    pub fn stage_reduce(&mut self, index: usize) -> Result<(), String> {
        self.transact(|s| {
            let (name, data, mut what) = s.working_bytes(index)?;
            let (out, psnr, colors) = reduce_one(&data)?;
            what.push(if psnr.is_finite() {
                format!("reduced to 8 bits: {colors} colours, PSNR {psnr:.1} dB")
            } else {
                format!("reduced to 8 bits: {colors} colours, lossless")
            });
            s.stage(Change::PutBytes {
                name,
                data: Arc::new(out),
                what,
            });
            Ok(())
        })
    }

    pub fn stage_spr_add(&mut self, index: usize, png: &Path) -> Result<(), String> {
        self.transact(|s| s.stage_spr_add_impl(index, png))
    }

    pub fn stage_spr_remove(&mut self, index: usize, frame: usize) -> Result<(), String> {
        self.transact(|s| s.stage_spr_remove_impl(index, frame))
    }

    pub fn stage_spr_palette(&mut self, index: usize, pal: &Path) -> Result<(), String> {
        self.transact(|s| s.stage_spr_palette_impl(index, pal))
    }

    pub fn stage_act_interval(
        &mut self,
        index: usize,
        action: usize,
        ms: f32,
    ) -> Result<(), String> {
        self.transact(|s| s.stage_act_interval_impl(index, action, ms))
    }

    pub fn pal_colors(&self, index: usize) -> Result<Vec<String>, String> {
        let (_, data, _) = self.working_bytes(index)?;
        pal_editor::colors(&data)
    }

    pub fn spr_image(
        &self,
        index: usize,
        frame: usize,
    ) -> Result<spr_editor::SprImageView, String> {
        let (_, data, _) = self.working_bytes(index)?;
        spr_editor::image(&data, frame)
    }

    /// Writes edited pixels (palette indices, base64) back into a palette image.
    pub fn stage_spr_pixels(
        &mut self,
        index: usize,
        frame: usize,
        pixels: &str,
    ) -> Result<(), String> {
        self.transact(|s| {
            let (name, data, mut what) = s.working_bytes(index)?;
            let out = spr_editor::apply(&data, frame, pixels)?;
            let note = format!("image {frame} edited pixel by pixel");
            if !what.contains(&note) {
                what.push(note);
            }
            s.stage(Change::PutBytes {
                name,
                data: Arc::new(out),
                what,
            });
            Ok(())
        })
    }

    /// Sets cells of a map; returns how many cells changed.
    pub fn stage_gat_edit(&mut self, index: usize, edit: &GatEdit) -> Result<usize, String> {
        let mut changed = 0;
        self.transact(|s| {
            let (name, data, mut what) = s.working_bytes(index)?;
            let (out, n) = gat_editor::apply(&data, edit)?;
            changed = n;
            what.push(format!(
                "{n} cell(s) of x {}..{}, y {}..{} set",
                edit.x0.min(edit.x1),
                edit.x0.max(edit.x1),
                edit.y0.min(edit.y1),
                edit.y0.max(edit.y1)
            ));
            s.stage(Change::PutBytes {
                name,
                data: Arc::new(out),
                what,
            });
            Ok(())
        })?;
        Ok(changed)
    }

    pub fn stage_pal_edit(&mut self, index: usize, edit: &PalEdit) -> Result<(), String> {
        self.transact(|s| s.stage_pal_edit_impl(index, edit))
    }

    pub fn stage_text_edit(&mut self, index: usize, text: &str) -> Result<(), String> {
        self.transact(|s| s.stage_text_edit_impl(index, text))
    }

    /// The whole content of a text or Lua entry as editable text (compiled Lua is decompiled).
    pub fn text_for_edit(&self, index: usize) -> Result<TextForEdit, String> {
        let (_, data, _) = self.working_bytes(index)?;
        if data.starts_with(b"\x1bLua") {
            let chunk = rgrf_formats::luac::parse_any(&data)
                .map_err(|e| format!("this compiled Lua cannot be read: {e}"))?;
            let names = |b: &[u8]| GrfPath::new(b.to_vec()).display(NameMode::Cp949);
            let text = rgrf_formats::luadec::decompile(&chunk, &names);
            let complete = rgrf_formats::luadec::is_complete(&text);
            return Ok(TextForEdit {
                text,
                format: format!("Lua {} bytecode", lua_version(&chunk)),
                complete,
            });
        }
        let (text, encoding) = decode_text(&data);
        Ok(TextForEdit {
            text,
            format: format!("text ({encoding})"),
            complete: true,
        })
    }

    pub fn stage_remove(&mut self, indices: &[usize]) -> Result<usize, String> {
        self.transact(|s| s.stage_remove_impl(indices))
    }

    pub fn stage_act_edit(
        &mut self,
        index: usize,
        action: usize,
        frame: usize,
        edit: &ActEdit,
    ) -> Result<(), String> {
        self.transact(|s| s.stage_act_edit_impl(index, action, frame, edit))
    }

    /// Runs a change of the pending list as one undo step; if it fails nothing is changed.
    fn transact<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, String>) -> Result<T, String> {
        let before = self.pending.clone();
        match f(self) {
            Ok(v) => {
                self.undo.push(before);
                if self.undo.len() > MAX_UNDO {
                    self.undo.remove(0);
                }
                self.redo.clear();
                self.persist_draft();
                Ok(v)
            }
            Err(e) => {
                self.pending = before;
                Err(e)
            }
        }
    }

    /// Takes back the last change; false when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(prev) => {
                self.redo.push(std::mem::replace(&mut self.pending, prev));
                self.persist_draft();
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(next) => {
                self.undo.push(std::mem::replace(&mut self.pending, next));
                self.persist_draft();
                true
            }
            None => false,
        }
    }

    /// (can undo, can redo)
    pub fn history(&self) -> (bool, bool) {
        (!self.undo.is_empty(), !self.redo.is_empty())
    }

    fn stage(&mut self, change: Change) {
        let key = change.name().key();
        self.pending.retain(|c| c.name().key() != key);
        self.pending.push(change);
    }

    fn entry_name(&self, index: usize) -> Result<GrfPath, String> {
        let e = self
            .arc
            .entries()
            .get(index)
            .ok_or_else(|| format!("no entry {index}"))?;
        if e.is_marker() {
            return Err(err!("err-empty-marker"));
        }
        Ok(e.name.clone())
    }

    /// The content of entry `index` will be the content of `file`.
    fn stage_replace_impl(&mut self, index: usize, file: &Path) -> Result<(), String> {
        let name = self.entry_name(index)?;
        check_file(file)?;
        self.stage(Change::Put {
            name,
            file: file.to_path_buf(),
        });
        Ok(())
    }

    /// Adds `file` under the archive path `path` (or replaces the entry that has it).
    fn stage_add_impl(&mut self, path: &str, file: &Path) -> Result<(), String> {
        let text = path.trim().replace('/', "\\");
        if text.is_empty() || text.ends_with('\\') {
            return Err(err!("err-enter-path"));
        }
        let name = GrfPath::from_display(&text, self.mode).ok_or_else(|| err!("err-name-chars"))?;
        check_file(file)?;
        self.stage(Change::Put {
            name,
            file: file.to_path_buf(),
        });
        Ok(())
    }

    /// The content of entry `index` as it will be saved: the staged edit if there is one, else the archive's.
    fn read_working(&self, index: usize) -> Result<Vec<u8>, String> {
        self.working_bytes(index).map(|(_, data, _)| data)
    }

    /// The content the next edit of entry `index` starts from: an earlier staged edit, else the archive.
    fn working_bytes(&self, index: usize) -> Result<(GrfPath, Vec<u8>, Vec<String>), String> {
        let name = self.entry_name(index)?;
        let key = name.key();
        match self.pending.iter().find(|c| c.name().key() == key) {
            Some(Change::PutBytes { data, what, .. }) => Ok((name, (**data).clone(), what.clone())),
            Some(Change::Put { file, .. }) => {
                let data = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
                Ok((name, data, vec![format!("replaced by {}", file.display())]))
            }
            Some(Change::Remove { .. }) => Err(err!("err-marked-removal")),
            None => Ok((name, self.read(index)?, Vec::new())),
        }
    }

    /// Replaces image `frame` of a sprite with a PNG (palette sprites map its colours to the palette).
    fn stage_spr_image_impl(
        &mut self,
        index: usize,
        frame: usize,
        png: &Path,
    ) -> Result<(), String> {
        let (name, data, mut what) = self.working_bytes(index)?;
        let png_bytes = std::fs::read(png).map_err(|e| format!("{}: {e}", png.display()))?;
        let out = replace_spr_image(&data, frame, &png_bytes)?;
        what.push(format!(
            "image {frame} from {}",
            png.file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        ));
        self.stage(Change::PutBytes {
            name,
            data: Arc::new(out),
            what,
        });
        Ok(())
    }

    /// Appends a PNG to a sprite as a new image.
    fn stage_spr_add_impl(&mut self, index: usize, png: &Path) -> Result<(), String> {
        let (name, data, mut what) = self.working_bytes(index)?;
        let png_bytes = std::fs::read(png).map_err(|e| format!("{}: {e}", png.display()))?;
        what.push(format!("image added from {}", file_name_of(png)));
        let out = add_spr_image(&data, &png_bytes)?;
        self.stage(Change::PutBytes {
            name,
            data: Arc::new(out),
            what,
        });
        Ok(())
    }

    /// Removes image `frame` of a sprite; the animation next to it follows when it exists.
    fn stage_spr_remove_impl(&mut self, index: usize, frame: usize) -> Result<(), String> {
        let (name, data, mut what) = self.working_bytes(index)?;
        let act_index = self.sibling(index, "act");
        let act = act_index.map(|i| self.working_bytes(i)).transpose()?;
        let (spr_out, act_out) =
            remove_spr_image(&data, frame, act.as_ref().map(|a| a.1.as_slice()))?;
        what.push(format!("image {frame} removed"));
        self.stage(Change::PutBytes {
            name,
            data: Arc::new(spr_out),
            what,
        });
        if let (Some((aname, _, mut awhat)), Some(bytes)) = (act, act_out) {
            awhat.push(format!("layers follow the removal of sprite image {frame}"));
            self.stage(Change::PutBytes {
                name: aname,
                data: Arc::new(bytes),
                what: awhat,
            });
        }
        Ok(())
    }

    /// Gives a sprite the palette of a `.pal` file.
    fn stage_spr_palette_impl(&mut self, index: usize, pal: &Path) -> Result<(), String> {
        let (name, data, mut what) = self.working_bytes(index)?;
        let pal_bytes = std::fs::read(pal).map_err(|e| format!("{}: {e}", pal.display()))?;
        what.push(format!("palette from {}", file_name_of(pal)));
        let out = set_spr_palette(&data, &pal_bytes)?;
        self.stage(Change::PutBytes {
            name,
            data: Arc::new(out),
            what,
        });
        Ok(())
    }

    /// One frame of an animation as data and picture, with the staged changes applied.
    pub fn act_view(
        &self,
        index: usize,
        action: usize,
        frame: usize,
    ) -> Result<ActFrameView, String> {
        self.act_view_with(index, action, frame, &[])
    }

    /// The topmost layer of the animation `index` with a visible pixel at a frame point.
    pub fn act_layer_at(
        &self,
        index: usize,
        action: usize,
        frame: usize,
        x: f64,
        y: f64,
    ) -> Result<Option<usize>, String> {
        let act = self.working_bytes(index)?.1;
        let spr = self
            .sibling(index, "spr")
            .ok_or_else(|| err!("err-no-spr"))?;
        let spr = self.working_bytes(spr)?.1;
        act_editor::layer_at_point(&act, &spr, action, frame, x, y)
    }

    /// [`Self::act_view`] with reference animations of this archive drawn in the picture.
    pub fn act_view_with(
        &self,
        index: usize,
        action: usize,
        frame: usize,
        references: &[ReferenceSpec],
    ) -> Result<ActFrameView, String> {
        let pair = |i: usize| -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
            let act = self.working_bytes(i)?.1;
            let spr = self
                .sibling(i, "spr")
                .map(|s| self.working_bytes(s))
                .transpose()?
                .map(|t| t.1);
            Ok((act, spr))
        };
        let (act, spr) = pair(index)?;
        let mut found = Vec::new();
        let mut missing = Vec::new();
        for r in references {
            let path = GrfPath::from_display(&r.path.replace('/', "\\"), self.mode);
            match path.and_then(|p| self.arc.find(&p)) {
                Some(i) if i != index => found.push((r, pair(i)?)),
                Some(_) => missing.push((r, "is the animation being edited")),
                None => missing.push((r, "is not in this archive")),
            }
        }
        let refs: Vec<act_editor::Reference<'_>> = found
            .iter()
            .map(|(r, (a, s))| act_editor::Reference {
                name: r.path.clone(),
                act: a,
                spr: s.as_deref(),
                under: r.under,
            })
            .collect();
        let mut v = act_editor::view_with(&act, spr.as_deref(), action, frame, &refs)?;
        v.references.extend(
            missing
                .into_iter()
                .map(|(r, why)| act_editor::ReferenceView {
                    name: r.path.clone(),
                    problem: Some(why.into()),
                }),
        );
        Ok(v)
    }

    /// Changes colours of a palette file.
    fn stage_pal_edit_impl(&mut self, index: usize, edit: &PalEdit) -> Result<(), String> {
        let (name, data, mut what) = self.working_bytes(index)?;
        let out = pal_editor::apply(&data, edit)?;
        let note = match edit {
            PalEdit::Set { index, .. } => format!("colour {index} set"),
            PalEdit::Adjust { from, to, .. } => format!("colours {from} to {to} adjusted"),
        };
        what.push(note);
        self.stage(Change::PutBytes {
            name,
            data: Arc::new(out),
            what,
        });
        Ok(())
    }

    /// Changes a layer, anchor or frame of an animation.
    fn stage_act_edit_impl(
        &mut self,
        index: usize,
        action: usize,
        frame: usize,
        edit: &ActEdit,
    ) -> Result<(), String> {
        let (name, data, mut what) = self.working_bytes(index)?;
        let out = act_editor::apply(&data, action, frame, edit)?;
        let kind = match edit {
            ActEdit::SetLayer { .. } => "layers edited",
            ActEdit::AddLayer => "layer added",
            ActEdit::RemoveLayer { .. } => "layer removed",
            ActEdit::MoveLayer { .. } => "layer moved",
            ActEdit::SetAnchor { .. } | ActEdit::AddAnchor | ActEdit::RemoveAnchor { .. } => {
                "anchors edited"
            }
            ActEdit::CopyFrame => "frame copied",
            ActEdit::RemoveFrame => "frame removed",
            ActEdit::MoveFrame { .. } => "frame moved",
            ActEdit::CopyAction => "action copied",
            ActEdit::RemoveAction => "action removed",
            ActEdit::MoveAction { .. } => "action moved",
            ActEdit::SetInterval { .. } => "interval set",
            ActEdit::Interpolate { .. } => "in-between frames added",
            ActEdit::SetSound { .. } => "sound set",
        };
        let note = format!("action {action} frame {frame}: {kind}");
        if !what.contains(&note) {
            what.push(note);
        }
        self.stage(Change::PutBytes {
            name,
            data: Arc::new(out),
            what,
        });
        Ok(())
    }

    /// Sets the frame interval of an action of an animation, in milliseconds.
    fn stage_act_interval_impl(
        &mut self,
        index: usize,
        action: usize,
        ms: f32,
    ) -> Result<(), String> {
        let (name, data, mut what) = self.working_bytes(index)?;
        let out = set_act_interval(&data, action, ms)?;
        what.push(format!("action {action}: {ms} ms per frame"));
        self.stage(Change::PutBytes {
            name,
            data: Arc::new(out),
            what,
        });
        Ok(())
    }

    /// Replaces a text or Lua entry by edited text. Compiled Lua is compiled again (same Lua version,
    /// same debug information as before); plain text keeps its encoding and line endings.
    fn stage_text_edit_impl(&mut self, index: usize, text: &str) -> Result<(), String> {
        let (name, data, mut what) = self.working_bytes(index)?;
        let out = if data.starts_with(b"\x1bLua") {
            let old = rgrf_formats::luac::parse_any(&data)
                .map_err(|e| format!("this compiled Lua cannot be read: {e}"))?;
            let stripped = old.main.source.is_empty() && old.main.lines.is_empty();
            let cp949 = |run: &str| {
                GrfPath::from_display(run, NameMode::Cp949).map(|p| p.as_bytes().to_vec())
            };
            let enc = rgrf_formats::luacomp::TextEncoding::Auto;
            let lub = if lua_version(&old) == "5.0" {
                let mut chunk =
                    rgrf_formats::luacomp50::compile_text(text, &old.main.source, enc, &cp949)
                        .map_err(|e| e.to_string())?;
                if stripped {
                    rgrf_formats::luacomp50::strip(&mut chunk);
                }
                rgrf_formats::luac50::dump32(&chunk)
            } else {
                let mut chunk =
                    rgrf_formats::luacomp::compile_text(text, &old.main.source, enc, &cp949)
                        .map_err(|e| e.to_string())?;
                if stripped {
                    rgrf_formats::luacomp::strip(&mut chunk);
                }
                rgrf_formats::luac::dump32(&chunk)
            };
            rgrf_formats::luac::check_bytecode(&lub)
                .map_err(|e| format!("internal error, the result is invalid: {e}"))?;
            what.push("Lua recompiled from edited text".into());
            lub
        } else {
            let (_, encoding) = decode_text(&data);
            let mut text = text.replace("\r\n", "\n");
            if data.windows(2).any(|w| w == b"\r\n") {
                text = text.replace('\n', "\r\n");
            }
            let bytes = match encoding {
                "UTF-8" => text.into_bytes(),
                "CP949" => encode_legacy(encoding_rs::EUC_KR, &text)?,
                _ => encode_legacy(encoding_rs::WINDOWS_1252, &text)?,
            };
            what.push(format!("text edited ({encoding})"));
            bytes
        };
        self.stage(Change::PutBytes {
            name,
            data: Arc::new(out),
            what,
        });
        Ok(())
    }

    fn stage_remove_impl(&mut self, indices: &[usize]) -> Result<usize, String> {
        let names = indices
            .iter()
            .map(|&i| self.entry_name(i))
            .collect::<Result<Vec<_>, _>>()?;
        for name in &names {
            self.stage(Change::Remove { name: name.clone() });
        }
        Ok(names.len())
    }

    pub fn discard_pending(&mut self) {
        let _ = self.transact(|s| {
            s.pending.clear();
            Ok(())
        });
    }

    pub fn pending(&self) -> Vec<PendingRow> {
        self.pending
            .iter()
            .map(|c| match c {
                Change::Put { name, file } => PendingRow {
                    kind: if self.arc.find(name).is_some() {
                        "replace"
                    } else {
                        "add"
                    },
                    name: name.display(self.mode),
                    file: Some(file.display().to_string()),
                },
                Change::PutBytes { name, what, .. } => PendingRow {
                    kind: "replace",
                    name: name.display(self.mode),
                    file: Some(what.join(", ")),
                },
                Change::Remove { name } => PendingRow {
                    kind: "remove",
                    name: name.display(self.mode),
                    file: None,
                },
            })
            .collect()
    }

    pub fn edit_plan(&self) -> Result<EditPlan, String> {
        if self.pending.is_empty() {
            return Err(err!("err-no-changes"));
        }
        Ok(self.plan())
    }

    /// The open archive and its pending changes (possibly none).
    pub fn plan(&self) -> EditPlan {
        EditPlan {
            src: self.path.clone(),
            changes: self.pending.clone(),
            key: self.crypt_key,
        }
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
        self.views = Default::default();
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
            format: self.arc.header().format_name(),
            entries: self.arc.len(),
            files: self.arc.entries().iter().filter(|e| e.is_file()).count(),
            file_size: self.arc.file_len(),
            encryption: self.arc.encryption().map(|e| EncryptionOut {
                encrypted_entries: e.encrypted_entries,
                key_needed: !e.key_set,
            }),
            draft: self
                .draft_root
                .as_deref()
                .filter(|_| self.draft_offered)
                .and_then(|r| crate::draft::info(r, &self.path)),
        }
    }

    /// Sets the filter of the main list (empty = everything; `*` and `?` make a glob, else a
    /// substring) and returns the number of matches.
    pub fn search(&mut self, query: &str) -> usize {
        let spec = ViewSpec {
            query: query.to_string(),
            layout: "flat".into(),
            ..ViewSpec::default()
        };
        self.set_view(0, spec).map_or(0, |s| s.matches)
    }

    pub fn page(&self, offset: usize, limit: usize) -> Vec<Row> {
        self.page_view(0, offset, limit)
    }

    fn meta_of(&self, i: usize) -> Meta {
        let e = &self.arc.entries()[i];
        Meta {
            size: e.size,
            stored: e.zsize,
            offset: e.abs_offset(),
        }
    }

    /// Arranges list `view` (0 main, 1 folder pane): filter, sort and layout.
    pub fn set_view(&mut self, view: usize, spec: ViewSpec) -> Result<ViewSummary, String> {
        if view >= 2 {
            return Err("no such list".into());
        }
        let mut idx: Vec<usize> = if spec.query.is_empty() {
            (0..self.names.len()).collect()
        } else {
            self.arc.select(Some(&spec.query), self.mode)
        };
        if let (Some(folder), "flat", true) =
            (&spec.folder, spec.layout.as_str(), spec.query.is_empty())
        {
            view::in_folder(&mut idx, &self.names, folder, spec.recursive);
        }
        let meta = |i: usize| self.meta_of(i);
        view::filter_indices(&mut idx, &self.names, &meta, &kind_of, &spec);
        view::sort_indices(&mut idx, &self.names, &meta, &spec.sort, spec.desc);
        let matches = idx.len();
        let tree_layout = matches!(spec.layout.as_str(), "tree" | "folders");
        let (tree, lines, flat) = if tree_layout {
            let folders_desc = spec.desc && matches!(spec.sort.as_str(), "name" | "path");
            (
                Some(Tree::build(&self.names, &idx, &meta, folders_desc)),
                Vec::new(),
                Vec::new(),
            )
        } else {
            (None, Vec::new(), idx.clone())
        };
        let st = &mut self.views[view];
        if let Some(t) = &tree {
            if spec.auto_expand {
                st.expanded = if spec.layout == "tree" && matches <= view::AUTO_EXPAND_LIMIT {
                    t.all_keys()
                } else {
                    t.opening_keys()
                };
            }
            st.lines = view::visible(t, &st.expanded, spec.layout == "tree");
        } else {
            st.lines = lines;
        }
        st.tree = tree;
        st.flat = flat;
        let rows = if st.tree.is_some() {
            st.lines.len()
        } else {
            st.flat.len()
        };
        st.spec = spec;
        if view == 0 {
            self.filtered = idx;
        }
        Ok(ViewSummary { rows, matches })
    }

    pub fn page_view(&self, view: usize, offset: usize, limit: usize) -> Vec<Row> {
        let Some(st) = self.views.get(view) else {
            return Vec::new();
        };
        let file_row = |i: usize, depth: u16| {
            let e = &self.arc.entries()[i];
            Row {
                index: i,
                name: self.names[i].clone(),
                size: e.size,
                kind: kind_of(&self.names[i]),
                stored: e.zsize,
                depth,
                folder: false,
                expanded: false,
                count: 0,
                bytes: u64::from(e.size),
            }
        };
        match &st.tree {
            None => {
                // before the first `set_view` the list is the whole archive
                if st.spec.layout.is_empty() && st.flat.is_empty() && self.views_unset(view) {
                    return self
                        .filtered
                        .iter()
                        .skip(offset)
                        .take(limit)
                        .map(|&i| file_row(i, 0))
                        .collect();
                }
                st.flat
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .map(|&i| file_row(i, 0))
                    .collect()
            }
            Some(t) => st
                .lines
                .iter()
                .skip(offset)
                .take(limit)
                .map(|l| match *l {
                    Line::File { entry, depth } => file_row(entry, depth),
                    Line::Folder { node, depth } => {
                        let n = &t.nodes[node];
                        Row {
                            index: FOLDER_ROW,
                            name: n.path.clone(),
                            size: 0,
                            kind: "folder",
                            stored: 0,
                            depth,
                            folder: true,
                            expanded: st.expanded.contains(&n.key),
                            count: n.total_files,
                            bytes: n.total_bytes,
                        }
                    }
                })
                .collect(),
        }
    }

    fn views_unset(&self, view: usize) -> bool {
        view == 0 && self.views[0].spec == ViewSpec::default()
    }

    fn relayout(&mut self, view: usize) -> usize {
        let st = &mut self.views[view];
        if let Some(t) = &st.tree {
            st.lines = view::visible(t, &st.expanded, st.spec.layout == "tree");
            st.lines.len()
        } else {
            st.flat.len()
        }
    }

    /// Opens or closes one folder of a tree list; returns the number of lines.
    pub fn tree_toggle(&mut self, view: usize, path: &str) -> Result<usize, String> {
        let st = self.views.get_mut(view).ok_or("no such list")?;
        let key = path.trim_matches('\\').to_lowercase();
        if !st.expanded.remove(&key) {
            st.expanded.insert(key);
        }
        Ok(self.relayout(view))
    }

    /// Opens or closes every folder of a tree list.
    pub fn tree_set_all(&mut self, view: usize, open: bool) -> Result<usize, String> {
        let st = self.views.get_mut(view).ok_or("no such list")?;
        st.expanded = match (&st.tree, open) {
            (Some(t), true) => t.all_keys(),
            _ => Default::default(),
        };
        Ok(self.relayout(view))
    }

    /// The line of an entry (or of a folder, by path) in list `view`. In a tree list the folders
    /// above it are opened first, so it is always visible; `rows` is the number of lines after that.
    pub fn locate(
        &mut self,
        view: usize,
        entry: Option<usize>,
        folder: Option<&str>,
    ) -> Result<Located, String> {
        let path = match (entry, folder) {
            (Some(i), _) => self.names.get(i).cloned().ok_or("no such entry")?,
            (None, Some(f)) => f.to_string(),
            _ => return Ok(Located::default()),
        };
        let st = self.views.get_mut(view).ok_or("no such list")?;
        if st.tree.is_none() {
            return Ok(Located {
                line: entry.and_then(|e| st.flat.iter().position(|&x| x == e)),
                rows: st.flat.len(),
            });
        }
        st.expanded.extend(view::ancestors(&path));
        let rows = self.relayout(view);
        let st = &self.views[view];
        let Some(t) = &st.tree else {
            return Ok(Located::default());
        };
        let want = path.trim_matches('\\').to_lowercase();
        let line = st.lines.iter().position(|l| match *l {
            Line::File { entry: e, .. } => Some(e) == entry,
            Line::Folder { node, .. } => entry.is_none() && t.nodes[node].key == want,
        });
        Ok(Located { line, rows })
    }

    /// The entries below a folder of a tree list (all matches for an empty path).
    pub fn folder_entries(&self, view: usize, path: &str) -> Result<Vec<usize>, String> {
        let st = self.views.get(view).ok_or("no such list")?;
        let t = st.tree.as_ref().ok_or("this list has no folders")?;
        let n = t.find(path).ok_or("no such folder")?;
        Ok(t.files_below(n))
    }

    pub fn details(&self, index: usize) -> Result<Details, String> {
        let e = self.arc.entry(index).map_err(|e| e.to_string())?;
        let codec = match self.arc.sniff_codec(index) {
            Ok(Some(rgrf_core::codec::Codec::Zlib)) => "zlib",
            Ok(Some(rgrf_core::codec::Codec::Lzma)) => "LZMA",
            _ if e.is_protected() => "Gravity-protected",
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
            return Err(err!("err-preview-large", "mb": e.size / 1024 / 1024));
        }
        let d = self.arc.read(index).map_err(|e| e.to_string())?;
        if d.status == DecodeStatus::Encrypted {
            return Err(err!("err-entry-encrypted"));
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

    /// Thumbnails of the given entries, made in parallel; a bad entry yields an `error`, never a failure.
    pub fn thumbnails(&self, indices: &[usize], side: u32) -> Vec<ThumbOut> {
        use rayon::prelude::*;
        let side = side.clamp(16, 512);
        indices
            .par_iter()
            .map(|&index| {
                let made = self
                    .names
                    .get(index)
                    .ok_or_else(|| "no such entry".to_string())
                    .and_then(|name| {
                        let data = self.read_working(index)?;
                        crate::thumb::make(&ext(name), &data, side)
                    });
                match made {
                    Ok(t) => ThumbOut {
                        index,
                        mime: t.mime.into(),
                        data: Some(b64(&t.bytes)),
                        width: t.width,
                        height: t.height,
                        error: None,
                    },
                    Err(e) => ThumbOut {
                        index,
                        mime: String::new(),
                        data: None,
                        width: 0,
                        height: 0,
                        error: Some(e),
                    },
                }
            })
            .collect()
    }

    pub fn preview(&self, index: usize, action: Option<usize>) -> Result<Preview, String> {
        let name = self.names.get(index).ok_or("no such entry")?.clone();
        let entry = self.arc.entry(index).map_err(|e| e.to_string())?;
        if !entry.is_file() {
            return Ok(Preview::Message {
                text: note!("pvw-n-empty-marker"),
            });
        }
        let data = match self.read_working(index) {
            Ok(d) => d,
            Err(e) => return Ok(Preview::Message { text: e.into() }),
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
                    bits: (kind == "bmp")
                        .then(|| rgrf_formats::bmp::info(&data).ok().map(|i| i.bits))
                        .flatten(),
                    data: b64(&data),
                }
            }
            "tga" => {
                let img = rgrf_formats::tga::parse(&data).map_err(|e| e.to_string())?;
                Preview::Image {
                    mime: "image/png".into(),
                    data: b64(&rgba_to_png(&img).map_err(|e| e.to_string())?),
                    bits: None,
                }
            }
            "ogg" => Preview::Audio {
                mime: "audio/ogg".into(),
                data: b64(&data),
            },
            "wav" => match rgrf_formats::wav::playable(&data) {
                Ok(rgrf_formats::wav::Playable::AsIs) => Preview::Audio {
                    mime: "audio/wav".into(),
                    data: b64(&data),
                },
                // ADPCM sounds are converted so that every webview can play them
                Ok(rgrf_formats::wav::Playable::Decoded(pcm)) => Preview::Audio {
                    mime: "audio/wav".into(),
                    data: b64(&pcm),
                },
                Err(e) => Preview::Message {
                    text: note!("pvw-n-sound-fail", "error": e.to_string()),
                },
            },
            "mp3" => Preview::Audio {
                mime: "audio/mpeg".into(),
                data: b64(&data),
            },
            "str" => self.preview_str(index, &data)?,
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
                    frames: png_frame(&img, note!("pvw-n-palette-label"))
                        .into_iter()
                        .collect(),
                    info: vec![note!("pvw-n-palette")],
                }
            }
            "gat" => {
                let g = Gat::parse(&data).map_err(|e| e.to_string())?;
                let (lo, hi) = g.height_range();
                let mut info = vec![note!(
                    "pvw-n-gat",
                    "version": g.version.to_string(), "w": g.width, "h": g.height,
                    "lo": format!("{lo:.1}"), "hi": format!("{hi:.1}")
                )];
                for (k, n) in g.type_counts() {
                    info.push(note!("pvw-n-gat-type", "k": k, "n": n));
                }
                let img = g.to_image().ok_or_else(|| err!("err-map-large"))?;
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
                    gat: Some(GatMeta {
                        cells_x: g.width,
                        cells_y: g.height,
                        scale: s as u32,
                        types: (0..=6)
                            .map(|k| (k, gat::type_name(k).to_string()))
                            .collect(),
                    }),
                }
            }
            "rsw" => self.preview_scene(index, &data, true)?,
            "gnd" => self.preview_scene(index, &data, false)?,
            "rsm" | "rsm2" => self.preview_model(&data)?,
            "txt" | "xml" | "lua" | "lub" | "ini" | "csv" | "json" | "log" => {
                if data.starts_with(b"\x1bLua") {
                    match rgrf_formats::luac::parse_any(&data) {
                        Ok(chunk) => {
                            let names =
                                |b: &[u8]| GrfPath::new(b.to_vec()).display(NameMode::Cp949);
                            let mut text = rgrf_formats::luadec::decompile(&chunk, &names);
                            let complete = rgrf_formats::luadec::is_complete(&text);
                            let version = if chunk.main.orig_code.is_some() {
                                "5.0"
                            } else {
                                "5.1"
                            };
                            let truncated = text.chars().count() > MAX_TEXT_CHARS;
                            if truncated {
                                text = text.chars().take(MAX_TEXT_CHARS).collect();
                            }
                            Preview::Text {
                                text,
                                encoding: if complete {
                                    format!("decompiled from Lua {} bytecode", version)
                                } else {
                                    "partly decompiled (functions of an unknown shape stay as a listing)".to_string()
                                },
                                truncated,
                            }
                        }
                        Err(e) => Preview::Message {
                            text: note!("pvw-n-luac-fail", "error": e.to_string()),
                        },
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
            "bik" | "bk2" => match bink_header(&data) {
                Some((width, height, frames, fps)) => Preview::Video {
                    width,
                    height,
                    frames,
                    fps,
                    size: data.len() as u32,
                },
                None => Preview::Hex {
                    dump: hex_dump(&data),
                    size: data.len() as u32,
                },
            },
            _ => Preview::Hex {
                dump: hex_dump(&data),
                size: data.len() as u32,
            },
        })
    }

    /// Writes an entry (with staged changes) into a temporary folder and returns its path, so
    /// that another program can open it.
    pub fn export_temp(&self, index: usize) -> Result<PathBuf, String> {
        let (name, data, _) = self.working_bytes(index)?;
        let shown = name.display(self.mode);
        let file = shown.rsplit('\\').next().unwrap_or("file");
        // names come from the archive: keep only what is safe as one file name
        let safe: String = file
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || "._- ()".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let dir = std::env::temp_dir().join(format!("rustygrf-open-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(if safe.trim_matches('.').is_empty() {
            "file".into()
        } else {
            safe
        });
        std::fs::write(&path, data).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    /// `len` bytes of an entry (with staged changes) from `offset`, for the hex editor.
    pub fn hex_page(&self, index: usize, offset: u64, len: usize) -> Result<HexPage, String> {
        let (_, data, _) = self.working_bytes(index)?;
        let start = usize::try_from(offset).map_or(data.len(), |o| o.min(data.len()));
        let end = (start + len.min(HEX_PAGE_MAX)).min(data.len());
        Ok(HexPage {
            offset: start as u64,
            size: data.len() as u64,
            bytes: b64(&data[start..end]),
        })
    }

    /// The first place at or after `from` where `pattern` occurs in the entry (with staged
    /// changes), wrapping round to the start once; `None` when it is nowhere.
    pub fn hex_find(&self, index: usize, pattern: &[u8], from: u64) -> Result<Option<u64>, String> {
        if pattern.is_empty() || pattern.len() > 256 {
            return Err(err!("err-search-len"));
        }
        let (_, data, _) = self.working_bytes(index)?;
        let from = usize::try_from(from).map_or(data.len(), |f| f.min(data.len()));
        let find = |range: &[u8]| range.windows(pattern.len()).position(|w| w == pattern);
        Ok(find(&data[from..])
            .map(|p| p + from)
            .or_else(|| find(&data[..(from + pattern.len() - 1).min(data.len())]))
            .map(|p| p as u64))
    }

    /// Overwrites single bytes of an entry (the size never changes) and stages the result.
    pub fn stage_hex(&mut self, index: usize, edits: &[(u64, u8)]) -> Result<(), String> {
        self.transact(|s| {
            let (name, mut data, mut what) = s.working_bytes(index)?;
            if edits.is_empty() {
                return Err(err!("err-no-byte"));
            }
            let len = data.len();
            for &(at, byte) in edits {
                let slot = usize::try_from(at)
                    .ok()
                    .and_then(|a| data.get_mut(a))
                    .ok_or_else(|| format!("offset {at} is outside the file ({len} bytes)"))?;
                *slot = byte;
            }
            what.push(format!("{} byte(s) changed", edits.len()));
            s.stage(Change::PutBytes {
                name,
                data: Arc::new(data),
                what,
            });
            Ok(())
        })
    }

    /// Entry index by path key, made once (looking up one name with `find` scans the whole archive).
    fn key_index(&self) -> &HashMap<Vec<u8>, usize> {
        self.keys.get_or_init(|| {
            self.arc
                .entries()
                .iter()
                .enumerate()
                .map(|(i, e)| (e.name.key(), i))
                .collect()
        })
    }

    /// A texture of the archive as pixels: BMP (pure magenta is transparent, as in the client) or TGA.
    fn texture_image(&self, entry: usize) -> Option<RgbaImage> {
        let e = self.arc.entry(entry).ok()?;
        if e.size > MAX_PREVIEW_BYTES {
            return None;
        }
        let data = self.arc.read(entry).ok()?.data;
        let lower = self.arc.entries()[entry]
            .name
            .as_bytes()
            .to_ascii_lowercase();
        if lower.ends_with(b".tga") {
            rgrf_formats::tga::parse(&data).ok()
        } else {
            let mut img = rgrf_formats::bmp::parse(&data).ok()?;
            for p in img.pixels.as_chunks_mut::<4>().0 {
                if p[..3] == [255, 0, 255] {
                    p[3] = 0;
                }
            }
            Some(img)
        }
    }

    /// A map in 3D: the ground of a GND, and for an RSW also its models, water and light.
    fn preview_scene(&self, index: usize, data: &[u8], is_world: bool) -> Result<Preview, String> {
        let keys = self.key_index();
        let mut pool = TexturePool::default();
        let mut info = Vec::new();
        let (world, ground_bytes);
        if is_world {
            let r = Rsw::parse(data).map_err(|e| e.to_string())?;
            let gnd_name = {
                let mut n = b"data\\".to_vec();
                n.extend(rsw::text(&r.ground).chars().map(|c| c as u8));
                n
            };
            let gi = keys
                .get(&GrfPath::new(gnd_name.clone()).key())
                .copied()
                .ok_or_else(|| {
                    format!(
                        "the ground file {} is not in this archive",
                        self.shown_bytes(&gnd_name)
                    )
                })?;
            ground_bytes = self.read(gi)?;
            world = Some(r);
        } else {
            ground_bytes = data.to_vec();
            world = None;
        }
        let g = gnd_map::parse(&ground_bytes).map_err(|e| e.to_string())?;
        let _ = index;

        // textures of the ground, then of the models, in one pool
        let tex_of: Vec<usize> = g
            .textures
            .iter()
            .map(|name| pool.get(self, keys, b"data\\texture\\", name))
            .collect();
        let mut tiles = Vec::with_capacity(g.tiles.len() * 36);
        for t in &g.tiles {
            for v in t.uv {
                tiles.extend(v.to_le_bytes());
            }
            let tex = tex_of.get(usize::from(t.texture)).copied().unwrap_or(0) as u16;
            tiles.extend(tex.to_le_bytes());
            tiles.extend(t.lightmap.to_le_bytes());
        }
        let mut cubes = Vec::with_capacity(g.cubes.len() * 28);
        for c in &g.cubes {
            for v in c.height {
                cubes.extend(v.to_le_bytes());
            }
            for v in [c.up, c.front, c.right] {
                cubes.extend(v.to_le_bytes());
            }
        }
        info.push(note!(
            "pvw-n-gnd",
            "version": g.version.to_string(), "w": g.width, "h": g.height,
            "tiles": g.tiles.len(), "lightmaps": g.lightmap_count, "textures": g.textures.len()
        ));
        let ground = GroundOut {
            width: g.width,
            height: g.height,
            zoom: g.zoom,
            tiles: b64(&tiles),
            cubes: b64(&cubes),
            lightmap_count: g.lightmap_count,
            lightmaps: b64(&g.lightmaps),
        };

        let mut models: Vec<ModelDefOut> = Vec::new();
        let mut instances = Vec::new();
        let mut light = LightOut {
            longitude: 45,
            latitude: 45,
            diffuse: [1.0; 3],
            ambient: [0.3; 3],
            intensity: 1.0,
        };
        let mut water = None;
        if let Some(r) = &world {
            if let Some(l) = &r.light {
                light = LightOut {
                    longitude: l.longitude,
                    latitude: l.latitude,
                    diffuse: l.diffuse,
                    ambient: l.ambient,
                    intensity: l.intensity,
                };
            }
            if let Some(w) = &r.water {
                // data\texture\워터\water<kind><00..31>.jpg; the cycle ends at the first missing picture
                let dir = encoding_rs::EUC_KR
                    .encode("data\\texture\\\u{c6cc}\u{d130}\\")
                    .0
                    .into_owned();
                let kind = w.kind.clamp(0, 9);
                let mut frames = Vec::new();
                for n in 0..32 {
                    let path =
                        [dir.clone(), format!("water{kind}{n:02}.jpg").into_bytes()].concat();
                    let Some(d) = keys
                        .get(&GrfPath::new(path).key())
                        .and_then(|&i| self.arc.read(i).ok())
                    else {
                        break;
                    };
                    frames.push(b64(&d.data));
                }
                // assumption: a cycle step is `texture_cycling` ticks of 1/60 s (default 3), not measured
                let ticks = if w.texture_cycling > 0 {
                    w.texture_cycling
                } else {
                    3
                };
                // files before RSW 1.8 store no waves: assumption (not measured) of the usual
                // values 1.0, 2.0 and 50.0
                let stored = w.wave_height != 0.0 || w.wave_speed != 0.0 || w.wave_pitch != 0.0;
                let (height, speed, pitch) = if stored {
                    (w.wave_height, w.wave_speed, w.wave_pitch)
                } else {
                    (1.0, 2.0, 50.0)
                };
                water = Some(WaterOut {
                    level: w.level / 5.0,
                    frames,
                    frame_ms: ticks as f32 * 1000.0 / 60.0,
                    wave_height: height / 5.0,
                    wave_speed: speed,
                    wave_pitch: pitch,
                });
            }
            let (m, l, s_, e) = r.object_counts();
            info.push(note!(
                "pvw-n-rsw",
                "version": r.version.to_string(), "ground": self.shown(&rsw::text(&r.ground)),
                "models": m, "lights": l, "sounds": s_, "effects": e
            ));
            let mut by_model: HashMap<Vec<u8>, Option<usize>> = HashMap::new();
            let mut missing: Vec<String> = Vec::new();
            for o in &r.objects {
                let rsw::Object::Model(mo) = o else { continue };
                let name: Vec<u8> = mo
                    .model_name
                    .iter()
                    .take_while(|&&b| b != 0)
                    .copied()
                    .collect();
                let key = GrfPath::new([b"data\\model\\".as_slice(), &name].concat()).key();
                let def = *by_model.entry(key.clone()).or_insert_with(|| {
                    let ei = keys.get(&key).copied();
                    let loaded = ei
                        .and_then(|ei| self.read(ei).ok())
                        .and_then(|d| rsm_model::parse(&d).ok());
                    let Some(model) = loaded else {
                        missing.push(self.shown_bytes(&name));
                        return None;
                    };
                    let f32s = |v: &[f32]| {
                        b64(&v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>())
                    };
                    let duration = rsm_model::duration_ms(&model);
                    if duration > 0.0 {
                        models.push(animated_def(self, &model, duration, &mut pool, keys, &f32s));
                        return Some(models.len() - 1);
                    }
                    let posed = rsm_model::static_pose(&model);
                    // the model's space has y down with the model standing on y = 0
                    let flip_y = |v: &[f32]| -> Vec<f32> {
                        v.as_chunks::<3>()
                            .0
                            .iter()
                            .flat_map(|c| [c[0], -c[1], c[2]])
                            .collect()
                    };
                    let parts = posed
                        .parts
                        .iter()
                        .map(|p| ModelPartOut {
                            texture: pool.get(self, keys, b"data\\texture\\", &p.texture),
                            positions: f32s(&flip_y(&p.positions)),
                            normals: f32s(&flip_y(&p.normals)),
                            uvs: f32s(&p.uvs),
                            nodes: None,
                            uv_anim: None,
                        })
                        .collect();
                    models.push(ModelDefOut { parts, anim: None });
                    Some(models.len() - 1)
                });
                let Some(model) = def else { continue };
                let place = rsm_model::instance_matrix(
                    mo.position.map(|v| v / 5.0),
                    [g.width as f32, 0.0, g.height as f32],
                    mo.rotation,
                    mo.scale.map(|v| v / 5.0),
                );
                // to the viewer's axes: y up and z towards the south
                let flip = [
                    1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0,
                    1.0,
                ];
                instances.push(InstanceOut {
                    model,
                    matrix: rsm_model::multiply(&flip, &place).to_vec(),
                    // assumption: type 0 stands still, other types run at `animation_speed`
                    speed: if mo.animation_type == 0 {
                        0.0
                    } else {
                        mo.animation_speed
                    },
                });
            }
            info.push(note!(
                "pvw-n-instances",
                "instances": instances.len(), "models": models.len()
            ));
            if !missing.is_empty() {
                info.push(note!(
                    "pvw-n-models-missing",
                    "n": missing.len(),
                    "names": missing.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
                ));
            }
        }
        let missing_tex = pool.out.iter().filter(|t| t.png.is_none()).count();
        info.push(note!("pvw-n-textures", "n": pool.out.len()));
        if missing_tex > 0 {
            info.push(note!("pvw-n-textures-missing", "n": missing_tex));
        }
        Ok(Preview::Scene {
            textures: pool.out,
            ground: Some(ground),
            models,
            instances,
            light,
            water,
            info,
        })
    }

    /// An RSM model for the 3D view: node geometry plus the node matrices sampled over the animation.
    fn preview_model(&self, data: &[u8]) -> Result<Preview, String> {
        let model = rsm_model::parse(data).map_err(|e| e.to_string())?;
        let posed = rsm_model::static_pose(&model);
        let keys = self.key_index();
        let f32s = |v: &[f32]| b64(&v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>());
        let mut pool = TexturePool::default();
        let duration = rsm_model::duration_ms(&model);
        let samples = sample_count(duration);
        let view = rsm_model::view_matrix(&model);
        let mut nodes = Vec::new();
        for ni in 0..model.nodes.len() {
            let parts = rsm_model::node_parts(&model, ni)
                .iter()
                .map(|p| ModelPartOut {
                    texture: pool.get(self, keys, b"data\\texture\\", &p.texture),
                    positions: f32s(&p.positions),
                    normals: f32s(&p.normals),
                    uvs: f32s(&p.uvs),
                    nodes: None,
                    uv_anim: uv_samples(&model, ni, &p.texture, duration, samples)
                        .map(|(_, v)| f32s(&v)),
                })
                .collect();
            let mut matrices: Vec<f32> = Vec::with_capacity(samples * 16);
            for k in 0..samples {
                let t = duration * k as f32 / samples as f32;
                let world = rsm_model::node_matrices_at(&model, t);
                matrices.extend(rsm_model::multiply(&view, &world[ni]));
            }
            nodes.push(ModelNodeOut {
                parts,
                matrices: f32s(&matrices),
            });
        }
        let missing = pool
            .out
            .iter()
            .filter(|t| t.png.is_none() && !t.name.is_empty())
            .count();
        let mut info = vec![note!(
            "pvw-n-rsm",
            "version": model.version.to_string(), "nodes": model.nodes.len(),
            "triangles": posed.triangles, "textures": pool.out.len()
        )];
        if missing > 0 {
            info.push(note!("pvw-n-textures-missing", "n": missing));
        }
        if duration > 0.0 {
            info.push(note!("pvw-n-rsm-loop", "secs": format!("{:.1}", duration / 1000.0)));
        }
        for n in &model.nodes {
            let moves = n.rot_keys.len() > 1 || n.pos_keys.len() > 1 || n.scale_keys.len() > 1;
            info.push(note!(
                if moves { "pvw-n-node-moves" } else { "pvw-n-node" },
                "name": self.shown_bytes(&n.name),
                "v": n.vertices.len(), "f": n.faces.len()
            ));
        }
        Ok(Preview::Model {
            nodes,
            textures: pool.out,
            duration_ms: duration,
            samples,
            min: posed.min,
            max: posed.max,
            alpha: posed.alpha,
            triangles: posed.triangles,
            info,
        })
    }

    /// An STR effect as an animated GIF; its textures come from the folder of the STR file, else
    /// from `data\texture\effect`.
    fn preview_str(&self, index: usize, data: &[u8]) -> Result<Preview, String> {
        let s = str_effect::parse(data).map_err(|e| e.to_string())?;
        let entry_name = self.arc.entries()[index].name.as_bytes().to_vec();
        let dir_len = entry_name
            .iter()
            .rposition(|&b| b == b'\\')
            .map_or(0, |p| p + 1);
        let keys = self.key_index();
        let mut textures = str_effect::Textures::new();
        let mut missing: Vec<String> = Vec::new();
        for name in str_effect::texture_names(&s) {
            let mut near = entry_name[..dir_len].to_vec();
            near.extend(&name);
            let mut shared = b"data\\texture\\effect\\".to_vec();
            shared.extend(&name);
            let found = [near, shared]
                .iter()
                .find_map(|p| keys.get(&GrfPath::new(p.clone()).key()).copied());
            let image = found.and_then(|ti| self.texture_image(ti));
            match image {
                Some(img) => {
                    textures.insert(name, img);
                }
                None => missing.push(self.shown_bytes(&name)),
            }
        }
        let total = s.last_frame as usize + 1;
        let step = total.div_ceil(MAX_EFFECT_FRAMES).max(1);
        let canvases: Vec<_> = (0..total)
            .step_by(step)
            .map(|f| str_effect::render(&s, f as f32, &textures, EFFECT_SCALE))
            .collect();
        let fps = s.fps.max(1);
        let delay = ((100.0 * step as f32 / fps as f32).round().max(2.0)) as u16;
        let gif =
            encode_gif(&canvases, &vec![delay; canvases.len()], 1).map_err(|e| e.to_string())?;
        let mut info = vec![note!(
            "pvw-n-str",
            "fps": fps, "frames": total, "layers": s.layers.len(),
            "textures": textures.len() + missing.len()
        )];
        if step > 1 {
            info.push(note!("pvw-n-str-step", "step": step));
        }
        if !missing.is_empty() {
            info.push(note!("pvw-n-str-missing", "n": missing.len()));
            info.extend(
                missing
                    .iter()
                    .take(12)
                    .map(|m| Note::Plain(format!("  {m}"))),
            );
        }
        info.push(note!("pvw-n-str-simple"));
        Ok(Preview::Effect {
            gif: b64(&gif),
            width: canvases[0].width,
            height: canvases[0].height,
            frames: canvases.len(),
            fps,
            info,
        })
    }

    fn shown_bytes(&self, bytes: &[u8]) -> String {
        GrfPath::new(bytes.to_vec()).display(self.mode)
    }

    /// A name read as one char per byte, shown in the chosen name mode (Korean by default).
    fn shown(&self, latin1: &str) -> String {
        let bytes: Option<Vec<u8>> = latin1
            .chars()
            .map(|c| u8::try_from(u32::from(c)).ok())
            .collect();
        bytes.map_or_else(|| latin1.to_owned(), |b| GrfPath::new(b).display(self.mode))
    }

    fn preview_spr(&self, data: &[u8]) -> Result<Preview, String> {
        let spr = Spr::parse(data).map_err(|e| e.to_string())?;
        let frames = spr.frames_rgba();
        let mut info = vec![note!(
            "pvw-n-spr",
            "version": spr.version.to_string(), "pal": spr.indexed.len(), "rgba": spr.rgba.len()
        )];
        info.extend(
            spr.warnings
                .iter()
                .map(|w| note!("pvw-n-warning", "text": w)),
        );
        if frames.len() > MAX_SPR_FRAMES {
            info.push(note!("pvw-n-spr-limit", "shown": MAX_SPR_FRAMES, "total": frames.len()));
        }
        let out = frames
            .iter()
            .take(MAX_SPR_FRAMES)
            .enumerate()
            .filter_map(|(i, f)| png_frame(f, format!("{i}: {} x {}", f.width, f.height).into()))
            .collect();
        Ok(Preview::Frames { frames: out, info })
    }

    fn preview_act(&self, index: usize, data: &[u8], action: usize) -> Result<Preview, String> {
        let act = Act::parse(data).map_err(|e| e.to_string())?;
        let mut info = vec![note!(
            "pvw-n-act",
            "version": act.version.to_string(), "actions": act.actions.len(),
            "frames": act.actions.iter().map(|a| a.frames.len()).sum::<usize>(),
            "layers": act.layers().count()
        )];
        let Some(si) = self.sibling(index, "spr") else {
            return Ok(Preview::Info {
                title: "Animation".into(),
                lines: {
                    info.push(note!("pvw-n-act-nospr"));
                    info
                },
            });
        };
        let spr = Spr::parse(&self.read_working(si)?).map_err(|e| e.to_string())?;
        let actions = act.actions.len();
        let action = action.min(actions.saturating_sub(1));
        let Some(a) = act.actions.get(action) else {
            return Ok(Preview::Message {
                text: note!("pvw-n-act-none"),
            });
        };
        let Some(bounds) =
            bounds_of(&act, &spr, action, 0..a.frames.len()).map_err(|e| e.to_string())?
        else {
            return Ok(Preview::Message {
                text: note!("pvw-n-act-empty", "action": action),
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

    /// Extracts the map of entry `index` (its RSW, GAT or GND) with every file it needs, see
    /// [`rgrf_formats::map_files`]. Files the map names but the archive lacks are listed.
    pub fn extract_map(
        &self,
        index: usize,
        dest: &Path,
        rename: bool,
        skip: &[String],
    ) -> Result<MapExtractSummary, String> {
        for p in skip {
            if !["models", "water", "sounds", "minimap", "effects"].contains(&p.as_str()) {
                return Err(err!("err-map-part", "part": p));
            }
        }
        struct Src<'a>(&'a Session);
        impl rgrf_formats::map_files::MapSource for Src<'_> {
            fn read(&self, path: &[u8]) -> Option<Vec<u8>> {
                let &i = self.0.key_index().get(&GrfPath::new(path.to_vec()).key())?;
                self.0.read(i).ok()
            }
            fn exists(&self, path: &[u8]) -> bool {
                self.0
                    .key_index()
                    .contains_key(&GrfPath::new(path.to_vec()).key())
            }
        }
        let name = self
            .arc
            .entry(index)
            .map_err(|e| e.to_string())?
            .name
            .display(NameMode::Cp1252);
        let mut files = rgrf_formats::map_files::resolve(&name, &Src(self))?;
        let before = files.len();
        files.retain(|f| !map_part(f.kind).is_some_and(|p| skip.iter().any(|s| s == p)));
        let skipped = before - files.len();
        let keys = self.key_index();
        let mut indices = Vec::new();
        let mut missing = Vec::new();
        for f in &files {
            match keys.get(&GrfPath::new(f.path.clone()).key()) {
                Some(&i) => indices.push(i),
                None => missing.push(self.shown_bytes(&f.path)),
            }
        }
        let problems = files
            .iter()
            .filter_map(|f| {
                f.problem
                    .as_ref()
                    .map(|p| format!("{}: {p}", self.shown_bytes(&f.path)))
            })
            .collect();
        let rep = self.extract(&indices, dest, rename)?;
        Ok(MapExtractSummary {
            map: rgrf_formats::map_files::map_base(&name).to_string(),
            files: rep.files,
            damaged: rep.damaged,
            missing,
            problems,
            dest: rep.dest,
            skipped,
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

    /// The number of cells of a type in a GAT preview, from its info lines.
    fn gat_cells(info: &[Note], kind: i64) -> Option<u64> {
        info.iter().find_map(|l| match l {
            Note::Key {
                key: "pvw-n-gat-type",
                args,
            } if args["k"] == kind => args["n"].as_u64(),
            _ => None,
        })
    }
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
    fn map_cells_are_staged_and_the_preview_shows_them() {
        let (dir, grf) = sample("gatedit");
        let mut s = Session::open(&grf).unwrap();
        let i = find(&s, "map.gat");
        let Preview::Map {
            gat: Some(meta), ..
        } = s.preview(i, None).unwrap()
        else {
            panic!("a GAT preview carries its size");
        };
        assert_eq!((meta.cells_x, meta.cells_y, meta.scale), (2, 2, 4));
        assert_eq!(meta.types.len(), 7);
        // the whole map becomes type 1; every cell that was not 1 changes (types were 0, 1, 3, 5)
        let edit = GatEdit {
            x0: 0,
            y0: 0,
            x1: 1,
            y1: 1,
            kind: Some(1),
            height: None,
        };
        assert_eq!(s.stage_gat_edit(i, &edit).unwrap(), 3);
        assert_eq!(s.pending().len(), 1);
        let Preview::Map { info, .. } = s.preview(i, None).unwrap() else {
            panic!()
        };
        assert_eq!(gat_cells(&info, 1), Some(4), "{info:?}");
        // saved as a new archive and opened again, the map has the new cells
        let out = dir.join("edited.grf");
        let r = save_edited(&s.edit_plan().unwrap(), &out, false, &Control::default()).unwrap();
        assert!(r.verified);
        let mut again = Session::open(&out).unwrap();
        let j = find(&again, "map.gat");
        let Preview::Map { info, .. } = again.preview(j, None).unwrap() else {
            panic!()
        };
        assert_eq!(gat_cells(&info, 1), Some(4), "{info:?}");
        // and the original archive is untouched
        let orig = Session::open(&grf).unwrap();
        let Preview::Map { info, .. } = orig.preview(find(&orig, "map.gat"), None).unwrap() else {
            panic!()
        };
        assert!(gat_cells(&info, 5).is_some(), "{info:?}");
        let _ = &mut again;
        // wrong input stages nothing
        let bad = GatEdit {
            kind: Some(9),
            ..edit.clone()
        };
        assert!(s.stage_gat_edit(i, &bad).is_err());
        assert_eq!(s.pending().len(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn lists_are_filtered_sorted_and_shown_as_a_tree() {
        let (dir, grf) = sample("views");
        let mut s = Session::open(&grf).unwrap();
        let spec = |f: &dyn Fn(&mut ViewSpec)| {
            let mut v = ViewSpec {
                layout: "flat".into(),
                ..ViewSpec::default()
            };
            f(&mut v);
            v
        };
        // sorting by size puts the smallest first and descending reverses it
        let up = s.set_view(0, spec(&|v| v.sort = "size".into())).unwrap();
        assert_eq!((up.rows, up.matches), (7, 7));
        let sizes: Vec<u32> = s.page_view(0, 0, 10).iter().map(|r| r.size).collect();
        assert!(sizes.windows(2).all(|w| w[0] <= w[1]), "{sizes:?}");
        s.set_view(0, spec(&|v| (v.sort = "size".into(), v.desc = true).0))
            .unwrap();
        let sizes: Vec<u32> = s.page_view(0, 0, 10).iter().map(|r| r.size).collect();
        assert!(sizes.windows(2).all(|w| w[0] >= w[1]), "{sizes:?}");
        // a type filter keeps only that kind; "all in the list" follows it
        let t = s
            .set_view(0, spec(&|v| v.kinds = vec!["text".into()]))
            .unwrap();
        assert_eq!(t.matches, 2);
        assert_eq!(s.filtered().len(), 2);
        assert!(s.page_view(0, 0, 10).iter().all(|r| r.kind == "text"));
        // two lists are independent
        let both = s.set_view(1, spec(&|_| {})).unwrap();
        assert_eq!(both.rows, 7);
        assert_eq!(s.filtered().len(), 2);

        // a tree: folders first, files below open folders, counts include subfolders
        let tree = |q: &str, auto: bool| ViewSpec {
            query: q.into(),
            layout: "tree".into(),
            auto_expand: auto,
            ..ViewSpec::default()
        };
        let closed = s.set_view(0, tree("", false)).unwrap();
        let rows = s.page_view(0, 0, 100);
        assert_eq!(rows.len(), closed.rows);
        assert!(
            rows.iter()
                .filter(|r| r.folder)
                .all(|r| r.index == FOLDER_ROW && !r.expanded)
        );
        let top: Vec<&Row> = rows.iter().filter(|r| r.depth == 0).collect();
        let files_top = top.iter().filter(|r| !r.folder).count();
        let counted: usize = top.iter().filter(|r| r.folder).map(|r| r.count).sum();
        assert_eq!(counted + files_top, 7, "every entry is in the tree once");
        // opening everything shows all seven files
        s.tree_set_all(0, true).unwrap();
        let all = s.page_view(0, 0, 100);
        assert_eq!(all.iter().filter(|r| !r.folder).count(), 7);
        // a search opens the folders by itself
        let found = s.set_view(0, tree("*.txt", true)).unwrap();
        assert_eq!(found.matches, 2);
        assert_eq!(
            s.page_view(0, 0, 100).iter().filter(|r| !r.folder).count(),
            2
        );
        // reveal: collapse, then open the way to one entry
        s.set_view(0, tree("", true)).unwrap();
        s.tree_set_all(0, false).unwrap();
        let target = find(&s, ".txt");
        let at = s.locate(0, Some(target), None).unwrap();
        let line = at.line.unwrap();
        assert_eq!(s.page_view(0, line, 1)[0].index, target);
        assert!(at.rows > line);
        // in a flat list it is the position in the order
        s.set_view(
            0,
            ViewSpec {
                layout: "flat".into(),
                sort: "size".into(),
                ..ViewSpec::default()
            },
        )
        .unwrap();
        let flat = s.locate(0, Some(target), None).unwrap();
        assert_eq!(s.page_view(0, flat.line.unwrap(), 1)[0].index, target);
        s.set_view(0, tree("", true)).unwrap();
        s.tree_set_all(0, false).unwrap();
        // a folder's entries, and a folder toggle
        let folder = s
            .page_view(0, 0, 100)
            .into_iter()
            .find(|r| r.folder)
            .unwrap();
        let below = s.folder_entries(0, &folder.name).unwrap();
        assert_eq!(below.len(), folder.count);
        let before = s.page_view(0, 0, 500).len();
        let after = s.tree_toggle(0, &folder.name).unwrap();
        assert_ne!(before, after);
        // the folder pane lists folders only
        s.set_view(
            1,
            ViewSpec {
                layout: "folders".into(),
                auto_expand: true,
                ..ViewSpec::default()
            },
        )
        .unwrap();
        assert!(s.page_view(1, 0, 100).iter().all(|r| r.folder));
        assert!(s.set_view(2, spec(&|_| {})).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn open_search_and_page() {
        let (dir, grf) = sample("open");
        let mut s = Session::open(&grf).unwrap();
        let info = s.info();
        assert_eq!(
            (info.format.as_str(), info.entries, info.files),
            ("GRF 0x200", 7, 7)
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
    fn thumbnails_of_pictures_and_sprites() {
        let (dir, grf) = sample("thumbs");
        let s = Session::open(&grf).unwrap();
        let ids = [
            find(&s, "pic.bmp"),
            find(&s, "a.spr"),
            find(&s, "readme.txt"),
            9999,
        ];
        let t = s.thumbnails(&ids, 64);
        assert_eq!(t.len(), 4);
        // the sample "BMP" is only a header: a bad picture is an error text, not a failure
        assert!(t[0].data.is_none() && t[0].error.is_some());
        assert!(t[1].data.is_some() && t[1].mime == "image/png");
        assert!(t[2].data.is_none() && t[2].error.is_some());
        assert_eq!(t[3].error.as_deref(), Some("no such entry"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn finds_bytes_and_wraps_round() {
        let (dir, grf) = sample("hexfind");
        let s = Session::open(&grf).unwrap();
        // blob.bin holds the bytes 0 to 99
        let blob = find(&s, "blob.bin");
        assert_eq!(s.hex_find(blob, &[40, 41, 42], 0).unwrap(), Some(40));
        assert_eq!(s.hex_find(blob, &[40, 41, 42], 40).unwrap(), Some(40));
        // after the only hit the search wraps round to it
        assert_eq!(s.hex_find(blob, &[40, 41, 42], 41).unwrap(), Some(40));
        assert_eq!(s.hex_find(blob, &[99], 50).unwrap(), Some(99));
        assert_eq!(s.hex_find(blob, &[200, 201], 0).unwrap(), None);
        assert!(s.hex_find(blob, &[], 0).is_err());
        assert!(s.hex_find(blob, &[0; 257], 0).is_err());
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
                assert!(
                    matches!(&info[0], Note::Key { key: "pvw-n-spr", args } if args["version"] == "2.1")
                );
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
                assert!(gat_cells(&info, 1).is_some(), "{info:?}");
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
    fn an_encrypted_archive_opens_with_its_key_and_is_saved_encrypted() {
        let key = rgrf_core::grfcrypt::key_from_password("secret pw").unwrap();
        let dir = tmp("crypt");
        fs::create_dir_all(dir.join("src/data")).unwrap();
        fs::write(dir.join("src/data/a.txt"), "hello hello hello hello\n").unwrap();
        fs::write(dir.join("src/data/b.txt"), "second file second file\n").unwrap();
        let grf = dir.join("e.grf");
        build(
            &dir.join("src"),
            &grf,
            &BuildOptions {
                encrypt: Some(key),
                ..BuildOptions::default()
            },
            &Control::default(),
        )
        .unwrap();
        // without the key the archive opens, says so, and the text cannot be shown
        let s = Session::open(&grf).unwrap();
        let info = s.info();
        let e = info.encryption.as_ref().unwrap();
        assert!(e.key_needed && e.encrypted_entries == 2, "{info:?}");
        let a = find(&s, "a.txt");
        assert!(!matches!(
            s.preview(a, None).unwrap(),
            Preview::Text { text, .. } if text.contains("hello")
        ));
        // a wrong key is refused
        let wrong = rgrf_core::grfcrypt::key_from_password("wrong pw").unwrap();
        assert!(Session::open_with(&grf, Some(&wrong)).is_err());
        // with the key everything reads, and a saved edit stays encrypted
        let mut s = Session::open_with(&grf, Some(&key)).unwrap();
        assert!(!s.info().encryption.unwrap().key_needed);
        let a = find(&s, "a.txt");
        assert!(matches!(
            s.preview(a, None).unwrap(),
            Preview::Text { text, .. } if text.contains("hello")
        ));
        let new = dir.join("new.txt");
        fs::write(&new, "changed\n").unwrap();
        s.stage_replace(a, &new).unwrap();
        let out = dir.join("out.grf");
        save_edited(&s.edit_plan().unwrap(), &out, false, &Control::default()).unwrap();
        let o = Archive::open(&out).unwrap();
        assert_eq!(o.encryption().unwrap().encrypted_entries, 2);
        let o = Archive::open_with_key(&out, &key).unwrap();
        let ia = o.find(&GrfPath::new(&b"data\\a.txt"[..])).unwrap();
        assert_eq!(o.read(ia).unwrap().data, b"changed\n");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn edited_text_and_lua_are_staged_in_their_own_format() {
        use rgrf_formats::{luac, luacomp};
        let dir = tmp("textedit");
        let src = dir.join("src/data");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("crlf.txt"), "a\r\nb \u{e9}\r\n").unwrap();
        fs::write(src.join("korean.txt"), [0xb0u8, 0xa1, 0x0a]).unwrap(); // CP949 "가"
        let cp949 =
            |run: &str| GrfPath::from_display(run, NameMode::Cp949).map(|p| p.as_bytes().to_vec());
        let lua = "x = \"\u{ac00}\"\nfunction f(a)\n  return a + 1\nend\n";
        let mut chunk =
            luacomp::compile_text(lua, b"@t.lua", luacomp::TextEncoding::Auto, &cp949).unwrap();
        fs::write(src.join("t.lub"), luac::dump32(&chunk)).unwrap();
        luacomp::strip(&mut chunk);
        fs::write(src.join("s.lub"), luac::dump32(&chunk)).unwrap();
        let chunk50 = rgrf_formats::luacomp50::compile_text(
            lua,
            b"@t50.lua",
            luacomp::TextEncoding::Auto,
            &cp949,
        )
        .unwrap();
        fs::write(src.join("t50.lub"), rgrf_formats::luac50::dump32(&chunk50)).unwrap();
        let out = dir.join("t.grf");
        build(
            &dir.join("src"),
            &out,
            &BuildOptions::default(),
            &Control::default(),
        )
        .unwrap();
        let mut s = Session::open(&out).unwrap();
        let (t, st, crlf, kr) = (
            find(&s, "t.lub"),
            find(&s, "s.lub"),
            find(&s, "crlf.txt"),
            find(&s, "korean.txt"),
        );

        let doc = s.text_for_edit(t).unwrap();
        assert!(doc.complete && doc.format == "Lua 5.1 bytecode", "{doc:?}");
        assert!(
            doc.text.contains("f = function(a)") && doc.text.contains('\u{ac00}'),
            "{}",
            doc.text
        );
        s.stage_text_edit(t, &doc.text.replace("a + 1", "a + 2"))
            .unwrap();
        let (_, bytes, _) = s.working_bytes(t).unwrap();
        let new = luac::parse(&bytes).unwrap();
        assert_eq!(new.main.source, b"@t.lua", "debug information is kept");
        assert!(
            rgrf_formats::luadec::decompile(&new, &|b| String::from_utf8_lossy(b).into_owned())
                .contains("a + 2")
        );

        // Lua 5.0 is compiled as 5.0
        let t50 = find(&s, "t50.lub");
        let doc = s.text_for_edit(t50).unwrap();
        assert_eq!(doc.format, "Lua 5.0 bytecode");
        s.stage_text_edit(t50, &doc.text.replace("a + 1", "a + 3"))
            .unwrap();
        let (_, bytes, _) = s.working_bytes(t50).unwrap();
        assert_eq!(bytes[4], 0x50, "still Lua 5.0 bytecode");
        let new = luac::parse_any(&bytes).unwrap();
        assert_eq!(new.main.source, b"@t50.lua");
        assert!(
            rgrf_formats::luadec::decompile(&new, &|b| String::from_utf8_lossy(b).into_owned())
                .contains("a + 3")
        );

        // a stripped file stays stripped
        let doc = s.text_for_edit(st).unwrap();
        s.stage_text_edit(st, &doc.text).unwrap();
        let (_, bytes, _) = s.working_bytes(st).unwrap();
        let new = luac::parse(&bytes).unwrap();
        assert!(new.main.source.is_empty() && new.main.lines.is_empty());

        // a syntax error changes nothing and names the line
        let before = s.pending().len();
        let err = s.stage_text_edit(t, "x = 1\ny = = 2\n").unwrap_err();
        assert!(err.contains(":2:"), "{err}");
        assert_eq!(s.pending().len(), before);

        // plain text keeps its line endings and encoding
        s.stage_text_edit(crlf, "one\ntwo \u{e9}\n").unwrap();
        assert_eq!(
            s.working_bytes(crlf).unwrap().1,
            "one\r\ntwo \u{e9}\r\n".as_bytes()
        );
        s.stage_text_edit(kr, "\u{b098}\n").unwrap();
        assert_eq!(s.working_bytes(kr).unwrap().1, [0xb3, 0xaa, 0x0a]);
        assert!(s.stage_text_edit(kr, "\u{1f600}\n").is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn edits_inside_a_sprite_and_an_animation() {
        let (dir, grf) = sample("inner");
        let mut s = Session::open(&grf).unwrap();
        let find = |s: &mut Session, name: &str| {
            assert_eq!(s.search(name), 1);
            s.page(0, 1)[0].index
        };
        let (spr_i, act_i) = (find(&mut s, "a.spr"), find(&mut s, "a.act"));
        let red = RgbaImage {
            width: 3,
            height: 1,
            pixels: vec![255, 0, 0, 255, 250, 5, 5, 255, 0, 0, 0, 0],
        };
        let png = dir.join("red.png");
        fs::write(&png, rgba_to_png(&red).unwrap()).unwrap();
        s.stage_spr_image(spr_i, 0, &png).unwrap();
        // the preview shows the staged content, not the archive's
        match s.preview(spr_i, None).unwrap() {
            Preview::Frames { frames, .. } => assert_eq!(frames[0].width, 3),
            other => panic!("{other:?}"),
        }

        assert!(s.stage_spr_image(spr_i, 5, &png).is_err(), "no such image");
        s.stage_act_interval(act_i, 1, 100.0).unwrap();
        assert!(s.stage_act_interval(act_i, 9, 100.0).is_err());
        assert!(s.stage_act_interval(act_i, 0, 0.0).is_err());
        // a second edit of the same entry builds on the first
        s.stage_act_interval(act_i, 0, 50.0).unwrap();
        let rows = s.pending();
        assert_eq!(rows.len(), 2);
        assert!(
            rows[1].file.as_deref().unwrap().contains("action 1"),
            "{rows:?}"
        );
        assert!(
            rows[1].file.as_deref().unwrap().contains("action 0"),
            "{rows:?}"
        );

        let out = dir.join("out.grf");
        save_edited(&s.edit_plan().unwrap(), &out, false, &Control::default()).unwrap();
        let readme = find(&mut s, "readme.txt");
        s.stage_remove(&[readme]).unwrap();
        assert!(matches!(
            s.preview(readme, None).unwrap(),
            Preview::Message { text: Note::Plain(t) } if t.contains("removal")
        ));
        let mut r = Session::open(&out).unwrap();
        let (spr_i, act_i) = (find(&mut r, "a.spr"), find(&mut r, "a.act"));
        let spr = Spr::parse(&r.read(spr_i).unwrap()).unwrap();
        // the PNG's colours are mapped to the sprite's palette: red is the sprite's colour 1, transparent becomes 0
        assert_eq!(spr.indexed[0].width, 3);
        assert_eq!(spr.indexed[0].pixels, vec![1, 1, 0]);
        let act = Act::parse(&r.read(act_i).unwrap()).unwrap();
        assert_eq!(
            (act.actions[0].interval, act.actions[1].interval),
            (2.0, 4.0)
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn adds_and_removes_sprite_images_and_sets_the_palette() {
        let (dir, grf) = sample("sprite-ops");
        let mut s = Session::open(&grf).unwrap();
        let find = |s: &mut Session, name: &str| {
            assert_eq!(s.search(name), 1);
            s.page(0, 1)[0].index
        };
        let (spr_i, act_i) = (find(&mut s, "a.spr"), find(&mut s, "a.act"));
        let red = RgbaImage {
            width: 2,
            height: 1,
            pixels: vec![255, 0, 0, 255, 0, 0, 0, 0],
        };
        let png = dir.join("red.png");
        fs::write(&png, rgba_to_png(&red).unwrap()).unwrap();

        s.stage_spr_add(spr_i, &png).unwrap();
        let frames = |s: &Session| match s.preview(spr_i, None).unwrap() {
            Preview::Frames { frames, .. } => frames.len(),
            other => panic!("{other:?}"),
        };
        assert_eq!(frames(&s), 2);

        // removing image 0 also changes the animation next to it, and both are staged
        s.stage_spr_remove(spr_i, 0).unwrap();
        assert_eq!(frames(&s), 1);
        let rows = s.pending();
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert!(rows.iter().any(|r| r.name.ends_with("a.act")));
        let act = Act::parse(&s.read_working(act_i).unwrap()).unwrap();
        assert!(
            act.layers().all(|(_, _, _, l)| l.sprite_index == -1),
            "layers lost their image"
        );
        assert!(s.stage_spr_remove(spr_i, 9).is_err(), "no such image");

        // a palette file: 256 entries of R, G, B, 0
        let mut pal = vec![0u8; 1024];
        pal[4..8].copy_from_slice(&[0, 0, 255, 0]);
        let pal_path = dir.join("blue.pal");
        fs::write(&pal_path, &pal).unwrap();
        s.stage_spr_palette(spr_i, &pal_path).unwrap();
        let spr = Spr::parse(&s.read_working(spr_i).unwrap()).unwrap();
        assert_eq!(&spr.palette.unwrap()[4..8], &[0, 0, 255, 0]);
        assert!(s.stage_spr_palette(spr_i, &png).is_err(), "not a palette");

        let out = dir.join("out.grf");
        save_edited(&s.edit_plan().unwrap(), &out, false, &Control::default()).unwrap();
        let mut r = Session::open(&out).unwrap();
        let i = find(&mut r, "a.spr");
        let spr = Spr::parse(&r.read(i).unwrap()).unwrap();
        assert_eq!((spr.indexed.len(), spr.palette.unwrap()[5]), (1, 0));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn undo_and_redo_take_back_staged_changes() {
        let (dir, grf) = sample("undo");
        let mut s = Session::open(&grf).unwrap();
        let find = |s: &mut Session, name: &str| {
            assert_eq!(s.search(name), 1);
            s.page(0, 1)[0].index
        };
        let (spr_i, act_i) = (find(&mut s, "a.spr"), find(&mut s, "a.act"));
        assert_eq!(s.history(), (false, false));
        assert!(!s.undo() && !s.redo());

        s.stage_act_interval(act_i, 0, 100.0).unwrap();
        s.stage_act_interval(act_i, 1, 50.0).unwrap();
        let png = dir.join("none.png");
        // a failing change is not a step and leaves the list as it was
        assert!(s.stage_spr_image(spr_i, 0, &png).is_err());
        assert_eq!((s.pending().len(), s.history()), (1, (true, false)));

        let interval = |s: &Session, action: usize| {
            Act::parse(&s.read_working(act_i).unwrap()).unwrap().actions[action].interval
        };
        assert_eq!((interval(&s, 0), interval(&s, 1)), (4.0, 2.0));
        assert!(s.undo());
        assert_eq!((interval(&s, 0), interval(&s, 1)), (4.0, 4.0));
        assert_eq!(s.history(), (true, true));
        assert!(s.undo());
        assert!(s.pending().is_empty());
        assert!(s.redo() && s.redo() && !s.redo());
        assert_eq!(interval(&s, 1), 2.0);

        // a new change after an undo drops what could be redone
        assert!(s.undo());
        s.stage_remove(&[spr_i]).unwrap();
        assert_eq!(s.history(), (true, false));
        // discarding everything can be undone, too
        s.discard_pending();
        assert!(s.pending().is_empty());
        assert!(s.undo());
        assert_eq!(s.pending().len(), 2);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stages_edits_and_saves_a_new_archive() {
        let dir = tmp("edit");
        let src = dir.join("src/data");
        fs::create_dir_all(&src).unwrap();
        for (n, c) in [("a.txt", "old a"), ("b.txt", "b"), ("c.txt", "c")] {
            fs::write(src.join(n), c).unwrap();
        }
        let grf = dir.join("in.grf");
        let build_req = BuildRequest {
            src: dir.join("src"),
            dst: grf.clone(),
            method: "zlib".into(),
            format: "0x200".into(),
            dedupe: false,
            overwrite: false,
        };
        build_archive(&build_req, &Control::default()).unwrap();
        let before = fs::read(&grf).unwrap();
        let new_a = dir.join("new_a.txt");
        fs::write(&new_a, "new a").unwrap();

        let mut s = Session::open(&grf).unwrap();
        assert!(s.edit_plan().is_err());
        let idx = |s: &Session, name: &str| {
            (0..s.info().entries)
                .find(|&i| s.page(i, 1)[0].name == name)
                .unwrap()
        };
        s.stage_replace(idx(&s, "data\\a.txt"), &new_a).unwrap();
        s.stage_add("data/sub/d.txt", &new_a).unwrap();
        s.stage_remove(&[idx(&s, "data\\b.txt")]).unwrap();
        // staging the same path again replaces the earlier change
        s.stage_add("DATA\\sub\\d.txt", &new_a).unwrap();
        assert!(s.stage_add("data\\", &new_a).is_err());
        assert!(s.stage_add("data\\x.txt", &dir.join("missing")).is_err());
        let kinds: Vec<_> = s.pending().iter().map(|p| p.kind).collect();
        assert_eq!(kinds, ["replace", "remove", "add"]);

        let out = dir.join("out.grf");
        let r = save_edited(&s.edit_plan().unwrap(), &out, false, &Control::default()).unwrap();
        assert_eq!(
            (r.added, r.replaced, r.removed, r.files, r.verified),
            (1, 1, 1, 3, true)
        );
        let res = Session::open(&out).unwrap();
        let names: Vec<_> = res
            .page(0, 10)
            .into_iter()
            .map(|r| r.name.to_lowercase())
            .collect();
        assert!(names.contains(&"data\\sub\\d.txt".to_string()), "{names:?}");
        assert!(!names.contains(&"data\\b.txt".to_string()));
        assert_eq!(
            fs::read(&grf).unwrap(),
            before,
            "the open archive is unchanged"
        );
        s.discard_pending();
        assert!(s.pending().is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn true_colour_bmps_are_reduced_one_by_one_and_for_the_whole_archive() {
        let dir = tmp("reduce");
        let src = dir.join("src/data");
        fs::create_dir_all(&src).unwrap();
        let picture = |w: u16, h: u16, f: &dyn Fn(u32, u32) -> [u8; 3]| {
            let mut pixels = Vec::new();
            for y in 0..u32::from(h) {
                for x in 0..u32::from(w) {
                    pixels.extend(f(x, y));
                    pixels.push(255);
                }
            }
            rgrf_formats::bmp::write_rgb(&RgbaImage {
                width: w,
                height: h,
                pixels,
            })
        };
        // a smooth gradient (lossy) and 200 colours at random (lossless, still smaller)
        fs::write(
            src.join("grad.bmp"),
            picture(64, 64, &|x, y| [(x * 2) as u8, (y * 2) as u8, 0]),
        )
        .unwrap();
        fs::write(
            src.join("noise.bmp"),
            picture(128, 128, &|x, y| {
                let mut r = u64::from(x * 128 + y + 1);
                for _ in 0..3 {
                    r ^= r << 13;
                    r ^= r >> 7;
                    r ^= r << 17;
                }
                let r = (r % 200) as u8;
                [r, r / 2, 255 - r]
            }),
        )
        .unwrap();
        fs::write(src.join("a.txt"), "text").unwrap();
        let grf = dir.join("a.grf");
        build(
            &dir.join("src"),
            &grf,
            &BuildOptions::default(),
            &Control::default(),
        )
        .unwrap();

        let mut s = Session::open(&grf).unwrap();
        let index = |s: &mut Session, name: &str| {
            assert_eq!(s.search(name), 1);
            s.page(0, 1)[0].index
        };
        let grad = index(&mut s, "grad.bmp");
        match s.preview(grad, None).unwrap() {
            Preview::Image { bits, .. } => assert_eq!(bits, Some(24)),
            other => panic!("{other:?}"),
        }
        let p = s.reduce_preview(grad).unwrap();
        assert!(
            p.psnr.is_some_and(|v| v > 40.0) && p.bytes_after < p.bytes_before,
            "{p:?}"
        );
        s.stage_reduce(grad).unwrap();
        assert_eq!(s.pending().len(), 1);
        assert!(
            s.reduce_preview(grad)
                .unwrap_err()
                .contains("err-already-bits")
        );
        let txt = index(&mut s, "a.txt");
        assert!(s.reduce_preview(txt).is_err());

        let plan = s.plan();
        assert!(reduce_images(&plan, &grf, 40.0, &Control::default()).is_err());
        let out = dir.join("out.grf");
        let r = reduce_images(&plan, &out, 40.0, &Control::default()).unwrap();
        // the staged gradient is not looked at again; the noise is reduced without loss
        assert_eq!(
            (r.matched, r.reduced, r.lossless, r.written, r.verified),
            (1, 1, 1, true, true)
        );
        let a = Archive::open(&out).unwrap();
        for name in ["data\\grad.bmp", "data\\noise.bmp"] {
            let i = a
                .find(&GrfPath::from_display(name, NameMode::Cp1252).unwrap())
                .unwrap();
            assert_eq!(
                rgrf_formats::bmp::info(&a.read(i).unwrap().data)
                    .unwrap()
                    .bits,
                8,
                "{name}"
            );
        }
        let i = a
            .find(&GrfPath::from_display("data\\a.txt", NameMode::Cp1252).unwrap())
            .unwrap();
        assert_eq!(a.read(i).unwrap().data, b"text");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bytes_are_edited_in_place_and_the_header_of_a_video_is_read() {
        let (dir, grf) = sample("hex");
        let mut s = Session::open(&grf).unwrap();
        s.search("blob.bin");
        let i = s.page(0, 1)[0].index;
        let page = s.hex_page(i, 90, 4096).unwrap();
        assert_eq!((page.offset, page.size), (90, 100));
        let bytes = B64.decode(&page.bytes).unwrap();
        assert_eq!(bytes, (90..100u8).collect::<Vec<_>>());
        // an offset past the end is an empty page, not an error
        assert!(
            B64.decode(&s.hex_page(i, 500, 16).unwrap().bytes)
                .unwrap()
                .is_empty()
        );
        s.stage_hex(i, &[(0, 0xaa), (99, 0xbb)]).unwrap();
        assert_eq!(s.pending().len(), 1);
        let now = B64.decode(&s.hex_page(i, 0, 4096).unwrap().bytes).unwrap();
        assert_eq!((now[0], now[1], now[99], now.len()), (0xaa, 1, 0xbb, 100));
        // a bad offset changes nothing, and neither does an empty edit
        assert!(s.stage_hex(i, &[(100, 1)]).is_err());
        assert!(s.stage_hex(i, &[]).is_err());
        assert_eq!(s.pending().len(), 1);
        s.undo();
        assert!(s.pending().is_empty());

        // the header of a real Bink file: 512 x 256, 450 frames, 30 fps
        let mut bik = b"BIKi".to_vec();
        for v in [0x21e9f0u32, 450, 0x5ed8, 450, 512, 256, 10_000_000, 333_333] {
            bik.extend(v.to_le_bytes());
        }
        bik.resize(64, 0);
        let (w, h, frames, fps) = bink_header(&bik).unwrap();
        assert_eq!((w, h, frames), (512, 256, 450));
        assert!((fps - 30.0).abs() < 0.01);
        assert!(bink_header(b"BIKi").is_none() && bink_header(&[0u8; 64]).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pending_changes_survive_as_a_draft_and_come_back() {
        let (dir, grf) = sample("draft");
        let root = dir.join("drafts");
        let new_text = dir.join("new.txt");
        fs::write(&new_text, "replacement").unwrap();
        let find = |s: &mut Session, name: &str| {
            assert_eq!(s.search(name), 1);
            s.page(0, 1)[0].index
        };

        let mut s = Session::open(&grf).unwrap();
        s.set_draft_root(Some(root.clone()));
        assert!(s.info().draft.is_none());
        let (txt, act, blob) = (
            find(&mut s, "readme.txt"),
            find(&mut s, "a.act"),
            find(&mut s, "blob.bin"),
        );
        s.stage_replace(txt, &new_text).unwrap();
        s.stage_act_interval(act, 0, 100.0).unwrap();
        s.stage_remove(&[blob]).unwrap();
        let before = s.pending();
        let edited_act = s.read_working(act).unwrap();
        drop(s); // the window closes or the app crashes

        // the next session offers the draft and does not overwrite it before a decision
        let mut s = Session::open(&grf).unwrap();
        s.set_draft_root(Some(root.clone()));
        let d = s.info().draft.unwrap();
        assert_eq!(
            (d.changes, d.archive_changed, d.missing_files),
            (3, false, 0)
        );
        let other = find(&mut s, "korean.txt");
        s.stage_remove(&[other]).unwrap();
        s.discard_pending();
        assert_eq!(crate::draft::info(&root, &grf).unwrap().changes, 3);
        let r = s.restore_draft().unwrap();
        assert_eq!((r.restored, r.skipped), (3, 0));
        assert!(s.info().draft.is_none());
        assert_eq!(format!("{:?}", s.pending()), format!("{before:?}"));
        assert_eq!(s.read_working(act).unwrap(), edited_act);
        // the restore is one step for undo, and from now on the draft follows the changes
        assert!(s.undo());
        assert!(crate::draft::info(&root, &grf).is_none());
        assert!(s.redo());
        assert_eq!(crate::draft::info(&root, &grf).unwrap().changes, 3);
        let out = dir.join("out.grf");
        let r = save_edited(&s.edit_plan().unwrap(), &out, false, &Control::default()).unwrap();
        assert_eq!((r.replaced, r.removed, r.verified), (2, 1, true));

        // "later": the draft is set aside, this session saves its own, both are offered in turn
        drop(s);
        let mut s = Session::open(&grf).unwrap();
        s.set_draft_root(Some(root.clone()));
        assert_eq!(s.info().draft.unwrap().more, 0);
        s.defer_draft();
        let k = find(&mut s, "korean.txt");
        s.stage_remove(&[k]).unwrap();
        drop(s);
        let mut s = Session::open(&grf).unwrap();
        s.set_draft_root(Some(root.clone()));
        let d = s.info().draft.unwrap();
        assert_eq!((d.changes, d.more), (1, 1));
        assert_eq!(s.restore_draft().unwrap().restored, 1);
        drop(s);
        let mut s = Session::open(&grf).unwrap();
        s.set_draft_root(Some(root.clone()));
        let d = s.info().draft.unwrap();
        assert_eq!(
            (d.changes, d.more),
            (1, 1),
            "the restored one was saved again"
        );
        s.discard_draft();
        drop(s);
        let s = Session::open(&grf);
        let mut s = s.unwrap();
        s.set_draft_root(Some(root.clone()));
        assert_eq!(
            s.info().draft.unwrap().changes,
            3,
            "the one set aside comes next"
        );

        // a draft that is discarded is gone, and the session saves its own changes again
        drop(s);
        let mut s = Session::open(&grf).unwrap();
        s.set_draft_root(Some(root.clone()));
        assert!(s.info().draft.is_some());
        s.discard_draft();
        assert!(crate::draft::info(&root, &grf).is_none());
        let k = find(&mut s, "korean.txt");
        s.stage_remove(&[k]).unwrap();
        assert_eq!(crate::draft::info(&root, &grf).unwrap().changes, 1);
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
