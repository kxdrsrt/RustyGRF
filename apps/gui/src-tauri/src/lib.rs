//! RustyGRF desktop app: thin Tauri commands around [`backend::Session`].

pub mod act_editor;
pub mod backend;

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
async fn open_archive(state: State<'_, AppState>, path: String) -> Res<ArchiveInfo> {
    let shared = Arc::clone(&state.session);
    tauri::async_runtime::spawn_blocking(move || {
        let s = Session::open(&PathBuf::from(path))?;
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
    let mut args = std::env::args().skip(1);
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_archive,
            search,
            page,
            set_names,
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
            act_frame,
            stage_act_edit,
            pending,
            discard_pending,
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
