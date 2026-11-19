//! RustyGRF desktop app: thin Tauri commands around [`backend::Session`].

pub mod act_editor;
pub mod backend;
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
    state: State<'_, AppState>,
    path: String,
    password: Option<String>,
) -> Res<ArchiveInfo> {
    let shared = Arc::clone(&state.session);
    tauri::async_runtime::spawn_blocking(move || {
        let key = password
            .filter(|p| !p.is_empty())
            .map(|p| rgrf_core::grfcrypt::key_from_password(&p).map_err(|e| e.to_string()))
            .transpose()?;
        let s = Session::open_with(&PathBuf::from(path), key.as_ref())?;
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
) -> Res<act_editor::ActFrameView> {
    with_session(&state, move |s| s.act_view(index, action, frame)).await
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

/// The system's default menu bar plus every toolbar command (so the toolbar is never the only
/// place for a command), the list layouts and the preview position, and "Settings..." (in the
/// application menu on macOS, in Edit elsewhere). The page runs the commands; a command whose
/// button is disabled at that moment does nothing.
fn build_menu(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
    let menu = Menu::default(app)?;
    let item = |id: &str, text: &str, accel: Option<&str>| {
        MenuItem::with_id(app, format!("{COMMAND_PREFIX}{id}"), text, true, accel)
    };
    let sep = || PredefinedMenuItem::separator(app);
    let find = |name: &str| -> tauri::Result<Option<Submenu<tauri::Wry>>> {
        for kind in menu.items()? {
            if let Some(sub) = kind.as_submenu()
                && sub.text()? == name
            {
                return Ok(Some(sub.clone()));
            }
        }
        Ok(None)
    };

    if let Some(file) = find("File")? {
        let entries = [
            item("open", "Open...", None)?,
            item("build", "Build...", None)?,
            item("merge", "Merge...", None)?,
            item("extract", "Extract...", None)?,
            item("verify", "Verify", None)?,
            item("changes", "Save Changes as New Archive...", None)?,
        ];
        file.insert(&entries[0], 0)?;
        file.insert(&sep()?, 1)?;
        for (k, e) in entries[1..].iter().enumerate() {
            file.insert(e, 2 + k)?;
        }
        file.insert(&sep()?, 2 + entries.len() - 1)?;
    }
    if let Some(edit) = find("Edit")? {
        edit.append(&sep()?)?;
        edit.append(&item("add", "Add File...", None)?)?;
        edit.append(&item("replace", "Replace...", None)?)?;
        edit.append(&item("remove", "Remove", None)?)?;
        edit.append(&item("changes", "Review Changes...", None)?)?;
    }
    if let Some(view) = find("View")? {
        let entries = [
            item("layout-list", "As List", Some("CmdOrCtrl+1"))?,
            item("layout-details", "As Details", Some("CmdOrCtrl+2"))?,
            item("layout-tree", "As Tree", Some("CmdOrCtrl+3"))?,
            item("layout-folders", "As Folders", Some("CmdOrCtrl+4"))?,
        ];
        for (k, e) in entries.iter().enumerate() {
            view.insert(e, k)?;
        }
        view.insert(&sep()?, 4)?;
        view.insert(&item("preview-right", "Preview on the Right", None)?, 5)?;
        view.insert(&item("preview-bottom", "Preview at the Bottom", None)?, 6)?;
        view.insert(&item("preview-off", "Hide Preview", None)?, 7)?;
        view.insert(&sep()?, 8)?;
    }

    let settings = MenuItem::with_id(
        app,
        SETTINGS_MENU_ID,
        "Settings...",
        true,
        Some("CmdOrCtrl+,"),
    )?;
    let home = if cfg!(target_os = "macos") {
        find(&app.package_info().name)?
    } else {
        find("Edit")?
    };
    if let Some(sub) = home {
        // after "About" on macOS, at the end of Edit elsewhere
        let at = if cfg!(target_os = "macos") {
            1
        } else {
            sub.items()?.len()
        };
        sub.insert(&sep()?, at)?;
        sub.insert(&settings, at + 1)?;
    }
    app.set_menu(menu)?;
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
            verify,
            build_archive,
            merge_archives,
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
