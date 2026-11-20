"use strict";

const T = window.__TAURI__;
const invoke = (cmd, args) => T.core.invoke(cmd, args);
let ROW_H = 24; // changed by the list density setting
const OVERSCAN = 6;
const $ = (id) => document.getElementById(id);

const SVGNS = "http://www.w3.org/2000/svg";
/** An icon from the symbol sprite in index.html. */
function svgIcon(name, cls = "") {
  const svg = document.createElementNS(SVGNS, "svg");
  svg.setAttribute("class", `ic ${cls}`.trim());
  svg.setAttribute("aria-hidden", "true");
  const use = document.createElementNS(SVGNS, "use");
  use.setAttribute("href", `#i-${name}`);
  svg.append(use);
  return svg;
}
const KIND_ICON = { image: "image", sprite: "sprite", animation: "animation", palette: "palette", map: "map", model: "model", audio: "audio", text: "text", effect: "effect", imf: "file", file: "file", video: "video" };
const kindIcon = (kind) => el("span", { class: `icon k-${KIND_ICON[kind] || "file"}` }, svgIcon(KIND_ICON[kind] || "file"));

const S = {
  info: null,
  total: 0, // lines in the list (folders count in a tree)
  matches: 0, // entries that match the search and filters
  offset: 0, // position of rows[0] in the list
  rows: [],
  selected: null, // position in the filtered list
  query: "",
  loading: false,
  pending: false,
  previewToken: 0,
  changes: [], // changes waiting to be saved
  changed: new Map(), // lower-case path -> replace | add | remove
};

function el(tag, attrs = {}, ...children) {
  const n = document.createElement(tag);
  let iconName = null;
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") n.className = v;
    else if (k === "icon") iconName = v;
    else if (k === "text") n.textContent = v;
    else if (k.startsWith("on")) n.addEventListener(k.slice(2), v);
    else if (v !== false && v != null) n.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat()) if (c != null) n.append(c);
  if (iconName) n.prepend(svgIcon(iconName));
  return n;
}

function fmtBytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

const fmtNum = (n) => n.toLocaleString("en-US");
const setStatus = (t) => ($("status").textContent = t);
/** An error from the backend as text; coded ones (a sentinel, then JSON) are translated. */
const errText = (e) => {
  const text = typeof e === "string" ? e : e?.message || JSON.stringify(e);
  return text.startsWith("\u0001") ? noteText(text) : text;
};

// ---------- dialogs ----------

/** The icon of a message: what the title and the buttons suggest. */
function dialogKind(title, buttons) {
  if (/^(could not|cannot|error)|failed|damaged/i.test(title)) return "error";
  if (/^(remove|open another|discard|overwrite|delete)/i.test(title)) return "warning";
  if (/encrypted|password/i.test(title)) return "lock";
  if (/^(extracted|saved|done|verified|written)/i.test(title)) return "success";
  return buttons.length > 1 ? "question" : "info";
}

// buttons that appear in many dialogs come from the catalog
const COMMON_LABELS = { OK: "dlg-ok", Cancel: "dlg-cancel", Close: "dlg-close", Done: "dlg-done", Quick: "dlg-quick", Full: "dlg-full" };
const label = (x) => (COMMON_LABELS[x] ? t(COMMON_LABELS[x]) : x);

function ask(title, body, buttons, kind) {
  return new Promise((resolve) => {
    const dlg = $("dlg");
    const k = kind || dialogKind(title, buttons);
    $("dlg-title").replaceChildren(svgIcon(k, `dlg-ic k-${k}`), title);
    const b = $("dlg-body");
    b.replaceChildren(...(Array.isArray(body) ? body : [body]).filter(Boolean));
    const actions = $("dlg-actions");
    actions.replaceChildren(
      ...buttons.map((x) =>
        el("button", {
          type: "button",
          class: x.primary ? "primary" : "",
          text: label(x.label),
          onclick: () => {
            dlg.close();
            resolve(x.value);
          },
        }),
      ),
    );
    dlg.addEventListener("cancel", () => resolve(null), { once: true });
    dlg.showModal();
  });
}

const note = (title, text, isError = false) =>
  ask(title, el("p", { class: isError ? "error" : "", text }), [{ label: "OK", value: true, primary: true }], isError ? "error" : undefined);

// ---------- archive ----------

async function chooseArchive() {
  const path = await T.dialog.open({
    multiple: false,
    directory: false,
    filters: [{ name: "GRF, GPF, Thor and RGZ archives", extensions: ["grf", "gpf", "thor", "rgz"] }],
  });
  if (path) await openPath(Array.isArray(path) ? path[0] : path);
}

// An archive with GRF Editor's encryption needs its password: ask for it (again after a wrong one).
async function openWithKey(path) {
  let password = null;
  let problem = "";
  for (;;) {
    let info = null;
    try {
      info = await invoke("open_archive", { path, password });
      if (!info.encryption?.key_needed) return info;
    } catch (e) {
      // only a missing or wrong key is asked about again
      if (!/key/i.test(errText(e)) || !/encrypted|CRC32/i.test(errText(e))) throw e;
      problem = /CRC32/.test(errText(e)) ? t("enc-wrong") : "";
    }
    if (info && !problem && password !== null) problem = t("enc-wrong");
    const input = el("input", { type: "password", class: "field", autocomplete: "off", spellcheck: "false" });
    const answer = await ask(
      t("enc-title"),
      [
        el("p", { text: t("enc-body") }),
        problem ? el("p", { class: "error", text: problem }) : null,
        input,
      ],
      [
        { label: t("enc-skip"), value: "skip" },
        { label: t("pvw-open"), value: "ok", primary: true },
      ],
    );
    if (answer === "skip" || answer === null) {
      if (info) return info;
      throw new Error(t("enc-needs"));
    }
    password = input.value;
  }
}

/** The backend refused because another window shows the archive (and brought that window forward). */
const isOpenElsewhere = (e) => typeof e === "string" && e.includes('"err-open-elsewhere"');
/** A new archive made by File › New: an empty GRF in the app's folder, named for its type. */
const untitledKind = (path) => /untitled-\d+\.(grf|gpf|rgz|thor)$/i.exec(path || "")?.[1].toLowerCase() || null;

async function openPath(path, force = false) {
  // a window shows one archive: another one opens in a new window (or comes forward in its own)
  if (S.info && !force && path !== S.info.path) {
    try {
      if (await invoke("open_in_window", { path })) return;
    } catch {}
  }
  if (!force && S.changes.length && SET.warnDiscard) {
    const go = await ask(t("open-another-title"), el("p", { text: t("open-another-body", { n: S.changes.length }) }), [{ label: t("pvw-open"), value: true }, { label: "Cancel", value: null, primary: true }]);
    if (!go) return;
  }
  setStatus(t("open-running", { path }));
  let info;
  try {
    info = await openWithKey(path);
  } catch (e) {
    if (isOpenElsewhere(e)) {
      setStatus(S.info ? "" : t("open-elsewhere"));
      return;
    }
    setStatus(t("open-failed-status"));
    await note(t("open-failed"), errText(e), true);
    return;
  }
  await showOpened(info);
}

/** Puts a freshly opened (or saved and reopened) archive on the window. */
async function showOpened(info) {
  const path = info.path;
  S.info = info;
  S.untitled = untitledKind(path);
  if (S.untitled) S.info.file_name = `${t("untitled")}.${S.untitled}`;
  $("empty").hidden = true;
  $("lists").hidden = false;
  for (const id of ["search", "btn-extract", "btn-verify", "names", "btn-add", "btn-changes", "sort", "sort-dir", "size-filter"]) $(id).disabled = false;
  V.folder = null;
  buildKindFilter();
  syncViewBar();
  THUMB.reset();
  S.changes = [];
  S.changed = new Map();
  updateTitle();
  updateEditBar();
  $("search").value = "";
  $("names").value = SET.names;
  S.query = "";
  if (SET.names !== "cp949") await invoke("set_names", { mode: SET.names });
  if (!S.untitled) {
    SET.lastArchive = path;
    addRecent(path);
  }
  await runSearch("");
  setStatus(
    t("open-status", { name: S.info.file_name, format: S.info.format, files: t("pvw-files", { n: S.info.files }), size: fmtBytes(S.info.file_size) }),
  );
  if (S.info.draft) await offerDraft(S.info.draft);
  $("viewport").focus();
}

/** Closes the archive of this window; the window stays and shows the start page. */
async function closeArchive() {
  if (!S.info) return;
  if (S.changes.length && SET.warnDiscard) {
    const go = await ask(t("close-title"), el("p", { text: t("close-body", { n: S.changes.length }) }), [{ label: t("close-anyway"), value: true }, { label: "Cancel", value: null, primary: true }], "warning");
    if (!go) return;
  }
  await invoke("close_archive").catch(() => {});
  S.viewer?.destroy();
  S.viewer = null;
  Object.assign(S, { info: null, untitled: null, total: 0, matches: 0, offset: 0, rows: [], selected: null, query: "", changes: [], changed: new Map() });
  THUMB.reset();
  $("empty").hidden = false;
  $("lists").hidden = true;
  for (const id of ["search", "btn-extract", "btn-verify", "names", "btn-add", "btn-changes", "btn-replace", "sort", "sort-dir", "size-filter"]) $(id).disabled = true;
  $("search").value = "";
  $("count").textContent = "";
  clearPreview();
  updateEditBar();
  updateTitle();
  setStatus("");
}

// ---------- recent archives, menu ----------

/** The recent archives, read fresh: another window may have opened one meanwhile. */
function recentList() {
  try {
    const stored = JSON.parse(localStorage.getItem(SETTINGS_KEY) || "{}").recent;
    return Array.isArray(stored) ? stored : SET.recent || [];
  } catch {
    return SET.recent || [];
  }
}

function addRecent(path) {
  SET.recent = [path, ...recentList().filter((p) => p !== path)].slice(0, 10);
  saveSettings();
  syncMenu();
}

function clearRecent() {
  SET.recent = [];
  saveSettings();
  syncMenu();
}

/** The native menu in the page's language, with the recent archives. */
function syncMenu() {
  invoke("set_menu_labels", { labels: I18N.menuLabels(), recent: recentList() }).catch(() => {});
}

async function newArchive(kind) {
  try {
    await openPath(await invoke("new_archive", { kind }));
  } catch (e) {
    await note(t("new-failed"), errText(e), true);
  }
}

/** Unsaved changes of an earlier session were found for this archive: restore or discard them. */
async function offerDraft(d) {
  const when = new Date(d.saved_at * 1000).toLocaleString();
  const body = [el("p", { text: t("draft-body", { n: d.changes, when }) })];
  if (d.archive_changed) body.push(el("p", { class: "error", text: t("draft-archive-changed") }));
  if (d.missing_files) body.push(el("p", { class: "muted", text: t("draft-missing", { n: d.missing_files }) }));
  if (d.more) body.push(el("p", { class: "muted", text: t("draft-more", { n: d.more }) }));
  body.push(el("p", { class: "muted", text: t("draft-later-note") }));
  const act = await ask(t("draft-title"), body, [{ label: t("draft-restore"), value: "restore", primary: true }, { label: t("draft-discard"), value: "discard" }, { label: t("draft-later"), value: null }]);
  try {
    if (act === "restore") {
      const r = await invoke("restore_draft");
      setPending(await invoke("pending"));
      refreshPreview();
      setStatus(r.skipped ? t("draft-restored-skipped", { restored: t("draft-restored", { n: r.restored }), skipped: r.skipped }) : t("draft-restored", { n: r.restored }));
    } else if (act === "discard") {
      await invoke("discard_draft");
      setStatus(t("draft-discarded"));
    } else {
      await invoke("defer_draft");
      setStatus(t("draft-kept"));
    }
  } catch (e) {
    await note(t("draft-failed"), errText(e), true);
  }
}

// ---------- lists ----------

const KINDS = ["image", "sprite", "animation", "palette", "map", "model", "audio", "text", "effect", "file"];
const kindName = (k) => (KINDS.includes(k) ? t(`kind-${k}`) : k);
/** The list layouts in the order of the view bar and of Cmd/Ctrl+1–4. */
const LAYOUTS = ["tree", "details", "folders", "gallery"];
const SIZE_UNITS = { k: 1024, m: 1024 * 1024 };
const PREFS_KEY = "rustygrf-view";

const V = Object.assign(
  { layout: "tree", sort: "archive", desc: false, kinds: [], size: "", recursive: false, preview: "right", folder: null, listW: null, previewH: null, foldersW: null },
  (() => {
    try {
      return JSON.parse(localStorage.getItem(PREFS_KEY) || "{}");
    } catch {
      return {};
    }
  })(),
);
V.folder = null; // the chosen folder is per archive
if (!LAYOUTS.includes(V.layout)) V.layout = V.layout === "list" ? "details" : "tree"; // "list" was merged into "details"
const savePrefs = () => {
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify({ ...V, folder: null }));
  } catch {}
};

const F = { total: 0, offset: 0, rows: [], selected: null, loading: false, pending: false }; // the folder pane

/** A virtual list: only the visible lines exist; pages come from the backend (`page_view`). */
class VList {
  constructor(st, ids, view, renderRow, onPick) {
    Object.assign(this, { st, view, renderRow, onPick, prefix: ids.rows });
    this.vp = $(ids.vp);
    this.spacer = $(ids.spacer);
    this.rowsEl = $(ids.rows);
  }
  /** Lines of one row each, or (gallery) cells in `cols` columns: `{cols, w, h}`. */
  geom() {
    return this.grid || { cols: 1, w: 0, h: ROW_H };
  }
  spacerHeight(total) {
    const { cols, h } = this.geom();
    return Math.ceil(total / cols) * h;
  }
  range() {
    const { cols, h } = this.geom();
    const over = this.grid ? 2 : OVERSCAN;
    const first = Math.max(0, (Math.floor(this.vp.scrollTop / h) - over) * cols);
    const last = Math.min(this.st.total, (Math.ceil((this.vp.scrollTop + this.vp.clientHeight) / h) + over) * cols);
    return [first, last];
  }
  row(pos) {
    return this.st.rows[pos - this.st.offset];
  }
  async load() {
    const st = this.st;
    if (st.loading) {
      st.pending = true;
      return;
    }
    st.loading = true;
    try {
      do {
        st.pending = false;
        const [first, last] = this.range();
        const want = Math.max(last - first, 20);
        const offset = Math.max(0, first - want);
        if (st.total === 0) {
          st.rows = [];
          st.offset = 0;
        } else {
          st.rows = await invoke("page_view", { view: this.view, offset, limit: want * 3 });
          st.offset = offset;
        }
        this.draw();
      } while (st.pending);
    } catch (e) {
      setStatus(`Error: ${errText(e)}`);
    } finally {
      st.loading = false;
    }
  }
  draw() {
    const [first, last] = this.range();
    const out = [];
    const tree = this.vp.getAttribute("role") === "tree";
    for (let pos = first; pos < last; pos++) {
      const r = this.row(pos);
      if (!r) continue;
      const e = this.renderRow(r, pos);
      e.id = `${this.prefix}-${pos}`;
      e.setAttribute("role", tree ? "treeitem" : "option");
      e.setAttribute("aria-selected", String(pos === this.st.selected));
      if (tree) {
        e.setAttribute("aria-level", String((r.depth || 0) + 1));
        if (r.folder) e.setAttribute("aria-expanded", String(!!r.expanded));
      }
      e.setAttribute("aria-label", rowLabel(r));
      out.push(e);
    }
    this.rowsEl.style.top = "0";
    this.rowsEl.replaceChildren(...out);
    this.onDraw?.();
    if (this.st.selected != null) this.vp.setAttribute("aria-activedescendant", `${this.prefix}-${this.st.selected}`);
    else this.vp.removeAttribute("aria-activedescendant");
  }
  scrolled() {
    this.draw();
    const st = this.st;
    const [first, last] = this.range();
    if ((first < st.offset && st.offset > 0) || (last > st.offset + st.rows.length && st.offset + st.rows.length < st.total)) this.load();
  }
  async reset(total, keepScroll = false) {
    const st = this.st;
    st.total = total;
    st.rows = [];
    st.offset = 0;
    this.spacer.style.height = `${this.spacerHeight(total)}px`;
    if (!keepScroll) this.vp.scrollTop = 0;
    await this.load();
  }
  scrollTo(pos) {
    const { cols, h } = this.geom();
    const top = Math.floor(pos / cols) * h;
    const vp = this.vp;
    if (top < vp.scrollTop) vp.scrollTop = top;
    else if (top + h > vp.scrollTop + vp.clientHeight) vp.scrollTop = top + h - vp.clientHeight;
  }
  async pick(pos, scroll = true) {
    const st = this.st;
    if (st.total === 0) return;
    pos = Math.max(0, Math.min(st.total - 1, pos));
    st.selected = pos;
    if (scroll) this.scrollTo(pos);
    this.draw();
    let r = this.row(pos);
    if (!r) {
      await this.load();
      r = this.row(pos);
    }
    if (r) this.onPick(r, pos);
  }
}

/** What a screen reader says for a line. */
function rowLabel(r) {
  const base = r.name.slice(r.name.lastIndexOf("\\") + 1);
  return r.folder ? `${base}, folder, ${fmtNum(r.count)} files, ${fmtBytes(r.bytes)}${r.expanded ? ", open" : ", closed"}` : `${base}, ${r.kind}, ${fmtBytes(r.size)}`;
}

const baseOf = (name) => name.slice(name.lastIndexOf("\\") + 1);
const dirOf = (name) => (name.includes("\\") ? name.slice(0, name.lastIndexOf("\\")) : "");
const extOf = (name) => (baseOf(name).includes(".") ? baseOf(name).split(".").pop() : "");

function mainRow(r, pos) {
  if (V.layout === "gallery") return galleryCell(r, pos);
  const sel = pos === S.selected ? " selected" : "";
  const tree = V.layout === "tree";
  const indent = tree ? `padding-left:${8 + r.depth * 16}px;` : "";
  if (r.folder) {
    return el(
      "div",
      { class: `row tree details folder${sel}`, style: `top:${pos * ROW_H}px;${indent}`, title: r.name, "data-pos": pos },
      el("span", { class: `icon twisty${r.expanded ? " open" : ""}`, "data-twist": "1" }, svgIcon("chevron")),
      el("span", { class: "name" }, svgIcon(r.expanded ? "folder-open" : "folder", "folder-ic"), baseOf(r.name), el("span", { class: "count", text: fmtNum(r.count) })),
      el("span", { class: "ext" }),
      el("span", { class: "size", text: fmtBytes(r.bytes) }),
      el("span", { class: "stored" }),
    );
  }
  const key = changeKey(r.name);
  const state = S.changed.has(key) ? ` changed ${S.changed.get(key)}` : "";
  const base = baseOf(r.name);
  const dir = dirOf(r.name);
  const cls = `row details${sel}${state}${tree ? " tree" : ""}`;
  const icon = kindIcon(r.kind);
  return el(
    "div",
    { class: cls, style: `top:${pos * ROW_H}px;${indent}`, title: r.name, "data-pos": pos },
    icon,
    el("span", { class: "name" }, base, V.layout === "details" && dir ? el("span", { class: "muted", text: `  ${dir}` }) : null),
    el("span", { class: "ext", text: extOf(r.name) }),
    el("span", { class: "size", text: fmtBytes(r.size) }),
    el("span", { class: "stored", text: fmtBytes(r.stored) }),
  );
}

const PICTURE_KINDS = ["image", "sprite"];
const THUMB_EXTS = new Set(["bmp", "tga", "png", "jpg", "jpeg", "gif", "spr"]);
const THUMB_SIDE = 160;
const THUMB_KEEP = 1500;
const CELL_MIN_W = 124;
const CELL_H = 136;

/** Thumbnails of the gallery: made by the backend, kept here (index -> thumbnail or its error). */
const THUMB = {
  cache: new Map(),
  pending: new Set(),
  gen: 0,
  timer: null,
  reset() {
    this.cache.clear();
    this.pending.clear();
    this.gen++;
  },
};

function fillThumb(box, r, t) {
  if (t?.data) {
    box.replaceChildren(el("img", { src: `data:${t.mime};base64,${t.data}`, alt: "", draggable: "false" }));
  } else {
    box.replaceChildren(kindIcon(r.kind));
  }
}

function galleryCell(r, pos) {
  const { cols, w, h } = mainList.grid;
  const t = THUMB.cache.get(r.index);
  const key = changeKey(r.name);
  const state = S.changed.has(key) ? ` changed ${S.changed.get(key)}` : "";
  const box = el("span", { class: "gthumb", "data-thumb": r.index });
  fillThumb(box, r, t);
  const sub = t?.width ? `${t.width} × ${t.height}` : fmtBytes(r.size);
  return el(
    "div",
    { class: `row gcell${pos === S.selected ? " selected" : ""}${state}`, style: `top:${Math.floor(pos / cols) * h}px;left:${(pos % cols) * w}px;width:${w}px;height:${h}px`, title: r.name, "data-pos": pos },
    box,
    el("span", { class: "gname", text: baseOf(r.name) }),
    el("span", { class: "gsub", text: sub }),
  );
}

/** Columns and cell size of the gallery for the width of the list (none in the other layouts). */
function setGrid() {
  const was = mainList.grid;
  if (V.layout !== "gallery") {
    mainList.grid = null;
  } else {
    const width = mainList.vp.clientWidth || 600;
    const cols = Math.max(1, Math.floor(width / CELL_MIN_W));
    mainList.grid = { cols, w: Math.floor(width / cols), h: CELL_H };
  }
  mainList.onDraw = mainList.grid ? scheduleThumbs : null;
  const now = mainList.grid;
  if (S.info && (was?.cols !== now?.cols || was?.w !== now?.w || !was !== !now)) {
    mainList.spacer.style.height = `${mainList.spacerHeight(S.total)}px`;
    mainList.draw();
  }
}

function scheduleThumbs() {
  clearTimeout(THUMB.timer);
  THUMB.timer = setTimeout(loadThumbs, 60);
}

async function loadThumbs() {
  if (!mainList.grid || !S.info) return;
  const [first, last] = mainList.range();
  const want = [];
  for (let p = first; p < last; p++) {
    const r = mainList.row(p);
    if (r && !r.folder && THUMB_EXTS.has(extOf(r.name).toLowerCase()) && !THUMB.cache.has(r.index) && !THUMB.pending.has(r.index)) want.push(r.index);
  }
  if (!want.length) return;
  const gen = THUMB.gen;
  for (const i of want) THUMB.pending.add(i);
  let made = [];
  try {
    made = await invoke("thumbnails", { entries: want, side: THUMB_SIDE });
  } catch {}
  if (gen !== THUMB.gen) return;
  for (const i of want) THUMB.pending.delete(i);
  for (const t of made) {
    THUMB.cache.set(t.index, t);
    if (THUMB.cache.size > THUMB_KEEP) THUMB.cache.delete(THUMB.cache.keys().next().value);
    const box = mainList.rowsEl.querySelector(`.gthumb[data-thumb="${t.index}"]`);
    if (!box) continue;
    const r = mainList.row(Number(box.parentElement.dataset.pos));
    if (!r) continue;
    fillThumb(box, r, t);
    if (t.width) box.parentElement.querySelector(".gsub").textContent = `${t.width} × ${t.height}`;
  }
  // a thumbnail that came late may belong to a cell that was scrolled in meanwhile
  if (made.length) scheduleThumbs();
}

new ResizeObserver(() => {
  if (V.layout === "gallery") setGrid();
}).observe($("viewport"));

function folderRow(r, pos) {
  const on = V.folder != null && r.name.toLowerCase() === V.folder.toLowerCase();
  return el(
    "div",
    { class: `row tree folder${on ? " selected" : ""}`, style: `top:${pos * ROW_H}px;padding-left:${8 + r.depth * 16}px`, title: r.name, "data-pos": pos },
    el("span", { class: `icon twisty${r.expanded ? " open" : ""}`, "data-twist": "1" }, svgIcon("chevron")),
    el("span", { class: "name" }, svgIcon(r.expanded ? "folder-open" : "folder", "folder-ic"), baseOf(r.name), el("span", { class: "count", text: fmtNum(r.count) })),
    el("span", { class: "size", text: "" }),
  );
}

const mainList = new VList(S, { vp: "viewport", spacer: "spacer", rows: "rows" }, 0, mainRow, (r) => {
  updateEditBar();
  if (V.preview === "off") return;
  if (r.folder) showFolderPreview(r);
  else showPreview(r);
});
const folderList = new VList(F, { vp: "folder-viewport", spacer: "folder-spacer", rows: "folder-rows" }, 1, folderRow, (r, pos) => {
  V.folder = r.name;
  F.selected = pos;
  folderList.draw();
  $("folder-all").classList.remove("on");
  applyView({ keepSelection: false, pane: false });
});

const visibleRange = () => mainList.range();
const loadWindow = () => mainList.load();
const render = () => mainList.draw();
const onScroll = () => mainList.scrolled();
const select = (pos, scroll = true) => mainList.pick(pos, scroll);

const isTreeLayout = () => V.layout === "tree";

function sizeLimits() {
  const m = /^(?:(\d+)([km]))?-(?:(\d+)([km]))?$/.exec(V.size);
  if (!m) return [0, 0];
  const n = (v, u) => (v ? Number(v) * SIZE_UNITS[u] : 0);
  return [n(m[1], m[2]), n(m[3], m[4])];
}

function mainSpec(autoExpand) {
  const [min, max] = sizeLimits();
  const folders = V.layout === "folders";
  return {
    query: S.query,
    kinds: V.layout === "gallery" && !V.kinds.length ? PICTURE_KINDS : V.kinds,
    min_size: min,
    max_size: max,
    sort: V.sort,
    desc: V.desc,
    layout: isTreeLayout() ? "tree" : "flat",
    folder: folders ? V.folder : null,
    recursive: V.recursive,
    auto_expand: autoExpand,
  };
}

const filtersActive = () => V.kinds.length > 0 || V.size !== "" || S.query !== "";

/** Arranges the lists after the search, filters, sort or layout changed. */
let applySeq = 0;
async function applyView({ keepSelection = true, autoExpand = false, pane = true } = {}) {
  if (!S.info) return;
  const seq = ++applySeq;
  const before = keepSelection && S.selected != null ? mainList.row(S.selected) : null;
  const sum = await invoke("set_view", { view: 0, spec: mainSpec(autoExpand) });
  if (seq !== applySeq) return; // a newer change is on its way (fast arrow keys)
  S.matches = sum.matches;
  S.selected = null;
  updateEditBar();
  let rows = sum.rows;
  let line = null;
  if (before && !before.folder) {
    const at = await invoke("locate", { view: 0, entry: before.index, folder: null });
    if (at.line != null) {
      rows = at.rows;
      line = at.line;
    }
  }
  if (seq !== applySeq) return;
  await mainList.reset(rows);
  if (line != null) {
    S.selected = line;
    mainList.scrollTo(line);
    mainList.draw();
  } else {
    clearPreview();
  }
  if (V.layout === "folders" && pane) await applyFolderPane(autoExpand);
  $("count").textContent = filtersActive() ? t("count-filtered", { n: S.matches, total: S.info.entries }) : t("count-entries", { n: S.matches });
  $("btn-reset-filters").disabled = !filtersActive();
  const note = V.layout === "folders" && S.query ? ` ${t("count-note-folders")}` : V.layout === "gallery" && !V.kinds.length ? ` ${t("count-note-pictures")}` : "";
  if (note) $("count").textContent += note;
}

async function applyFolderPane(autoExpand) {
  const [min, max] = sizeLimits();
  const sum = await invoke("set_view", { view: 1, spec: { query: S.query, kinds: V.kinds, min_size: min, max_size: max, sort: "name", layout: "folders", auto_expand: autoExpand } });
  F.selected = null;
  await folderList.reset(sum.rows, !autoExpand);
  $("folder-all").classList.toggle("on", V.folder == null);
}

async function runSearch(q) {
  S.query = q;
  await applyView({ keepSelection: false, autoExpand: true });
}

async function toggleFolder(list, pos) {
  const r = list.row(pos);
  if (!r?.folder) return;
  const total = await invoke("tree_toggle", { view: list.view, path: r.name });
  const top = list.vp.scrollTop;
  list.st.total = total;
  list.spacer.style.height = `${list.spacerHeight(total)}px`;
  list.st.rows = [];
  list.vp.scrollTop = top;
  await list.load();
  const cur = list === mainList && list.st.selected != null ? list.row(list.st.selected) : null;
  if (cur?.folder && V.preview !== "off") showFolderPreview(cur);
}

async function expandAll(open, which) {
  const list = which || (V.layout === "folders" ? folderList : mainList);
  const total = await invoke("tree_set_all", { view: list.view, open });
  list.st.total = total;
  list.spacer.style.height = `${list.spacerHeight(total)}px`;
  list.st.rows = [];
  list.vp.scrollTop = 0;
  await list.load();
  if (list === mainList) {
    S.selected = null;
    updateEditBar();
  }
}

function setLayout(layout) {
  V.layout = layout;
  savePrefs();
  syncViewBar();
  if (S.info) applyView({ autoExpand: layout === "folders" || layout === "tree" });
}

function syncViewBar() {
  for (const b of document.querySelectorAll("#view-seg button")) b.classList.toggle("on", b.dataset.layout === V.layout);
  for (const b of document.querySelectorAll("#preview-seg button")) b.classList.toggle("on", b.dataset.preview === V.preview);
  const folders = V.layout === "folders";
  $("lists").classList.toggle("with-folders", folders);
  $("folder-pane").hidden = !folders;
  $("split-folders").hidden = !folders;
  $("colhead").hidden = V.layout === "gallery";
  $("recursive-wrap").hidden = !folders;
  $("recursive").checked = V.recursive;
  $("btn-expand").hidden = !(V.layout === "tree" || folders);
  $("btn-collapse").hidden = !(V.layout === "tree" || folders);
  $("sort").value = V.sort;
  $("sort-dir-use").setAttribute("href", V.desc ? "#i-sort-desc" : "#i-sort-asc");
  $("size-filter").value = V.size;
  for (const b of document.querySelectorAll("#colhead button")) b.classList.toggle("on", b.dataset.sort === V.sort);
  const kinds = $("kind-label");
  kinds.textContent = V.layout === "gallery" && !V.kinds.length ? t("kind-label-pictures") : V.kinds.length ? (V.kinds.length === 1 ? t("kind-label-one", { kind: kindName(V.kinds[0]) }) : t("kind-label-n", { n: V.kinds.length })) : t("kind-label-all");
  $("kind-filter").querySelector("summary").classList.toggle("active", V.kinds.length > 0);
  $("viewport").setAttribute("role", V.layout === "tree" ? "tree" : "listbox");
  setGrid();
  for (const b of document.querySelectorAll("#view-seg button")) b.setAttribute("aria-pressed", String(b.dataset.layout === V.layout));
  for (const b of document.querySelectorAll("#preview-seg button")) b.setAttribute("aria-pressed", String(b.dataset.preview === V.preview));
  const main = document.querySelector("main");
  main.className = `pv-${V.preview}${folders ? " lay-folders" : ""}`;
  applyPaneSizes();
}

function applyPaneSizes() {
  const main = document.querySelector("main");
  main.style.removeProperty("--list-w");
  main.style.removeProperty("--prev-h");
  if (V.preview === "right" && V.listW) main.style.setProperty("--list-w", `${V.listW}px`);
  if (V.preview === "bottom" && V.previewH) main.style.setProperty("--prev-h", `${V.previewH}px`);
  if (V.foldersW) $("lists").style.setProperty("--folders-w", `${V.foldersW}px`);
}

function setPreviewMode(mode) {
  V.preview = mode;
  savePrefs();
  syncViewBar();
  if (mode !== "off") {
    const r = S.selected != null ? mainList.row(S.selected) : null;
    if (r) (r.folder ? showFolderPreview(r) : showPreview(r));
  }
}

function buildKindFilter() {
  const box = $("kind-list");
  box.replaceChildren(
    ...KINDS.map((k) => el("label", {}, el("input", { type: "checkbox", value: k, checked: V.kinds.includes(k), onchange: onKindChange }), kindName(k))),
    el("div", { class: "links" }, el("a", { text: t("kind-all"), onclick: () => setKinds([]) }), el("a", { text: t("kind-only-images"), onclick: () => setKinds(["image"]) })),
  );
}

function onKindChange() {
  setKinds([...document.querySelectorAll("#kind-list input:checked")].map((i) => i.value));
}

function setKinds(kinds) {
  V.kinds = kinds;
  savePrefs();
  buildKindFilter();
  syncViewBar();
  if (S.info) applyView({ autoExpand: true });
}

function setSort(key, desc) {
  V.sort = key;
  if (desc !== undefined) V.desc = desc;
  savePrefs();
  syncViewBar();
  if (S.info) applyView();
}

function resetFilters() {
  V.kinds = [];
  V.size = "";
  S.query = "";
  $("search").value = "";
  savePrefs();
  buildKindFilter();
  syncViewBar();
  applyView({ keepSelection: false, autoExpand: true });
}

/** A line shown for a folder selected in the tree. */
function showFolderPreview(r) {
  ++S.previewToken;
  S.viewer?.destroy();
  S.viewer = null;
  $("preview-head").replaceChildren(el("div", { class: "title", text: r.name }), el("div", { class: "meta", text: t("pvw-folder-meta", { files: t("pvw-files", { n: r.count }), size: fmtBytes(r.bytes) }) }));
  $("preview-body").replaceChildren(
    el("div", { class: "controls" }, el("button", { type: "button", class: "small", text: r.expanded ? t("dlg-close") : t("pvw-open"), onclick: () => toggleFolder(mainList, S.selected) })),
    el("p", { class: "placeholder", text: t("pvw-folder-note") }),
  );
}

// ---------- preview ----------

function clearPreview() {
  $("preview-head").replaceChildren();
  $("preview-body").replaceChildren(el("p", { class: "placeholder", text: S.info ? t("pvw-select") : "" }));
}

async function showPreview(row, action) {
  const token = ++S.previewToken;
  S.viewer?.destroy();
  S.viewer = null;
  const head = $("preview-head");
  const body = $("preview-body");
  head.replaceChildren(el("div", { class: "title", text: row.name }), el("div", { class: "meta", text: t("pvw-loading") }));
  body.replaceChildren();
  let details;
  let preview;
  try {
    [details, preview] = await Promise.all([
      invoke("details", { index: row.index }),
      invoke("preview", { index: row.index, action: action ?? null }),
    ]);
  } catch (e) {
    if (token !== S.previewToken) return;
    head.replaceChildren(el("div", { class: "title", text: row.name }));
    body.replaceChildren(el("p", { class: "error", text: errText(e) }));
    return;
  }
  if (token !== S.previewToken) return; // the selection moved on
  const meta = el("div", { class: "meta" });
  meta.textContent = t("pvw-meta", { size: fmtBytes(details.size), stored: fmtBytes(details.stored), codec: details.codec, offset: details.offset });
  const staged = S.changed.get(changeKey(details.name));
  if (staged) meta.textContent += ` — ${t(`pvw-staged-${staged === "remove" ? "removed" : staged === "add" ? "added" : "changed"}`)}`;
  const hexBtn = el("button", { type: "button", class: "small hexbtn", icon: "file", text: "Hex…", title: t("pvw-hex-title"), onclick: () => openHexEditor(row, details.name) });
  head.replaceChildren(el("div", { class: "title", text: details.name }), meta, hexBtn);
  body.replaceChildren(...renderPreview(preview, row, meta).filter(Boolean));
}

const HEX_ROWS = 24;

/** Every byte of a file, 16 per row; typing a hex value in a cell changes that byte. Apply stages all changes as one. */
async function openHexEditor(row, title) {
  const PAGE = HEX_ROWS * 16;
  const edits = new Map();
  let offset = 0;
  let size = 0;
  let hit = null; // the bytes the last search found
  const grid = el("div", { class: "hexgrid", role: "group", "aria-label": t("hex-bytes-aria") });
  const where = el("span", { class: "muted", role: "status" });
  const goto = el("input", { type: "text", class: "field", size: "10", placeholder: t("hex-goto-ph"), "aria-label": t("hex-goto-aria") });
  const count = el("span", { class: "muted" });
  const prev = el("button", { type: "button", class: "small", text: t("hex-prev"), onclick: () => load(offset - PAGE) });
  const next = el("button", { type: "button", class: "small", text: t("hex-next"), onclick: () => load(offset + PAGE) });
  const hex2b = (n) => n.toString(16).padStart(2, "0");
  const asciiOf = (b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : ".");
  async function load(at) {
    at = Math.max(0, Math.min(at, Math.max(0, size - 1)));
    at -= at % 16;
    const page = await invoke("hex_page", { index: row.index, offset: at, len: PAGE });
    size = page.size;
    offset = page.offset;
    const bytes = Uint8Array.from(atob(page.bytes), (c) => c.charCodeAt(0));
    grid.replaceChildren();
    for (let r = 0; r * 16 < bytes.length; r++) {
      const cells = [];
      const ascii = el("span", { class: "hexascii" });
      for (let c = 0; c < 16 && r * 16 + c < bytes.length; c++) {
        const at2 = offset + r * 16 + c;
        const input = el("input", { type: "text", class: "hexb", maxlength: "2", value: hex2b(edits.get(at2) ?? bytes[r * 16 + c]), "aria-label": t("hex-byte-aria", { at: at2 }), spellcheck: "false", autocomplete: "off" });
        input.classList.toggle("changed", edits.has(at2));
        input.classList.toggle("found", hit != null && at2 >= hit.at && at2 < hit.at + hit.len);
        const glyph = el("span", { text: asciiOf(edits.get(at2) ?? bytes[r * 16 + c]) });
        ascii.append(glyph);
        input.addEventListener("focus", () => input.select());
        input.addEventListener("input", () => {
          const ok = /^[0-9a-f]{1,2}$/i.test(input.value);
          input.setAttribute("aria-invalid", String(!ok));
          if (!ok) return;
          const v = parseInt(input.value, 16);
          if (v === bytes[r * 16 + c]) edits.delete(at2);
          else edits.set(at2, v);
          input.classList.toggle("changed", edits.has(at2));
          glyph.textContent = asciiOf(v);
          count.textContent = edits.size ? t("hex-count", { n: edits.size }) : "";
          // two digits typed: on to the next byte
          if (input.value.length === 2) (input.parentElement.nextElementSibling?.querySelector?.("input") ?? input.closest(".hexrow").nextElementSibling?.querySelector("input"))?.focus();
        });
        input.addEventListener("blur", () => {
          if (!/^[0-9a-f]{1,2}$/i.test(input.value)) input.value = hex2b(edits.get(at2) ?? bytes[r * 16 + c]);
          else input.value = hex2b(parseInt(input.value, 16));
          input.setAttribute("aria-invalid", "false");
        });
        cells.push(el("span", { class: "hexcell" }, input));
      }
      grid.append(el("div", { class: "hexrow" }, el("span", { class: "hexoff", text: (offset + r * 16).toString(16).padStart(8, "0") }), ...cells, ascii));
    }
    where.textContent = t("hex-where", { from: offset.toString(16).padStart(8, "0"), to: (offset + bytes.length).toString(16), size: size.toString(16), human: fmtBytes(size) });
    prev.disabled = offset === 0;
    next.disabled = offset + PAGE >= size;
  }
  const mode = el("select", { "aria-label": t("hex-find-mode") }, el("option", { value: "hex", text: t("hex-mode-hex") }), el("option", { value: "text", text: t("hex-mode-text") }));
  const needle = el("input", { type: "text", class: "field", size: "14", placeholder: t("hex-find-ph"), "aria-label": t("hex-find-btn"), spellcheck: "false", autocomplete: "off" });
  const found = el("span", { class: "muted", role: "status" });
  let searchFrom = 0;
  async function findNext() {
    let pattern;
    if (mode.value === "text") pattern = [...new TextEncoder().encode(needle.value)];
    else {
      const t2 = needle.value.replace(/\s+/g, "");
      pattern = /^([0-9a-f]{2})+$/i.test(t2) ? t2.match(/../g).map((h) => parseInt(h, 16)) : null;
      if (!pattern) {
        found.textContent = t("hex-bad-pattern");
        return;
      }
    }
    if (!pattern.length) return;
    try {
      const at = await invoke("hex_find", { index: row.index, pattern, from: searchFrom });
      if (at == null) {
        hit = null;
        for (const e of grid.querySelectorAll(".found")) e.classList.remove("found");
        found.textContent = t("hex-not-found");
        return;
      }
      hit = { at, len: pattern.length };
      searchFrom = at + 1;
      found.textContent = t("hex-found", { at: at.toString(16).padStart(8, "0") });
      await load(at);
    } catch (e) {
      found.textContent = errText(e);
    }
  }
  needle.addEventListener("input", () => (searchFrom = 0));
  mode.addEventListener("change", () => (searchFrom = 0));
  needle.addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    e.preventDefault();
    findNext();
  });
  const findBtn = el("button", { type: "button", class: "small", text: t("hex-find-btn"), onclick: findNext });
  goto.addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    e.preventDefault();
    const t = goto.value.trim();
    const n = /^0x/i.test(t) ? parseInt(t, 16) : /^[0-9a-f]+$/i.test(t) && /[a-f]/i.test(t) ? parseInt(t, 16) : parseInt(t, 10);
    if (Number.isFinite(n)) load(n);
  });
  try {
    await load(0);
  } catch (e) {
    return note(t("hex-cannot"), errText(e), true);
  }
  const dlg = $("dlg");
  dlg.classList.add("wide");
  const go = await ask(
    t("hex-title", { name: title.split("\\").pop() }),
    [el("p", { class: "muted", text: t("hex-intro") }), el("div", { class: "controls" }, prev, next, goto, where, count), el("div", { class: "controls" }, mode, needle, findBtn, found), grid],
    [{ label: t("hex-apply"), value: true, primary: true }, { label: "Close", value: null }],
  );
  dlg.classList.remove("wide");
  if (!go || !edits.size) return;
  try {
    setPending(await invoke("stage_hex", { index: row.index, edits: [...edits] }));
    setStatus(t("hex-status", { name: title, n: edits.size }));
    refreshPreview();
  } catch (e) {
    await note(t("hex-failed"), errText(e), true);
  }
}

/** A "Reset view" button that appears once the camera has left its start position. */
function resetViewButton(handle) {
  const b = el("button", { type: "button", class: "small", icon: "reset", text: t("pvw-reset-view"), title: t("pvw-reset-view-title"), hidden: true, onclick: () => handle.v?.view.reset() });
  // the viewer is created after the controls, so the hook is attached when it exists
  const attach = () => {
    const v = handle.v;
    if (!v) return requestAnimationFrame(attach);
    v.view.onchange = () => (b.hidden = v.view.atHome());
  };
  attach();
  return b;
}

const dataUrl = (mime, data) => `data:${mime};base64,${data}`;
/** The name of a GAT cell type in the current language. */
const gatTypeName = (k) => t(k < 0 ? "gat-tn-weird" : k <= 6 ? `gat-tn-${k}` : "gat-tn-4");

/** A line from the backend: plain text, or a catalogue key with its values. */
const NOTE_UNITS = {
  tiles: "tiles", lightmaps: "lightmaps", textures: "textures", models: "models", lights: "lights", sounds: "sounds", effects: "effects",
  nodes: "nodes", triangles: "triangles", v: "vertices", f: "faces", pal: "palimg", rgba: "rgbaimg", actions: "actions", layers: "layers",
};
/** Values that are counts of things get their unit (with the right plural) from the catalog. */
const NOTE_UNIT_OVERRIDES = {
  "pvw-n-textures": { n: "textures" },
  "pvw-n-instances": { instances: "modelinst", models: "diffmodels" },
};

function noteText(n) {
  if (typeof n === "string") {
    if (!n.startsWith("\u0001")) return n;
    try {
      n = JSON.parse(n.slice(1));
    } catch {
      return n;
    }
  }
  const args = { ...n.args };
  const units = { ...NOTE_UNITS, ...NOTE_UNIT_OVERRIDES[n.key] };
  for (const [name, unit] of Object.entries(units)) {
    if (name in args) args[name] = t(`un-${unit}`, { n: args[name] });
  }
  if ("frames" in args) args.frames = t("pvw-frames", { n: args.frames });
  if (n.key === "pvw-n-gat-type") {
    args.name = gatTypeName(args.k);
    args.cells = t("gat-cells", { n: args.n });
  }
  return t(n.key, args);
}

const infoBlock = (lines) => (lines?.length ? el("div", { class: "info" }, el("pre", { text: lines.map(noteText).join("\n") })) : null);

function renderPreview(p, row, meta) {
  switch (p.kind) {
    case "image": {
      const img = el("img", { src: dataUrl(p.mime, p.data), alt: row.name });
      img.addEventListener("load", () => {
        meta.textContent += `, ${img.naturalWidth} x ${img.naturalHeight} px${p.bits ? `, ${p.bits}-bit` : ""}`;
        if (img.naturalWidth <= 128) img.style.width = `${img.naturalWidth * 4}px`;
      });
      const reduce =
        p.bits > 8
          ? el(
              "div",
              { class: "controls" },
              el("button", { type: "button", class: "small", icon: "palette", text: t("reduce1-btn"), title: t("reduce1-btn-title"), onclick: () => openReduceDialog(row) }),
            )
          : null;
      return [reduce, el("div", { class: "checker" }, img)];
    }
    case "audio":
      return [el("audio", { controls: true, src: dataUrl(p.mime, p.data) })];
    case "text": {
      const pre = el("pre", { text: p.text });
      const note = el("div", { class: "muted" }, `${t("txt-encoding")} `, el("span", { class: "badge", text: p.encoding }), p.truncated ? ` ${t("txt-shortened")}` : "");
      const edit = el("button", { type: "button", class: "small", icon: "edit", text: t("txt-edit"), title: t("txt-edit-title"), onclick: () => openTextEditor(row) });
      return [el("div", { class: "controls" }, edit), note, pre];
    }
    case "frames":
      return [
        row.name.toLowerCase().endsWith(".pal")
          ? el(
              "div",
              { class: "controls" },
              el("button", { type: "button", class: "small", icon: "palette", text: t("pal-adjust"), title: t("pal-adjust-title"), onclick: () => adjustPalette(row) }),
              el("span", { class: "muted", text: t("pal-click-hint") }),
            )
          : null,
        row.name.toLowerCase().endsWith(".spr")
          ? el(
              "div",
              { class: "controls" },
              el("button", { type: "button", class: "small", icon: "add", text: t("spr-add"), title: t("spr-add-title"), onclick: () => addImage(row) }),
              el("button", { type: "button", class: "small", icon: "palette", text: t("spr-setpal"), title: t("spr-setpal-title"), onclick: () => setPalette(row) }),
            )
          : null,
        el(
          "div",
          { class: "frames" },
          p.frames.map((f, i) =>
            el(
              "figure",
              {},
              el("div", { class: "checker" }, palImage(row, f)),
              el("figcaption", { text: noteText(f.label) }),
              row.name.toLowerCase().endsWith(".spr")
                ? el(
                    "div",
                    { class: "figactions" },
                    el("button", { type: "button", class: "small", icon: "pencil", text: t("spr-pixels"), title: t("spr-pixels-title"), onclick: () => openPixelEditor(row, i) }),
                    el("button", { type: "button", class: "small", text: t("spr-replace"), title: t("spr-replace-title"), onclick: () => replaceImage(row, i) }),
                    el("button", { type: "button", class: "small", text: t("btn-remove"), title: t("spr-remove-title"), onclick: () => removeImage(row, i) }),
                  )
                : null,
            ),
          ),
        ),
        infoBlock(p.info),
      ];
    case "animation": {
      const sel = el("select", {
        onchange: () => showPreview(row, Number(sel.value)),
      });
      for (let i = 0; i < p.actions; i++) sel.append(el("option", { value: i, text: t("pvw-action-n", { n: i }), selected: i === p.action }));
      const controls = el(
        "div",
        { class: "controls" },
        el("label", { text: `${t("pvw-action")} ` }, sel),
        el("span", { class: "muted", text: t("pvw-anim-meta", { frames: t("pvw-frames", { n: p.frames }), ms: p.interval_ms }) }),
        el("button", { type: "button", class: "small", icon: "edit", text: t("pvw-anim-edit"), title: t("pvw-anim-edit-title"), onclick: () => openActEditor(row, p.action) }),
        el("button", { type: "button", class: "small", icon: "animation", text: t("pvw-anim-interval"), title: t("pvw-anim-interval-title"), onclick: () => setInterval_(row, p.action, p.interval_ms) }),
      );
      return [controls, el("div", { class: "checker" }, el("img", { src: dataUrl("image/gif", p.gif), width: p.width, height: p.height, alt: "animation" })), infoBlock(p.info), p.note ? el("div", { class: "info", text: t("pvw-warning", { note: p.note }) }) : null];
    }
    case "model": {
      const host = el("div", { class: "glhost" });
      const handle = { v: null };
      // the canvas needs to be in the page before WebGL measures it
      queueMicrotask(() => (S.viewer = handle.v = Viewer3D.show(host, p)));
      const lit = el("input", { type: "checkbox", checked: true, onchange: () => handle.v?.set("lit", lit.checked) });
      const wire = el("input", { type: "checkbox", onchange: () => handle.v?.set("wire", wire.checked) });
      const animate = el("input", { type: "checkbox", checked: true, onchange: () => handle.v?.set("animate", animate.checked) });
      return [
        el("div", { class: "controls" }, el("span", { class: "muted", text: t("pvw-model-hint", { n: p.triangles }) }), el("label", {}, lit, ` ${t("pvw-lighting")}`), el("label", {}, wire, ` ${t("pvw-wireframe")}`), p.duration_ms > 0 ? el("label", {}, animate, ` ${t("pvw-animate")}`) : null, resetViewButton(handle)),
        host,
        infoBlock(p.info),
      ];
    }
    case "scene": {
      const host = el("div", { class: "glhost tall" });
      const handle = { v: null };
      queueMicrotask(() => (S.viewer = handle.v = Viewer3D.showScene(host, p)));
      // the parts of the picture; "All" switches them together (Animate is motion, not a part)
      const parts = {};
      const allLabel = el("span");
      const allBtn = el("button", { type: "button", class: "small", icon: "eye", title: t("pvw-all-title") }, allLabel);
      const syncAll = () => {
        const on = Object.values(parts).some((c) => c.checked);
        allLabel.textContent = on ? t("pvw-hide-all") : t("pvw-show-all");
        allBtn.setAttribute("aria-pressed", String(Object.values(parts).every((c) => c.checked)));
      };
      const toggle = (label, name, isPart = true) => {
        const c = el("input", { type: "checkbox", checked: true, onchange: () => { handle.v?.set(name, c.checked); syncAll(); } });
        if (isPart) parts[name] = c;
        return el("label", {}, c, ` ${label}`);
      };
      allBtn.addEventListener("click", () => {
        const on = !Object.values(parts).some((c) => c.checked);
        for (const [name, c] of Object.entries(parts)) {
          c.checked = on;
          handle.v?.set(name, on);
        }
        syncAll();
      });
      const shown = () => Object.fromEntries(Object.entries(parts).map(([k, c]) => [k, c.checked]));
      const controls = el(
        "div",
        { class: "controls" },
        toggle(t("pvw-lighting"), "lit"),
        toggle(t("pvw-lightmaps"), "lightmap"),
        toggle(t("pvw-models"), "models"),
        toggle(t("pvw-water"), "water"),
        allBtn,
        resetViewButton(handle),
        p.water?.frames?.length > 1 || p.models.some((m) => m.anim) ? toggle(t("pvw-animate"), "animate", false) : null,
        el("button", { type: "button", class: "small", icon: "image", text: t("pvw-save-picture"), title: t("pvw-save-picture-title"), onclick: () => saveMapPicture(row, handle.v) }),
        el("button", { type: "button", class: "small", icon: "extract", text: t("pvw-extract-map"), title: t("pvw-extract-map-title"), onclick: () => doExtractMap(row, shown()) }),
      );
      syncAll();
      const hint = el("p", { class: "muted viewhint", text: t("pvw-map-hint") });
      return [controls, hint, host, infoBlock(p.info)];
    }
    case "effect":
      return [el("div", { class: "muted", text: t("pvw-effect", { frames: t("pvw-frames", { n: p.frames }), fps: p.fps }) }), el("div", { class: "fxstage" }, el("img", { src: dataUrl("image/gif", p.gif), width: p.width, height: p.height, alt: "effect" })), infoBlock(p.info)];
    case "map":
      if (p.gat) return gatEditor(row, p);
      return [el("div", { class: "checker" }, el("img", { src: dataUrl("image/png", p.png), width: p.width, height: p.height, alt: "map" })), infoBlock(p.info)];
    case "info":
      return [el("pre", { text: p.lines.map(noteText).join("\n") })];
    case "video": {
      const note = el("p", { class: "muted", role: "status" });
      const open = el("button", { type: "button", class: "small", icon: "video", text: t("pvw-video-open"), title: t("pvw-video-open-title"), onclick: async () => {
        try {
          const path = await invoke("open_external", { index: row.index });
          note.textContent = t("pvw-video-opened", { path });
        } catch (e) {
          note.textContent = errText(e);
        }
      } });
      return [
        el("p", { text: t("pvw-video-info", { w: p.width, h: p.height, frames: t("pvw-frames", { n: p.frames }), fps: p.fps.toFixed(2), secs: (p.frames / Math.max(p.fps, 0.01)).toFixed(1), size: fmtBytes(p.size) }) }),
        el("p", { class: "muted", text: t("pvw-video-note") }),
        el("div", { class: "controls" }, open),
        note,
      ];
    }
    case "hex":
      return [el("div", { class: "muted", text: t("pvw-hex-head", { n: p.size }) }), el("pre", { text: p.dump })];
    default:
      return [el("p", { class: "placeholder", text: noteText(p.text || "") })];
  }
}

// ---------- actions ----------

/** The entries a selected line stands for: the file itself, or everything below a folder. */
async function entriesOf(row) {
  return row.folder ? invoke("folder_entries", { view: 0, path: row.name }) : [row.index];
}

/** Saves the 3D view of a map as it is now as PNG. */
async function saveMapPicture(row, viewer) {
  if (!viewer?.snapshot) return note(t("sp-cannot"), t("sp-not-ready"), true);
  const scale = el("select", { class: "field" }, ...[1, 2, 4].map((v) => el("option", { value: String(v), text: t(`sp-scale-${v}`), selected: v === 2 })));
  const go = await ask(t("sp-title"), [el("p", { text: t("sp-intro") }), el("div", { class: "form" }, el("label", {}, `${t("sp-size")} `, scale))], [{ label: t("sp-choose-file"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  const base = row.name.split("\\").pop().replace(/\.[^.]+$/, "");
  const path = await T.dialog.save({ title: t("sp-save-as"), defaultPath: `${base}.png`, filters: [{ name: t("sp-filter"), extensions: ["png"] }] });
  if (!path) return;
  try {
    const shot = viewer.snapshot(Number(scale.value));
    const bytes = await invoke("save_picture", { path, png: shot.url.slice(shot.url.indexOf(",") + 1) });
    setStatus(t("sp-status", { path, w: shot.width, h: shot.height, size: fmtBytes(bytes) }));
  } catch (e) {
    await note(t("sp-failed"), errText(e), true);
  }
}

const MAP_PARTS = () => ["models", "water", "sounds", "minimap", "effects"].map((k) => [k, t(`em-part-${k}`)]);

/** Writes the map of `row` (RSW, GAT or GND) with its files into a folder; `shown` are the switches of the 3D view. */
async function doExtractMap(row, shown = {}) {
  const boxes = Object.fromEntries(MAP_PARTS().map(([k]) => [k, el("input", { type: "checkbox", checked: shown[k] ?? true })]));
  const offNote = shown.lit === false || shown.lightmap === false ? el("p", { class: "muted", text: t("em-lit-note") }) : null;
  const go = await ask(
    t("em-title"),
    [
      el("p", { text: t("em-intro") }),
      el("div", { class: "form" }, ...MAP_PARTS().map(([k, t]) => el("label", {}, boxes[k], ` ${t}`))),
      offNote,
    ],
    [{ label: t("em-choose-folder"), value: true, primary: true }, { label: "Cancel", value: null }],
  );
  if (!go) return;
  const skip = MAP_PARTS().map(([k]) => k).filter((k) => !boxes[k].checked);
  const dest = await T.dialog.open({ directory: true, multiple: false, title: t("em-dest") });
  if (!dest) return;
  setStatus(t("em-running"));
  try {
    let r;
    try {
      r = await invoke("extract_map", { index: row.index, dest, rename: false, skip });
    } catch (e) {
      if (!/exists/i.test(errText(e))) throw e;
      const go = await ask(t("em-exists-title"), el("p", { text: t("em-exists-body", { error: errText(e) }) }), [{ label: t("em-keep-both"), value: true, primary: true }, { label: "Cancel", value: null }]);
      if (!go) return setStatus(t("em-nothing"));
      r = await invoke("extract_map", { index: row.index, dest, rename: true, skip });
    }
    setStatus(t("em-status", { map: r.map, files: t("pvw-files", { n: r.files }), dest: r.dest }));
    const body = [el("p", { text: `${t("em-body", { files: t("pvw-files", { n: r.files }), map: r.map, dest: r.dest })}${r.skipped ? ` ${t("em-skipped", { files: t("pvw-files", { n: r.skipped }), parts: skip.map((k) => t(`em-part-${k}`)).join(", ") })}` : ""}` })];
    if (r.missing.length) body.push(el("p", { text: t("em-missing", { files: t("pvw-files", { n: r.missing.length }) }) }), el("ul", { class: "pending" }, ...r.missing.slice(0, 50).map((m) => el("li", { text: m }))));
    if (r.problems.length) body.push(el("p", { class: "error", text: r.problems.slice(0, 10).join("\n") }));
    if (r.damaged) body.push(el("p", { class: "error", text: t("em-damaged", { files: t("pvw-files", { n: r.damaged }) }) }));
    await ask(r.missing.length || r.problems.length ? t("em-done-gaps") : t("em-done"), body, [{ label: "OK", value: true, primary: true }], r.missing.length || r.problems.length ? "warning" : "success");
  } catch (e) {
    setStatus(t("em-failed"));
    await note(t("em-failed"), errText(e), true);
  }
}

async function doExtract() {
  if (!S.info) return;
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  const buttons = [];
  if (sel) buttons.push({ label: sel.folder ? t("ext-folder", { n: sel.count }) : t("ext-entry"), value: "one", primary: true });
  buttons.push({ label: t("ext-all", { n: S.matches }), value: "all", primary: !sel });
  buttons.push({ label: "Cancel", value: null });
  const what = await ask(t("ext-title"), el("p", { text: filtersActive() ? t("ext-filtered") : t("ext-whole") }), buttons);
  if (!what) return;
  const dest = await T.dialog.open({ directory: true, multiple: false, title: t("ext-dest") });
  if (!dest) return;
  const indices = what === "one" ? await entriesOf(sel) : [];
  if (what === "one" && !indices.length) return note(t("ext-title"), t("ext-empty-folder"));
  setStatus(t("ext-running"));
  let rename = false;
  for (;;) {
    try {
      const r = await invoke("extract", { indices, dest, rename });
      setStatus(t("ext-done-status", { files: r.files, dest: r.dest }));
      await ask(t("ext-done-title"), el("p", { text: `${t("ext-done-body", { files: r.files, dest: r.dest })}${r.damaged ? ` ${t("ext-damaged", { n: r.damaged })}` : ""}` }), [{ label: "OK", value: true, primary: true }], "success");
      return;
    } catch (e) {
      setStatus(t("ext-stopped"));
      const again = await ask(t("ext-could-not"), el("p", { class: "error", text: errText(e) }), [
        { label: t("ext-rename"), value: true },
        { label: "Cancel", value: false, primary: true },
      ], "error");
      if (!again) return;
      rename = true;
    }
  }
}

// ---------- editing (changes are collected and saved as a new archive) ----------

function updateEditBar() {
  const cur = S.selected != null ? S.rows[S.selected - S.offset] : null;
  $("btn-replace").disabled = !cur || cur.folder;
  $("btn-remove").disabled = !S.info;
  const n = S.changes.length;
  $("changes-badge").textContent = n;
  $("changes-badge").hidden = n === 0;
  $("btn-changes").setAttribute("aria-label", n ? t("save-aria-n", { n }) : t("save-aria-none"));
  $("btn-changes").title = (n ? t("save-n", { n }) : t("save-none")) + " (Cmd/Ctrl+S)";
  if (S.info) {
    invoke("history")
      .then((h) => {
        for (const id of ["btn-undo", "act-undo"]) if ($(id)) $(id).disabled = !h.can_undo;
        for (const id of ["btn-redo", "act-redo"]) if ($(id)) $(id).disabled = !h.can_redo;
      })
      .catch(() => {});
  }
}

const changeKey = (name) => name.toLowerCase().replace(/\//g, "\\");

/** The window title: the archive, marked while it has unsaved changes (the platform's way). */
function updateTitle() {
  const name = S.info?.file_name;
  const edited = (S.changes?.length ?? 0) > 0;
  const mac = /Mac/.test(navigator.platform);
  let title = "RustyGRF";
  if (name) title = mac ? `${name}${edited ? " — Edited" : ""}` : `${edited ? "*" : ""}${name} - RustyGRF`;
  if (document.title === title) return;
  document.title = title;
  // the native title bar does not follow document.title by itself
  T.window?.getCurrentWindow?.().setTitle(title).catch(() => {});
}

function setPending(rows) {
  THUMB.reset();
  S.changes = rows;
  S.changed = new Map(rows.map((r) => [changeKey(r.name), r.kind]));
  updateTitle();
  updateEditBar();
  render();
}

async function doHistory(redo) {
  try {
    setPending(await invoke(redo ? "redo_change" : "undo_change"));
    setStatus(redo ? t("history-redone") : t("history-undone"));
    if ($("act-dlg").open) await loadActFrame();
    refreshPreview($("act-dlg").open ? E.action : undefined);
  } catch (e) {
    await note(t("history-failed"), errText(e), true);
  }
}

/** Shows the selected entry again, so it reflects the staged changes. */
function refreshPreview(action) {
  const r = S.selected != null ? S.rows[S.selected - S.offset] : null;
  if (r && !r.folder && V.preview !== "off") showPreview(r, action);
}

async function replaceImage(row, frame) {
  const png = await T.dialog.open({ multiple: false, directory: false, title: t("spr-replace-pick", { frame, name: row.name }), filters: [{ name: t("spr-png-filter"), extensions: ["png"] }] });
  if (!png) return;
  try {
    setPending(await invoke("stage_spr_image", { index: row.index, frame, png }));
    setStatus(t("spr-replace-status", { frame, name: row.name }));
    refreshPreview();
  } catch (e) {
    await note(t("spr-replace-failed"), errText(e), true);
  }
}

async function addImage(row) {
  const png = await T.dialog.open({ multiple: false, directory: false, title: t("spr-add-pick", { name: row.name }), filters: [{ name: t("spr-png-filter"), extensions: ["png"] }] });
  if (!png) return;
  try {
    setPending(await invoke("stage_spr_add", { index: row.index, png }));
    setStatus(t("spr-add-status", { name: row.name }));
    refreshPreview();
  } catch (e) {
    await note(t("spr-add-failed"), errText(e), true);
  }
}

async function removeImage(row, frame) {
  const go = await ask(t("spr-remove-heading", { n: frame }), el("p", { text: t("spr-remove-body") }), [{ label: t("btn-remove"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  try {
    setPending(await invoke("stage_spr_remove", { index: row.index, frame }));
    setStatus(t("spr-remove-status", { frame, name: row.name }));
    refreshPreview();
  } catch (e) {
    await note(t("spr-remove-failed"), errText(e), true);
  }
}

// ---------- pixel editor ----------

/** Draws on the palette indices of one sprite image; Apply stages the result. */
async function openPixelEditor(row, frame) {
  let v;
  try {
    v = await invoke("spr_image", { index: row.index, frame });
  } catch (e) {
    await note(t("pix-error"), errText(e), true);
    return;
  }
  const { width: w, height: h, palette } = v;
  const px = Uint8Array.from(atob(v.pixels), (c) => c.charCodeAt(0));
  const original = px.slice();
  const undo = [];
  const st = { tool: "pencil", color: 1, zoom: Math.max(2, Math.min(24, Math.floor(480 / Math.max(w, h)))), down: false, last: null, grid: true };
  const rgb = palette.map((p) => [parseInt(p.slice(0, 2), 16), parseInt(p.slice(2, 4), 16), parseInt(p.slice(4, 6), 16)]);

  const canvas = el("canvas", { class: "pixcanvas", tabindex: "0", "aria-label": t("pix-canvas-aria", { frame, w, h }) });
  const off = document.createElement("canvas");
  off.width = w;
  off.height = h;
  const readout = el("span", { class: "muted", text: t("pix-size", { w, h }) });
  const cursor = { x: 0, y: 0 };

  const draw = () => {
    const z = st.zoom;
    canvas.width = w * z;
    canvas.height = h * z;
    const octx = off.getContext("2d");
    const img = octx.createImageData(w, h);
    for (let i = 0; i < w * h; i++) {
      const ix = px[i];
      const [r, g, b] = rgb[ix];
      const o = i * 4;
      img.data.set(ix === 0 ? [0, 0, 0, 0] : [r, g, b, 255], o);
    }
    octx.putImageData(img, 0, 0);
    const ctx = canvas.getContext("2d");
    ctx.imageSmoothingEnabled = false;
    // checkerboard for the transparent colour
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      ctx.fillStyle = (x + y) % 2 ? "#3a3835" : "#2c2a28";
      ctx.fillRect(x * z, y * z, z, z);
    }
    ctx.drawImage(off, 0, 0, w * z, h * z);
    if (st.grid && z >= 6) {
      ctx.strokeStyle = "rgba(255,255,255,0.12)";
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (let x = 0; x <= w; x++) { ctx.moveTo(x * z + 0.5, 0); ctx.lineTo(x * z + 0.5, h * z); }
      for (let y = 0; y <= h; y++) { ctx.moveTo(0, y * z + 0.5); ctx.lineTo(w * z, y * z + 0.5); }
      ctx.stroke();
    }
    ctx.strokeStyle = "#ea580c";
    ctx.lineWidth = 2;
    ctx.strokeRect(cursor.x * z + 1, cursor.y * z + 1, z - 2, z - 2);
    status();
  };
  const status = () => {
    const i = cursor.y * w + cursor.x;
    readout.textContent = t(px[i] === 0 ? "pix-readout-t" : "pix-readout", { w, h, x: cursor.x, y: cursor.y, c: px[i], hex: palette[px[i]] });
  };
  const pushUndo = () => {
    undo.push(px.slice());
    if (undo.length > 50) undo.shift();
  };
  const put = (x, y, c) => {
    if (x >= 0 && y >= 0 && x < w && y < h) px[y * w + x] = c;
  };
  const line = (x0, y0, x1, y1, c) => {
    const dx = Math.abs(x1 - x0), dy = -Math.abs(y1 - y0), sx = x0 < x1 ? 1 : -1, sy = y0 < y1 ? 1 : -1;
    let err = dx + dy;
    for (;;) {
      put(x0, y0, c);
      if (x0 === x1 && y0 === y1) break;
      const e2 = 2 * err;
      if (e2 >= dy) { err += dy; x0 += sx; }
      if (e2 <= dx) { err += dx; y0 += sy; }
    }
  };
  const fill = (x, y, c) => {
    const target = px[y * w + x];
    if (target === c) return;
    const stack = [[x, y]];
    while (stack.length) {
      const [cx, cy] = stack.pop();
      if (cx < 0 || cy < 0 || cx >= w || cy >= h || px[cy * w + cx] !== target) continue;
      px[cy * w + cx] = c;
      stack.push([cx + 1, cy], [cx - 1, cy], [cx, cy + 1], [cx, cy - 1]);
    }
  };
  const act = (x, y, first) => {
    cursor.x = x;
    cursor.y = y;
    if (st.tool === "picker") {
      st.color = px[y * w + x];
      swatches();
      return;
    }
    if (first) pushUndo();
    const c = st.tool === "eraser" ? 0 : st.color;
    if (st.tool === "fill") fill(x, y, c);
    else if (st.last && !first) line(st.last[0], st.last[1], x, y, c);
    else put(x, y, c);
    st.last = [x, y];
  };
  const at = (e) => {
    const r = canvas.getBoundingClientRect();
    return [Math.max(0, Math.min(w - 1, Math.floor(((e.clientX - r.left) / r.width) * w))), Math.max(0, Math.min(h - 1, Math.floor(((e.clientY - r.top) / r.height) * h)))];
  };
  canvas.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    canvas.focus();
    canvas.setPointerCapture(e.pointerId);
    st.down = true;
    st.last = null;
    act(...at(e), true);
    draw();
  });
  canvas.addEventListener("pointermove", (e) => {
    const [x, y] = at(e);
    if (st.down && st.tool !== "fill" && st.tool !== "picker") act(x, y, false);
    else {
      cursor.x = x;
      cursor.y = y;
    }
    draw();
  });
  const up = () => (st.down = false);
  canvas.addEventListener("pointerup", up);
  canvas.addEventListener("pointercancel", up);
  canvas.addEventListener("keydown", (e) => {
    const d = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[e.key];
    if (d) {
      e.preventDefault();
      cursor.x = Math.max(0, Math.min(w - 1, cursor.x + d[0]));
      cursor.y = Math.max(0, Math.min(h - 1, cursor.y + d[1]));
      if (e.shiftKey) act(cursor.x, cursor.y, !st.last);
      draw();
    } else if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      st.last = null;
      act(cursor.x, cursor.y, true);
      draw();
    }
  });
  canvas.addEventListener("keyup", (e) => e.key === "Shift" && (st.last = null));

  const toolBtns = {};
  const tool = (name, icon, label, key) => {
    const b = el("button", { type: "button", class: "small", icon, text: label, "aria-pressed": String(st.tool === name), title: `${label} (${key})`, onclick: () => setTool(name) });
    toolBtns[name] = b;
    return b;
  };
  const setTool = (name) => {
    st.tool = name;
    for (const [n, b] of Object.entries(toolBtns)) {
      b.classList.toggle("on", n === name);
      b.setAttribute("aria-pressed", String(n === name));
    }
  };
  const bar = el("div", { class: "controls wrap" }, tool("pencil", "pencil", t("pix-pencil"), "P"), tool("eraser", "eraser", t("pix-eraser"), "E"), tool("fill", "fill", t("pix-fill"), "F"), tool("picker", "picker", t("pix-picker"), "I"));
  const zoom = el("input", { type: "range", min: "1", max: "32", value: String(st.zoom), "aria-label": t("pix-zoom") });
  zoom.addEventListener("input", () => {
    st.zoom = Number(zoom.value);
    draw();
  });
  const grid = el("input", { type: "checkbox", checked: true, onchange: () => {
    st.grid = grid.checked;
    draw();
  } });
  const undoBtn = el("button", { type: "button", class: "small", icon: "undo", text: t("btn-undo"), onclick: () => {
    if (undo.length) px.set(undo.pop());
    draw();
  } });
  const resetBtn = el("button", { type: "button", class: "small", icon: "reset", text: t("pix-revert"), onclick: () => {
    pushUndo();
    px.set(original);
    draw();
  } });
  bar.append(el("label", { class: "muted", text: `${t("pix-zoom")} ` }, zoom), el("label", { class: "muted" }, grid, ` ${t("pix-grid")}`), undoBtn, resetBtn);

  const sw = el("div", { class: "swatches", role: "listbox", "aria-label": t("pix-palette") });
  const swatches = () => {
    sw.replaceChildren(
      ...palette.map((p, i) => el("button", { type: "button", class: `swatch${i === st.color ? " cur" : ""}${i === 0 ? " zero" : ""}`, style: i === 0 ? "" : `background:#${p}`, title: `${i}: #${p}${i === 0 ? ` ${t("pix-swatch-zero")}` : ""}`, role: "option", "aria-selected": String(i === st.color), "aria-label": t("pal-colour", { n: i }), onclick: () => {
        st.color = i;
        if (st.tool === "picker") setTool("pencil");
        swatches();
      } })),
    );
  };
  swatches();
  setTool("pencil");

  $("pix-title").textContent = t("pix-title", { frame, name: row.name });
  $("pix-body").replaceChildren(el("div", { class: "pixed" }, el("div", {}, bar, el("div", { class: "pixstage" }, canvas), readout), el("div", {}, el("h3", { class: "muted", text: t("pix-palette-heading") }), sw)));
  const dlg = $("pix-dlg");
  const actions = $("pix-actions");
  const finish = (go) =>
    new Promise((resolve) => {
      actions.replaceChildren(
        el("button", { type: "button", text: t("dlg-cancel"), onclick: () => { dlg.close(); resolve(false); } }),
        el("button", { type: "button", class: "primary", icon: "changes", text: t("dlg-apply"), onclick: () => { dlg.close(); resolve(true); } }),
      );
      dlg.addEventListener("cancel", () => resolve(false), { once: true });
      dlg.showModal();
      draw();
      canvas.focus();
    });
  const keyTools = (e) => {
    if (e.target instanceof HTMLInputElement) return;
    const m = { p: "pencil", e: "eraser", f: "fill", i: "picker" }[e.key.toLowerCase()];
    if (m && !e.metaKey && !e.ctrlKey) setTool(m);
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
      e.preventDefault();
      e.stopPropagation();
      undoBtn.click();
    }
  };
  dlg.addEventListener("keydown", keyTools);
  const go = await finish();
  dlg.removeEventListener("keydown", keyTools);
  if (!go) return;
  if (px.every((v, i) => v === original[i])) return setStatus(t("pix-nochange"));
  let bin = "";
  for (let i = 0; i < px.length; i += 8192) bin += String.fromCharCode(...px.subarray(i, i + 8192));
  try {
    setPending(await invoke("stage_spr_pixels", { index: row.index, frame, pixels: btoa(bin) }));
    setStatus(t("pix-status", { frame, name: row.name }));
    refreshPreview();
  } catch (e) {
    await note(t("pix-failed"), errText(e), true);
  }
}

// ---------- map cell editing ----------

/** A GAT as a picture on which a rectangle of cells can be chosen and set (mouse or number fields). */
function gatEditor(row, p) {
  const { cells_x: cx, cells_y: cy, types } = p.gat;
  const sel = { x0: 0, y0: 0, x1: 0, y1: 0, on: false };
  const img = el("img", { src: dataUrl("image/png", p.png), width: p.width, height: p.height, alt: t("gat-alt", { x: cx, y: cy }), draggable: "false" });
  const box = el("div", { class: "gatsel", hidden: true });
  const stage = el("div", { class: "checker gatstage" }, img, box);
  const field = (label, key, max) => {
    const i = el("input", { type: "number", min: "0", max: String(max), step: "1", class: "narrow", "aria-label": label });
    i.addEventListener("change", () => {
      const v = Math.max(0, Math.min(max, Math.round(Number(i.value) || 0)));
      sel[key] = v;
      sel.on = true;
      draw();
    });
    return [el("label", { class: "muted", text: `${label} ` }, i), i];
  };
  const [lx0, ix0] = field(t("gat-x-from"), "x0", cx - 1);
  const [lx1, ix1] = field(t("gat-to"), "x1", cx - 1);
  const [ly0, iy0] = field(t("gat-y-from"), "y0", cy - 1);
  const [ly1, iy1] = field(t("gat-to"), "y1", cy - 1);
  const kind = el("select", { "aria-label": t("gat-type-aria") }, el("option", { value: "", text: t("gat-type-nochange") }), ...types.map(([k, n]) => el("option", { value: String(k), text: `${k}: ${gatTypeName(k)}` })));
  const height = el("input", { type: "number", step: "0.5", placeholder: t("gat-height-ph"), class: "narrow wide", "aria-label": t("gat-height") });
  const apply = el("button", { type: "button", class: "small", icon: "edit", text: t("gat-set"), disabled: true, title: t("gat-set-title") });
  const readout = el("span", { class: "muted", text: t("gat-drag") });
  const draw = () => {
    const [xa, xb, ya, yb] = [Math.min(sel.x0, sel.x1), Math.max(sel.x0, sel.x1), Math.min(sel.y0, sel.y1), Math.max(sel.y0, sel.y1)];
    box.hidden = !sel.on;
    if (sel.on) {
      Object.assign(box.style, { left: `${(xa / cx) * 100}%`, width: `${((xb - xa + 1) / cx) * 100}%`, top: `${((cy - 1 - yb) / cy) * 100}%`, height: `${((yb - ya + 1) / cy) * 100}%` });
      ix0.value = xa;
      ix1.value = xb;
      iy0.value = ya;
      iy1.value = yb;
      readout.textContent = t("gat-readout", { xa, xb, ya, yb, cells: t("gat-cells", { n: (xb - xa + 1) * (yb - ya + 1) }) });
    }
    apply.disabled = !sel.on;
  };
  const cell = (e) => {
    const r = img.getBoundingClientRect();
    const x = Math.max(0, Math.min(cx - 1, Math.floor(((e.clientX - r.left) / r.width) * cx)));
    const rowFromTop = Math.max(0, Math.min(cy - 1, Math.floor(((e.clientY - r.top) / r.height) * cy)));
    return [x, cy - 1 - rowFromTop]; // game y grows to the north, the picture's rows to the south
  };
  let dragging = false;
  stage.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    stage.setPointerCapture(e.pointerId);
    [sel.x0, sel.y0] = cell(e);
    [sel.x1, sel.y1] = [sel.x0, sel.y0];
    sel.on = true;
    dragging = true;
    draw();
  });
  stage.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    [sel.x1, sel.y1] = cell(e);
    draw();
  });
  stage.addEventListener("pointerup", () => (dragging = false));
  apply.addEventListener("click", async () => {
    const h = height.value.trim() === "" ? null : Number(height.value);
    if (kind.value === "" && h === null) return note(t("gat-set"), t("gat-need"));
    try {
      const r = await invoke("stage_gat_edit", { index: row.index, edit: { x0: sel.x0, y0: sel.y0, x1: sel.x1, y1: sel.y1, kind: kind.value === "" ? null : Number(kind.value), height: h } });
      setPending(r.pending);
      setStatus(t("gat-status", { cells: t("gat-cells", { n: r.changed }), name: row.name }));
      refreshPreview();
    } catch (e) {
      await note(t("gat-failed"), errText(e), true);
    }
  });
  const all = el("button", { type: "button", class: "small", text: t("gat-select-all"), onclick: () => {
      Object.assign(sel, { x0: 0, y0: 0, x1: cx - 1, y1: cy - 1, on: true });
      draw();
    },
  });
  return [
    el("div", { class: "controls wrap" }, lx0, lx1, ly0, ly1, all),
    el("div", { class: "controls wrap" }, kind, el("label", { class: "muted", text: `${t("gat-height")} ` }, height), apply, readout),
    stage,
    infoBlock(p.info),
  ];
}

// ---------- palette editing ----------

function palImage(row, f) {
  const img = el("img", { src: dataUrl("image/png", f.png), alt: noteText(f.label) });
  if (row.name.toLowerCase().endsWith(".pal")) {
    img.style.cursor = "crosshair";
    img.style.width = "256px";
    img.style.maxWidth = "none";
    img.title = t("pal-click-title");
    img.addEventListener("click", (e) => {
      const r = img.getBoundingClientRect();
      const cx = Math.min(15, Math.max(0, Math.floor(((e.clientX - r.left) / r.width) * 16)));
      const cy = Math.min(15, Math.max(0, Math.floor(((e.clientY - r.top) / r.height) * 16)));
      pickColour(row, cy * 16 + cx);
    });
  }
  return img;
}

async function pickColour(row, index) {
  let current;
  try {
    current = (await invoke("pal_colors", { index: row.index }))[index];
  } catch (e) {
    await note(t("pal-read-failed"), errText(e), true);
    return;
  }
  const input = el("input", { type: "color", value: `#${current}` });
  const go = await ask(t("pal-colour", { n: index }), [el("p", { text: index === 0 ? t("pal-zero") : t("pal-number", { n: index }) }), input], [{ label: t("act-interval-set"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  const h = input.value.slice(1);
  await stagePal(row, { op: "set", index, rgb: [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)] }, t("pal-changed", { n: index, name: row.name }));
}

async function adjustPalette(row) {
  const f = (label, value, step) => el("label", { class: "muted", text: `${label} ` }, el("input", { type: "number", value: String(value), step: String(step), class: "narrow" }));
  const [from, to, hue, sat, bri] = [f(t("pal-from"), 1, 1), f(t("pal-to"), 255, 1), f(t("pal-hue"), 0, 5), f(t("pal-sat"), 1, 0.05), f(t("pal-bri"), 1, 0.05)];
  const v = (l) => Number(l.querySelector("input").value);
  const go = await ask(t("pal-adjust-heading", { name: row.name }), [el("p", { text: t("pal-range-note") }), el("div", { class: "bar" }, from, to), el("div", { class: "bar" }, hue, sat, bri)], [{ label: t("dlg-apply"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  await stagePal(row, { op: "adjust", from: v(from), to: v(to), hue: v(hue), saturation: v(sat), brightness: v(bri) }, t("pal-adjusted", { name: row.name }));
}

async function stagePal(row, edit, message) {
  try {
    setPending(await invoke("stage_pal_edit", { index: row.index, edit }));
    setStatus(message);
    refreshPreview();
  } catch (e) {
    await note(t("pal-failed"), errText(e), true);
  }
}

async function setPalette(row) {
  const pal = await T.dialog.open({ multiple: false, directory: false, title: t("pal-pick", { name: row.name }), filters: [{ name: t("pal-filter"), extensions: ["pal"] }] });
  if (!pal) return;
  try {
    setPending(await invoke("stage_spr_palette", { index: row.index, pal }));
    setStatus(t("pal-set-status", { name: row.name }));
    refreshPreview();
  } catch (e) {
    await note(t("pal-set-failed"), errText(e), true);
  }
}

// ---------- animation editor ----------

// reference animations (a body or head to fit headgear on) stay chosen between sessions
const ACT_REFS_KEY = "rustygrf.actReferences";
const E = { row: null, action: 0, frame: 0, view: null, busy: false, playing: false, timer: 0, sounds: new Map(), refs: loadActRefs(), layer: null };

function loadActRefs() {
  try {
    const r = JSON.parse(localStorage.getItem(ACT_REFS_KEY) || "[]");
    return Array.isArray(r) ? r.filter((x) => typeof x?.path === "string").map((x) => ({ path: x.path, under: !!x.under })) : [];
  } catch {
    return [];
  }
}
function saveActRefs() {
  try {
    localStorage.setItem(ACT_REFS_KEY, JSON.stringify(E.refs));
  } catch {}
}

const REF_PRESETS = () => [
  ["data\\sprite\\인간족\\몸통\\남\\초보자_남.act", t("ref-preset-body-m")],
  ["data\\sprite\\인간족\\몸통\\여\\초보자_여.act", t("ref-preset-body-f")],
  ["data\\sprite\\인간족\\머리통\\남\\1_남.act", t("ref-preset-head-m")],
  ["data\\sprite\\인간족\\머리통\\여\\1_여.act", t("ref-preset-head-f")],
];

async function addActReference() {
  const input = el("input", { type: "text", class: "field", list: "ref-presets", placeholder: "data\\sprite\\…\\name.act", spellcheck: "false", value: S.selected != null && /\.act$/i.test(S.rows[S.selected - S.offset]?.name || "") && S.rows[S.selected - S.offset].name !== E.row?.name ? S.rows[S.selected - S.offset].name : "" });
  const presets = el("datalist", { id: "ref-presets" }, REF_PRESETS().map(([v, label]) => el("option", { value: v, label })));
  const under = el("input", { type: "checkbox", checked: true });
  const go = await ask(
    t("ref-title"),
    [
      el("p", { text: t("ref-intro") }),
      el("div", { class: "form" }, el("label", {}, `${t("ref-animation")} `, input, presets), el("label", {}, under, ` ${t("ref-behind-check")}`)),
      el("div", { class: "controls" }, ...REF_PRESETS().map(([v, label]) => el("button", { type: "button", class: "small", text: label, onclick: () => (input.value = v) }))),
    ],
    [{ label: t("dlg-add"), value: true, primary: true }, { label: "Cancel", value: null }],
  );
  const path = input.value.trim();
  if (!go || !path) return;
  E.refs = [...E.refs.filter((r) => r.path.toLowerCase() !== path.toLowerCase()), { path, under: under.checked }];
  saveActRefs();
  await loadActFrame();
}

function referenceRow(v) {
  const problem = new Map((v.references || []).map((r) => [r.path ?? r.name, r.problem]));
  const chips = E.refs.map((r, i) =>
    el(
      "span",
      { class: "refchip", title: r.path },
      el("span", { class: "name", text: r.path.split("\\").pop() }),
      el("button", { type: "button", class: "small", text: r.under ? t("ref-behind") : t("ref-front"), title: t("ref-toggle-title"), onclick: () => { E.refs[i] = { ...r, under: !r.under }; saveActRefs(); loadActFrame(); } }),
      el("button", { type: "button", class: "small", text: "✕", "aria-label": t("ref-remove-aria", { path: r.path }), onclick: () => { E.refs.splice(i, 1); saveActRefs(); loadActFrame(); } }),
      problem.get(r.path) ? el("span", { class: "muted", text: problem.get(r.path) }) : null,
    ),
  );
  return el("div", { class: "refs" }, el("span", { class: "muted", text: t("ref-label") }), ...chips, el("button", { type: "button", class: "small", text: t("ref-add-btn"), onclick: addActReference }));
}

async function openActEditor(row, action) {
  stopActPlay();
  // sounds are read again each time: a staged change may have replaced one
  Object.assign(E, { row, action, frame: 0, view: null, sounds: new Map() });
  $("act-title").textContent = t("act-title", { name: row.name });
  $("act-body").replaceChildren(el("p", { class: "muted", text: t("pvw-loading") }));
  $("act-dlg").showModal();
  await loadActFrame();
}

$("act-dlg").addEventListener("close", () => {
  stopActPlay();
  if (E.row) refreshPreview(E.action);
  E.row = null;
});

async function openTextEditor(row) {
  $("text-title").textContent = t("te-title", { name: row.name });
  $("text-body").replaceChildren(el("p", { class: "muted", text: t("pvw-loading") }));
  $("text-dlg").showModal();
  let doc;
  try {
    doc = await invoke("text_for_edit", { index: row.index });
  } catch (e) {
    $("text-body").replaceChildren(el("p", { class: "error", text: errText(e) }));
    return;
  }
  const area = el("textarea", { spellcheck: "false", wrap: "off" });
  area.value = doc.text;
  const msg = el("p", { class: "muted" });
  const lua = doc.format.startsWith("Lua");
  if (!doc.complete) msg.textContent = t("te-partial");
  const save = el("button", { type: "button", class: "primary", text: lua ? t("te-compile") : t("te-stage") });
  save.addEventListener("click", async () => {
    save.disabled = true;
    msg.className = "muted";
    msg.textContent = lua ? t("te-compiling") : t("te-staging");
    try {
      setPending(await invoke("stage_text_edit", { index: row.index, text: area.value }));
      setStatus(t(lua ? "te-status-compiled" : "te-status", { name: row.name }));
      $("text-dlg").close();
      refreshPreview();
    } catch (e) {
      const text = errText(e);
      msg.className = "error";
      msg.textContent = text;
      const m = /:(\d+):/.exec(text);
      if (m) {
        const line = Number(m[1]);
        const at = area.value.split("\n").slice(0, line - 1).join("\n").length + (line > 1 ? 1 : 0);
        area.focus();
        area.setSelectionRange(at, at);
      }
    } finally {
      save.disabled = false;
    }
  });
  $("text-body").replaceChildren(el("div", { class: "texted" }, el("div", { class: "bar" }, el("span", { class: "badge", text: doc.format }), save), msg, area));
}

async function loadActFrame() {
  try {
    E.view = await invoke("act_frame", { index: E.row.index, action: E.action, frame: E.frame, references: E.refs });
    E.action = E.view.action;
    E.frame = E.view.frame;
    drawActEditor();
  } catch (e) {
    $("act-body").replaceChildren(el("p", { class: "error", text: errText(e) }));
  }
}

/** Plays the sound of an animation frame from the archive; returns what was found. */
async function playActSound(name) {
  if (!name) return null;
  let s = E.sounds.get(name);
  if (!s) {
    s = await invoke("act_sound", { name }).catch((e) => ({ problem: errText(e) }));
    E.sounds.set(name, s);
  }
  if (s.data) new Audio(dataUrl("audio/wav", s.data)).play().catch(() => {});
  return s;
}

function stopActPlay() {
  E.playing = false;
  clearTimeout(E.timer);
}

// one frame after the other at the action's frame time, with the frames' sounds
async function actPlayStep() {
  if (!E.playing || !$("act-dlg").open) return stopActPlay();
  const started = performance.now();
  E.frame = (E.frame + 1) % E.view.frames;
  await loadActFrame();
  if (!E.playing) return;
  playActSound(E.view.sound);
  E.timer = setTimeout(actPlayStep, Math.max(16, E.view.interval_ms - (performance.now() - started)));
}

function toggleActPlay() {
  if (E.playing) {
    stopActPlay();
    drawActEditor();
    return;
  }
  E.playing = true;
  drawActEditor();
  playActSound(E.view.sound);
  E.timer = setTimeout(actPlayStep, Math.max(16, E.view.interval_ms));
}

async function actEdit(edit) {
  if (E.busy) return;
  stopActPlay();
  E.busy = true;
  try {
    setPending(await invoke("stage_act_edit", { index: E.row.index, action: E.action, frame: E.frame, edit }));
    if (edit.op === "copy_frame") E.frame += 1;
    if (edit.op === "remove_frame") E.frame = Math.max(0, E.frame - 1);
    if (edit.op === "move_frame") E.frame = edit.to;
    if (edit.op === "copy_action") Object.assign(E, { action: E.action + 1, frame: 0 });
    if (edit.op === "remove_action") Object.assign(E, { action: Math.max(0, E.action - 1), frame: 0 });
    if (edit.op === "move_action") E.action = edit.to;
  } catch (e) {
    await loadActFrame();
    $("act-body").prepend(el("p", { class: "error", text: errText(e) }));
    E.busy = false;
    return;
  }
  E.busy = false;
  await loadActFrame();
}

/** Asks how many in-between frames to add, then stages them (see `Act::interpolate`). */
async function openInterpolate(v) {
  const count = el("input", { type: "number", min: "1", max: "20", value: "1", class: "narrow", "aria-label": t("ip-count") });
  const whole = el("input", { type: "checkbox" });
  const smooth = el("input", { type: "checkbox" });
  const wrap = el("input", { type: "checkbox" });
  const keep = el("input", { type: "checkbox", checked: true });
  const last = v.frame >= v.frames - 1;
  const sync = () => (keep.disabled = !whole.checked);
  whole.addEventListener("change", sync);
  sync();
  const row = (input, text) => el("label", { class: "check" }, input, ` ${text}`);
  const go = await ask(
    t("ip-title"),
    [
      el("p", { text: t("ip-intro") }),
      el("p", {}, el("label", {}, `${t("ip-count")} `, count)),
      row(whole, t("ip-whole", { frames: v.frames, frame: v.frame + 1 })),
      row(smooth, t("ip-smooth")),
      row(wrap, t("ip-loop")),
      row(keep, t("ip-keep")),
      last ? el("p", { class: "muted", text: t("ip-last") }) : null,
    ],
    [{ label: t("ip-go"), value: true, primary: true }, { label: "Cancel", value: null }],
    "info",
  );
  if (!go) return;
  const n = Math.max(1, Math.min(20, Math.round(Number(count.value) || 1)));
  await actEdit({ op: "interpolate", count: n, smooth: smooth.checked, whole: whole.checked, wrap: wrap.checked, keep_speed: whole.checked && keep.checked });
}

async function removeAction(i) {
  const go = await ask(t("act-remove-title", { n: i }), el("p", { text: t("act-remove-body") }), [{ label: t("btn-remove"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (go) await actEdit({ op: "remove_action" });
}

const hex2 = (n) => n.toString(16).padStart(2, "0");

function soundField(v) {
  const list = el("datalist", { id: "act-sounds" }, v.sounds.map((s) => el("option", { value: s })));
  const input = el("input", { type: "text", style: "width:13em", list: "act-sounds", value: v.sound ?? "", placeholder: t("act-sound-none"), title: t("act-sound-title"), size: "22" });
  input.addEventListener("change", () => actEdit({ op: "set_sound", name: input.value.trim() === "" ? null : input.value.trim() }));
  const note = el("span", { class: "muted", role: "status" });
  const hear = el("button", {
    type: "button",
    class: "small",
    icon: "audio",
    title: t("act-sound-play-title"),
    "aria-label": t("act-sound-play-aria"),
    disabled: !v.sound,
    onclick: async () => {
      const s = await playActSound(v.sound);
      note.textContent = s?.problem ? `${s.path ?? v.sound}: ${s.problem}` : "";
    },
  });
  return el("span", { class: "soundfield" }, el("label", { class: "muted", text: `${t("act-sound")} ` }, input, list), hear, note);
}

/** Picks, drags and nudges layers on the frame picture; each finished move is one staged change. */
function wireStage(stage, v) {
  // picture pixels per screen pixel (the picture may be shown smaller than it is)
  const ratio = () => v.width / stage.querySelector("img").getBoundingClientRect().width;
  const move = (i, dx, dy) => {
    const l = v.layers[i];
    if (l && (dx || dy)) actEdit({ op: "set_layer", layer: i, offset_x: Math.round(l.offset_x + dx), offset_y: Math.round(l.offset_y + dy) });
  };
  let drag = null;
  stage.addEventListener("pointerdown", async (e) => {
    if (e.button !== 0) return;
    stopActPlay();
    // capturing fails for a pointer that is already gone; the drag then simply ends early
    try {
      stage.setPointerCapture(e.pointerId);
    } catch {}
    const dot = e.target.closest?.("[data-anchor]");
    if (dot) {
      drag = { x: e.clientX, y: e.clientY, dx: 0, dy: 0, anchor: Number(dot.dataset.anchor), dot };
      return;
    }
    const r = stage.querySelector("img").getBoundingClientRect();
    const px = (e.clientX - r.left) * ratio();
    const py = (e.clientY - r.top) * ratio();
    drag = { x: e.clientX, y: e.clientY, dx: 0, dy: 0, layer: undefined };
    const d = drag;
    const hit = await invoke("act_layer_at", { index: E.row.index, action: E.action, frame: E.frame, x: (px - v.origin_x) / v.scale, y: (py - v.origin_y) / v.scale }).catch(() => null);
    d.layer = hit ?? null;
    if (d.layer !== E.layer) {
      E.layer = d.layer;
      if (drag !== d) return drawActEditor();
      // show the new selection without losing the pointer: only the box and the table change
      const b = d.layer != null ? v.layers[d.layer]?.bounds : null;
      stage.querySelector(".layerbox")?.remove();
      if (b) stage.append(el("div", { class: "layerbox", style: `left:${b[0]}px;top:${b[1]}px;width:${b[2] - b[0]}px;height:${b[3] - b[1]}px` }));
    }
  });
  stage.addEventListener("pointermove", (e) => {
    if (!drag) return;
    drag.dx = e.clientX - drag.x;
    drag.dy = e.clientY - drag.y;
    if (drag.dot) {
      drag.dot.style.transform = `translate(${drag.dx}px, ${drag.dy}px)`;
      return;
    }
    // the box follows the pointer 1:1; the picture is drawn again when the move is staged
    const box = stage.querySelector(".layerbox");
    if (box && drag.layer != null) box.style.transform = `translate(${drag.dx}px, ${drag.dy}px)`;
  });
  const end = (e) => {
    const d = drag;
    drag = null;
    if (!d) return;
    try {
      stage.releasePointerCapture(e.pointerId);
    } catch {}
    const k = ratio() / v.scale;
    const [fx, fy] = [Math.round(d.dx * k), Math.round(d.dy * k)];
    if (d.dot) {
      const a = v.anchors[d.anchor];
      if (a && Math.hypot(d.dx, d.dy) >= 3 && (fx || fy)) return actEdit({ op: "set_anchor", anchor: d.anchor, x: a.x + fx, y: a.y + fy });
      return drawActEditor();
    }
    // a click (or a tiny slip) only selects
    if (d.layer != null && Math.hypot(d.dx, d.dy) >= 3 && (fx || fy)) move(d.layer, fx, fy);
    else drawActEditor();
  };
  stage.addEventListener("pointerup", end);
  stage.addEventListener("pointercancel", () => {
    drag = null;
    drawActEditor();
  });
  stage.addEventListener("keydown", (e) => {
    const step = e.shiftKey ? 10 : 1;
    const dir = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }[e.key];
    if (!dir) return;
    e.preventDefault();
    if (E.layer == null) E.layer = v.layers.length - 1;
    move(E.layer, ...dir);
  });
}

function drawActEditor() {
  const v = E.view;
  const num = (value, onset, opts = {}) => {
    const i = el("input", { type: "number", value: String(value), step: opts.step ?? "1", class: opts.narrow ? "narrow" : "" });
    i.addEventListener("change", () => (i.value !== "" && Number.isFinite(Number(i.value)) ? onset(Number(i.value)) : loadActFrame()));
    return i;
  };
  const setL = (layer, fields) => actEdit({ op: "set_layer", layer, ...fields });
  const actionSel = el("select", { onchange: () => { E.action = Number(actionSel.value); E.frame = 0; loadActFrame(); } });
  for (let i = 0; i < v.actions; i++) actionSel.append(el("option", { value: i, text: t("pvw-action-n", { n: i }), selected: i === v.action }));
  const bar = el(
    "div",
    { class: "bar" },
    el("label", { text: `${t("pvw-action")} ` }, actionSel),
    el("button", { type: "button", class: "small", text: "◀", disabled: v.frame === 0, onclick: () => { E.frame -= 1; loadActFrame(); } }),
    el("span", { text: t("act-frame-label", { n: v.frame + 1, total: v.frames }) }),
    el("button", { type: "button", class: "small", text: "▶", disabled: v.frame >= v.frames - 1, onclick: () => { E.frame += 1; loadActFrame(); } }),
    el("button", { type: "button", class: "small", text: E.playing ? t("act-stop") : t("act-play"), "aria-pressed": String(E.playing), "data-act-play": "", title: t("act-play-title"), disabled: v.frames <= 1, onclick: toggleActPlay }),
    el("label", { class: "muted", text: `${t("act-ms")} ` }, num(v.interval_ms, (n) => actEdit({ op: "set_interval", ms: n }), { narrow: true })),
    el("span", { class: "muted", text: `ACT ${v.version}` }),
    v.sounds_supported ? soundField(v) : null,
    el("button", { type: "button", class: "small", id: "act-undo", text: t("btn-undo"), onclick: () => doHistory(false) }),
    el("button", { type: "button", class: "small", id: "act-redo", text: t("btn-redo"), onclick: () => doHistory(true) }),
    el("button", { type: "button", class: "small", text: t("act-copy-frame"), title: t("act-copy-frame-title"), onclick: () => actEdit({ op: "copy_frame" }) }),
    el("button", { type: "button", class: "small", icon: "interpolate", text: t("act-inbetween"), title: t("act-inbetween-title"), disabled: v.frames <= 1, onclick: () => openInterpolate(v) }),
    el("button", { type: "button", class: "small", text: t("act-frame-earlier"), title: t("act-frame-earlier-title"), disabled: v.frame === 0, onclick: () => actEdit({ op: "move_frame", to: v.frame - 1 }) }),
    el("button", { type: "button", class: "small", text: t("act-frame-later"), title: t("act-frame-later-title"), disabled: v.frame >= v.frames - 1, onclick: () => actEdit({ op: "move_frame", to: v.frame + 1 }) }),
    el("button", { type: "button", class: "small", text: t("act-copy-action"), title: t("act-copy-action-title"), onclick: () => actEdit({ op: "copy_action" }) }),
    el("button", { type: "button", class: "small", text: t("act-action-up"), title: t("act-action-up-title"), disabled: v.action === 0, onclick: () => actEdit({ op: "move_action", to: v.action - 1 }) }),
    el("button", { type: "button", class: "small", text: t("act-action-down"), title: t("act-action-down-title"), disabled: v.action >= v.actions - 1, onclick: () => actEdit({ op: "move_action", to: v.action + 1 }) }),
    el("button", { type: "button", class: "small", text: t("act-remove-action"), disabled: v.actions <= 1, onclick: () => removeAction(v.action) }),
    el("button", { type: "button", class: "small", text: t("act-remove-frame"), disabled: v.frames <= 1, onclick: () => actEdit({ op: "remove_frame" }) }),
  );
  if (E.layer != null && E.layer >= v.layers.length) E.layer = null;
  const stage = el("div", { class: "stage checker", tabindex: "0", "data-act-stage": "", role: "group", "aria-roledescription": t("act-stage-role"), "aria-label": `${t("act-stage-aria", { n: v.frame + 1 })}${E.layer != null ? ` ${t("act-stage-sel", { n: E.layer })}` : ""}` });
  if (v.png) {
    stage.append(el("img", { src: dataUrl("image/png", v.png), width: v.width, height: v.height, alt: "", draggable: "false" }), el("div", { class: "cross-h", style: `top:${v.origin_y}px` }), el("div", { class: "cross-v", style: `left:${v.origin_x}px` }));
    v.anchors.forEach((a, i) => stage.append(el("div", { class: "dot", title: t("act-anchor-title", { n: i }), "data-anchor": String(i), style: `left:${v.origin_x + a.x * v.scale}px;top:${v.origin_y + a.y * v.scale}px` })));
    const b = E.layer != null ? v.layers[E.layer]?.bounds : null;
    if (b) stage.append(el("div", { class: "layerbox", style: `left:${b[0]}px;top:${b[1]}px;width:${b[2] - b[0]}px;height:${b[3] - b[1]}px` }));
    wireStage(stage, v);
  }
  const left = el("div", {}, stage, v.error ? el("p", { class: "muted", text: t("act-no-picture", { error: v.error }) }) : null, el("p", { class: "muted", text: t("act-stage-note") }));
  const head = ["#", t("act-th-type"), t("act-th-image"), "x", "y", t("act-th-mirror"), v.separate_scale ? t("act-th-scale-xy") : t("act-th-scale"), t("act-th-rot"), t("act-th-colour"), "α", ""];
  const rows = v.layers.map((l, i) => {
    const typeSel = el("select", { onchange: () => setL(i, { sprite_type: Number(typeSel.value) }) }, el("option", { value: 0, text: t("act-type-palette", { n: v.indexed_images }), selected: l.sprite_type === 0 }), el("option", { value: 1, text: t("act-type-rgba", { n: v.rgba_images }), selected: l.sprite_type === 1 }));
    const mirror = el("input", { type: "checkbox", checked: l.mirror, onchange: () => setL(i, { mirror: mirror.checked }) });
    const colour = el("input", { type: "color", value: `#${hex2(l.color[0])}${hex2(l.color[1])}${hex2(l.color[2])}` });
    colour.addEventListener("change", () => setL(i, { color: [parseInt(colour.value.slice(1, 3), 16), parseInt(colour.value.slice(3, 5), 16), parseInt(colour.value.slice(5, 7), 16), l.color[3]] }));
    return el(
      "tr",
      { class: i === E.layer ? "sel" : "" },
      el("td", {}, el("button", { type: "button", class: "small", text: String(i), title: t("act-layer-select"), "aria-pressed": String(i === E.layer), onclick: () => { E.layer = i === E.layer ? null : i; drawActEditor(); } })),
      el("td", {}, typeSel),
      el("td", {}, num(l.sprite_index, (n) => setL(i, { sprite_index: n }), { narrow: true })),
      el("td", {}, num(l.offset_x, (n) => setL(i, { offset_x: n }), { narrow: true })),
      el("td", {}, num(l.offset_y, (n) => setL(i, { offset_y: n }), { narrow: true })),
      el("td", {}, mirror),
      el("td", {}, num(l.scale_x, (n) => setL(i, { scale_x: n }), { step: "0.1", narrow: true }), v.separate_scale ? num(l.scale_y, (n) => setL(i, { scale_y: n }), { step: "0.1", narrow: true }) : null),
      el("td", {}, num(l.rotation, (n) => setL(i, { rotation: Math.round(n) }), { narrow: true })),
      el("td", {}, colour),
      el("td", {}, num(l.color[3], (n) => setL(i, { color: [l.color[0], l.color[1], l.color[2], Math.max(0, Math.min(255, Math.round(n)))] }), { narrow: true })),
      el("td", {}, el("button", { type: "button", class: "small", text: "↑", title: t("act-layer-earlier"), disabled: i === 0, onclick: () => actEdit({ op: "move_layer", layer: i, to: i - 1 }) }), el("button", { type: "button", class: "small", text: "↓", title: t("act-layer-later"), disabled: i === v.layers.length - 1, onclick: () => actEdit({ op: "move_layer", layer: i, to: i + 1 }) }), el("button", { type: "button", class: "small", text: "✕", title: t("act-layer-remove"), onclick: () => actEdit({ op: "remove_layer", layer: i }) })),
    );
  });
  const layers = el("div", {}, el("div", { class: "tscroll" }, el("table", {}, el("thead", {}, el("tr", {}, head.map((t) => el("th", { text: t })))), el("tbody", {}, rows))), el("div", { class: "bar" }, el("button", { type: "button", class: "small", text: t("act-add-layer"), onclick: () => actEdit({ op: "add_layer" }) })));
  const anchors = v.anchors_supported
    ? el(
        "div",
        {},
        el("h3", { text: t("act-anchors-heading") }),
        el("table", {}, el("tbody", {}, v.anchors.map((a, i) => el("tr", {}, el("td", { text: `#${i}` }), el("td", {}, num(a.x, (n) => actEdit({ op: "set_anchor", anchor: i, x: Math.round(n) }), { narrow: true })), el("td", {}, num(a.y, (n) => actEdit({ op: "set_anchor", anchor: i, y: Math.round(n) }), { narrow: true })), el("td", {}, el("button", { type: "button", class: "small", text: "✕", onclick: () => actEdit({ op: "remove_anchor", anchor: i }) })))))),
        el("button", { type: "button", class: "small", text: t("act-add-anchor"), onclick: () => actEdit({ op: "add_anchor" }) }),
      )
    : el("p", { class: "muted", text: t("act-no-anchors", { version: v.version }) });
  // every frame is drawn anew: keep the keyboard on Play/Stop while it plays
  const keepPlay = document.activeElement?.hasAttribute("data-act-play");
  const keepStage = document.activeElement?.hasAttribute("data-act-stage");
  $("act-body").replaceChildren(el("div", { class: "acted" }, bar, referenceRow(v), left, el("div", {}, layers, anchors)));
  if (keepPlay) $("act-body").querySelector("[data-act-play]")?.focus();
  if (keepStage) $("act-body").querySelector("[data-act-stage]")?.focus();
  updateEditBar();
}

async function setInterval_(row, action, current) {
  const input = el("input", { type: "number", class: "field", min: "1", step: "1", value: String(current) });
  const go = await ask(t("act-interval-title", { n: action }), [el("p", { text: t("act-interval-prompt") }), input], [{ label: t("act-interval-set"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  try {
    setPending(await invoke("stage_act_interval", { index: row.index, action, ms: Number(input.value) }));
    setStatus(t("act-interval-status", { n: action }));
    refreshPreview(action);
  } catch (e) {
    await note(t("act-interval-failed"), errText(e), true);
  }
}

async function doReplace() {
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  if (!sel) return;
  const file = await T.dialog.open({ multiple: false, directory: false, title: t("replace-pick", { name: sel.name }) });
  if (!file) return;
  try {
    setPending(await invoke("stage_replace", { index: sel.index, file }));
    setStatus(t("replace-status", { name: sel.name }));
    refreshPreview();
  } catch (e) {
    await note(t("replace-failed"), errText(e), true);
  }
}

async function doAdd() {
  const file = await T.dialog.open({ multiple: false, directory: false, title: t("add-pick") });
  if (!file) return;
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  const folder = sel ? (sel.folder ? `${sel.name}\\` : sel.name.slice(0, sel.name.lastIndexOf("\\") + 1)) : "data\\";
  const input = el("input", { type: "text", class: "field", value: folder + file.split(/[\\/]/).pop(), spellcheck: "false" });
  const go = await ask(t("add-title"), [el("p", { text: t("add-prompt") }), input], [{ label: t("dlg-add"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  try {
    setPending(await invoke("stage_add", { path: input.value, file }));
    setStatus(t("add-status", { path: input.value }));
  } catch (e) {
    await note(t("add-failed"), errText(e), true);
  }
}

async function doRemove() {
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  const buttons = [];
  if (sel) buttons.push({ label: sel.folder ? t("ext-folder", { n: sel.count }) : t("ext-entry"), value: "one", primary: true });
  buttons.push({ label: t("ext-all", { n: S.matches }), value: "all", primary: !sel });
  buttons.push({ label: "Cancel", value: null });
  const what = await ask(t("remove-title"), el("p", { text: t("remove-body") }), buttons, "warning");
  if (!what) return;
  try {
    const list = what === "one" ? await entriesOf(sel) : [];
    if (what === "one" && !list.length) return note(t("btn-remove"), t("ext-empty-folder"));
    setPending(await invoke("stage_remove", { indices: list }));
    setStatus(t("remove-status", { n: what === "one" ? list.length : S.matches }));
    refreshPreview();
  } catch (e) {
    await note(t("remove-failed"), errText(e), true);
  }
}

async function doChanges() {
  if (!S.changes.length) {
    if (S.info) return saveDialog("as");
    await note(t("changes-title"), t("changes-none"));
    return;
  }
  const list = el("ul", { class: "pending" }, ...S.changes.slice(0, 500).map((p) => el("li", {}, el("span", { class: "kind", text: p.kind }), el("span", { text: p.name }), p.file ? el("span", { class: "file", text: p.file }) : null)));
  const more = S.changes.length > 500 ? el("p", { class: "muted", text: t("changes-more", { n: S.changes.length - 500 }) }) : null;
  const buttons = S.untitled ? [] : [{ label: t("save-here"), value: "here", primary: true }];
  buttons.push({ label: t("save-as-button"), value: "save", primary: !!S.untitled }, { label: t("changes-discard"), value: "discard" }, { label: "Close", value: null });
  const act = await ask(t("changes-heading", { n: S.changes.length }), [list, more], buttons);
  if (act === "here") return doSave();
  if (act === "discard") {
    await invoke("discard_pending");
    setPending([]);
    setStatus(t("changes-discarded"));
    refreshPreview();
  } else if (act === "save") {
    await saveDialog("as");
  }
}

/** Cmd/Ctrl+S: writes the changes into the archive itself (a new archive asks where first). */
async function doSave() {
  if (!S.info) return;
  if (S.untitled) return saveDialog("as");
  if (!S.changes.length) {
    setStatus(t("save-nothing"));
    return;
  }
  await saveInPlace(null);
}

/** Writes the archive over itself: next to it first, checked, then swapped in and opened again. */
async function saveInPlace(req) {
  const job = showJob(t("save-running"));
  setStatus(t("save-status", { dst: S.info.path }));
  try {
    const r = await invoke("save_archive", { req });
    job.close();
    await showOpened(r.info);
    setStatus(t("save-here-done", { name: r.info.file_name, files: t("pvw-files", { n: r.summary.files }), size: fmtBytes(r.summary.file_bytes) }));
  } catch (e) {
    job.close();
    const cancelled = /cancel/i.test(errText(e));
    setStatus(cancelled ? t("save-cancelled") : t("save-failed"));
    await note(cancelled ? t("save-cancelled") : t("save-could-not"), cancelled ? t("nothing-written") : errText(e), !cancelled);
  }
}

const SAVE_TYPES = { grf: "GRF", gpf: "GPF", thor: "Thor", rgz: "RGZ" };
const extOfPath = (p) => (/\.([a-z0-9]+)$/i.exec(p || "")?.[1] || "grf").toLowerCase();

/**
 * Save as, Save advanced and Archive settings in one dialog (GRF Editor has them in three
 * places): how the files are stored (copied or compressed again), the GRF version and the
 * encryption. `preset` picks the starting choice: as, repack, compact, encrypt, decrypt, settings.
 */
async function saveDialog(preset = "as") {
  if (!S.info) return;
  const info = S.info;
  const kind = S.untitled || extOfPath(info.path);
  const encrypted = !!info.encryption;
  const method = el("select", {}, ["zlib", "zopfli", "lzma"].map((k) => el("option", { value: k, text: t(`method-${k}`) })));
  method.value = SET.method;
  const store = el("select", {}, el("option", { value: "copy", text: t("sa-store-copy") }), el("option", { value: "recompress", text: t("sa-store-recompress") }));
  store.value = preset === "repack" ? "recompress" : "copy";
  const format = el("select", {}, el("option", { value: "keep", text: t("sa-format-keep", { format: info.format }) }), ...["0x200", "0x300", "0x103", "0x102"].map((k) => el("option", { value: k, text: t(`format-${k}`) })));
  const enc = el("select", {}, encrypted ? el("option", { value: "keep", text: t("sa-enc-keep") }) : null, el("option", { value: "none", text: encrypted ? t("sa-enc-none") : t("sa-enc-plain") }), el("option", { value: "password", text: t("sa-enc-password") }));
  enc.value = preset === "encrypt" ? "password" : preset === "decrypt" ? "none" : encrypted ? "keep" : "none";
  const pass = el("input", { type: "password", class: "field", autocomplete: "new-password", placeholder: t("sa-password"), "aria-label": t("sa-password") });
  const pass2 = el("input", { type: "password", class: "field", autocomplete: "new-password", placeholder: t("sa-password-again"), "aria-label": t("sa-password-again") });
  const passRow = el("div", { class: "form" }, pass, pass2);
  // format and encryption are GRF things; a Thor or RGZ has neither
  const grfTarget = kind !== "thor" && kind !== "rgz";
  const sync = () => {
    method.disabled = store.value !== "recompress";
    passRow.hidden = !grfTarget || enc.value !== "password";
  };
  for (const c of [store, enc]) c.addEventListener("change", sync);
  sync();
  const facts = el("dl", { class: "facts" },
    el("dt", { text: t("as-path") }), el("dd", { text: S.untitled ? t("as-unsaved") : info.path }),
    el("dt", { text: t("as-type") }), el("dd", { text: info.format }),
    el("dt", { text: t("as-content") }), el("dd", { text: `${t("pvw-files", { n: info.files })} · ${fmtBytes(info.file_size)}` }),
    el("dt", { text: t("as-encryption") }), el("dd", { text: encrypted ? t("as-encrypted") : t("as-plain") }),
    el("dt", { text: t("as-names") }), el("dd", { text: $("names").selectedOptions[0]?.textContent || "" }),
  );
  const fmtLabel = el("label", {}, el("span", { class: "fl", text: t("set-format") }), format);
  const encLabel = el("label", {}, el("span", { class: "fl", text: t("sa-encryption") }), enc);
  for (const n of [fmtLabel, encLabel, passRow]) if (!grfTarget) n.hidden = true;
  const body = [
    preset === "settings" ? facts : null,
    el("div", { class: "form buildform" },
      el("label", {}, el("span", { class: "fl", text: t("sa-store") }), store),
      el("label", {}, el("span", { class: "fl", text: t("set-compression") }), method),
      fmtLabel,
      encLabel,
    ),
    passRow,
    el("p", { class: "hint", text: t("sa-hint") }),
  ];
  const titles = { as: "sa-title", repack: "sa-title-repack", compact: "sa-title-compact", encrypt: "sa-title-encrypt", decrypt: "sa-title-decrypt", settings: "as-title" };
  const buttons = [];
  if (!S.untitled) buttons.push({ label: t("save-here"), value: "here", primary: preset !== "as" });
  buttons.push({ label: t("save-as-button"), value: "as", primary: preset === "as" || !!S.untitled }, { label: "Cancel", value: null });
  for (;;) {
    const act = await ask(t(titles[preset] || "sa-title"), body, buttons, preset === "settings" ? "info" : undefined);
    if (!act) return;
    if (enc.value === "password" && (!pass.value || pass.value !== pass2.value)) {
      await note(t("sa-title-encrypt"), pass.value ? t("sa-password-mismatch") : t("err-password-empty"), true);
      continue;
    }
    const req = {
      dst: "",
      recompress: store.value === "recompress",
      method: method.value,
      format: format.value === "keep" ? null : format.value,
      encryption: enc.value,
      password: enc.value === "password" ? pass.value : null,
    };
    if (act === "here") return saveInPlace(req);
    const base = S.untitled ? t("untitled") : info.file_name.replace(/\.[^.]+$/, "");
    const types = Object.keys(SAVE_TYPES).sort((a, b) => (a === kind ? -1 : b === kind ? 1 : 0));
    const dst = await T.dialog.save({ title: t("save-pick"), defaultPath: `${base}${S.untitled ? "" : "-new"}.${kind}`, filters: types.map((k) => ({ name: t("sa-filter", { type: SAVE_TYPES[k] }), extensions: [k] })) });
    if (!dst) continue;
    return saveAs({ ...req, dst });
  }
}

async function saveAs(req) {
  const job = showJob(t("save-running"));
  setStatus(t("save-status", { dst: req.dst }));
  try {
    const r = await invoke("save_as", { req });
    job.close();
    if (!r.verified) return note(t("save-could-not"), t("save-unverified", { dst: req.dst }), true);
    // like every "Save as": the changes are in the new file now, and this window shows it
    await invoke("discard_pending").catch(() => {});
    S.changes = [];
    await openPath(req.dst, true);
    setStatus(t("save-as-done", { dst: req.dst }));
  } catch (e) {
    job.close();
    const cancelled = /cancel/i.test(errText(e));
    setStatus(cancelled ? t("save-cancelled") : t("save-failed"));
    await note(cancelled ? t("save-cancelled") : t("save-could-not"), cancelled ? t("nothing-written") : errText(e), !cancelled);
  }
}

/** Tools › Hashes: CRC32, MD5 and SHA-256 of the chosen file (as it will be saved). */
async function doHashes() {
  const row = S.selected != null ? S.rows[S.selected - S.offset] : null;
  if (!row || row.folder) return note(t("hash-title"), t("hash-pick"));
  try {
    const h = await invoke("entry_hashes", { index: row.index });
    const line = (k, v) => [el("dt", { text: k }), el("dd", {}, el("code", { text: v }))];
    await ask(t("hash-title"), [el("p", { text: row.name }), el("dl", { class: "facts selectable" }, ...line(t("hash-size"), `${fmtNum(h.size)} B`), ...line("CRC32", h.crc32), ...line("MD5", h.md5), ...line("SHA-256", h.sha256))], [{ label: "OK", value: true, primary: true }]);
  } catch (e) {
    await note(t("hash-title"), errText(e), true);
  }
}

/** Tools › Compare: what differs from another archive, and those differences as a patch. */
async function doCompare() {
  if (!S.info) return;
  const other = await T.dialog.open({ multiple: false, directory: false, title: t("cmp-pick"), filters: [{ name: "GRF, GPF, Thor, RGZ", extensions: ["grf", "gpf", "thor", "rgz"] }] });
  if (!other) return;
  const job = showJob(t("cmp-running"));
  let r;
  try {
    r = await invoke("compare_with", { other });
    job.close();
  } catch (e) {
    job.close();
    return note(t("cmp-failed"), errText(e), true);
  }
  const list = (title, names, total) =>
    total ? el("details", {}, el("summary", { text: `${title} (${fmtNum(total)})` }), el("ul", { class: "pending" }, ...names.slice(0, 300).map((n) => el("li", { text: n }))), total > 300 ? el("p", { class: "muted", text: t("changes-more", { n: total - 300 }) }) : null) : null;
  const same = !r.only_here_total && !r.only_there_total && !r.changed_total;
  const body = [
    el("p", { text: t("cmp-summary", { other: baseOf(r.other.replace(/\//g, "\\")), here: t("pvw-files", { n: r.only_here_total }), changed: t("pvw-files", { n: r.changed_total }), there: t("pvw-files", { n: r.only_there_total }), same: fmtNum(r.same) }) }),
    list(t("cmp-only-here"), r.only_here, r.only_here_total),
    list(t("cmp-changed"), r.changed, r.changed_total),
    list(t("cmp-only-there"), r.only_there, r.only_there_total),
    same ? null : el("p", { class: "hint", text: t("cmp-patch-hint") }),
  ];
  const act = await ask(same ? t("cmp-same") : t("cmp-title"), body, same ? [{ label: "OK", value: null, primary: true }] : [{ label: t("cmp-make-patch"), value: "patch", primary: true }, { label: "Close", value: null }], same ? "success" : undefined);
  if (act !== "patch") return;
  const base = S.info.file_name.replace(/\.[^.]+$/, "");
  const dst = await T.dialog.save({ title: t("cmp-patch-pick"), defaultPath: `${base}-patch.thor`, filters: [{ name: t("sa-filter", { type: "Thor" }), extensions: ["thor"] }, { name: t("sa-filter", { type: "GRF" }), extensions: ["grf"] }] });
  if (!dst) return;
  const job2 = showJob(t("cmp-patch-running"));
  try {
    const p = await invoke("make_patch", { old: other, dst, method: SET.method });
    job2.close();
    const go = await ask(t("cmp-patch-done"), el("p", { text: t("cmp-patch-result", { added: p.added, changed: p.changed, removed: p.removed, size: fmtBytes(p.file_bytes), dst }) }), [{ label: t("dlg-open-it"), value: true, primary: true }, { label: "Close", value: false }], "success");
    if (go) await openPath(dst);
  } catch (e) {
    job2.close();
    await note(t("cmp-failed"), errText(e), true);
  }
}

/** Help › About: version, who made it, and whose work it stands on. */
async function openAbout() {
  let version = "";
  try {
    version = (await T.app?.getVersion?.()) || "";
  } catch {}
  const link = (text, url) => el("a", { href: url, text, onclick: (e) => { e.preventDefault(); invoke("open_link", { url }).catch(() => {}); } });
  const body = [
    el("div", { class: "about" },
      el("img", { src: "icon.png", alt: "", width: 72, height: 72 }),
      el("div", {},
        el("h3", { text: "RustyGRF" }),
        el("p", { class: "muted", text: version ? t("about-version", { version }) : "" }),
        el("p", { text: t("about-tagline") }),
      ),
    ),
    el("h4", { text: t("about-made-by") }),
    el("p", {}, "Kadir Sert · ", link("github.com/kxdrsrt", "https://github.com/kxdrsrt"), " · ", link("bento.me/kadir-sert", "https://bento.me/kadir-sert")),
    el("h4", { text: t("about-thanks") }),
    el("ul", { class: "credits" },
      el("li", {}, link("Tokeiburu", "https://github.com/Tokeiburu"), ` — ${t("about-tokeiburu")}`),
      el("li", {}, link("Lua", "https://www.lua.org/"), ` — ${t("about-lua")}`),
      el("li", {}, link("Tauri", "https://github.com/tauri-apps/tauri"), ` — ${t("about-tauri")}`),
    ),
    el("p", { class: "muted", text: t("about-license") }),
  ];
  await ask(t("about-title"), body, [{ label: "OK", value: true, primary: true }]);
}

async function saveEdited() {
  const base = S.info.file_name.replace(/\.[^.]+$/, "");
  const dst = await T.dialog.save({ title: t("save-pick"), defaultPath: `${base}-edited.grf`, filters: [{ name: "GRF archives", extensions: ["grf"] }] });
  if (!dst) return;
  const job = showJob(t("save-running"));
  setStatus(t("save-status", { dst }));
  try {
    const r = await invoke("save_edited", { dst });
    job.close();
    setStatus(t("save-done-status", { dst }));
    const body = [el("p", { text: t("save-result", { files: r.files, size: fmtBytes(r.file_bytes), verified: r.verified ? t("verified-yes") : t("verified-no"), added: r.added, replaced: r.replaced, removed: r.removed }) }), el("p", { class: "muted", text: t("save-note") })];
    const open = await ask(t("save-done-title"), body, [{ label: t("dlg-open-it"), value: true, primary: true }, { label: "Close", value: false }]);
    if (open) await openPath(dst, true);
  } catch (e) {
    job.close();
    const cancelled = /cancel/i.test(errText(e));
    setStatus(cancelled ? t("save-cancelled") : t("save-failed"));
    await note(cancelled ? t("save-cancelled") : t("save-could-not"), cancelled ? t("nothing-written") : errText(e), !cancelled);
  }
}

// What the build options mean, shown next to the choice (Build dialog and Settings)
const METHODS = () => ["zlib", "zopfli", "lzma"].map((k) => [k, t(`method-${k}`), t(`method-${k}.hint`)]);
const FORMATS = () => ["0x200", "0x300", "0x103", "0x102"].map((k) => [k, t(`format-${k}`), t(`format-${k}.hint`)]);
/** A select for one of `table`'s choices with a line under it that explains the current choice. */
function explained(table, value, onchange) {
  const hint = el("div", { class: "hint" });
  const sel = el("select", {}, table.map(([v, t]) => el("option", { value: v, text: t })));
  const show = () => (hint.textContent = table.find((r) => r[0] === sel.value)?.[2] || "");
  sel.value = value;
  sel.addEventListener("change", () => { show(); onchange?.(sel.value); });
  show();
  return { sel, hint };
}

async function doBuild() {
  const src = await T.dialog.open({ directory: true, multiple: false, title: t("build-pick") });
  if (!src) return;
  const dedupe = el("input", { type: "checkbox" });
  const m = explained(METHODS(), SET.method);
  const f = explained(FORMATS(), SET.format, (v) => {
    const old = v === "0x102" || v === "0x103";
    dedupe.disabled = old;
    if (old) dedupe.checked = false;
  });
  const method = m.sel, format = f.sel;
  dedupe.disabled = format.value === "0x102" || format.value === "0x103";
  const body = [
    el("p", { text: t("build-pack", { src }) }),
    el("div", { class: "form buildform" },
      el("label", {}, el("span", { class: "fl", text: t("set-compression") }), method), m.hint,
      el("label", {}, el("span", { class: "fl", text: t("set-format") }), format), f.hint,
      el("label", {}, dedupe, ` ${t("build-share")}`),
      el("div", { class: "hint", text: t("build-share-hint") })),
    el("p", { class: "muted", text: t("build-verify-note") }),
  ];
  const go = await ask(t("build-title"), body, [{ label: t("dlg-choose-target"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  const base = src.split(/[\\/]/).filter(Boolean).pop() || "data";
  const dst = await T.dialog.save({ title: t("build-save-as"), defaultPath: `${base}.grf`, filters: [{ name: "GRF archives", extensions: ["grf"] }] });
  if (!dst) return;
  const job = showJob(t("build-running"));
  setStatus(t("build-status", { dst }));
  try {
    const r = await invoke("build_archive", { req: { src, dst, method: method.value, format: format.value, dedupe: dedupe.checked, overwrite: true } });
    job.close();
    setStatus(t("build-done-status", { dst, files: r.files }));
    const body = [el("p", { text: t("build-result", { files: r.files, blocks: r.blocks, from: fmtBytes(r.unpacked_bytes), to: fmtBytes(r.file_bytes), verified: r.verified ? t("verified-yes") : t("verified-no") }) })];
    if (r.shared) body.push(el("p", { class: "muted", text: t("build-shared", { n: r.shared }) }));
    if (r.skipped.length) body.push(el("p", { text: t("build-skipped", { n: r.skipped.length }) }), el("pre", { text: r.skipped.join("\n") }));
    const open = await ask(t("build-done-title"), body, [{ label: t("dlg-open-it"), value: true, primary: true }, { label: "Close", value: false }]);
    if (open) await openPath(dst);
  } catch (e) {
    job.close();
    const cancelled = /cancel/i.test(errText(e));
    setStatus(cancelled ? t("build-cancelled") : t("build-failed"));
    await note(cancelled ? t("build-cancelled") : t("build-could-not"), cancelled ? t("nothing-written") : errText(e), !cancelled);
  }
}

async function doMerge() {
  const picked = await T.dialog.open({ multiple: true, directory: false, title: t("merge-pick"), filters: [{ name: "GRF, GPF, Thor and RGZ archives", extensions: ["grf", "gpf", "thor", "rgz"] }] });
  const paths = Array.isArray(picked) ? [...picked] : picked ? [picked] : [];
  if (paths.length < 2) {
    if (paths.length) await note(t("merge-title"), t("merge-need-two"), true);
    return;
  }
  const replaceOnly = el("input", { type: "checkbox" });
  const list = el("ol", { class: "prio" });
  const draw = () =>
    list.replaceChildren(
      ...paths.map((p, i) =>
        el(
          "li",
          {},
          el("span", { class: "name", text: p.split(/[\\/]/).pop(), title: p }),
          el("button", { type: "button", text: "↑", title: t("merge-higher"), disabled: i === 0, onclick: () => { [paths[i - 1], paths[i]] = [paths[i], paths[i - 1]]; draw(); } }),
          el("button", { type: "button", text: "↓", title: t("merge-lower"), disabled: i === paths.length - 1, onclick: () => { [paths[i + 1], paths[i]] = [paths[i], paths[i + 1]]; draw(); } }),
        ),
      ),
    );
  draw();
  const body = [
    el("p", { text: t("merge-intro") }),
    list,
    el("div", { class: "form" }, el("label", {}, replaceOnly, ` ${t("merge-replace-only")}`)),
  ];
  const go = await ask(t("merge-title"), body, [{ label: t("dlg-choose-target"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  const dst = await T.dialog.save({ title: t("merge-save-as"), defaultPath: "merged.grf", filters: [{ name: "GRF archives", extensions: ["grf"] }] });
  if (!dst) return;
  const job = showJob(t("merge-running"));
  setStatus(t("merge-status", { dst }));
  try {
    const r = await invoke("merge_archives", { req: { sources: paths, dst, replace_only: replaceOnly.checked, overwrite: true } });
    job.close();
    setStatus(t("merge-done-status", { dst, files: r.files }));
    const lines = r.sources.map((s) => `${t("merge-line", { files: s.files, used: s.used, replaced: s.overridden })}${s.ignored_new ? t("merge-line-ignored", { n: s.ignored_new }) : ""}  ${s.path.split(/[\\/]/).pop()}`);
    const out = [el("p", { text: `${t("merge-result", { files: r.files, size: fmtBytes(r.file_bytes), verified: r.verified ? t("verified-yes") : t("verified-no") })}${r.damaged ? ` ${t("merge-damaged", { n: r.damaged })}` : ""}` }), el("pre", { text: lines.join("\n") })];
    const open = await ask(t("merge-done-title"), out, [{ label: t("dlg-open-it"), value: true, primary: true }, { label: "Close", value: false }]);
    if (open) await openPath(dst);
  } catch (e) {
    job.close();
    const cancelled = /cancel/i.test(errText(e));
    setStatus(cancelled ? t("merge-cancelled") : t("merge-failed"));
    await note(cancelled ? t("merge-cancelled") : t("merge-could-not"), cancelled ? t("nothing-written") : errText(e), !cancelled);
  }
}

const PSNR_HINT = () => t("psnr-hint");

/** One true-colour BMP next to its 8-bit version; staging replaces the entry. */
async function openReduceDialog(row) {
  let p;
  try {
    p = await invoke("reduce_preview", { index: row.index });
  } catch (e) {
    return note(t("reduce1-cannot"), errText(e), true);
  }
  const pic = (data, caption) => {
    const img = el("img", { src: dataUrl("image/bmp", data), alt: caption });
    img.addEventListener("load", () => {
      if (img.naturalWidth <= 160) {
        img.style.width = `${img.naturalWidth * 2}px`;
        img.style.imageRendering = "pixelated";
      }
    });
    return el("figure", {}, el("div", { class: "checker" }, img), el("figcaption", { class: "muted", text: caption }));
  };
  const quality = p.psnr == null ? t("reduce1-lossless") : t("reduce1-psnr", { db: p.psnr.toFixed(1) });
  const go = await ask(
    t("reduce1-title"),
    [
      el("p", { text: t("reduce1-info", { before: fmtBytes(p.bytes_before), after: fmtBytes(p.bytes_after), colors: p.colors, quality }) }),
      p.psnr == null ? null : el("p", { class: "muted", text: PSNR_HINT() }),
      el("div", { class: "compare" }, pic(p.before, t("reduce1-original")), pic(p.after, t("reduce1-8bit"))),
    ],
    [{ label: t("reduce1-go"), value: true, primary: true }, { label: "Cancel", value: null }],
  );
  if (!go) return;
  try {
    setPending(await invoke("stage_reduce", { index: row.index }));
    setStatus(t("reduce1-status", { name: row.name }));
    refreshPreview();
  } catch (e) {
    await note(t("reduce-could-not"), errText(e), true);
  }
}

/** A copy of the open archive (with the pending changes) whose true-colour BMPs are 8-bit. */
async function doReduceImages() {
  if (!S.info) return;
  const level = el(
    "select",
    { class: "field" },
    ...[45, 40, 38, 35].map((v) => el("option", { value: String(v), text: t(`reduce-level-${v}`), selected: v === 40 })),
  );
  const pending = S.changes.length;
  const go = await ask(
    t("reduce-title"),
    [
      el("p", { text: t("reduce-intro") }),
      el("p", { class: "muted", text: `${PSNR_HINT()} ${t("reduce-notes")}` }),
      el("div", { class: "form" }, el("label", {}, `${t("reduce-quality")} `, level)),
      pending ? el("p", { text: t("reduce-pending", { n: pending }) }) : null,
    ],
    [{ label: t("dlg-choose-target"), value: true, primary: true }, { label: "Cancel", value: null }],
  );
  if (!go) return;
  const base = S.info.file_name.replace(/\.[^.]+$/, "");
  const dst = await T.dialog.save({ title: t("reduce-save-as"), defaultPath: `${base}-8bit.grf`, filters: [{ name: "GRF archives", extensions: ["grf"] }] });
  if (!dst) return;
  const job = showJob(t("reduce-running"));
  setStatus(t("reduce-status", { dst }));
  try {
    const r = await invoke("reduce_images", { dst, minPsnr: Number(level.value) });
    job.close();
    const why = (k) => (t(`reduce-why-${k}`) !== `reduce-why-${k}` ? t(`reduce-why-${k}`) : k);
    const kept = Object.entries(r.kept).map(([k, n]) => `${fmtNum(n)} ${why(k)}`).join(", ");
    const saved = r.stored_before - r.stored_after;
    const lines = [
      el("p", { text: t("reduce-summary", { reduced: r.reduced, matched: r.matched, lossless: r.lossless, worst: r.worst_psnr != null ? t("reduce-worst", { db: r.worst_psnr.toFixed(1) }) : "", saved: fmtBytes(saved) }) }),
      kept ? el("p", { class: "muted", text: t("reduce-kept", { list: kept }) }) : null,
    ];
    if (!r.written) {
      setStatus(t("reduce-nothing"));
      return ask(t("reduce-nothing"), lines, [{ label: "OK", value: true, primary: true }], "info");
    }
    lines.push(el("p", { text: t("reduce-files", { files: r.files, size: fmtBytes(r.file_bytes), verified: r.verified ? t("verified-yes") : t("verified-no") }) }));
    setStatus(t("reduce-done-status", { dst, saved: fmtBytes(saved) }));
    const open = await ask(t("reduce-done-title"), lines, [{ label: t("dlg-open-it"), value: true, primary: true }, { label: "Close", value: false }]);
    if (open) await openPath(dst, true);
  } catch (e) {
    job.close();
    const cancelled = /cancel/i.test(errText(e));
    setStatus(cancelled ? t("reduce-cancelled") : t("reduce-failed"));
    await note(cancelled ? t("reduce-cancelled") : t("reduce-could-not"), cancelled ? t("nothing-written") : errText(e), !cancelled);
  }
}

/** A progress dialog for a long job; the page feeds it with `job-progress` events. */
function showJob(title) {
  const bar = el("progress", { max: 100, value: 0 });
  const label = el("p", { class: "muted", text: t("job-starting") });
  const cancel = el("button", {
    type: "button",
    text: t("dlg-cancel"),
    onclick: () => {
      cancel.disabled = true;
      label.textContent = t("job-cancelling");
      invoke("cancel_job").catch(() => {});
    },
  });
  const dlg = $("dlg");
  $("dlg-title").textContent = title;
  $("dlg-body").replaceChildren(bar, label);
  $("dlg-actions").replaceChildren(cancel);
  const keep = (e) => e.preventDefault();
  dlg.addEventListener("cancel", keep);
  S.onJob = (p) => {
    bar.value = p.total ? (p.done * 100) / p.total : 0;
    label.textContent = t("job-progress", { done: p.done, total: p.total });
  };
  dlg.showModal();
  return {
    close() {
      S.onJob = null;
      dlg.removeEventListener("cancel", keep);
      dlg.close();
    },
  };
}

async function doVerify() {
  if (!S.info) return;
  const mode = await ask(t("verify-title"), el("p", { text: t("verify-body") }), [
    { label: "Quick", value: false },
    { label: "Full", value: true, primary: true },
    { label: "Cancel", value: null },
  ]);
  if (mode === null) return;
  setStatus(t("verify-running"));
  try {
    const r = await invoke("verify", { decode: mode });
    setStatus(t("verify-status", { errors: r.errors, warnings: r.warnings }));
    const lines = [t(r.decoded ? "verify-summary-full" : "verify-summary-quick", { entries: r.entries, errors: r.errors, warnings: r.warnings, infos: r.infos })];
    const body = [el("p", { text: lines[0] })];
    if (r.issues.length) body.push(el("pre", { text: r.issues.join("\n") }));
    await ask(r.errors ? t("verify-problems") : t("verify-ok"), body, [{ label: "OK", value: true, primary: true }]);
  } catch (e) {
    setStatus(t("verify-failed"));
    await note(t("verify-failed"), errText(e), true);
  }
}

// ---------- settings ----------

const SETTINGS_KEY = "rustygrf-settings";
const DEFAULTS = { language: "system", theme: "system", accent: "orange", fontsize: "normal", density: "normal", plainIcons: false, motion: "system", names: "cp949", layout: "last", preview: "last", reopen: false, warnDiscard: true, method: "zlib", format: "0x200", lastArchive: "", recent: [] };
const SET = (() => {
  try {
    return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(SETTINGS_KEY) || "{}") };
  } catch {
    return { ...DEFAULTS };
  }
})();
function saveSettings() {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(SET));
  } catch {}
}
const DENSITY = { compact: 22, normal: 24, comfortable: 30 };

/** Tells the app the scheme the page shows, so the next window starts in its background colour. */
const DARK_QUERY = window.matchMedia?.("(prefers-color-scheme: dark)");
function rememberScheme() {
  const scheme = SET.theme === "system" ? (DARK_QUERY?.matches ? "dark" : "light") : SET.theme;
  if (scheme === rememberScheme.last) return;
  rememberScheme.last = scheme;
  invoke("remember_theme", { theme: scheme }).catch(() => {});
}
DARK_QUERY?.addEventListener?.("change", rememberScheme);

/** Puts the settings on the page; called at start and after each change. */
function applySettings() {
  const root = document.documentElement;
  rememberScheme();
  const flag = (name, on, value = "") => (on ? root.setAttribute(name, value) : root.removeAttribute(name));
  flag("data-theme", SET.theme !== "system", SET.theme);
  flag("data-accent", SET.accent !== "orange", SET.accent);
  flag("data-fontsize", SET.fontsize !== "normal", SET.fontsize);
  flag("data-plainicons", SET.plainIcons);
  flag("data-motion", SET.motion === "off", "off");
  const rowH = DENSITY[SET.density] || 24;
  if (rowH !== ROW_H) {
    ROW_H = rowH;
    root.style.setProperty("--row", `${rowH}px`);
    if (typeof mainList !== "undefined" && S.info) {
      mainList.reset(S.total, true);
      folderList.reset(F.total, true);
    }
  } else root.style.setProperty("--row", `${rowH}px`);
}

function openSettings() {
  const dlg = $("settings-dlg");
  $("settings-title").replaceChildren(svgIcon("settings", "dlg-ic"), t("set-title"));
  const change = (key, value) => {
    SET[key] = value;
    saveSettings();
    applySettings();
  };
  const row = (label, control, hint) => el("div", { class: "srow" }, el("label", { text: label }), control, hint ? el("div", { class: "hint", text: hint }) : null);
  const pick = (key, label, options, hint) => {
    const sel = el("select", { "aria-label": label }, options.map(([v, text]) => el("option", { value: v, text, selected: SET[key] === v })));
    sel.addEventListener("change", () => change(key, sel.value));
    return sel;
  };
  const check = (key, text) => {
    const i = el("input", { type: "checkbox", checked: !!SET[key] });
    i.addEventListener("change", () => change(key, i.checked));
    return el("label", { class: "check" }, i, text);
  };
  const accents = el("div", { class: "accents", role: "group", "aria-label": t("set-accent") }, [["orange", "#ea580c"], ["blue", "#3b82f6"], ["green", "#22a553"], ["purple", "#8b5cf6"]].map(([name, col]) => {
    const b = el("button", { type: "button", title: t(`set-accent-${name}`), "aria-label": t(`set-accent-${name}`), "aria-pressed": String(SET.accent === name), class: SET.accent === name ? "on" : "", style: `background:${col}` });
    b.addEventListener("click", () => {
      change("accent", name);
      for (const x of accents.children) {
        x.classList.toggle("on", x === b);
        x.setAttribute("aria-pressed", String(x === b));
      }
    });
    return b;
  }));
  const languages = [["system", t("set-language-system")], ...I18N.LANGS.map(([code, name, beta]) => [code, beta ? t("set-language-beta", { name }) : name])];
  const langSel = pick("language", t("set-language"), languages);
  langSel.addEventListener("change", () => {
    setLanguage();
    openSettings();
  });
  const layouts = [["last", t("set-as-last")], ...LAYOUTS.map((k) => [k, t(`view-${k}`)])];
  const L = (label, key, table) => {
    const e = explained(table, SET[key], (v) => change(key, v));
    e.sel.setAttribute("aria-label", label);
    return el("div", { class: "srow" }, el("label", { text: label }), e.sel, e.hint);
  };
  $("settings-body").replaceChildren(
    el(
      "div",
      { class: "settings" },
      el("h3", { text: t("set-appearance") }),
      row(t("set-theme"), pick("theme", t("set-theme"), [["system", t("set-theme-system")], ["light", t("set-theme-light")], ["dark", t("set-theme-dark")]])),
      row(t("set-language"), langSel, t("set-language-note")),
      row(t("set-accent"), accents),
      row(t("set-textsize"), pick("fontsize", t("set-textsize"), [["small", t("set-small")], ["normal", t("set-normal")], ["large", t("set-large")]])),
      row(t("set-density"), pick("density", t("set-density"), [["compact", t("set-compact")], ["normal", t("set-normal")], ["comfortable", t("set-comfortable")]])),
      row(t("set-fileicons"), check("plainIcons", t("set-plainicons"))),
      row(t("set-animations"), pick("motion", t("set-animations"), [["system", t("set-theme-system")], ["off", t("set-motion-off")]]), t("set-animations-hint")),
      el("h3", { text: t("set-opening") }),
      row(t("set-names"), pick("names", t("set-names"), [["cp949", t("set-names-cp949")], ["cp1252", t("set-names-cp1252")], ["utf8", t("set-names-utf8")]])),
      row(t("set-layout"), pick("layout", t("set-layout"), layouts), t("set-next-start")),
      row(t("set-preview"), pick("preview", t("set-preview"), [["last", t("set-as-last")], ["right", t("pv-right")], ["bottom", t("pv-bottom")], ["off", t("pv-off")]]), t("set-next-start")),
      row(t("set-startup"), check("reopen", t("set-reopen"))),
      row(t("set-changes"), check("warnDiscard", t("set-warn-discard"))),
      el("h3", { text: t("set-building") }),
      L(t("set-compression"), "method", METHODS()),
      L(t("set-format"), "format", FORMATS()),
      el("p", { class: "muted", text: t("set-building-note") }),
      el("p", { class: "muted", text: t("set-stored-note", { key: /Mac/.test(navigator.platform) ? "Cmd" : "Ctrl" }) }),
    ),
  );
  $("settings-actions").replaceChildren(
    el("button", { type: "button", icon: "reset", text: t("set-reset"), onclick: () => {
      Object.assign(SET, DEFAULTS, { lastArchive: SET.lastArchive });
      saveSettings();
      applySettings();
      setLanguage();
      openSettings();
    } }),
    el("button", { type: "button", class: "primary", icon: "success", text: t("dlg-done"), onclick: () => dlg.close() }),
  );
  if (!dlg.open) {
    dlg.showModal();
    // showModal focuses the first focusable element, which is the scrolling body itself
    $("settings-body").querySelector("select, input, button")?.focus();
  }
}

/** Loads the catalog of the chosen language and puts it on the window, the lists and the native menu. */
function setLanguage() {
  I18N.load(I18N.resolve(SET.language));
  I18N.apply();
  syncViewBar();
  buildKindFilter();
  if (typeof updateEditBar === "function") updateEditBar();
  if (S.info) $("count").textContent = filtersActive() ? t("count-filtered", { n: S.matches, total: S.info.entries }) : t("count-entries", { n: S.matches });
  mainList.draw();
  syncMenu();
}

// ---------- wiring ----------

let searchTimer = 0;
$("btn-open").addEventListener("click", chooseArchive);
$("btn-settings").addEventListener("click", openSettings);
$("btn-open-2").addEventListener("click", chooseArchive);
$("btn-merge").addEventListener("click", doMerge);
$("btn-replace").addEventListener("click", doReplace);
$("btn-add").addEventListener("click", doAdd);
$("btn-remove").addEventListener("click", doRemove);
$("btn-changes").addEventListener("click", doChanges);
$("btn-undo").addEventListener("click", () => doHistory(false));
$("btn-redo").addEventListener("click", () => doHistory(true));
document.addEventListener("keydown", (e) => {
  if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== "z" || !S.info) return;
  const t = e.target;
  // inside a text field Cmd/Ctrl+Z keeps its normal meaning
  if (t instanceof HTMLInputElement && !["checkbox", "color", "button"].includes(t.type)) return;
  if (t instanceof HTMLTextAreaElement) return;
  e.preventDefault();
  doHistory(e.shiftKey);
});
$("btn-build").addEventListener("click", doBuild);
$("btn-extract").addEventListener("click", doExtract);
$("btn-verify").addEventListener("click", doVerify);
$("search").addEventListener("input", (e) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => runSearch(e.target.value.trim()), 180);
});
$("names").addEventListener("change", async (e) => {
  try {
    await invoke("set_names", { mode: e.target.value });
    setPending(await invoke("pending"));
    $("search").value = "";
    await runSearch("");
  } catch (err) {
    setStatus(`Error: ${errText(err)}`);
  }
});
function wireList(list) {
  const { vp } = list;
  vp.addEventListener("scroll", () => list.scrolled(), { passive: true });
  vp.addEventListener("click", async (e) => {
    const row = e.target.closest(".row");
    if (!row) return;
    const pos = Number(row.dataset.pos);
    await list.pick(pos, false);
    if (e.target.closest("[data-twist]")) await toggleFolder(list, pos);
  });
  vp.addEventListener("dblclick", (e) => {
    const row = e.target.closest(".row");
    if (row && list.row(Number(row.dataset.pos))?.folder) toggleFolder(list, Number(row.dataset.pos));
  });
  vp.addEventListener("keydown", async (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    const st = list.st;
    const page = Math.max(1, Math.floor(vp.clientHeight / ROW_H) - 1);
    const cur = st.selected ?? -1;
    const r = cur >= 0 ? list.row(cur) : null;
    const treeList = list === folderList || V.layout === "tree";
    const go = async (path) => {
      if (!path) return;
      const at = await invoke("locate", { view: list.view, entry: null, folder: path });
      if (at.line != null) await list.pick(at.line);
    };
    if (r && treeList) {
      // the keys of a tree: Right opens or enters, Left closes or goes up, Enter/Space toggle
      if (e.key === "ArrowRight") {
        e.preventDefault();
        if (r.folder && !r.expanded) return toggleFolder(list, cur);
        if (r.folder) return list.pick(cur + 1);
        return;
      }
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        if (r.folder && r.expanded) return toggleFolder(list, cur);
        return go(dirOf(r.name));
      }
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        if (list === folderList && e.key === "Enter") return $("viewport").focus();
        if (r.folder) return toggleFolder(list, cur);
        return;
      }
      if (e.key === "*") {
        e.preventDefault();
        return expandAll(true, list);
      }
    }
    const g = list.grid;
    const moves = g
      ? { ArrowRight: cur + 1, ArrowLeft: cur - 1, ArrowDown: cur + g.cols, ArrowUp: cur - g.cols, PageDown: cur + g.cols * Math.max(1, Math.floor(vp.clientHeight / g.h) - 1), PageUp: cur - g.cols * Math.max(1, Math.floor(vp.clientHeight / g.h) - 1), Home: 0, End: st.total - 1 }
      : { ArrowDown: cur + 1, ArrowUp: cur - 1, PageDown: cur + page, PageUp: cur - page, Home: 0, End: st.total - 1 };
    if (e.key in moves) {
      e.preventDefault();
      list.pick(cur < 0 && (e.key === "ArrowDown" || e.key === "PageDown" || e.key === "ArrowRight") ? 0 : Math.max(0, Math.min(st.total - 1, moves[e.key])));
    }
  });
}
wireList(mainList);
wireList(folderList);

for (const b of document.querySelectorAll("#view-seg button")) b.addEventListener("click", () => setLayout(b.dataset.layout));
for (const b of document.querySelectorAll("#preview-seg button")) b.addEventListener("click", () => setPreviewMode(b.dataset.preview));
for (const b of document.querySelectorAll("#colhead button")) b.addEventListener("click", () => setSort(b.dataset.sort, V.sort === b.dataset.sort ? !V.desc : false));
$("sort").addEventListener("change", (e) => setSort(e.target.value, false));
$("sort-dir").addEventListener("click", () => setSort(V.sort, !V.desc));
$("size-filter").addEventListener("change", (e) => {
  V.size = e.target.value;
  savePrefs();
  applyView({ autoExpand: true });
});
$("recursive").addEventListener("change", (e) => {
  V.recursive = e.target.checked;
  savePrefs();
  applyView({ keepSelection: false, pane: false });
});
$("folder-all").addEventListener("click", () => {
  V.folder = null;
  F.selected = null;
  folderList.draw();
  $("folder-all").classList.add("on");
  applyView({ keepSelection: false, pane: false });
});
$("btn-expand").addEventListener("click", () => expandAll(true));
$("btn-collapse").addEventListener("click", () => expandAll(false));
$("btn-reset-filters").addEventListener("click", resetFilters);
document.addEventListener("click", (e) => {
  const d = $("kind-filter");
  if (d.open && !d.contains(e.target)) d.open = false;
});

// drag the dividers between the panes; the sizes are remembered
function dragSplit(handle, onMove, onDone) {
  handle.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    handle.setPointerCapture(e.pointerId);
    handle.classList.add("drag");
    const move = (ev) => onMove(ev);
    const up = () => {
      handle.classList.remove("drag");
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      onDone();
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
  });
}
/** Dividers can be moved with the arrow keys too (Shift = bigger steps). */
function keySplit(handle, step) {
  handle.addEventListener("keydown", (e) => {
    const d = { ArrowLeft: -1, ArrowUp: -1, ArrowRight: 1, ArrowDown: 1 }[e.key];
    if (!d) return;
    e.preventDefault();
    step(d * (e.shiftKey ? 64 : 16));
  });
}
keySplit($("split-main"), (d) => {
  const main = document.querySelector("main").getBoundingClientRect();
  if (V.preview === "bottom") V.previewH = Math.round(Math.min(main.height - 120, Math.max(120, (V.previewH || main.height * 0.4) - d)));
  else V.listW = Math.round(Math.min(main.width - 240, Math.max(260, (V.listW || $("list-pane").getBoundingClientRect().width) + d)));
  applyPaneSizes();
  savePrefs();
  mainList.scrolled();
});
keySplit($("split-folders"), (d) => {
  const box = $("lists").getBoundingClientRect();
  V.foldersW = Math.round(Math.min(box.width - 260, Math.max(140, (V.foldersW || $("folder-pane").getBoundingClientRect().width) + d)));
  applyPaneSizes();
  savePrefs();
  folderList.scrolled();
  mainList.scrolled();
});
dragSplit(
  $("split-main"),
  (ev) => {
    const main = document.querySelector("main").getBoundingClientRect();
    if (V.preview === "bottom") V.previewH = Math.round(Math.min(main.height - 120, Math.max(120, main.bottom - ev.clientY)));
    else V.listW = Math.round(Math.min(main.width - 240, Math.max(260, ev.clientX - main.left)));
    applyPaneSizes();
  },
  () => {
    savePrefs();
    mainList.scrolled();
  },
);
dragSplit(
  $("split-folders"),
  (ev) => {
    const box = $("lists").getBoundingClientRect();
    V.foldersW = Math.round(Math.min(box.width - 260, Math.max(140, ev.clientX - box.left)));
    applyPaneSizes();
  },
  () => {
    savePrefs();
    folderList.scrolled();
    mainList.scrolled();
  },
);
window.addEventListener("resize", () => {
  mainList.scrolled();
  folderList.scrolled();
});
window.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
    e.preventDefault();
    if (S.info && !document.querySelector("dialog[open]")) e.shiftKey ? saveDialog("as") : doSave();
  }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "n" && !document.querySelector("dialog[open]")) {
    e.preventDefault();
    e.shiftKey ? invoke("open_in_window", { path: null }).catch(() => {}) : newArchive("grf");
  }
  if ((e.metaKey || e.ctrlKey) && e.key === ",") {
    e.preventDefault();
    openSettings();
  }
  if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && "1234".includes(e.key) && e.key.length === 1 && S.info) {
    e.preventDefault();
    setLayout(LAYOUTS[Number(e.key) - 1]);
  }
  if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "p") {
    e.preventDefault();
    setPreviewMode(V.preview === "off" ? "right" : "off");
  }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "o") {
    e.preventDefault();
    chooseArchive();
  }
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f" && !$("search").disabled) {
    e.preventDefault();
    $("search").focus();
    $("search").select();
  }
});

if (T.event?.listen) {
  T.event.listen("open-file", (e) => e.payload && openPath(e.payload));
  T.event.listen("job-progress", (e) => S.onJob?.(e.payload));
  T.event.listen("open-settings", () => openSettings());
  T.event.listen("menu-command", (e) => runMenuCommand(e.payload));
  T.event.listen("tauri://drag-drop", (e) => {
    const p = e.payload?.paths?.[0];
    if (p) openPath(p);
  });
}

// the native menu bar offers every toolbar command; a disabled button means the command does not apply now
const MENU_BUTTONS = { open: "btn-open", build: "btn-build", merge: "btn-merge", extract: "btn-extract", verify: "btn-verify", add: "btn-add", replace: "btn-replace", remove: "btn-remove", changes: "btn-changes" };
const NEEDS_ARCHIVE = new Set(["close-archive", "save", "save-as", "save-repack", "save-compact", "save-encrypt", "save-decrypt", "archive-settings", "hash", "compare", "reduce-images"]);
const MENU_ACTIONS = {
  "new-window": () => invoke("open_in_window", { path: null }),
  "new-grf": () => newArchive("grf"),
  "new-gpf": () => newArchive("gpf"),
  "new-rgz": () => newArchive("rgz"),
  "new-thor": () => newArchive("thor"),
  "close-archive": closeArchive,
  save: doSave,
  "save-as": () => saveDialog("as"),
  "save-repack": () => saveDialog("repack"),
  "save-compact": () => saveDialog("compact"),
  "save-encrypt": () => saveDialog("encrypt"),
  "save-decrypt": () => saveDialog("decrypt"),
  "archive-settings": () => saveDialog("settings"),
  about: openAbout,
  hash: doHashes,
  compare: doCompare,
  "data-folder": () => invoke("open_data_folder"),
  "recent-clear": clearRecent,
  "reduce-images": () => doReduceImages(),
};
function runMenuCommand(id) {
  if (document.querySelector("dialog[open]")) return;
  if (id.startsWith("layout-")) return S.info && setLayout(id.slice(7));
  if (id.startsWith("preview-")) return setPreviewMode(id.slice(8));
  const recent = /^recent-(\d+)$/.exec(id);
  if (recent) {
    const path = recentList()[Number(recent[1])];
    return path && openPath(path);
  }
  if (MENU_ACTIONS[id]) {
    if (NEEDS_ARCHIVE.has(id) && !S.info) return;
    return Promise.resolve(MENU_ACTIONS[id]()).catch((e) => note(t("open-failed"), errText(e), true));
  }
  const b = MENU_BUTTONS[id] && $(MENU_BUTTONS[id]);
  if (b && !b.disabled) b.click();
}

// buttons whose words are hidden in narrow windows keep their name for screen readers and tooltips
for (const b of document.querySelectorAll("button")) {
  const t = b.querySelector("span:not(.sr)")?.textContent?.trim();
  if (t && !b.hasAttribute("aria-label")) b.setAttribute("aria-label", t.replace(/…$/, ""));
  if (t && !b.title) b.title = t;
}
I18N.load(I18N.resolve(SET.language));
I18N.apply();
syncMenu();
if (SET.layout === "list") SET.layout = "details";
if (SET.layout !== "last") V.layout = SET.layout;
if (SET.preview !== "last") V.preview = SET.preview;
applySettings();
clearPreview();
syncViewBar();
buildKindFilter();
// the window is created hidden in the page's background colour: show it once the page is drawn
requestAnimationFrame(() => setTimeout(() => invoke("window_ready").catch(() => {}), 0));

invoke("startup_path")
  .then(async (p) => {
    if (!p && SET.reopen && SET.lastArchive) return openPath(SET.lastArchive);
    if (!p) return;
    await openPath(p);
    const q = await invoke("startup_select");
    if (q && S.info) {
      $("search").value = q;
      await runSearch(q);
      if (S.total) select(0);
    }
  })
  .catch(() => {});
