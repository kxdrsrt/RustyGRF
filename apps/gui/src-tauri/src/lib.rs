//! RustyGRF desktop app: thin Tauri commands around [`backend::Session`].

pub mod backend;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use backend::{ArchiveInfo, Details, ExtractSummary, Preview, Row, Session, VerifySummary};
use tauri::State;

#[derive(Default)]
struct AppState(Arc<Mutex<Option<Session>>>);

type Res<T> = Result<T, String>;

/// Runs `f` on a worker thread with the open session, so a long job does not freeze the window.
async fn with_session<T, F>(state: &State<'_, AppState>, f: F) -> Res<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Session) -> Res<T> + Send + 'static,
{
    let shared = Arc::clone(&state.0);
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
    let shared = Arc::clone(&state.0);
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

/// A GRF path given on the command line (`rustygrf-gui file.grf`), opened by the page at start.
#[tauri::command]
fn startup_path() -> Option<String> {
    std::env::args()
        .skip(1)
        .find(|a| !a.starts_with('-') && PathBuf::from(a).is_file())
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
            startup_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running RustyGRF");
}
