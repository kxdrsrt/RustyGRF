//! Tabs: windows that show one after the other in the same place. Every window keeps its archive,
//! its changes and its jobs (nothing in the page or the session knows about tabs); a group of
//! windows shows one of them and hides the others, and every window draws the strip of its group.
//! Switching copies the frame of the visible window to the one that comes up. A tab is moved to a
//! new window by dragging it out, and into another window's group by dropping it on that window's
//! top. Works the same on every system, which a native tab bar does not.

use std::collections::HashMap;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};

use crate::AppState;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub maximized: bool,
}

#[derive(Default)]
pub struct Tabs {
    groups: Vec<Vec<String>>,
    /// Title and "has unsaved changes" of every window.
    info: HashMap<String, (String, bool)>,
    /// The last frame of a window that was in front, for the tab that takes over when it closes.
    frames: HashMap<String, Frame>,
}

impl Tabs {
    fn group_index(&self, label: &str) -> Option<usize> {
        self.groups
            .iter()
            .position(|g| g.iter().any(|l| l == label))
    }

    /// The labels of the group of `label` in tab order (just `label` when it has no group yet).
    pub fn members(&self, label: &str) -> Vec<String> {
        self.group_index(label)
            .map(|i| self.groups[i].clone())
            .unwrap_or_else(|| vec![label.to_string()])
    }

    /// Puts `label` into the group of `near` after it, or into a group of its own.
    pub fn add(&mut self, label: &str, near: Option<&str>) {
        self.remove(label);
        if let Some(n) = near
            && let Some(i) = self.group_index(n)
        {
            let at = self.groups[i].iter().position(|l| l == n).unwrap_or(0) + 1;
            self.groups[i].insert(at, label.to_string());
            return;
        }
        self.groups.push(vec![label.to_string()]);
    }

    /// Takes `label` out of its group; returns the others and the place it had.
    pub fn remove(&mut self, label: &str) -> (Vec<String>, usize) {
        let Some(i) = self.group_index(label) else {
            return (Vec::new(), 0);
        };
        let at = self.groups[i].iter().position(|l| l == label).unwrap_or(0);
        self.groups[i].retain(|l| l != label);
        let rest = self.groups[i].clone();
        if rest.is_empty() {
            self.groups.remove(i);
        }
        (rest, at)
    }

    fn forget(&mut self, label: &str) {
        self.info.remove(label);
        self.frames.remove(label);
    }
}

#[derive(Serialize)]
pub struct TabOut {
    pub label: String,
    pub title: String,
    pub dirty: bool,
    pub active: bool,
}

fn frame_of(w: &WebviewWindow) -> Option<Frame> {
    let p = w.outer_position().ok()?;
    let s = w.inner_size().ok()?;
    Some(Frame {
        x: p.x,
        y: p.y,
        w: s.width,
        h: s.height,
        maximized: w.is_maximized().unwrap_or(false),
    })
}

fn apply_frame(w: &WebviewWindow, f: Frame) {
    if f.maximized {
        let _ = w.maximize();
    } else {
        let _ = w.unmaximize();
        let _ = w.set_size(PhysicalSize::new(f.w, f.h));
        let _ = w.set_position(PhysicalPosition::new(f.x, f.y));
    }
}

fn lock(state: &AppState) -> std::sync::MutexGuard<'_, Tabs> {
    state.tabs.lock().unwrap_or_else(|e| e.into_inner())
}

/// Tells every window of the group of `label` that its strip changed.
pub fn changed(app: &AppHandle, label: &str) {
    let members = lock(&app.state::<AppState>()).members(label);
    for m in members {
        let _ = app.emit_to(m.as_str(), "tabs-changed", ());
    }
}

/// Shows the window `label` and hides the other windows of its group. The window comes up in the
/// frame of the one that was visible, or in `from`.
pub fn reveal(app: &AppHandle, label: &str, from: Option<Frame>) {
    let Some(w) = app.get_webview_window(label) else {
        return;
    };
    let members = lock(&app.state::<AppState>()).members(label);
    let others: Vec<WebviewWindow> = members
        .iter()
        .filter(|m| *m != label)
        .filter_map(|m| app.get_webview_window(m))
        .collect();
    let shown = others.iter().find(|o| o.is_visible().unwrap_or(false));
    if let Some(f) = shown.and_then(frame_of).or(from)
        && !w.is_visible().unwrap_or(false)
    {
        apply_frame(&w, f);
    }
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
    for o in &others {
        let _ = o.hide();
    }
    changed(app, label);
}

/// The window comes to the front; a tab that is hidden takes the place of the visible one.
pub fn focus(app: &AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label)
        && !w.is_visible().unwrap_or(true)
    {
        reveal(app, label, None);
        return;
    }
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// A new window joins the group of `near` (a tab) or gets a group of its own; it starts in the
/// frame of `near`.
pub fn register(app: &AppHandle, label: &str, near: Option<&str>) {
    lock(&app.state::<AppState>()).add(label, near);
    changed(app, label);
}

/// The frame a new tab next to `near` should open in.
pub fn frame_for_new_tab(app: &AppHandle, near: &str) -> Option<Frame> {
    app.get_webview_window(near).and_then(|w| frame_of(&w))
}

/// A window moved or changed its size: remember the frame of the visible window of a group.
pub fn moved(app: &AppHandle, w: &tauri::Window) {
    if !w.is_visible().unwrap_or(false) {
        return;
    }
    let (Ok(p), Ok(s)) = (w.outer_position(), w.inner_size()) else {
        return;
    };
    let f = Frame {
        x: p.x,
        y: p.y,
        w: s.width,
        h: s.height,
        maximized: w.is_maximized().unwrap_or(false),
    };
    lock(&app.state::<AppState>())
        .frames
        .insert(w.label().to_string(), f);
}

/// A window is gone: its tab leaves the group, and a hidden neighbour takes over its place.
pub fn gone(app: &AppHandle, label: &str) {
    let state = app.state::<AppState>();
    let (rest, at, frame) = {
        let mut t = lock(&state);
        let frame = t.frames.get(label).copied();
        let (rest, at) = t.remove(label);
        t.forget(label);
        (rest, at, frame)
    };
    if rest.is_empty() {
        return;
    }
    let anyone = rest
        .iter()
        .filter_map(|m| app.get_webview_window(m))
        .any(|w| w.is_visible().unwrap_or(false));
    if !anyone {
        let next = rest[at.min(rest.len() - 1)].clone();
        reveal(app, &next, frame);
    } else {
        changed(app, &rest[0]);
    }
}

#[tauri::command]
pub fn tab_set_info(app: AppHandle, window: WebviewWindow, title: String, dirty: bool) {
    let state = app.state::<AppState>();
    {
        let mut t = lock(&state);
        if t.info.get(window.label()) == Some(&(title.clone(), dirty)) {
            return;
        }
        t.info.insert(window.label().to_string(), (title, dirty));
    }
    changed(&app, window.label());
}

#[tauri::command]
pub fn tab_list(window: WebviewWindow, state: State<'_, AppState>) -> Vec<TabOut> {
    let t = lock(&state);
    t.members(window.label())
        .into_iter()
        .map(|l| {
            let (title, dirty) = t.info.get(&l).cloned().unwrap_or_default();
            TabOut {
                active: l == window.label(),
                label: l,
                title,
                dirty,
            }
        })
        .collect()
}

#[tauri::command]
pub fn tab_activate(app: AppHandle, label: String) {
    reveal(&app, &label, None);
}

#[tauri::command]
pub fn tab_close(app: AppHandle, label: String) {
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.close();
    }
}

/// Puts the tab `label` at place `to` of its group.
#[tauri::command]
pub fn tab_move(app: AppHandle, label: String, to: usize) {
    let state = app.state::<AppState>();
    {
        let mut t = lock(&state);
        let Some(i) = t.group_index(&label) else {
            return;
        };
        let g = &mut t.groups[i];
        let Some(at) = g.iter().position(|l| *l == label) else {
            return;
        };
        let item = g.remove(at);
        g.insert(to.min(g.len()), item);
    }
    changed(&app, &label);
}

/// The tab `label` was dropped at the screen position (`x`, `y`) in logical pixels: on the top of
/// another window it joins that window's tabs, anywhere else it becomes a window of its own.
#[tauri::command]
pub fn tab_drop(app: AppHandle, label: String, x: f64, y: f64) {
    let state = app.state::<AppState>();
    let Some(me) = app.get_webview_window(&label) else {
        return;
    };
    let mine = lock(&state).members(&label);
    // a window of another group whose top is under the pointer
    let target = app.webview_windows().into_values().find(|w| {
        if mine.iter().any(|m| m == w.label()) || !w.is_visible().unwrap_or(false) {
            return false;
        }
        let (Ok(p), Ok(s), Ok(scale)) = (w.outer_position(), w.outer_size(), w.scale_factor())
        else {
            return false;
        };
        let (px, py) = (x * scale, y * scale);
        px >= f64::from(p.x)
            && px < f64::from(p.x) + f64::from(s.width)
            && py >= f64::from(p.y)
            && py < f64::from(p.y) + 150.0 * scale
    });
    let my_frame = frame_of(&me);
    let was_visible = me.is_visible().unwrap_or(false);
    let (rest, at) = lock(&state).remove(&label);
    if was_visible && !rest.is_empty() {
        let next = rest[at.min(rest.len() - 1)].clone();
        // the neighbour takes the place the dragged tab had
        reveal(&app, &next, my_frame);
    }
    match target {
        Some(t) => {
            lock(&state).add(&label, Some(t.label()));
            let _ = me.hide();
            reveal(&app, &label, None);
        }
        None => {
            lock(&state).add(&label, None);
            if let (Ok(scale), Some(f)) = (me.scale_factor(), my_frame) {
                let _ = me.unmaximize();
                let _ = me.set_size(PhysicalSize::new(f.w, f.h));
                let _ = me.set_position(PhysicalPosition::new(
                    (x * scale) as i32 - 80,
                    (y * scale) as i32 - 20,
                ));
            }
            let _ = me.show();
            let _ = me.set_focus();
            for m in rest {
                changed(&app, &m);
            }
            changed(&app, &label);
        }
    }
}

/// Puts every window into the group of this one.
#[tauri::command]
pub fn tab_merge_all(app: AppHandle, window: WebviewWindow) {
    let state = app.state::<AppState>();
    let me = window.label().to_string();
    let others: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|l| **l != me)
        .cloned()
        .collect();
    let mut sorted = others;
    sorted.sort();
    {
        let mut t = lock(&state);
        let mut last = me.clone();
        for l in &sorted {
            if t.members(&me).contains(l) {
                last = l.clone();
                continue;
            }
            t.add(l, Some(&last));
            last = l.clone();
        }
    }
    for l in &sorted {
        if let Some(w) = app.get_webview_window(l) {
            let _ = w.hide();
        }
    }
    let _ = window.show();
    changed(&app, &me);
}

#[tauri::command]
pub fn set_tabs_enabled(state: State<'_, AppState>, on: bool) {
    state
        .tabs_off
        .store(!on, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_join_leave_and_keep_their_order() {
        let mut t = Tabs::default();
        t.add("a", None);
        t.add("b", Some("a"));
        t.add("c", Some("a"));
        // a new tab comes right after the one it was opened from
        assert_eq!(t.members("a"), ["a", "c", "b"]);
        t.add("d", None);
        assert_eq!(t.members("d"), ["d"]);
        assert_eq!(t.members("unknown"), ["unknown"]);
        // leaving tells which neighbours remain and where the tab was
        let (rest, at) = t.remove("c");
        assert_eq!((rest, at), (vec!["a".to_string(), "b".to_string()], 1));
        // joining another group takes the tab out of the old one
        t.add("b", Some("d"));
        assert_eq!(t.members("a"), ["a"]);
        assert_eq!(t.members("d"), ["d", "b"]);
        // the last tab of a group takes the group with it
        t.remove("a");
        assert!(t.group_index("a").is_none());
    }
}
