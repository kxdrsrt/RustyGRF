//! RustyGRF desktop app: thin Tauri commands around [`backend::Session`].

pub mod act_editor;
pub mod backend;
pub mod draft;
pub mod gat_editor;
pub mod pal_editor;
pub mod spr_editor;
pub mod view;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use std::sync::atomic::{AtomicU64, Ordering};

use backend::{
    ArchiveInfo, BuildRequest, BuildSummary, Details, EditSummary, ExtractSummary, MergeRequest,
    MergeSummary, PendingRow, Preview, Row, Session, VerifySummary,
};
use rgrf_core::progress::{CancelToken, Control, Progress};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
struct AppState {
    session: Arc<Mutex<Option<Session>>>,
    /// Token of the running long job (build), if any.
    job: Mutex<Option<CancelToken>>,
    /// A file the system asked us to open (Finder "Open with" on macOS) before the page asked for it.
    opened: Mutex<Option<String>>,
}

type Res<T> = Result<T, String>;

/// Runs `f` on a worker thread with the open session, so a long job does not freeze the window.
async fn with_session<T, F>(state: &State<'_, AppState>, f: F) -> Res<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Session) -> Res<T> + Send + 'static,
{
    let shared = Arc::clone(&state.session);
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = shared
            .lock()
            .map_err(|_| "internal error: state is locked".to_string())?;
        let s = guard.as_mut().ok_or("no archive is open")?;
        f(s)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn open_archive(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    password: Option<String>,
) -> Res<ArchiveInfo> {
    let shared = Arc::clone(&state.session);
    let drafts = app.path().app_data_dir().ok().map(|d| d.join("drafts"));
    tauri::async_runtime::spawn_blocking(move || {
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
        Ok(info)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn search(state: State<'_, AppState>, query: String) -> Res<usize> {
    with_session(&state, move |s| Ok(s.search(&query))).await
}

/// Arranges list `view` (0 main, 1 folder pane): filter, sort, flat or tree.
#[tauri::command]
async fn set_view(
    state: State<'_, AppState>,
    view: usize,
    spec: view::ViewSpec,
) -> Res<backend::ViewSummary> {
    with_session(&state, move |s| s.set_view(view, spec)).await
}

#[tauri::command]
async fn page_view(
    state: State<'_, AppState>,
    view: usize,
    offset: usize,
    limit: usize,
) -> Res<Vec<Row>> {
    with_session(&state, move |s| {
        Ok(s.page_view(view, offset, limit.min(500)))
    })
    .await
}

/// Opens or closes a folder of a tree list; returns the number of lines.
#[tauri::command]
async fn tree_toggle(state: State<'_, AppState>, view: usize, path: String) -> Res<usize> {
    with_session(&state, move |s| s.tree_toggle(view, &path)).await
}

#[tauri::command]
async fn tree_set_all(state: State<'_, AppState>, view: usize, open: bool) -> Res<usize> {
    with_session(&state, move |s| s.tree_set_all(view, open)).await
}

/// The line of an entry or folder in list `view` (a tree opens the way to it first).
#[tauri::command]
async fn locate(
    state: State<'_, AppState>,
    view: usize,
    entry: Option<usize>,
    folder: Option<String>,
) -> Res<backend::Located> {
    with_session(&state, move |s| s.locate(view, entry, folder.as_deref())).await
}

/// The entries below a folder of a tree list.
#[tauri::command]
async fn folder_entries(state: State<'_, AppState>, view: usize, path: String) -> Res<Vec<usize>> {
    with_session(&state, move |s| s.folder_entries(view, &path)).await
}

#[tauri::command]
async fn page(state: State<'_, AppState>, offset: usize, limit: usize) -> Res<Vec<Row>> {
    with_session(&state, move |s| Ok(s.page(offset, limit.min(500)))).await
}

#[tauri::command]
async fn set_names(state: State<'_, AppState>, mode: String) -> Res<usize> {
    with_session(&state, move |s| {
        s.set_names(&mode)?;
        Ok(s.filtered().len())
    })
    .await
}

#[tauri::command]
async fn details(state: State<'_, AppState>, index: usize) -> Res<Details> {
    with_session(&state, move |s| s.details(index)).await
}

#[tauri::command]
async fn preview(state: State<'_, AppState>, index: usize, action: Option<usize>) -> Res<Preview> {
    with_session(&state, move |s| s.preview(index, action)).await
}

/// Extracts the given entries, or everything in the current search when `indices` is empty.
/// Writes the map of an entry with every file it needs below `dest`.
#[tauri::command]
async fn extract_map(
    state: State<'_, AppState>,
    index: usize,
    dest: String,
    rename: bool,
    skip: Option<Vec<String>>,
) -> Res<backend::MapExtractSummary> {
    with_session(&state, move |s| {
        s.extract_map(
            index,
            &PathBuf::from(dest),
            rename,
            &skip.unwrap_or_default(),
        )
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
    state: State<'_, AppState>,
    indices: Vec<usize>,
    dest: String,
    rename: bool,
) -> Res<ExtractSummary> {
    with_session(&state, move |s| {
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
async fn verify(state: State<'_, AppState>, decode: bool) -> Res<VerifySummary> {
    with_session(&state, move |s| s.verify(decode)).await
}

#[derive(Serialize, Clone)]
struct JobProgress {
    done: u64,
    total: u64,
}

/// Runs a long job on a worker thread, with cancel token and `job-progress` events for the page.
async fn run_job<T, F>(app: AppHandle, state: &State<'_, AppState>, job: F) -> Res<T>
where
    T: Send + 'static,
    F: FnOnce(&Control) -> Res<T> + Send + 'static,
{
    let token = CancelToken::new();
    {
        let mut slot = state.job.lock().map_err(|_| "internal error")?;
        if slot.is_some() {
            return Err("another job is already running".into());
        }
        *slot = Some(token.clone());
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        // report about every percent only, so the page is not flooded with events
        let last = AtomicU64::new(u64::MAX);
        let report = |p: Progress| {
            let step = (p.done * 100).checked_div(p.total).unwrap_or(0);
            if last.swap(step, Ordering::Relaxed) != step {
                let _ = app.emit(
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
    if let Ok(mut slot) = state.job.lock() {
        *slot = None;
    }
    result?
}

/// Packs a folder into a GRF.
#[tauri::command]
async fn build_archive(
    app: AppHandle,
    state: State<'_, AppState>,
    req: BuildRequest,
) -> Res<BuildSummary> {
    run_job(app, &state, move |ctl| backend::build_archive(&req, ctl)).await
}

/// Merges several archives into a new one.
#[tauri::command]
async fn merge_archives(
    app: AppHandle,
    state: State<'_, AppState>,
    req: MergeRequest,
) -> Res<MergeSummary> {
    run_job(app, &state, move |ctl| backend::merge_archives(&req, ctl)).await
}

#[tauri::command]
async fn stage_replace(
    state: State<'_, AppState>,
    index: usize,
    file: String,
) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
        s.stage_replace(index, &PathBuf::from(file))?;
        Ok(s.pending())
    })
    .await
}

#[tauri::command]
async fn stage_add(state: State<'_, AppState>, path: String, file: String) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
        s.stage_add(&path, &PathBuf::from(file))?;
        Ok(s.pending())
    })
    .await
}

/// Replaces image `frame` of the sprite `index` with a PNG file.
#[tauri::command]
async fn stage_spr_image(
    state: State<'_, AppState>,
    index: usize,
    frame: usize,
    png: String,
) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
        s.stage_spr_image(index, frame, &PathBuf::from(png))?;
        Ok(s.pending())
    })
    .await
}

/// Appends a PNG file to the sprite `index` as a new image.
#[tauri::command]
async fn stage_spr_add(
    state: State<'_, AppState>,
    index: usize,
    png: String,
) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
        s.stage_spr_add(index, &PathBuf::from(png))?;
        Ok(s.pending())
    })
    .await
}

/// Removes image `frame` of the sprite `index`; the animation next to it follows.
#[tauri::command]
async fn stage_spr_remove(
    state: State<'_, AppState>,
    index: usize,
    frame: usize,
) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
        s.stage_spr_remove(index, frame)?;
        Ok(s.pending())
    })
    .await
}

/// Gives the sprite `index` the palette of a `.pal` file.
#[tauri::command]
async fn stage_spr_palette(
    state: State<'_, AppState>,
    index: usize,
    pal: String,
) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
        s.stage_spr_palette(index, &PathBuf::from(pal))?;
        Ok(s.pending())
    })
    .await
}

/// One frame of the animation `index`: its layers and anchors as data, and a picture.
#[tauri::command]
async fn act_frame(
    state: State<'_, AppState>,
    index: usize,
    action: usize,
    frame: usize,
    references: Option<Vec<backend::ReferenceSpec>>,
) -> Res<act_editor::ActFrameView> {
    with_session(&state, move |s| {
        s.act_view_with(index, action, frame, &references.unwrap_or_default())
    })
    .await
}

/// The topmost layer with a visible pixel at a point of the frame (frame coordinates).
#[tauri::command]
async fn act_layer_at(
    state: State<'_, AppState>,
    index: usize,
    action: usize,
    frame: usize,
    x: f64,
    y: f64,
) -> Res<Option<usize>> {
    with_session(&state, move |s| s.act_layer_at(index, action, frame, x, y)).await
}

/// Changes a layer, an anchor or a frame of the animation `index` (staged).
#[tauri::command]
async fn stage_act_edit(
    state: State<'_, AppState>,
    index: usize,
    action: usize,
    frame: usize,
    edit: act_editor::ActEdit,
) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
        s.stage_act_edit(index, action, frame, &edit)?;
        Ok(s.pending())
    })
    .await
}

/// Sets the type and/or height of a rectangle of cells of the map `index` (staged).
#[tauri::command]
async fn stage_gat_edit(
    state: State<'_, AppState>,
    index: usize,
    edit: gat_editor::GatEdit,
) -> Res<GatStaged> {
    with_session(&state, move |s| {
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
    state: State<'_, AppState>,
    index: usize,
    frame: usize,
) -> Res<spr_editor::SprImageView> {
    with_session(&state, move |s| s.spr_image(index, frame)).await
}

/// Replaces the pixels (palette indices, base64) of one palette image (staged).
#[tauri::command]
async fn stage_spr_pixels(
    state: State<'_, AppState>,
    index: usize,
    frame: usize,
    pixels: String,
) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
        s.stage_spr_pixels(index, frame, &pixels)?;
        Ok(s.pending())
    })
    .await
}

/// The 256 colours (`rrggbb`) of the palette `index`, with staged changes applied.
#[tauri::command]
async fn pal_colors(state: State<'_, AppState>, index: usize) -> Res<Vec<String>> {
    with_session(&state, move |s| s.pal_colors(index)).await
}

/// Changes a colour or a range of colours of the palette `index` (staged).
#[tauri::command]
async fn stage_pal_edit(
    state: State<'_, AppState>,
    index: usize,
    edit: pal_editor::PalEdit,
) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
        s.stage_pal_edit(index, &edit)?;
        Ok(s.pending())
    })
    .await
}

/// The whole content of a text or Lua entry as editable text.
#[tauri::command]
async fn text_for_edit(state: State<'_, AppState>, index: usize) -> Res<backend::TextForEdit> {
    with_session(&state, move |s| s.text_for_edit(index)).await
}

/// Replaces a text or Lua entry by edited text (staged); compiled Lua is compiled again.
#[tauri::command]
async fn stage_text_edit(
    state: State<'_, AppState>,
    index: usize,
    text: String,
) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
        s.stage_text_edit(index, &text)?;
        Ok(s.pending())
    })
    .await
}

/// Sets the frame interval (milliseconds) of an action of the animation `index`.
#[tauri::command]
async fn stage_act_interval(
    state: State<'_, AppState>,
    index: usize,
    action: usize,
    ms: f32,
) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
        s.stage_act_interval(index, action, ms)?;
        Ok(s.pending())
    })
    .await
}

/// Opens an entry (with staged changes) with the program the system chooses for its type.
#[tauri::command]
async fn open_external(state: State<'_, AppState>, index: usize) -> Res<String> {
    let path = with_session(&state, move |s| s.export_temp(index)).await?;
    backend::open_with_system(&path)?;
    Ok(path.display().to_string())
}

/// A page of bytes of an entry for the hex editor.
#[tauri::command]
async fn hex_page(
    state: State<'_, AppState>,
    index: usize,
    offset: u64,
    len: usize,
) -> Res<backend::HexPage> {
    with_session(&state, move |s| s.hex_page(index, offset, len)).await
}

/// Overwrites bytes of an entry (staged).
#[tauri::command]
async fn stage_hex(
    state: State<'_, AppState>,
    index: usize,
    edits: Vec<(u64, u8)>,
) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
        s.stage_hex(index, &edits)?;
        Ok(s.pending())
    })
    .await
}

/// The sound an animation frame plays (`data\wav\<name>` of the open archive).
#[tauri::command]
async fn act_sound(state: State<'_, AppState>, name: String) -> Res<backend::SoundOut> {
    with_session(&state, move |s| s.act_sound(&name)).await
}

/// A true-colour BMP reduced to 8 bits, to compare with the original before staging it.
#[tauri::command]
async fn reduce_preview(state: State<'_, AppState>, index: usize) -> Res<backend::ReducePreview> {
    with_session(&state, move |s| s.reduce_preview(index)).await
}

#[tauri::command]
async fn stage_reduce(state: State<'_, AppState>, index: usize) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
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
    state: State<'_, AppState>,
    dst: String,
    min_psnr: f64,
) -> Res<backend::ReduceSummary> {
    let plan = with_session(&state, |s| Ok(s.plan())).await?;
    run_job(app, &state, move |ctl| {
        backend::reduce_images(&plan, &PathBuf::from(dst), min_psnr, ctl)
    })
    .await
}

/// Removes the given entries, or everything in the current search when `indices` is empty.
#[tauri::command]
async fn stage_remove(state: State<'_, AppState>, indices: Vec<usize>) -> Res<Vec<PendingRow>> {
    with_session(&state, move |s| {
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
async fn pending(state: State<'_, AppState>) -> Res<Vec<PendingRow>> {
    with_session(&state, |s| Ok(s.pending())).await
}

/// Takes the changes of the draft found when the archive was opened.
#[tauri::command]
async fn restore_draft(state: State<'_, AppState>) -> Res<backend::DraftRestored> {
    with_session(&state, |s| s.restore_draft()).await
}

/// Keeps the draft found when the archive was opened for a later session.
#[tauri::command]
async fn defer_draft(state: State<'_, AppState>) -> Res<()> {
    with_session(&state, |s| {
        s.defer_draft();
        Ok(())
    })
    .await
}

/// Deletes the draft found when the archive was opened.
#[tauri::command]
async fn discard_draft(state: State<'_, AppState>) -> Res<()> {
    with_session(&state, |s| {
        s.discard_draft();
        Ok(())
    })
    .await
}

#[tauri::command]
async fn discard_pending(state: State<'_, AppState>) -> Res<()> {
    with_session(&state, |s| {
        s.discard_pending();
        Ok(())
    })
    .await
}

/// Writes the pending changes as a new archive; the open archive is not touched.
#[tauri::command]
async fn save_edited(app: AppHandle, state: State<'_, AppState>, dst: String) -> Res<EditSummary> {
    let plan = with_session(&state, |s| s.edit_plan()).await?;
    run_job(app, &state, move |ctl| {
        backend::save_edited(&plan, &PathBuf::from(dst), true, ctl)
    })
    .await
}

#[derive(Serialize)]
struct History {
    can_undo: bool,
    can_redo: bool,
}

/// Whether the staged changes can be undone or redone.
#[tauri::command]
async fn history(state: State<'_, AppState>) -> Res<History> {
    with_session(&state, |s| {
        let (can_undo, can_redo) = s.history();
        Ok(History { can_undo, can_redo })
    })
    .await
}

/// Takes back the last staged change; returns the changes that are staged afterwards.
#[tauri::command]
async fn undo_change(state: State<'_, AppState>) -> Res<Vec<PendingRow>> {
    with_session(&state, |s| {
        s.undo();
        Ok(s.pending())
    })
    .await
}

#[tauri::command]
async fn redo_change(state: State<'_, AppState>) -> Res<Vec<PendingRow>> {
    with_session(&state, |s| {
        s.redo();
        Ok(s.pending())
    })
    .await
}

#[tauri::command]
fn cancel_job(state: State<'_, AppState>) {
    if let Ok(job) = state.job.lock()
        && let Some(t) = job.as_ref()
    {
        t.cancel();
    }
}

/// The archive to open at start: one the system handed us ("Open with" on macOS), else a path given
/// on the command line (`rustygrf-gui file.grf`; this is how Windows and Linux pass it).
#[tauri::command]
fn startup_path(state: State<'_, AppState>) -> Option<String> {
    if let Ok(mut opened) = state.opened.lock()
        && let Some(p) = opened.take()
    {
        return Some(p);
    }
    archive_in_args(std::env::args().skip(1))
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

/// The menu bar: the platform's usual menus plus every toolbar command (so the toolbar is never
/// the only place for a command), the list layouts and the preview position, and "Settings..."
/// (in the application menu on macOS, in Edit elsewhere). The page runs the commands; a command
/// whose button is disabled at that moment does nothing.
///
/// Built in one go from filled submenus, like Tauri's own default menu: on Windows a submenu
/// inserted into a finished menu bar does not show up, and Linux has no File menu by default.
fn build_menu(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{AboutMetadata, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
    type Item = Box<dyn IsMenuItem<tauri::Wry>>;
    let item = |id: &str, text: &str, accel: Option<&str>| -> tauri::Result<Item> {
        Ok(Box::new(MenuItem::with_id(
            app,
            format!("{COMMAND_PREFIX}{id}"),
            text,
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
        "Settings...",
        true,
        Some("CmdOrCtrl+,"),
    )?);
    let info = app.package_info();
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        copyright: app.config().bundle.copyright.clone(),
        authors: app.config().bundle.publisher.clone().map(|p| vec![p]),
        ..Default::default()
    };
    let about: Item = Box::new(PredefinedMenuItem::about(app, None, Some(about))?);

    let mut file = vec![
        item("open", "Open...", Some("CmdOrCtrl+O"))?,
        sep()?,
        item("build", "Build...", None)?,
        item("merge", "Merge...", None)?,
        item("extract", "Extract...", None)?,
        item("verify", "Verify", None)?,
        item("changes", "Save Changes as New Archive...", None)?,
        item("reduce-images", "Reduce Images...", None)?,
        sep()?,
        Box::new(PredefinedMenuItem::close_window(app, None)?),
    ];
    let mut edit: Vec<Item> = vec![
        Box::new(PredefinedMenuItem::undo(app, None)?),
        Box::new(PredefinedMenuItem::redo(app, None)?),
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
    ];
    let mut view = vec![
        item("layout-tree", "As Tree", Some("CmdOrCtrl+1"))?,
        item("layout-folders", "As Folders", Some("CmdOrCtrl+2"))?,
        item("layout-list", "As List", Some("CmdOrCtrl+3"))?,
        item("layout-details", "As Details", Some("CmdOrCtrl+4"))?,
        sep()?,
        item("preview-right", "Preview on the Right", None)?,
        item("preview-bottom", "Preview at the Bottom", None)?,
        item("preview-off", "Hide Preview", None)?,
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
        help = Some(submenu("Help", &[about])?);
    }
    menus.push(submenu("File", &file)?);
    menus.push(submenu("Edit", &edit)?);
    menus.push(submenu("View", &view)?);
    menus.push(submenu("Window", &window)?);
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
            if let Some(path) = archive_in_args(args.into_iter().skip(1)) {
                let _ = app.emit("open-file", path);
            }
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            build_menu(app.handle())?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            if id == SETTINGS_MENU_ID {
                let _ = app.emit("open-settings", ());
            } else if let Some(cmd) = id.strip_prefix(COMMAND_PREFIX) {
                let _ = app.emit("menu-command", cmd);
            }
        })
        .invoke_handler(tauri::generate_handler![
            open_archive,
            search,
            page,
            set_names,
            set_view,
            page_view,
            tree_toggle,
            tree_set_all,
            locate,
            folder_entries,
            details,
            preview,
            extract,
            extract_map,
            verify,
            build_archive,
            merge_archives,
            reduce_preview,
            act_sound,
            open_external,
            hex_page,
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
            save_edited,
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
                let path = path.display().to_string();
                if let Ok(mut opened) = app.state::<AppState>().opened.lock() {
                    *opened = Some(path.clone());
                }
                let _ = app.emit("open-file", path);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}

#[cfg(test)]
mod tests {
    use super::archive_in_args;

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
