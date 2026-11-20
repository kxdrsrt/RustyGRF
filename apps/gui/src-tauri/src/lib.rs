//! RustyGRF desktop app: thin Tauri commands around [`backend::Session`].

/// An error text the UI translates: a sentinel, then the catalogue key and its values as JSON.
pub fn coded(key: &str, args: serde_json::Value) -> String {
    format!("\u{1}{}", serde_json::json!({ "key": key, "args": args }))
}

macro_rules! err {
    ($key:literal) => {
        $crate::coded($key, serde_json::Value::Null)
    };
    ($key:literal, $($args:tt)+) => {
        $crate::coded($key, serde_json::json!({ $($args)+ }))
    };
}

pub mod act_editor;
pub mod backend;
pub mod draft;
pub mod gat_editor;
pub mod pal_editor;
pub mod saving;
pub mod spr_editor;
pub mod thumb;
pub mod view;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use std::sync::atomic::{AtomicU64, Ordering};

use backend::{
    ArchiveInfo, BuildRequest, BuildSummary, Details, EditSummary, ExtractSummary, MergeRequest,
    MergeSummary, PendingRow, Preview, Row, Session, VerifySummary,
};
use rgrf_core::progress::{CancelToken, Control, Progress};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

type Shared = Arc<Mutex<Option<Session>>>;

/// Every window shows one archive: its session, the archive's path and its long job are kept per
/// window label.
#[derive(Default)]
struct AppState {
    sessions: Mutex<HashMap<String, Shared>>,
    /// The canonical path of the archive open in a window, to find a window that has it already.
    paths: Mutex<HashMap<String, PathBuf>>,
    /// Token of the long job (build, merge, save) running in a window, if any.
    jobs: Mutex<HashMap<String, CancelToken>>,
    /// The archive a new window opens once its page is loaded.
    startup: Mutex<HashMap<String, String>>,
    /// Numbers the labels of new windows.
    counter: AtomicU64,
    /// The window that was in front last: menu commands go to it.
    front: Mutex<Option<String>>,
}

impl AppState {
    fn slot(&self, label: &str) -> Shared {
        let mut map = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(map.entry(label.to_string()).or_default())
    }

    /// The window that has the archive at `path` open, if any.
    fn window_with(&self, path: &std::path::Path) -> Option<String> {
        let canon = canonical(path);
        let paths = self.paths.lock().unwrap_or_else(|e| e.into_inner());
        paths
            .iter()
            .find(|(_, p)| **p == canon)
            .map(|(l, _)| l.clone())
    }

    fn forget(&self, label: &str) {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(label);
        self.paths
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(label);
        if let Some(t) = self
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(label)
        {
            t.cancel();
        }
        self.startup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(label);
    }
}

/// The path in the form used to compare archives (case and `..` resolved by the file system).
fn canonical(path: &std::path::Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

type Res<T> = Result<T, String>;

/// Runs `f` on a worker thread with the session of `window`, so a long job does not freeze it.
async fn with_session<T, F>(state: &State<'_, AppState>, window: &WebviewWindow, f: F) -> Res<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Session) -> Res<T> + Send + 'static,
{
    let shared = state.slot(window.label());
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = shared
            .lock()
            .map_err(|_| "internal error: state is locked".to_string())?;
        let s = guard.as_mut().ok_or_else(|| err!("err-no-archive"))?;
        f(s)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn open_archive(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
    password: Option<String>,
) -> Res<ArchiveInfo> {
    // an archive that is open in another window is shown there instead
    if let Some(other) = state.window_with(std::path::Path::new(&path))
        && other != window.label()
    {
        focus(&app, &other);
        return Err(err!("err-open-elsewhere"));
    }
    let shared = state.slot(window.label());
    let drafts = app.path().app_data_dir().ok().map(|d| d.join("drafts"));
    let label = window.label().to_string();
    let canon = canonical(std::path::Path::new(&path));
    let info = tauri::async_runtime::spawn_blocking(move || {
        let key = password
            .filter(|p| !p.is_empty())
            .map(|p| rgrf_core::grfcrypt::key_from_password(&p).map_err(|e| e.to_string()))
            .transpose()?;
        let mut s = Session::open_with(&PathBuf::from(path), key.as_ref())?;
        s.set_draft_root(drafts);
        let info = s.info();
        *shared
            .lock()
            .map_err(|_| "internal error: state is locked".to_string())? = Some(s);
        Ok::<_, String>(info)
    })
    .await
    .map_err(|e| e.to_string())??;
    state
        .paths
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(label, canon);
    Ok(info)
}

/// Closes the archive of this window; the window stays open and empty.
#[tauri::command]
async fn close_archive(window: WebviewWindow, state: State<'_, AppState>) -> Res<()> {
    let shared = state.slot(window.label());
    state
        .paths
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(window.label());
    tauri::async_runtime::spawn_blocking(move || {
        let taken = shared.lock().map_err(|_| "internal error")?.take();
        drop(taken);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens `path` (or nothing) in a new window. When the archive is open already, its window comes
/// to the front instead. Returns `false` where windows cannot be made (the development server).
#[tauri::command]
fn open_in_window(app: AppHandle, path: Option<String>) -> Res<bool> {
    open_somewhere(&app, path, true).map_err(|e| e.to_string())?;
    Ok(true)
}

/// The page has drawn itself: show the window (it is created hidden, so no blank page flashes).
#[tauri::command]
fn window_ready(window: WebviewWindow) {
    if !window.is_visible().unwrap_or(true) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Remembers the colour scheme the page shows ("light" or "dark", the system's already resolved),
/// so the next window starts in its background colour, and recolours the open windows.
#[tauri::command]
fn remember_theme(app: AppHandle, theme: String) {
    if let Ok(dir) = app.path().app_config_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("theme"), &theme);
    }
    for w in app.webview_windows().values() {
        let _ = w.set_background_color(Some(background_for(&app, w)));
    }
}

/// Opens a web address of the About box in the system browser (only these addresses).
#[tauri::command]
fn open_link(url: String) -> Res<()> {
    const ALLOWED: [&str; 2] = ["https://github.com/", "https://bento.me/"];
    if !ALLOWED.iter().any(|a| url.starts_with(a))
        || url.contains(['"', ' ', '&', '|', '^', '<', '>'])
    {
        return Err("this address is not opened".into());
    }
    backend::open_with_system(std::path::Path::new(&url))
}

const DARK_BG: tauri::window::Color = tauri::window::Color(0x1b, 0x1a, 0x19, 0xff);
const LIGHT_BG: tauri::window::Color = tauri::window::Color(0xf6, 0xf5, 0xf3, 0xff);

/// The scheme the page showed last ("dark" or "light"), if it is known.
fn remembered_dark(app: &AppHandle) -> Option<bool> {
    let chosen = app
        .path()
        .app_config_dir()
        .ok()
        .and_then(|d| std::fs::read_to_string(d.join("theme")).ok())?;
    match chosen.trim() {
        "dark" => Some(true),
        "light" => Some(false),
        _ => None,
    }
}

/// The background of the page (`--bg` in style.css) for the remembered scheme or the system's.
fn background_for(app: &AppHandle, w: &WebviewWindow) -> tauri::window::Color {
    let dark = remembered_dark(app)
        .unwrap_or_else(|| w.theme().map(|t| t == tauri::Theme::Dark).unwrap_or(false));
    if dark { DARK_BG } else { LIGHT_BG }
}

/// The window in front (the last one that had the focus and still exists).
fn front_window(app: &AppHandle) -> Option<WebviewWindow> {
    let windows = app.webview_windows();
    windows
        .values()
        .find(|w| w.is_focused().unwrap_or(false))
        .cloned()
        .or_else(|| {
            let last = app.state::<AppState>().front.lock().ok()?.clone()?;
            windows.get(&last).cloned()
        })
        .or_else(|| windows.values().next().cloned())
}

/// The `rgrf` scheme: `<kind>-<entry>[-<anything>]` (the page adds a number that changes with the
/// archive and its changes, so the browser cache never serves an old picture). The kinds are
/// those of [`backend::asset_response`].
pub fn parse_asset_path(path: &str) -> Option<(String, usize)> {
    let name = path.trim_start_matches('/');
    let mut parts = name.split('-');
    let kind = parts.next()?.to_string();
    let entry = parts.next()?.parse().ok()?;
    Some((kind, entry))
}

/// Answers a request of the `rgrf` scheme from the session of the window that asks.
fn serve_asset(
    app: &AppHandle,
    label: &str,
    path: &str,
) -> Result<(Vec<u8>, &'static str), String> {
    let (kind, entry) = parse_asset_path(path).ok_or("bad asset path")?;
    let shared = app.state::<AppState>().slot(label);
    // the session is only needed to read the bytes; decoding runs without holding it
    let (data, name) = {
        let guard = shared.lock().map_err(|_| "internal error")?;
        guard
            .as_ref()
            .ok_or_else(|| err!("err-no-archive"))?
            .asset_source(entry)?
    };
    backend::asset_response(&kind, data, &name)
}

/// Brings a window to the front.
fn focus(app: &AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Makes a window; it opens `startup` once its page is loaded. When the page's colour scheme is
/// known from the last start, window and webview are created in that background colour and the
/// window shows as soon as the webview starts loading (measured on Windows: a window shown right
/// away is white for its first frames). On the very first start it stays hidden until the page is
/// drawn. Either way it shows after two seconds at the latest.
fn create_window(app: &AppHandle, startup: Option<String>) -> tauri::Result<WebviewWindow> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    let state = app.state::<AppState>();
    let label = if app.get_webview_window("main").is_none() {
        "main".to_string()
    } else {
        format!("w{}", state.counter.fetch_add(1, Ordering::Relaxed) + 1)
    };
    if let Some(p) = startup {
        state
            .startup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(label.clone(), p);
    }
    let known = remembered_dark(app);
    let mut b = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title("RustyGRF")
        .inner_size(1240.0, 800.0)
        .min_inner_size(900.0, 640.0)
        .visible(false);
    if let Some(dark) = known {
        b = b.background_color(if dark { DARK_BG } else { LIGHT_BG });
    }
    // a new window sits a little below and right of the one in front
    if let Some(front) = front_window(app)
        && let (Ok(pos), Ok(scale)) = (front.outer_position(), front.scale_factor())
    {
        let p = pos.to_logical::<f64>(scale);
        b = b.position(p.x + 28.0, p.y + 28.0);
    } else {
        b = b.center();
    }
    let w = b.build()?;
    if known.is_none() {
        let _ = w.set_background_color(Some(background_for(app, &w)));
    } else {
        // shown only now that window and webview exist in the background colour
        let _ = w.show();
        let _ = w.set_focus();
    }
    let late = w.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if !late.is_visible().unwrap_or(true) {
            let _ = late.show();
        }
    });
    Ok(w)
}

/// Shows `path` in a window: the one that has it open, else an empty one (unless `new` asks for a
/// fresh window), else a new one. Without a path a window comes to the front (or a new one).
fn open_somewhere(app: &AppHandle, path: Option<String>, new: bool) -> tauri::Result<()> {
    let state = app.state::<AppState>();
    let Some(path) = path else {
        if new || app.webview_windows().is_empty() {
            create_window(app, None)?;
        } else if let Some(l) = app.webview_windows().keys().next().cloned() {
            focus(app, &l);
        }
        return Ok(());
    };
    if let Some(l) = state.window_with(std::path::Path::new(&path)) {
        focus(app, &l);
        return Ok(());
    }
    if !new {
        let open = state
            .paths
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let mut empty: Vec<String> = app
            .webview_windows()
            .keys()
            .filter(|l| !open.contains_key(*l))
            .cloned()
            .collect();
        empty.sort();
        if let Some(l) = empty.first() {
            // the page asks for its start archive when it loads, or takes the event if it runs
            state
                .startup
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(l.clone(), path.clone());
            let _ = app.emit_to(l.as_str(), "open-file", path);
            focus(app, l);
            return Ok(());
        }
    }
    create_window(app, Some(path))?;
    Ok(())
}

#[tauri::command]
async fn search(window: WebviewWindow, state: State<'_, AppState>, query: String) -> Res<usize> {
    with_session(&state, &window, move |s| Ok(s.search(&query))).await
}

/// Arranges list `view` (0 main, 1 folder pane): filter, sort, flat or tree.
#[tauri::command]
async fn set_view(
    window: WebviewWindow,
    state: State<'_, AppState>,
    view: usize,
    spec: view::ViewSpec,
) -> Res<backend::ViewSummary> {
    with_session(&state, &window, move |s| s.set_view(view, spec)).await
}

#[tauri::command]
async fn page_view(
    window: WebviewWindow,
    state: State<'_, AppState>,
    view: usize,
    offset: usize,
    limit: usize,
) -> Res<Vec<Row>> {
    with_session(&state, &window, move |s| {
        Ok(s.page_view(view, offset, limit.min(500)))
    })
    .await
}

/// Thumbnails of entries for the gallery view (the longer side is at most `side` pixels).
#[tauri::command]
async fn thumbnails(
    window: WebviewWindow,
    state: State<'_, AppState>,
    entries: Vec<usize>,
    side: u32,
) -> Res<Vec<backend::ThumbOut>> {
    with_session(&state, &window, move |s| Ok(s.thumbnails(&entries, side))).await
}

/// Rebuilds the menu bar with the texts of the chosen language (`menu-*` entries of the catalog).
#[tauri::command]
fn set_menu_labels(
    app: AppHandle,
    labels: HashMap<String, String>,
    recent: Option<Vec<String>>,
) -> Res<()> {
    build_menu(&app, &labels, &recent.unwrap_or_default()).map_err(|e| e.to_string())
}

/// Opens or closes a folder of a tree list; returns the number of lines.
#[tauri::command]
async fn tree_toggle(
    window: WebviewWindow,
    state: State<'_, AppState>,
    view: usize,
    path: String,
) -> Res<usize> {
    with_session(&state, &window, move |s| s.tree_toggle(view, &path)).await
}

#[tauri::command]
async fn tree_set_all(
    window: WebviewWindow,
    state: State<'_, AppState>,
    view: usize,
    open: bool,
) -> Res<usize> {
    with_session(&state, &window, move |s| s.tree_set_all(view, open)).await
}

/// The line of an entry or folder in list `view` (a tree opens the way to it first).
#[tauri::command]
async fn locate(
    window: WebviewWindow,
    state: State<'_, AppState>,
    view: usize,
    entry: Option<usize>,
    folder: Option<String>,
) -> Res<backend::Located> {
    with_session(&state, &window, move |s| {
        s.locate(view, entry, folder.as_deref())
    })
    .await
}

/// The entries below a folder of a tree list.
#[tauri::command]
async fn folder_entries(
    window: WebviewWindow,
    state: State<'_, AppState>,
    view: usize,
    path: String,
) -> Res<Vec<usize>> {
    with_session(&state, &window, move |s| s.folder_entries(view, &path)).await
}

#[tauri::command]
async fn page(
    window: WebviewWindow,
    state: State<'_, AppState>,
    offset: usize,
    limit: usize,
) -> Res<Vec<Row>> {
    with_session(&state, &window, move |s| Ok(s.page(offset, limit.min(500)))).await
}

#[tauri::command]
async fn set_names(window: WebviewWindow, state: State<'_, AppState>, mode: String) -> Res<usize> {
    with_session(&state, &window, move |s| {
        s.set_names(&mode)?;
        Ok(s.filtered().len())
    })
    .await
}

#[tauri::command]
async fn details(window: WebviewWindow, state: State<'_, AppState>, index: usize) -> Res<Details> {
    with_session(&state, &window, move |s| s.details(index)).await
}

#[tauri::command]
async fn preview(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    action: Option<usize>,
) -> Res<Preview> {
    with_session(&state, &window, move |s| s.preview(index, action)).await
}

/// Extracts the given entries, or everything in the current search when `indices` is empty.
/// Writes the map of an entry with every file it needs below `dest`.
#[tauri::command]
async fn extract_map(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    dest: String,
    rename: bool,
    skip: Option<Vec<String>>,
    flat: Option<backend::FlatReq>,
) -> Res<backend::MapExtractSummary> {
    with_session(&state, &window, move |s| {
        s.extract_map(
            index,
            &PathBuf::from(dest),
            rename,
            &skip.unwrap_or_default(),
            flat,
        )
    })
    .await
}

/// Changes the map of an entry with the flat maps maker (one step that can be taken back).
#[tauri::command]
async fn stage_flat_map(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    options: backend::FlatReq,
) -> Res<Vec<backend::PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_flat_map(index, options)?;
        Ok(s.pending())
    })
    .await
}

/// Writes a small archive with the maps (all, or those among `indices`) made flat.
#[tauri::command]
async fn flat_maps(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    dst: String,
    indices: Vec<usize>,
    options: backend::FlatReq,
) -> Res<backend::FlatSummary> {
    let (plan, bases) = with_session(&state, &window, move |s| {
        let bases = (!indices.is_empty()).then(|| s.map_bases(&indices));
        Ok((s.plan(), bases))
    })
    .await?;
    run_job(app, &state, &window, move |ctl| {
        backend::flat_maps(&plan, &PathBuf::from(dst), bases, options.into(), ctl)
    })
    .await
}

/// Writes a picture made by the page (the 3D view) as PNG.
#[tauri::command]
async fn save_picture(path: String, png: String) -> Res<u64> {
    tauri::async_runtime::spawn_blocking(move || backend::save_picture(&PathBuf::from(path), &png))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn extract(
    window: WebviewWindow,
    state: State<'_, AppState>,
    indices: Vec<usize>,
    dest: String,
    rename: bool,
) -> Res<ExtractSummary> {
    with_session(&state, &window, move |s| {
        let all;
        let list: &[usize] = if indices.is_empty() {
            all = s.filtered().to_vec();
            &all
        } else {
            &indices
        };
        s.extract(list, &PathBuf::from(dest), rename)
    })
    .await
}

#[tauri::command]
async fn verify(
    window: WebviewWindow,
    state: State<'_, AppState>,
    decode: bool,
) -> Res<VerifySummary> {
    with_session(&state, &window, move |s| s.verify(decode)).await
}

#[derive(Serialize, Clone)]
struct JobProgress {
    done: u64,
    total: u64,
}

/// Runs a long job on a worker thread, with cancel token and `job-progress` events for the page.
async fn run_job<T, F>(
    app: AppHandle,
    state: &State<'_, AppState>,
    window: &WebviewWindow,
    job: F,
) -> Res<T>
where
    T: Send + 'static,
    F: FnOnce(&Control) -> Res<T> + Send + 'static,
{
    let token = CancelToken::new();
    let label = window.label().to_string();
    {
        let mut jobs = state.jobs.lock().map_err(|_| "internal error")?;
        if jobs.contains_key(&label) {
            return Err(err!("err-job-running"));
        }
        jobs.insert(label.clone(), token.clone());
    }
    let target = label.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        // report about every percent only, so the page is not flooded with events
        let last = AtomicU64::new(u64::MAX);
        let report = |p: Progress| {
            let step = (p.done * 100).checked_div(p.total).unwrap_or(0);
            if last.swap(step, Ordering::Relaxed) != step {
                let _ = app.emit_to(
                    target.as_str(),
                    "job-progress",
                    JobProgress {
                        done: p.done,
                        total: p.total,
                    },
                );
            }
        };
        let ctl = Control {
            cancel: Some(&token),
            progress: Some(&report),
        };
        job(&ctl)
    })
    .await
    .map_err(|e| e.to_string());
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.remove(&label);
    }
    result?
}

/// Packs a folder into a GRF.
#[tauri::command]
async fn build_archive(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    req: BuildRequest,
) -> Res<BuildSummary> {
    run_job(app, &state, &window, move |ctl| {
        backend::build_archive(&req, ctl)
    })
    .await
}

/// Merges several archives into a new one.
#[tauri::command]
async fn merge_archives(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    req: MergeRequest,
) -> Res<MergeSummary> {
    run_job(app, &state, &window, move |ctl| {
        backend::merge_archives(&req, ctl)
    })
    .await
}

#[tauri::command]
async fn stage_replace(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    file: String,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_replace(index, &PathBuf::from(file))?;
        Ok(s.pending())
    })
    .await
}

#[tauri::command]
async fn stage_add(
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
    file: String,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_add(&path, &PathBuf::from(file))?;
        Ok(s.pending())
    })
    .await
}

/// Replaces image `frame` of the sprite `index` with a PNG file.
#[tauri::command]
async fn stage_spr_image(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    frame: usize,
    png: String,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_spr_image(index, frame, &PathBuf::from(png))?;
        Ok(s.pending())
    })
    .await
}

/// Appends a PNG file to the sprite `index` as a new image.
#[tauri::command]
async fn stage_spr_add(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    png: String,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_spr_add(index, &PathBuf::from(png))?;
        Ok(s.pending())
    })
    .await
}

/// Removes image `frame` of the sprite `index`; the animation next to it follows.
#[tauri::command]
async fn stage_spr_remove(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    frame: usize,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_spr_remove(index, frame)?;
        Ok(s.pending())
    })
    .await
}

/// Gives the sprite `index` the palette of a `.pal` file.
#[tauri::command]
async fn stage_spr_palette(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    pal: String,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_spr_palette(index, &PathBuf::from(pal))?;
        Ok(s.pending())
    })
    .await
}

/// One frame of the animation `index`: its layers and anchors as data, and a picture.
#[tauri::command]
async fn act_frame(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    action: usize,
    frame: usize,
    references: Option<Vec<backend::ReferenceSpec>>,
) -> Res<act_editor::ActFrameView> {
    with_session(&state, &window, move |s| {
        s.act_view_with(index, action, frame, &references.unwrap_or_default())
    })
    .await
}

/// The topmost layer with a visible pixel at a point of the frame (frame coordinates).
#[tauri::command]
async fn act_layer_at(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    action: usize,
    frame: usize,
    x: f64,
    y: f64,
) -> Res<Option<usize>> {
    with_session(&state, &window, move |s| {
        s.act_layer_at(index, action, frame, x, y)
    })
    .await
}

/// Changes a layer, an anchor or a frame of the animation `index` (staged).
#[tauri::command]
async fn stage_act_edit(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    action: usize,
    frame: usize,
    edit: act_editor::ActEdit,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_act_edit(index, action, frame, &edit)?;
        Ok(s.pending())
    })
    .await
}

/// Sets the type and/or height of a rectangle of cells of the map `index` (staged).
#[tauri::command]
async fn stage_gat_edit(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    edit: gat_editor::GatEdit,
) -> Res<GatStaged> {
    with_session(&state, &window, move |s| {
        let changed = s.stage_gat_edit(index, &edit)?;
        Ok(GatStaged {
            changed,
            pending: s.pending(),
        })
    })
    .await
}

#[derive(Serialize)]
struct GatStaged {
    changed: usize,
    pending: Vec<PendingRow>,
}

/// One palette image of the sprite `index` as palette indices, with staged changes applied.
#[tauri::command]
async fn spr_image(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    frame: usize,
) -> Res<spr_editor::SprImageView> {
    with_session(&state, &window, move |s| s.spr_image(index, frame)).await
}

/// Replaces the pixels (palette indices, base64) of one palette image (staged).
#[tauri::command]
async fn stage_spr_pixels(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    frame: usize,
    pixels: String,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_spr_pixels(index, frame, &pixels)?;
        Ok(s.pending())
    })
    .await
}

/// The 256 colours (`rrggbb`) of the palette `index`, with staged changes applied.
#[tauri::command]
async fn pal_colors(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
) -> Res<Vec<String>> {
    with_session(&state, &window, move |s| s.pal_colors(index)).await
}

/// Changes a colour or a range of colours of the palette `index` (staged).
#[tauri::command]
async fn stage_pal_edit(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    edit: pal_editor::PalEdit,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_pal_edit(index, &edit)?;
        Ok(s.pending())
    })
    .await
}

/// The whole content of a text or Lua entry as editable text.
#[tauri::command]
async fn text_for_edit(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
) -> Res<backend::TextForEdit> {
    with_session(&state, &window, move |s| s.text_for_edit(index)).await
}

/// Replaces a text or Lua entry by edited text (staged); compiled Lua is compiled again.
#[tauri::command]
async fn stage_text_edit(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    text: String,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_text_edit(index, &text)?;
        Ok(s.pending())
    })
    .await
}

/// Sets the frame interval (milliseconds) of an action of the animation `index`.
#[tauri::command]
async fn stage_act_interval(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    action: usize,
    ms: f32,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_act_interval(index, action, ms)?;
        Ok(s.pending())
    })
    .await
}

/// Opens an entry (with staged changes) with the program the system chooses for its type.
#[tauri::command]
async fn open_external(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
) -> Res<String> {
    let path = with_session(&state, &window, move |s| s.export_temp(index)).await?;
    backend::open_with_system(&path)?;
    Ok(path.display().to_string())
}

/// A page of bytes of an entry for the hex editor.
#[tauri::command]
async fn hex_page(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    offset: u64,
    len: usize,
) -> Res<backend::HexPage> {
    with_session(&state, &window, move |s| s.hex_page(index, offset, len)).await
}

/// The next place a byte pattern occurs in an entry (wrapping round), or none.
#[tauri::command]
async fn hex_find(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    pattern: Vec<u8>,
    from: u64,
) -> Res<Option<u64>> {
    with_session(&state, &window, move |s| s.hex_find(index, &pattern, from)).await
}

/// Overwrites bytes of an entry (staged).
#[tauri::command]
async fn stage_hex(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
    edits: Vec<(u64, u8)>,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_hex(index, &edits)?;
        Ok(s.pending())
    })
    .await
}

/// The sound an animation frame plays (`data\wav\<name>` of the open archive).
#[tauri::command]
async fn act_sound(
    window: WebviewWindow,
    state: State<'_, AppState>,
    name: String,
) -> Res<backend::SoundOut> {
    with_session(&state, &window, move |s| s.act_sound(&name)).await
}

/// A true-colour BMP reduced to 8 bits, to compare with the original before staging it.
#[tauri::command]
async fn reduce_preview(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
) -> Res<backend::ReducePreview> {
    with_session(&state, &window, move |s| s.reduce_preview(index)).await
}

#[tauri::command]
async fn stage_reduce(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.stage_reduce(index)?;
        Ok(s.pending())
    })
    .await
}

/// Writes a copy of the open archive (with its pending changes) whose true-colour BMPs are stored
/// as 8 bits where that keeps `min_psnr` dB and saves space.
#[tauri::command]
async fn reduce_images(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    dst: String,
    min_psnr: f64,
) -> Res<backend::ReduceSummary> {
    let plan = with_session(&state, &window, |s| Ok(s.plan())).await?;
    run_job(app, &state, &window, move |ctl| {
        backend::reduce_images(&plan, &PathBuf::from(dst), min_psnr, ctl)
    })
    .await
}

/// Removes the given entries, or everything in the current search when `indices` is empty.
#[tauri::command]
async fn stage_remove(
    window: WebviewWindow,
    state: State<'_, AppState>,
    indices: Vec<usize>,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        let list = if indices.is_empty() {
            s.filtered().to_vec()
        } else {
            indices
        };
        s.stage_remove(&list)?;
        Ok(s.pending())
    })
    .await
}

#[tauri::command]
async fn pending(window: WebviewWindow, state: State<'_, AppState>) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, |s| Ok(s.pending())).await
}

/// Takes the changes of the draft found when the archive was opened.
#[tauri::command]
async fn restore_draft(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Res<backend::DraftRestored> {
    with_session(&state, &window, |s| s.restore_draft()).await
}

/// Keeps the draft found when the archive was opened for a later session.
#[tauri::command]
async fn defer_draft(window: WebviewWindow, state: State<'_, AppState>) -> Res<()> {
    with_session(&state, &window, |s| {
        s.defer_draft();
        Ok(())
    })
    .await
}

/// Deletes the draft found when the archive was opened.
#[tauri::command]
async fn discard_draft(window: WebviewWindow, state: State<'_, AppState>) -> Res<()> {
    with_session(&state, &window, |s| {
        s.discard_draft();
        Ok(())
    })
    .await
}

#[tauri::command]
async fn discard_pending(window: WebviewWindow, state: State<'_, AppState>) -> Res<()> {
    with_session(&state, &window, |s| {
        s.discard_pending();
        Ok(())
    })
    .await
}

/// Writes the pending changes as a new archive; the open archive is not touched.
#[tauri::command]
async fn save_edited(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    dst: String,
) -> Res<EditSummary> {
    let plan = with_session(&state, &window, |s| s.edit_plan()).await?;
    run_job(app, &state, &window, move |ctl| {
        backend::save_edited(&plan, &PathBuf::from(dst), true, ctl)
    })
    .await
}

/// Writes the archive with its changes and the chosen options to another file ("Save as", "Save
/// advanced"). The open archive is not touched.
#[tauri::command]
async fn save_as(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    req: saving::SaveRequest,
) -> Res<EditSummary> {
    let plan = with_session(&state, &window, |s| Ok(s.plan())).await?;
    if canonical(&req.dst) == canonical(&plan.src) {
        return Err(err!("err-save-same"));
    }
    run_job(app, &state, &window, move |ctl| {
        saving::save_as(&plan, &req, true, ctl)
    })
    .await
}

/// "Save": writes the archive with its changes (and the options of `req`; its `dst` is ignored)
/// next to it, checks it, puts it in place of the open archive and opens it again.
#[tauri::command]
async fn save_archive(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    req: Option<saving::SaveRequest>,
) -> Res<saving::SavedInPlace> {
    let shared = state.slot(window.label());
    run_job(app, &state, &window, move |ctl| {
        saving::save_in_place(&shared, req.unwrap_or_default(), ctl)
    })
    .await
}

/// An empty archive in the app's cache folder, to open as a new archive of type `kind` (grf, gpf,
/// rgz, thor). It is an empty GRF named for its type; "Save" asks where to write it in that type.
#[tauri::command]
fn new_archive(app: AppHandle, kind: String) -> Res<String> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("new");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dir.join(saving::untitled_name(&kind, stamp)?);
    saving::create_empty(&path)?;
    Ok(path.display().to_string())
}

#[tauri::command]
async fn entry_hashes(
    window: WebviewWindow,
    state: State<'_, AppState>,
    index: usize,
) -> Res<saving::Hashes> {
    with_session(&state, &window, move |s| s.hashes(index)).await
}

/// Compares the open archive (as saved, without pending changes) with another one.
#[tauri::command]
async fn compare_with(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    other: String,
) -> Res<saving::Comparison> {
    let (plan, mode) = with_session(&state, &window, |s| Ok((s.plan(), s.name_mode()))).await?;
    run_job(app, &state, &window, move |ctl| {
        saving::compare_plan(&plan, &PathBuf::from(other), mode, ctl)
    })
    .await
}

/// Writes what changed from `old` to the open archive as a Thor or GRF patch.
#[tauri::command]
async fn make_patch(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    old: String,
    dst: String,
    method: Option<String>,
) -> Res<saving::PatchSummary> {
    let plan = with_session(&state, &window, |s| Ok(s.plan())).await?;
    run_job(app, &state, &window, move |ctl| {
        saving::patch_plan(
            &plan,
            &PathBuf::from(old),
            &PathBuf::from(dst),
            method.as_deref(),
            ctl,
        )
    })
    .await
}

/// Tools › Text Encoding.
#[tauri::command]
fn convert_text(text: String, from: String, to: String) -> Res<saving::ConvertedText> {
    saving::convert_text(&text, &from, &to)
}

/// Tools › Search in Files: the lines of the archive's files that match.
#[tauri::command]
async fn search_contents(
    app: AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    query: backend::ContentQuery,
) -> Res<backend::ContentSearch> {
    let shared = state.slot(window.label());
    run_job(app, &state, &window, move |ctl| {
        let guard = shared.lock().map_err(|_| "internal error")?;
        let s = guard.as_ref().ok_or_else(|| err!("err-no-archive"))?;
        s.search_contents(&query, ctl)
    })
    .await
}

/// Opens the folder with the app's settings, drafts and new archives.
#[tauri::command]
fn open_data_folder(app: AppHandle) -> Res<String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    backend::open_with_system(&dir)?;
    Ok(dir.display().to_string())
}

#[derive(Serialize)]
struct History {
    can_undo: bool,
    can_redo: bool,
    /// The steps that undo would take back (newest first) and redo would bring back.
    undo: Vec<backend::Step>,
    redo: Vec<backend::Step>,
}

/// Whether the staged changes can be undone or redone.
#[tauri::command]
async fn history(window: WebviewWindow, state: State<'_, AppState>) -> Res<History> {
    with_session(&state, &window, |s| {
        let (can_undo, can_redo) = s.history();
        let (undo, redo) = s.history_steps(30);
        Ok(History {
            can_undo,
            can_redo,
            undo,
            redo,
        })
    })
    .await
}

/// Takes back the last staged change; returns the changes that are staged afterwards.
#[tauri::command]
async fn undo_change(window: WebviewWindow, state: State<'_, AppState>) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, |s| {
        s.undo();
        Ok(s.pending())
    })
    .await
}

/// Takes back (or redoes) several steps at once, from the history list.
#[tauri::command]
async fn step_history(
    window: WebviewWindow,
    state: State<'_, AppState>,
    steps: usize,
    redo: bool,
) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, move |s| {
        s.step_history(steps, redo);
        Ok(s.pending())
    })
    .await
}

#[tauri::command]
async fn redo_change(window: WebviewWindow, state: State<'_, AppState>) -> Res<Vec<PendingRow>> {
    with_session(&state, &window, |s| {
        s.redo();
        Ok(s.pending())
    })
    .await
}

#[tauri::command]
fn cancel_job(window: WebviewWindow, state: State<'_, AppState>) {
    if let Ok(jobs) = state.jobs.lock()
        && let Some(t) = jobs.get(window.label())
    {
        t.cancel();
    }
}

/// The archive to open at start: one the system handed us ("Open with" on macOS), else a path given
/// on the command line (`rustygrf-gui file.grf`; this is how Windows and Linux pass it).
#[tauri::command]
fn startup_path(window: WebviewWindow, state: State<'_, AppState>) -> Option<String> {
    if let Ok(mut startup) = state.startup.lock()
        && let Some(p) = startup.remove(window.label())
    {
        return Some(p);
    }
    // only the first window takes the archive of the command line
    (window.label() == "main")
        .then(|| archive_in_args(std::env::args().skip(1)))
        .flatten()
}

/// The first argument that is an existing file (`--select` takes the next argument as its value).
fn archive_in_args(args: impl IntoIterator<Item = String>) -> Option<String> {
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        if a == "--select" {
            args.next();
        } else if !a.starts_with('-') && PathBuf::from(&a).is_file() {
            return Some(a);
        }
    }
    None
}

/// `--select <search>` on the command line: the page searches for it and selects the first hit.
#[tauri::command]
fn startup_select() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--select" {
            return args.next();
        }
    }
    None
}

const SETTINGS_MENU_ID: &str = "settings";
const COMMAND_PREFIX: &str = "cmd:";
const RECENT_PREFIX: &str = "recent:";

/// The menu bar: the platform's usual menus plus every toolbar command (so the toolbar is never
/// the only place for a command), the list layouts and the preview position, and "Settings..."
/// (in the application menu on macOS, in Edit elsewhere). The page runs the commands; a command
/// whose button is disabled at that moment does nothing.
///
/// Built in one go from filled submenus, like Tauri's own default menu: on Windows a submenu
/// inserted into a finished menu bar does not show up, and Linux has no File menu by default.
fn build_menu(
    app: &AppHandle,
    labels: &HashMap<String, String>,
    recent: &[String],
) -> tauri::Result<()> {
    use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
    type Item = Box<dyn IsMenuItem<tauri::Wry>>;
    // the text of a menu entry: the catalog's (`menu-<key>`), else the English default
    let lab = |key: &str, default: &str| -> String {
        labels
            .get(&format!("menu-{key}"))
            .cloned()
            .unwrap_or_else(|| default.to_string())
    };
    // `id` is the command and its label key (`menu-<id>`)
    let item = |id: &str, text: &str, accel: Option<&str>| -> tauri::Result<Item> {
        Ok(Box::new(MenuItem::with_id(
            app,
            format!("{COMMAND_PREFIX}{id}"),
            lab(id, text),
            true,
            accel,
        )?))
    };
    let sep = || -> tauri::Result<Item> { Ok(Box::new(PredefinedMenuItem::separator(app)?)) };
    let submenu = |name: &str, items: &[Item]| {
        let refs: Vec<&dyn IsMenuItem<tauri::Wry>> = items.iter().map(|i| i.as_ref()).collect();
        Submenu::with_items(app, name, true, &refs)
    };
    let settings: Item = Box::new(MenuItem::with_id(
        app,
        SETTINGS_MENU_ID,
        lab("settings", "Settings..."),
        true,
        Some("CmdOrCtrl+,"),
    )?);
    let info = app.package_info();
    let about = item("about", &format!("About {}...", info.name), None)?;

    let new: Item = Box::new(submenu(
        &lab("new", "New"),
        &[
            item("new-grf", "GRF Archive...", Some("CmdOrCtrl+N"))?,
            item("new-gpf", "GPF Patch...", None)?,
            item("new-rgz", "RGZ Patch...", None)?,
            item("new-thor", "Thor Patch...", None)?,
        ],
    )?);
    let mut recent_items: Vec<Item> = Vec::new();
    for (i, path) in recent.iter().take(10).enumerate() {
        recent_items.push(Box::new(MenuItem::with_id(
            app,
            format!("{RECENT_PREFIX}{i}"),
            path,
            true,
            None::<&str>,
        )?));
    }
    if recent_items.is_empty() {
        recent_items.push(Box::new(MenuItem::with_id(
            app,
            "recent-none",
            lab("recent-none", "No Recent Archives"),
            false,
            None::<&str>,
        )?));
    } else {
        recent_items.push(sep()?);
        recent_items.push(item("recent-clear", "Clear Menu", None)?);
    }
    let recent_menu: Item = Box::new(submenu(&lab("recent", "Open Recent"), &recent_items)?);
    let advanced: Item = Box::new(submenu(
        &lab("save-advanced", "Save Advanced"),
        &[
            item("save-repack", "Repack (Recompress)...", None)?,
            item("save-compact", "Compact (Copy Streams)...", None)?,
            item("save-encrypt", "Encrypt...", None)?,
            item("save-decrypt", "Save Decrypted...", None)?,
        ],
    )?);

    let mut file = vec![
        new,
        item("new-window", "New Window", Some("CmdOrCtrl+Shift+N"))?,
        item("open", "Open...", Some("CmdOrCtrl+O"))?,
        recent_menu,
        item("close-archive", "Close Archive", None)?,
        sep()?,
        item("save", "Save", Some("CmdOrCtrl+S"))?,
        item("save-as", "Save As...", Some("CmdOrCtrl+Shift+S"))?,
        advanced,
        sep()?,
        item("build", "Build from Folder...", None)?,
        item("merge", "Merge...", None)?,
        item("extract", "Extract...", None)?,
        sep()?,
        Box::new(PredefinedMenuItem::close_window(app, None)?),
    ];
    // own entries: the page decides whether a text field, the pixel editor or the archive's
    // changes are undone (the system's entries only reach text fields, and not on Windows)
    let mut edit: Vec<Item> = vec![
        item("undo", "Undo", Some("CmdOrCtrl+Z"))?,
        item("redo", "Redo", Some("CmdOrCtrl+Shift+Z"))?,
        sep()?,
        Box::new(PredefinedMenuItem::cut(app, None)?),
        Box::new(PredefinedMenuItem::copy(app, None)?),
        Box::new(PredefinedMenuItem::paste(app, None)?),
        Box::new(PredefinedMenuItem::select_all(app, None)?),
        sep()?,
        item("add", "Add File...", None)?,
        item("replace", "Replace...", None)?,
        item("remove", "Remove", None)?,
        item("changes", "Review Changes...", None)?,
        item("history", "Change History...", None)?,
        sep()?,
        item("archive-settings", "Archive Settings...", None)?,
    ];
    let mut view = vec![
        item("nav-back", "Back", None)?,
        item("nav-forward", "Forward", None)?,
        sep()?,
        item("layout-tree", "As Tree", Some("CmdOrCtrl+1"))?,
        item("layout-details", "As Details", Some("CmdOrCtrl+2"))?,
        item("layout-folders", "As Folders", Some("CmdOrCtrl+3"))?,
        item("layout-gallery", "As Gallery", Some("CmdOrCtrl+4"))?,
        sep()?,
        item("preview-right", "Preview on the Right", None)?,
        item("preview-bottom", "Preview at the Bottom", None)?,
        item("preview-off", "Hide Preview", None)?,
    ];
    let tools = vec![
        item("verify", "Verify Archive", None)?,
        item("hash", "Hashes...", None)?,
        item("compare", "Compare with Archive...", None)?,
        item(
            "search-contents",
            "Search in Files...",
            Some("CmdOrCtrl+Shift+F"),
        )?,
        sep()?,
        item("reduce-images", "Reduce Images...", None)?,
        item("flat-maps", "Flat Maps...", None)?,
        item("text-encoding", "Text Encoding...", None)?,
        sep()?,
        item("data-folder", "Open App Data Folder", None)?,
    ];
    let mut window: Vec<Item> = vec![
        Box::new(PredefinedMenuItem::minimize(app, None)?),
        Box::new(PredefinedMenuItem::maximize(app, None)?),
    ];
    let mut menus = Vec::new();
    let mut help = None;
    if cfg!(target_os = "macos") {
        menus.push(submenu(
            &info.name,
            &[
                about,
                sep()?,
                settings,
                sep()?,
                Box::new(PredefinedMenuItem::services(app, None)?),
                sep()?,
                Box::new(PredefinedMenuItem::hide(app, None)?),
                Box::new(PredefinedMenuItem::hide_others(app, None)?),
                sep()?,
                Box::new(PredefinedMenuItem::quit(app, None)?),
            ],
        )?);
        view.push(sep()?);
        view.push(Box::new(PredefinedMenuItem::fullscreen(app, None)?));
        window.push(sep()?);
        window.push(Box::new(PredefinedMenuItem::close_window(app, None)?));
    } else {
        file.push(Box::new(PredefinedMenuItem::quit(app, None)?));
        edit.push(sep()?);
        edit.push(settings);
        help = Some(submenu(&lab("help", "Help"), &[about])?);
    }
    menus.push(submenu(&lab("file", "File"), &file)?);
    menus.push(submenu(&lab("edit", "Edit"), &edit)?);
    menus.push(submenu(&lab("view", "View"), &view)?);
    menus.push(submenu(&lab("tools", "Tools"), &tools)?);
    menus.push(submenu(&lab("window", "Window"), &window)?);
    menus.extend(help);
    let refs: Vec<&dyn IsMenuItem<tauri::Wry>> = menus
        .iter()
        .map(|m| m as &dyn IsMenuItem<tauri::Wry>)
        .collect();
    app.set_menu(Menu::with_items(app, &refs)?)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // must come first: a second start hands its arguments to the running app and exits
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let _ = open_somewhere(app, archive_in_args(args.into_iter().skip(1)), false);
        }))
        .plugin(tauri_plugin_dialog::init())
        // pictures of map and model previews come one by one through `rgrf`, not inside the
        // preview: the map can be drawn before its textures are there
        .register_asynchronous_uri_scheme_protocol("rgrf", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let label = ctx.webview_label().to_string();
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn(async move {
                let result =
                    tauri::async_runtime::spawn_blocking(move || serve_asset(&app, &label, &path))
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r);
                let builder = tauri::http::Response::builder()
                    .header("Access-Control-Allow-Origin", "*")
                    .header("Cache-Control", "max-age=3600");
                let response = match result {
                    Ok((body, mime)) => builder.header("Content-Type", mime).status(200).body(body),
                    Err(e) => builder.status(404).body(e.into_bytes()),
                };
                if let Ok(r) = response {
                    responder.respond(r);
                }
            });
        })
        .manage(AppState::default())
        .setup(|app| {
            build_menu(app.handle(), &HashMap::new(), &[])?;
            create_window(app.handle(), None)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            let state = window.app_handle().state::<AppState>();
            match event {
                tauri::WindowEvent::Destroyed => state.forget(window.label()),
                tauri::WindowEvent::Focused(true) => {
                    if let Ok(mut f) = state.front.lock() {
                        *f = Some(window.label().to_string());
                    }
                }
                _ => {}
            }
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            // the menu bar belongs to the app (macOS) or to each window; it acts on the window in front
            let target = front_window(app).map(|w| w.label().to_string());
            if id == "cmd:new-window" || target.is_none() {
                let _ = create_window(app, None);
                return;
            }
            let Some(target) = target else { return };
            if id == SETTINGS_MENU_ID {
                let _ = app.emit_to(target.as_str(), "open-settings", ());
            } else if let Some(cmd) = id.strip_prefix(COMMAND_PREFIX) {
                let _ = app.emit_to(target.as_str(), "menu-command", cmd);
            } else if let Some(n) = id.strip_prefix(RECENT_PREFIX) {
                let _ = app.emit_to(target.as_str(), "menu-command", format!("recent-{n}"));
            }
        })
        .invoke_handler(tauri::generate_handler![
            open_archive,
            close_archive,
            open_in_window,
            window_ready,
            remember_theme,
            open_link,
            search,
            page,
            set_names,
            set_view,
            page_view,
            thumbnails,
            set_menu_labels,
            tree_toggle,
            tree_set_all,
            locate,
            folder_entries,
            details,
            preview,
            extract,
            extract_map,
            stage_flat_map,
            flat_maps,
            verify,
            build_archive,
            merge_archives,
            reduce_preview,
            act_sound,
            open_external,
            hex_page,
            hex_find,
            stage_hex,
            act_layer_at,
            save_picture,
            stage_reduce,
            reduce_images,
            stage_replace,
            stage_add,
            stage_remove,
            stage_spr_image,
            stage_spr_add,
            stage_spr_remove,
            stage_spr_palette,
            stage_act_interval,
            pal_colors,
            stage_gat_edit,
            spr_image,
            stage_spr_pixels,
            stage_pal_edit,
            act_frame,
            stage_act_edit,
            text_for_edit,
            stage_text_edit,
            pending,
            discard_pending,
            restore_draft,
            discard_draft,
            defer_draft,
            history,
            undo_change,
            redo_change,
            step_history,
            save_edited,
            save_as,
            save_archive,
            new_archive,
            entry_hashes,
            compare_with,
            make_patch,
            open_data_folder,
            search_contents,
            convert_text,
            cancel_job,
            startup_path,
            startup_select
        ])
        .build(tauri::generate_context!())
        .expect("error while building RustyGRF")
        .run(|app, event| {
            // macOS delivers "Open with" and double-clicks as an event, not as an argument
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = &event
                && let Some(path) = urls.iter().find_map(|u| u.to_file_path().ok())
            {
                let _ = open_somewhere(app, Some(path.display().to_string()), false);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}

#[cfg(test)]
mod tests {
    use super::archive_in_args;

    #[test]
    fn asset_paths() {
        use super::parse_asset_path;
        assert_eq!(parse_asset_path("/tex-12-g3"), Some(("tex".into(), 12)));
        assert_eq!(parse_asset_path("raw-7"), Some(("raw".into(), 7)));
        assert_eq!(parse_asset_path("/tex-x"), None);
    }

    #[test]
    fn finds_the_archive_argument() {
        let file = std::env::temp_dir().join(format!("rgrf-arg-{}.grf", std::process::id()));
        std::fs::write(&file, b"x").unwrap();
        let f = file.display().to_string();
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(archive_in_args(args(&[&f])), Some(f.clone()));
        assert_eq!(archive_in_args(args(&["--verbose", &f])), Some(f.clone()));
        // the value of --select is a search text, never the file
        assert_eq!(archive_in_args(args(&["--select", &f])), None);
        assert_eq!(archive_in_args(args(&["--select", "x", &f])), Some(f));
        assert_eq!(archive_in_args(args(&["/no/such/file.grf"])), None);
        let _ = std::fs::remove_file(file);
    }
}
