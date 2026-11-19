"use strict";

const T = window.__TAURI__;
const invoke = (cmd, args) => T.core.invoke(cmd, args);
const ROW_H = 24;
const OVERSCAN = 6;
const $ = (id) => document.getElementById(id);

const ICONS = { image: "▨", sprite: "◆", animation: "▶", palette: "◐", map: "▦", model: "◇", audio: "♪", text: "≡", effect: "✦" };

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
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") n.className = v;
    else if (k === "text") n.textContent = v;
    else if (k.startsWith("on")) n.addEventListener(k.slice(2), v);
    else if (v !== false && v != null) n.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat()) if (c != null) n.append(c);
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
const errText = (e) => (typeof e === "string" ? e : e?.message || JSON.stringify(e));

// ---------- dialogs ----------

function ask(title, body, buttons) {
  return new Promise((resolve) => {
    const dlg = $("dlg");
    $("dlg-title").textContent = title;
    const b = $("dlg-body");
    b.replaceChildren(...(Array.isArray(body) ? body : [body]).filter(Boolean));
    const actions = $("dlg-actions");
    actions.replaceChildren(
      ...buttons.map((x) =>
        el("button", {
          type: "button",
          class: x.primary ? "primary" : "",
          text: x.label,
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
  ask(title, el("p", { class: isError ? "error" : "", text }), [{ label: "OK", value: true, primary: true }]);

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
      problem = /CRC32/.test(errText(e)) ? "This password does not belong to the archive." : "";
    }
    if (info && !problem && password !== null) problem = "This password does not belong to the archive.";
    const input = el("input", { type: "password", class: "field", autocomplete: "off", spellcheck: "false" });
    const answer = await ask(
      "Encrypted archive",
      [
        el("p", { text: "This archive is encrypted (GRF Editor's scheme). Enter its password to read the files." }),
        problem ? el("p", { class: "error", text: problem }) : null,
        input,
      ],
      [
        { label: "Open without password", value: "skip" },
        { label: "Open", value: "ok", primary: true },
      ],
    );
    if (answer === "skip" || answer === null) {
      if (info) return info;
      throw new Error("The file table of this archive is encrypted; a password is needed to open it.");
    }
    password = input.value;
  }
}

async function openPath(path, force = false) {
  if (!force && S.changes.length) {
    const go = await ask("Open another archive?", el("p", { text: `${fmtNum(S.changes.length)} pending change(s) in the open archive will be lost.` }), [{ label: "Open anyway", value: true }, { label: "Cancel", value: null, primary: true }]);
    if (!go) return;
  }
  setStatus(`Opening ${path} …`);
  try {
    S.info = await openWithKey(path);
  } catch (e) {
    setStatus("Could not open the archive");
    await note("Could not open", errText(e), true);
    return;
  }
  document.title = `${S.info.file_name} - RustyGRF`;
  $("empty").hidden = true;
  $("lists").hidden = false;
  for (const id of ["search", "btn-extract", "btn-verify", "names", "btn-add", "btn-changes", "sort", "sort-dir", "size-filter"]) $(id).disabled = false;
  V.folder = null;
  buildKindFilter();
  syncViewBar();
  S.changes = [];
  S.changed = new Map();
  updateEditBar();
  $("search").value = "";
  $("names").value = "cp949";
  S.query = "";
  await runSearch("");
  setStatus(
    `${S.info.file_name}: ${S.info.format}, ${fmtNum(S.info.files)} files, ${fmtBytes(S.info.file_size)}`,
  );
  $("viewport").focus();
}

// ---------- lists ----------

const KINDS = [["image", "Images"], ["sprite", "Sprites"], ["animation", "Animations"], ["palette", "Palettes"], ["map", "Maps"], ["model", "Models"], ["audio", "Audio"], ["text", "Text and Lua"], ["effect", "Effects"], ["file", "Other"]];
const SIZE_UNITS = { k: 1024, m: 1024 * 1024 };
const PREFS_KEY = "rustygrf-view";

const V = Object.assign(
  { layout: "list", sort: "archive", desc: false, kinds: [], size: "", recursive: false, preview: "right", folder: null, listW: null, previewH: null, foldersW: null },
  (() => {
    try {
      return JSON.parse(localStorage.getItem(PREFS_KEY) || "{}");
    } catch {
      return {};
    }
  })(),
);
V.folder = null; // the chosen folder is per archive
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
  range() {
    const first = Math.max(0, Math.floor(this.vp.scrollTop / ROW_H) - OVERSCAN);
    const last = Math.min(this.st.total, Math.ceil((this.vp.scrollTop + this.vp.clientHeight) / ROW_H) + OVERSCAN);
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
    this.spacer.style.height = `${total * ROW_H}px`;
    if (!keepScroll) this.vp.scrollTop = 0;
    await this.load();
  }
  scrollTo(pos) {
    const top = pos * ROW_H;
    const vp = this.vp;
    if (top < vp.scrollTop) vp.scrollTop = top;
    else if (top + ROW_H > vp.scrollTop + vp.clientHeight) vp.scrollTop = top + ROW_H - vp.clientHeight;
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
  const sel = pos === S.selected ? " selected" : "";
  const tree = V.layout === "tree";
  const indent = tree ? `padding-left:${8 + r.depth * 16}px;` : "";
  if (r.folder) {
    return el(
      "div",
      { class: `row tree folder${sel}`, style: `top:${pos * ROW_H}px;${indent}`, title: r.name, "data-pos": pos },
      el("span", { class: "icon twisty", "data-twist": "1", text: r.expanded ? "▾" : "▸" }),
      el("span", { class: "name" }, baseOf(r.name), el("span", { class: "count", text: fmtNum(r.count) })),
      el("span", { class: "size", text: fmtBytes(r.bytes) }),
    );
  }
  const key = changeKey(r.name);
  const state = S.changed.has(key) ? ` changed ${S.changed.get(key)}` : "";
  const base = baseOf(r.name);
  const dir = dirOf(r.name);
  const cls = `row${sel}${state}${V.layout === "details" || V.layout === "folders" ? " details" : ""}${tree ? " tree" : ""}`;
  const icon = el("span", { class: "icon", text: ICONS[r.kind] || "·" });
  if (V.layout === "details" || V.layout === "folders") {
    return el(
      "div",
      { class: cls, style: `top:${pos * ROW_H}px`, title: r.name, "data-pos": pos },
      icon,
      el("span", { class: "name" }, base, V.layout === "details" && dir ? el("span", { class: "muted", text: `  ${dir}` }) : null),
      el("span", { class: "ext", text: extOf(r.name) }),
      el("span", { class: "size", text: fmtBytes(r.size) }),
      el("span", { class: "stored", text: fmtBytes(r.stored) }),
    );
  }
  return el(
    "div",
    { class: cls, style: `top:${pos * ROW_H}px;${indent}`, title: r.name, "data-pos": pos },
    icon,
    el("span", { class: "name" }, base, !tree && dir ? el("span", { class: "muted", text: `  ${dir}` }) : null),
    el("span", { class: "size", text: fmtBytes(r.size) }),
  );
}

function folderRow(r, pos) {
  const on = V.folder != null && r.name.toLowerCase() === V.folder.toLowerCase();
  return el(
    "div",
    { class: `row tree folder${on ? " selected" : ""}`, style: `top:${pos * ROW_H}px;padding-left:${8 + r.depth * 16}px`, title: r.name, "data-pos": pos },
    el("span", { class: "icon twisty", "data-twist": "1", text: r.expanded ? "▾" : "▸" }),
    el("span", { class: "name" }, baseOf(r.name), el("span", { class: "count", text: fmtNum(r.count) })),
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
    kinds: V.kinds,
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
  $("count").textContent = filtersActive() ? `${fmtNum(S.matches)} of ${fmtNum(S.info.entries)}` : `${fmtNum(S.matches)} entries`;
  $("btn-reset-filters").disabled = !filtersActive();
  const note = V.layout === "folders" && S.query ? " (search covers all folders)" : "";
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
  list.spacer.style.height = `${total * ROW_H}px`;
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
  list.spacer.style.height = `${total * ROW_H}px`;
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
  $("colhead").hidden = !(V.layout === "details" || folders);
  $("recursive-wrap").hidden = !folders;
  $("recursive").checked = V.recursive;
  $("btn-expand").hidden = !(V.layout === "tree" || folders);
  $("btn-collapse").hidden = !(V.layout === "tree" || folders);
  $("sort").value = V.sort;
  $("sort-dir").textContent = V.desc ? "↓" : "↑";
  $("size-filter").value = V.size;
  for (const b of document.querySelectorAll("#colhead button")) b.classList.toggle("on", b.dataset.sort === V.sort);
  const kinds = $("kind-filter").querySelector("summary");
  kinds.textContent = V.kinds.length ? `Type: ${V.kinds.length === 1 ? (KINDS.find((k) => k[0] === V.kinds[0]) || [0, V.kinds[0]])[1] : `${V.kinds.length} kinds`}` : "Type: all";
  kinds.classList.toggle("active", V.kinds.length > 0);
  $("viewport").setAttribute("role", V.layout === "tree" ? "tree" : "listbox");
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
    ...KINDS.map(([k, label]) => el("label", {}, el("input", { type: "checkbox", value: k, checked: V.kinds.includes(k), onchange: onKindChange }), label)),
    el("div", { class: "links" }, el("a", { text: "All", onclick: () => setKinds([]) }), el("a", { text: "Only images", onclick: () => setKinds(["image"]) })),
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
  $("preview-head").replaceChildren(el("div", { class: "title", text: r.name }), el("div", { class: "meta", text: `Folder — ${fmtNum(r.count)} file(s), ${fmtBytes(r.bytes)} unpacked` }));
  $("preview-body").replaceChildren(
    el("div", { class: "controls" }, el("button", { type: "button", class: "small", text: r.expanded ? "Close" : "Open", onclick: () => toggleFolder(mainList, S.selected) })),
    el("p", { class: "placeholder", text: "Extract… and Remove act on this folder when it is selected." }),
  );
}

// ---------- preview ----------

function clearPreview() {
  $("preview-head").replaceChildren();
  $("preview-body").replaceChildren(el("p", { class: "placeholder", text: S.info ? "Select an entry to preview it." : "" }));
}

async function showPreview(row, action) {
  const token = ++S.previewToken;
  S.viewer?.destroy();
  S.viewer = null;
  const head = $("preview-head");
  const body = $("preview-body");
  head.replaceChildren(el("div", { class: "title", text: row.name }), el("div", { class: "meta", text: "Loading …" }));
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
  meta.textContent = `${fmtBytes(details.size)} unpacked, ${fmtBytes(details.stored)} stored (${details.codec}), offset ${fmtNum(details.offset)}`;
  const staged = S.changed.get(changeKey(details.name));
  if (staged) meta.textContent += ` — staged: ${staged === "remove" ? "removed" : staged === "add" ? "added" : "changed"}, shown as it will be saved`;
  head.replaceChildren(el("div", { class: "title", text: details.name }), meta);
  body.replaceChildren(...renderPreview(preview, row, meta).filter(Boolean));
}

const dataUrl = (mime, data) => `data:${mime};base64,${data}`;
const infoBlock = (lines) => (lines?.length ? el("div", { class: "info" }, el("pre", { text: lines.join("\n") })) : null);

function renderPreview(p, row, meta) {
  switch (p.kind) {
    case "image": {
      const img = el("img", { src: dataUrl(p.mime, p.data), alt: row.name });
      img.addEventListener("load", () => {
        meta.textContent += `, ${img.naturalWidth} x ${img.naturalHeight} px`;
        if (img.naturalWidth <= 128) img.style.width = `${img.naturalWidth * 4}px`;
      });
      return [el("div", { class: "checker" }, img)];
    }
    case "audio":
      return [el("audio", { controls: true, src: dataUrl(p.mime, p.data) })];
    case "text": {
      const pre = el("pre", { text: p.text });
      const note = el("div", { class: "muted" }, "Encoding: ", el("span", { class: "badge", text: p.encoding }), p.truncated ? " (shortened)" : "");
      const edit = el("button", { type: "button", class: "small", text: "Edit…", title: "Edit the text; compiled Lua is compiled again (staged, saved with Changes…)", onclick: () => openTextEditor(row) });
      return [el("div", { class: "controls" }, edit), note, pre];
    }
    case "frames":
      return [
        row.name.toLowerCase().endsWith(".pal")
          ? el(
              "div",
              { class: "controls" },
              el("button", { type: "button", class: "small", text: "Adjust…", title: "Shift hue, scale saturation and brightness of a range of colours (staged)", onclick: () => adjustPalette(row) }),
              el("span", { class: "muted", text: "Click a colour to change it." }),
            )
          : null,
        row.name.toLowerCase().endsWith(".spr")
          ? el(
              "div",
              { class: "controls" },
              el("button", { type: "button", class: "small", text: "Add image…", title: "Append a PNG as a new image (staged)", onclick: () => addImage(row) }),
              el("button", { type: "button", class: "small", text: "Set palette…", title: "Use the colours of a .pal file (staged)", onclick: () => setPalette(row) }),
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
              el("figcaption", { text: f.label }),
              row.name.toLowerCase().endsWith(".spr")
                ? el(
                    "div",
                    { class: "figactions" },
                    el("button", { type: "button", class: "small", text: "Replace…", title: "Replace this image with a PNG (staged, saved with Changes…)", onclick: () => replaceImage(row, i) }),
                    el("button", { type: "button", class: "small", text: "Remove", title: "Remove this image; the animation next to it follows (staged)", onclick: () => removeImage(row, i) }),
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
      for (let i = 0; i < p.actions; i++) sel.append(el("option", { value: i, text: `Action ${i}`, selected: i === p.action }));
      const controls = el(
        "div",
        { class: "controls" },
        el("label", { text: "Action " }, sel),
        el("span", { class: "muted", text: `${p.frames} frame(s), ${p.interval_ms} ms each` }),
        el("button", { type: "button", class: "small", text: "Edit frames…", title: "Edit layers, anchors and frames of this animation (staged)", onclick: () => openActEditor(row, p.action) }),
        el("button", { type: "button", class: "small", text: "Set interval…", title: "Change the frame time of this action (staged, saved with Changes…)", onclick: () => setInterval_(row, p.action, p.interval_ms) }),
      );
      return [controls, el("div", { class: "checker" }, el("img", { src: dataUrl("image/gif", p.gif), width: p.width, height: p.height, alt: "animation" })), infoBlock(p.info), p.note ? el("div", { class: "info", text: `warning: ${p.note}` }) : null];
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
        el("div", { class: "controls" }, el("span", { class: "muted", text: `${fmtNum(p.triangles)} triangles — drag to turn, wheel to zoom, double-click to reset` }), el("label", {}, lit, " Lighting"), el("label", {}, wire, " Wireframe"), p.duration_ms > 0 ? el("label", {}, animate, " Animate") : null),
        host,
        infoBlock(p.info),
      ];
    }
    case "scene": {
      const host = el("div", { class: "glhost tall" });
      const handle = { v: null };
      queueMicrotask(() => (S.viewer = handle.v = Viewer3D.showScene(host, p)));
      const toggle = (label, name, on = true) => {
        const c = el("input", { type: "checkbox", checked: on, onchange: () => handle.v?.set(name, c.checked) });
        return el("label", {}, c, ` ${label}`);
      };
      return [
        el("div", { class: "controls" }, el("span", { class: "muted", text: "drag = turn, right/shift-drag or arrows = move, wheel = zoom, double-click = reset" }), toggle("Lighting", "lit"), toggle("Lightmaps", "lightmap"), toggle("Models", "models"), toggle("Water", "water"), p.water?.frames?.length > 1 || p.models.some((m) => m.anim) ? toggle("Animate", "animate") : null),
        host,
        infoBlock(p.info),
      ];
    }
    case "effect":
      return [el("div", { class: "muted", text: `${p.frames} frame(s) shown at ${p.fps} fps` }), el("div", { class: "fxstage" }, el("img", { src: dataUrl("image/gif", p.gif), width: p.width, height: p.height, alt: "effect" })), infoBlock(p.info)];
    case "map":
      return [el("div", { class: "checker" }, el("img", { src: dataUrl("image/png", p.png), width: p.width, height: p.height, alt: "map" })), infoBlock(p.info)];
    case "info":
      return [el("pre", { text: p.lines.join("\n") })];
    case "hex":
      return [el("div", { class: "muted", text: `${fmtNum(p.size)} bytes, first 512 shown` }), el("pre", { text: p.dump })];
    default:
      return [el("p", { class: "placeholder", text: p.text || "" })];
  }
}

// ---------- actions ----------

/** The entries a selected line stands for: the file itself, or everything below a folder. */
async function entriesOf(row) {
  return row.folder ? invoke("folder_entries", { view: 0, path: row.name }) : [row.index];
}

async function doExtract() {
  if (!S.info) return;
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  const buttons = [];
  if (sel) buttons.push({ label: sel.folder ? `Folder (${fmtNum(sel.count)} files)` : "Selected entry", value: "one", primary: true });
  buttons.push({ label: `All ${fmtNum(S.matches)} in the list`, value: "all", primary: !sel });
  buttons.push({ label: "Cancel", value: null });
  const what = await ask("Extract", el("p", { text: filtersActive() ? "The list is filtered; \"all\" means the entries that match." : "The list shows the whole archive." }), buttons);
  if (!what) return;
  const dest = await T.dialog.open({ directory: true, multiple: false, title: "Extract into folder" });
  if (!dest) return;
  const indices = what === "one" ? await entriesOf(sel) : [];
  if (what === "one" && !indices.length) return note("Extract", "This folder holds no files.");
  setStatus("Extracting …");
  let rename = false;
  for (;;) {
    try {
      const r = await invoke("extract", { indices, dest, rename });
      setStatus(`Extracted ${fmtNum(r.files)} file(s) to ${r.dest}${r.damaged ? ` (${r.damaged} damaged)` : ""}`);
      await note("Extracted", `${fmtNum(r.files)} file(s) written to ${r.dest}.${r.damaged ? ` ${r.damaged} entries were damaged; their recoverable part was written.` : ""}`);
      return;
    } catch (e) {
      setStatus("Extraction stopped");
      const again = await ask("Could not extract", el("p", { class: "error", text: errText(e) }), [
        { label: "Rename new files", value: true },
        { label: "Cancel", value: false, primary: true },
      ]);
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
  $("btn-changes").textContent = `Changes (${S.changes.length})…`;
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

function setPending(rows) {
  S.changes = rows;
  S.changed = new Map(rows.map((r) => [changeKey(r.name), r.kind]));
  updateEditBar();
  render();
}

async function doHistory(redo) {
  try {
    setPending(await invoke(redo ? "redo_change" : "undo_change"));
    setStatus(redo ? "Change redone" : "Change undone");
    if ($("act-dlg").open) await loadActFrame();
    refreshPreview($("act-dlg").open ? E.action : undefined);
  } catch (e) {
    await note("Could not change the history", errText(e), true);
  }
}

/** Shows the selected entry again, so it reflects the staged changes. */
function refreshPreview(action) {
  const r = S.selected != null ? S.rows[S.selected - S.offset] : null;
  if (r && !r.folder && V.preview !== "off") showPreview(r, action);
}

async function replaceImage(row, frame) {
  const png = await T.dialog.open({ multiple: false, directory: false, title: `PNG for image ${frame} of ${row.name}`, filters: [{ name: "PNG image", extensions: ["png"] }] });
  if (!png) return;
  try {
    setPending(await invoke("stage_spr_image", { index: row.index, frame, png }));
    setStatus(`Image ${frame} of ${row.name} staged (colours are mapped to the sprite's palette); the preview shows the result. Save with Changes….`);
    refreshPreview();
  } catch (e) {
    await note("Could not replace the image", errText(e), true);
  }
}

async function addImage(row) {
  const png = await T.dialog.open({ multiple: false, directory: false, title: `PNG to add to ${row.name}`, filters: [{ name: "PNG image", extensions: ["png"] }] });
  if (!png) return;
  try {
    setPending(await invoke("stage_spr_add", { index: row.index, png }));
    setStatus(`Image added to ${row.name} (staged); save with Changes….`);
    refreshPreview();
  } catch (e) {
    await note("Could not add the image", errText(e), true);
  }
}

async function removeImage(row, frame) {
  const go = await ask(`Remove image ${frame}?`, el("p", { text: "Later images move down by one. Layers of the animation next to this sprite that use the image become empty, and later ones follow." }), [{ label: "Remove", value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  try {
    setPending(await invoke("stage_spr_remove", { index: row.index, frame }));
    setStatus(`Image ${frame} of ${row.name} removed (staged); save with Changes….`);
    refreshPreview();
  } catch (e) {
    await note("Could not remove the image", errText(e), true);
  }
}

// ---------- palette editing ----------

function palImage(row, f) {
  const img = el("img", { src: dataUrl("image/png", f.png), alt: f.label });
  if (row.name.toLowerCase().endsWith(".pal")) {
    img.style.cursor = "crosshair";
    img.style.width = "256px";
    img.style.maxWidth = "none";
    img.title = "Click a colour to change it";
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
    await note("Could not read the palette", errText(e), true);
    return;
  }
  const input = el("input", { type: "color", value: `#${current}` });
  const go = await ask(`Colour ${index}`, [el("p", { text: index === 0 ? "Index 0 is the transparent colour in the client; its colour is rarely seen." : `Colour number ${index} of 255:` }), input], [{ label: "Set", value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  const h = input.value.slice(1);
  await stagePal(row, { op: "set", index, rgb: [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)] }, `Colour ${index} of ${row.name} changed (staged); save with Changes….`);
}

async function adjustPalette(row) {
  const f = (label, value, step) => el("label", { class: "muted", text: `${label} ` }, el("input", { type: "number", value: String(value), step: String(step), class: "narrow" }));
  const [from, to, hue, sat, bri] = [f("From colour", 1, 1), f("to", 255, 1), f("Hue shift (°)", 0, 5), f("Saturation ×", 1, 0.05), f("Brightness ×", 1, 0.05)];
  const v = (l) => Number(l.querySelector("input").value);
  const go = await ask(`Adjust colours of ${row.name}`, [el("p", { text: "Colour 0 is the transparent one, so the range starts at 1." }), el("div", { class: "bar" }, from, to), el("div", { class: "bar" }, hue, sat, bri)], [{ label: "Apply", value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  await stagePal(row, { op: "adjust", from: v(from), to: v(to), hue: v(hue), saturation: v(sat), brightness: v(bri) }, `Colours of ${row.name} adjusted (staged); save with Changes….`);
}

async function stagePal(row, edit, message) {
  try {
    setPending(await invoke("stage_pal_edit", { index: row.index, edit }));
    setStatus(message);
    refreshPreview();
  } catch (e) {
    await note("Could not change the palette", errText(e), true);
  }
}

async function setPalette(row) {
  const pal = await T.dialog.open({ multiple: false, directory: false, title: `Palette for ${row.name}`, filters: [{ name: "Palette", extensions: ["pal"] }] });
  if (!pal) return;
  try {
    setPending(await invoke("stage_spr_palette", { index: row.index, pal }));
    setStatus(`Palette of ${row.name} changed (staged); save with Changes….`);
    refreshPreview();
  } catch (e) {
    await note("Could not set the palette", errText(e), true);
  }
}

// ---------- animation editor ----------

const E = { row: null, action: 0, frame: 0, view: null, busy: false };

async function openActEditor(row, action) {
  Object.assign(E, { row, action, frame: 0, view: null });
  $("act-title").textContent = `Edit animation: ${row.name}`;
  $("act-body").replaceChildren(el("p", { class: "muted", text: "Loading …" }));
  $("act-dlg").showModal();
  await loadActFrame();
}

$("act-dlg").addEventListener("close", () => {
  if (E.row) refreshPreview(E.action);
  E.row = null;
});

async function openTextEditor(row) {
  $("text-title").textContent = `Edit ${row.name}`;
  $("text-body").replaceChildren(el("p", { class: "muted", text: "Loading …" }));
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
  if (!doc.complete) msg.textContent = "Parts of this file could not be decompiled and stay as a listing; they cannot be compiled again. Edit only what the text shows as Lua.";
  const save = el("button", { type: "button", class: "primary", text: lua ? "Compile and stage" : "Stage" });
  save.addEventListener("click", async () => {
    save.disabled = true;
    msg.className = "muted";
    msg.textContent = lua ? "Compiling …" : "Staging …";
    try {
      setPending(await invoke("stage_text_edit", { index: row.index, text: area.value }));
      setStatus(`${row.name} staged${lua ? " (compiled again)" : ""}. Save with Changes….`);
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
    E.view = await invoke("act_frame", { index: E.row.index, action: E.action, frame: E.frame });
    E.action = E.view.action;
    E.frame = E.view.frame;
    drawActEditor();
  } catch (e) {
    $("act-body").replaceChildren(el("p", { class: "error", text: errText(e) }));
  }
}

async function actEdit(edit) {
  if (E.busy) return;
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

async function removeAction(i) {
  const go = await ask(`Remove action ${i}?`, el("p", { text: "Later actions move down by one. The client picks actions by number (direction, state), so other files that use them may change meaning." }), [{ label: "Remove", value: true, primary: true }, { label: "Cancel", value: null }]);
  if (go) await actEdit({ op: "remove_action" });
}

const hex2 = (n) => n.toString(16).padStart(2, "0");

function soundField(v) {
  const list = el("datalist", { id: "act-sounds" }, v.sounds.map((s) => el("option", { value: s })));
  const input = el("input", { type: "text", style: "width:13em", list: "act-sounds", value: v.sound ?? "", placeholder: "none", title: "Sound file played with this frame, e.g. effect\\hit.wav (empty = none)", size: "22" });
  input.addEventListener("change", () => actEdit({ op: "set_sound", name: input.value.trim() === "" ? null : input.value.trim() }));
  return el("label", { class: "muted", text: "Sound " }, input, list);
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
  for (let i = 0; i < v.actions; i++) actionSel.append(el("option", { value: i, text: `Action ${i}`, selected: i === v.action }));
  const bar = el(
    "div",
    { class: "bar" },
    el("label", { text: "Action " }, actionSel),
    el("button", { type: "button", class: "small", text: "◀", disabled: v.frame === 0, onclick: () => { E.frame -= 1; loadActFrame(); } }),
    el("span", { text: `Frame ${v.frame + 1} / ${v.frames}` }),
    el("button", { type: "button", class: "small", text: "▶", disabled: v.frame >= v.frames - 1, onclick: () => { E.frame += 1; loadActFrame(); } }),
    el("label", { class: "muted", text: "ms/frame " }, num(v.interval_ms, (n) => actEdit({ op: "set_interval", ms: n }), { narrow: true })),
    el("span", { class: "muted", text: `ACT ${v.version}` }),
    v.sounds_supported ? soundField(v) : null,
    el("button", { type: "button", class: "small", id: "act-undo", text: "Undo", onclick: () => doHistory(false) }),
    el("button", { type: "button", class: "small", id: "act-redo", text: "Redo", onclick: () => doHistory(true) }),
    el("button", { type: "button", class: "small", text: "Copy frame", title: "Insert a copy after this frame", onclick: () => actEdit({ op: "copy_frame" }) }),
    el("button", { type: "button", class: "small", text: "Frame ←", title: "Move the frame one place earlier", disabled: v.frame === 0, onclick: () => actEdit({ op: "move_frame", to: v.frame - 1 }) }),
    el("button", { type: "button", class: "small", text: "Frame →", title: "Move the frame one place later", disabled: v.frame >= v.frames - 1, onclick: () => actEdit({ op: "move_frame", to: v.frame + 1 }) }),
    el("button", { type: "button", class: "small", text: "Copy action", title: "Insert a copy of this action after it", onclick: () => actEdit({ op: "copy_action" }) }),
    el("button", { type: "button", class: "small", text: "Action ↑", title: "Move the action one place earlier", disabled: v.action === 0, onclick: () => actEdit({ op: "move_action", to: v.action - 1 }) }),
    el("button", { type: "button", class: "small", text: "Action ↓", title: "Move the action one place later", disabled: v.action >= v.actions - 1, onclick: () => actEdit({ op: "move_action", to: v.action + 1 }) }),
    el("button", { type: "button", class: "small", text: "Remove action", disabled: v.actions <= 1, onclick: () => removeAction(v.action) }),
    el("button", { type: "button", class: "small", text: "Remove frame", disabled: v.frames <= 1, onclick: () => actEdit({ op: "remove_frame" }) }),
  );
  const stage = el("div", { class: "stage checker" });
  if (v.png) {
    stage.append(el("img", { src: dataUrl("image/png", v.png), width: v.width, height: v.height, alt: "frame" }), el("div", { class: "cross-h", style: `top:${v.origin_y}px` }), el("div", { class: "cross-v", style: `left:${v.origin_x}px` }));
    v.anchors.forEach((a, i) => stage.append(el("div", { class: "dot", title: `anchor ${i}`, style: `left:${v.origin_x + a.x * v.scale}px;top:${v.origin_y + a.y * v.scale}px` })));
  }
  const left = el("div", {}, stage, v.error ? el("p", { class: "muted", text: `No picture: ${v.error}` }) : null, el("p", { class: "muted", text: "The lines mark the origin; red dots are anchors. Layers are drawn in order, the last one on top." }));
  const head = ["#", "Type", "Image", "x", "y", "Mirror", v.separate_scale ? "Scale x / y" : "Scale", "Rot", "Colour", "α", ""];
  const rows = v.layers.map((l, i) => {
    const typeSel = el("select", { onchange: () => setL(i, { sprite_type: Number(typeSel.value) }) }, el("option", { value: 0, text: `palette (${v.indexed_images})`, selected: l.sprite_type === 0 }), el("option", { value: 1, text: `true colour (${v.rgba_images})`, selected: l.sprite_type === 1 }));
    const mirror = el("input", { type: "checkbox", checked: l.mirror, onchange: () => setL(i, { mirror: mirror.checked }) });
    const colour = el("input", { type: "color", value: `#${hex2(l.color[0])}${hex2(l.color[1])}${hex2(l.color[2])}` });
    colour.addEventListener("change", () => setL(i, { color: [parseInt(colour.value.slice(1, 3), 16), parseInt(colour.value.slice(3, 5), 16), parseInt(colour.value.slice(5, 7), 16), l.color[3]] }));
    return el(
      "tr",
      {},
      el("td", { text: String(i) }),
      el("td", {}, typeSel),
      el("td", {}, num(l.sprite_index, (n) => setL(i, { sprite_index: n }), { narrow: true })),
      el("td", {}, num(l.offset_x, (n) => setL(i, { offset_x: n }), { narrow: true })),
      el("td", {}, num(l.offset_y, (n) => setL(i, { offset_y: n }), { narrow: true })),
      el("td", {}, mirror),
      el("td", {}, num(l.scale_x, (n) => setL(i, { scale_x: n }), { step: "0.1", narrow: true }), v.separate_scale ? num(l.scale_y, (n) => setL(i, { scale_y: n }), { step: "0.1", narrow: true }) : null),
      el("td", {}, num(l.rotation, (n) => setL(i, { rotation: Math.round(n) }), { narrow: true })),
      el("td", {}, colour),
      el("td", {}, num(l.color[3], (n) => setL(i, { color: [l.color[0], l.color[1], l.color[2], Math.max(0, Math.min(255, Math.round(n)))] }), { narrow: true })),
      el("td", {}, el("button", { type: "button", class: "small", text: "↑", title: "Draw earlier (below)", disabled: i === 0, onclick: () => actEdit({ op: "move_layer", layer: i, to: i - 1 }) }), el("button", { type: "button", class: "small", text: "↓", title: "Draw later (above)", disabled: i === v.layers.length - 1, onclick: () => actEdit({ op: "move_layer", layer: i, to: i + 1 }) }), el("button", { type: "button", class: "small", text: "✕", title: "Remove the layer", onclick: () => actEdit({ op: "remove_layer", layer: i }) })),
    );
  });
  const layers = el("div", {}, el("div", { class: "tscroll" }, el("table", {}, el("thead", {}, el("tr", {}, head.map((t) => el("th", { text: t })))), el("tbody", {}, rows))), el("div", { class: "bar" }, el("button", { type: "button", class: "small", text: "+ Layer", onclick: () => actEdit({ op: "add_layer" }) })));
  const anchors = v.anchors_supported
    ? el(
        "div",
        {},
        el("h3", { text: "Anchors (where other sprites attach, e.g. headgear)" }),
        el("table", {}, el("tbody", {}, v.anchors.map((a, i) => el("tr", {}, el("td", { text: `#${i}` }), el("td", {}, num(a.x, (n) => actEdit({ op: "set_anchor", anchor: i, x: Math.round(n) }), { narrow: true })), el("td", {}, num(a.y, (n) => actEdit({ op: "set_anchor", anchor: i, y: Math.round(n) }), { narrow: true })), el("td", {}, el("button", { type: "button", class: "small", text: "✕", onclick: () => actEdit({ op: "remove_anchor", anchor: i }) })))))),
        el("button", { type: "button", class: "small", text: "+ Anchor", onclick: () => actEdit({ op: "add_anchor" }) }),
      )
    : el("p", { class: "muted", text: `ACT ${v.version} has no anchors.` });
  $("act-body").replaceChildren(el("div", { class: "acted" }, bar, left, el("div", {}, layers, anchors)));
  updateEditBar();
}

async function setInterval_(row, action, current) {
  const input = el("input", { type: "number", class: "field", min: "1", step: "1", value: String(current) });
  const go = await ask(`Interval of action ${action}`, [el("p", { text: "Time per frame in milliseconds (the file stores multiples of 25 ms):" }), input], [{ label: "Set", value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  try {
    setPending(await invoke("stage_act_interval", { index: row.index, action, ms: Number(input.value) }));
    setStatus(`Interval of action ${action} staged; save with Changes….`);
    refreshPreview(action);
  } catch (e) {
    await note("Could not set the interval", errText(e), true);
  }
}

async function doReplace() {
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  if (!sel) return;
  const file = await T.dialog.open({ multiple: false, directory: false, title: `File that replaces ${sel.name}` });
  if (!file) return;
  try {
    setPending(await invoke("stage_replace", { index: sel.index, file }));
    setStatus(`Replacement staged for ${sel.name}; save it with Changes…`);
    refreshPreview();
  } catch (e) {
    await note("Could not stage the replacement", errText(e), true);
  }
}

async function doAdd() {
  const file = await T.dialog.open({ multiple: false, directory: false, title: "File to add" });
  if (!file) return;
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  const folder = sel ? (sel.folder ? `${sel.name}\\` : sel.name.slice(0, sel.name.lastIndexOf("\\") + 1)) : "data\\";
  const input = el("input", { type: "text", class: "field", value: folder + file.split(/[\\/]/).pop(), spellcheck: "false" });
  const go = await ask("Add file", [el("p", { text: "Path inside the archive (an existing path is replaced):" }), input], [{ label: "Add", value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  try {
    setPending(await invoke("stage_add", { path: input.value, file }));
    setStatus(`Staged ${input.value}; save it with Changes…`);
  } catch (e) {
    await note("Could not stage the file", errText(e), true);
  }
}

async function doRemove() {
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  const buttons = [];
  if (sel) buttons.push({ label: sel.folder ? `Folder (${fmtNum(sel.count)} files)` : "Selected entry", value: "one", primary: true });
  buttons.push({ label: `All ${fmtNum(S.matches)} in the list`, value: "all", primary: !sel });
  buttons.push({ label: "Cancel", value: null });
  const what = await ask("Remove from the saved archive", el("p", { text: "Nothing is deleted now: the entries are left out when you save the changes as a new archive." }), buttons);
  if (!what) return;
  try {
    const list = what === "one" ? await entriesOf(sel) : [];
    if (what === "one" && !list.length) return note("Remove", "This folder holds no files.");
    setPending(await invoke("stage_remove", { indices: list }));
    setStatus(`${what === "one" ? `${fmtNum(list.length)} ${list.length === 1 ? "entry" : "entries"}` : `${fmtNum(S.matches)} entries`} marked for removal`);
    refreshPreview();
  } catch (e) {
    await note("Could not stage the removal", errText(e), true);
  }
}

async function doChanges() {
  if (!S.changes.length) {
    await note("Changes", "No pending changes. Use Replace…, Add file… or Remove; the open archive is never modified, the result is saved as a new archive.");
    return;
  }
  const list = el("ul", { class: "pending" }, ...S.changes.slice(0, 500).map((p) => el("li", {}, el("span", { class: "kind", text: p.kind }), el("span", { text: p.name }), p.file ? el("span", { class: "file", text: p.file }) : null)));
  const more = S.changes.length > 500 ? el("p", { class: "muted", text: `… and ${fmtNum(S.changes.length - 500)} more` }) : null;
  const act = await ask(`${fmtNum(S.changes.length)} pending change(s)`, [list, more], [{ label: "Save as new archive…", value: "save", primary: true }, { label: "Discard all", value: "discard" }, { label: "Close", value: null }]);
  if (act === "discard") {
    await invoke("discard_pending");
    setPending([]);
    setStatus("Changes discarded");
    refreshPreview();
  } else if (act === "save") {
    await saveEdited();
  }
}

async function saveEdited() {
  const base = S.info.file_name.replace(/\.[^.]+$/, "");
  const dst = await T.dialog.save({ title: "Save changed archive as", defaultPath: `${base}-edited.grf`, filters: [{ name: "GRF archives", extensions: ["grf"] }] });
  if (!dst) return;
  const job = showJob("Saving archive");
  setStatus(`Saving ${dst} …`);
  try {
    const r = await invoke("save_edited", { dst });
    job.close();
    setStatus(`Saved ${dst}`);
    const body = [el("p", { text: `${fmtNum(r.files)} files, ${fmtBytes(r.file_bytes)}, ${r.verified ? "verified" : "not verified"}: ${fmtNum(r.added)} added, ${fmtNum(r.replaced)} replaced, ${fmtNum(r.removed)} removed.` }), el("p", { class: "muted", text: "The original archive was not changed. The pending changes are kept until you discard them." })];
    const open = await ask("Archive saved", body, [{ label: "Open it", value: true, primary: true }, { label: "Close", value: false }]);
    if (open) await openPath(dst, true);
  } catch (e) {
    job.close();
    const cancelled = /cancel/i.test(errText(e));
    setStatus(cancelled ? "Save cancelled" : "Save failed");
    await note(cancelled ? "Save cancelled" : "Could not save", cancelled ? "Nothing was written." : errText(e), !cancelled);
  }
}

async function doBuild() {
  const src = await T.dialog.open({ directory: true, multiple: false, title: "Folder to pack (the folder that contains data\\ …)" });
  if (!src) return;
  const method = el("select", {}, el("option", { value: "zlib", text: "zlib (every client)", selected: true }), el("option", { value: "zopfli", text: "zopfli (smaller, slow, zlib-compatible)" }), el("option", { value: "lzma", text: "LZMA (only clients with LZMA support)" }));
  const format = el("select", {}, el("option", { value: "0x200", text: "0x200 (every client)", selected: true }), el("option", { value: "0x300", text: "0x300 (64-bit offsets, for archives over 4 GB)" }), el("option", { value: "0x103", text: "0x103 (old format, scrambled, for very old clients)" }), el("option", { value: "0x102", text: "0x102 (old format, scrambled)" }));
  const dedupe = el("input", { type: "checkbox" });
  const body = [
    el("p", { text: `Pack ${src}` }),
    el("div", { class: "form" }, el("label", { text: "Compression " }, method), el("label", { text: "Format " }, format), el("label", {}, dedupe, " Share identical files")),
    el("p", { class: "muted", text: "The finished archive is verified against the files on disk." }),
  ];
  const go = await ask("Build archive", body, [{ label: "Choose target…", value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  const base = src.split(/[\\/]/).filter(Boolean).pop() || "data";
  const dst = await T.dialog.save({ title: "Save archive as", defaultPath: `${base}.grf`, filters: [{ name: "GRF archives", extensions: ["grf"] }] });
  if (!dst) return;
  const job = showJob("Building archive");
  setStatus(`Building ${dst} …`);
  try {
    const r = await invoke("build_archive", { req: { src, dst, method: method.value, format: format.value, dedupe: dedupe.checked, overwrite: true } });
    job.close();
    setStatus(`Built ${dst}: ${fmtNum(r.files)} files`);
    const body = [el("p", { text: `${fmtNum(r.files)} files in ${fmtNum(r.blocks)} blocks (${fmtBytes(r.unpacked_bytes)} → ${fmtBytes(r.file_bytes)}), ${r.verified ? "verified" : "not verified"}.` })];
    if (r.shared) body.push(el("p", { class: "muted", text: `${fmtNum(r.shared)} identical files share a block.` }));
    if (r.skipped.length) body.push(el("p", { text: `${r.skipped.length} item(s) skipped:` }), el("pre", { text: r.skipped.join("\n") }));
    const open = await ask("Archive built", body, [{ label: "Open it", value: true, primary: true }, { label: "Close", value: false }]);
    if (open) await openPath(dst);
  } catch (e) {
    job.close();
    const cancelled = /cancel/i.test(errText(e));
    setStatus(cancelled ? "Build cancelled" : "Build failed");
    await note(cancelled ? "Build cancelled" : "Could not build", cancelled ? "Nothing was written." : errText(e), !cancelled);
  }
}

async function doMerge() {
  const picked = await T.dialog.open({ multiple: true, directory: false, title: "Archives to merge (at least two)", filters: [{ name: "GRF, GPF, Thor and RGZ archives", extensions: ["grf", "gpf", "thor", "rgz"] }] });
  const paths = Array.isArray(picked) ? [...picked] : picked ? [picked] : [];
  if (paths.length < 2) {
    if (paths.length) await note("Merge", "Choose at least two archives.", true);
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
          el("button", { type: "button", text: "↑", title: "Higher priority", disabled: i === 0, onclick: () => { [paths[i - 1], paths[i]] = [paths[i], paths[i - 1]]; draw(); } }),
          el("button", { type: "button", text: "↓", title: "Lower priority", disabled: i === paths.length - 1, onclick: () => { [paths[i + 1], paths[i]] = [paths[i], paths[i + 1]]; draw(); } }),
        ),
      ),
    );
  draw();
  const body = [
    el("p", { text: "Order = priority: for a path in several archives, the top one wins. The inputs are not changed." }),
    list,
    el("div", { class: "form" }, el("label", {}, replaceOnly, " Replace only: the last archive is the base, the others add no new paths")),
  ];
  const go = await ask("Merge archives", body, [{ label: "Choose target…", value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  const dst = await T.dialog.save({ title: "Save merged archive as", defaultPath: "merged.grf", filters: [{ name: "GRF archives", extensions: ["grf"] }] });
  if (!dst) return;
  const job = showJob("Merging archives");
  setStatus(`Merging into ${dst} …`);
  try {
    const r = await invoke("merge_archives", { req: { sources: paths, dst, replace_only: replaceOnly.checked, overwrite: true } });
    job.close();
    setStatus(`Merged into ${dst}: ${fmtNum(r.files)} files`);
    const lines = r.sources.map((s) => `${fmtNum(s.files)} files, ${fmtNum(s.used)} used, ${fmtNum(s.overridden)} replaced${s.ignored_new ? `, ${fmtNum(s.ignored_new)} new paths ignored` : ""}  ${s.path.split(/[\\/]/).pop()}`);
    const out = [el("p", { text: `${fmtNum(r.files)} files, ${fmtBytes(r.file_bytes)}, ${r.verified ? "verified" : "not verified"}.${r.damaged ? ` ${r.damaged} damaged block(s) copied unchanged.` : ""}` }), el("pre", { text: lines.join("\n") })];
    const open = await ask("Archives merged", out, [{ label: "Open it", value: true, primary: true }, { label: "Close", value: false }]);
    if (open) await openPath(dst);
  } catch (e) {
    job.close();
    const cancelled = /cancel/i.test(errText(e));
    setStatus(cancelled ? "Merge cancelled" : "Merge failed");
    await note(cancelled ? "Merge cancelled" : "Could not merge", cancelled ? "Nothing was written." : errText(e), !cancelled);
  }
}

/** A progress dialog for a long job; the page feeds it with `job-progress` events. */
function showJob(title) {
  const bar = el("progress", { max: 100, value: 0 });
  const label = el("p", { class: "muted", text: "Starting …" });
  const cancel = el("button", {
    type: "button",
    text: "Cancel",
    onclick: () => {
      cancel.disabled = true;
      label.textContent = "Cancelling …";
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
    label.textContent = `${fmtNum(p.done)} of ${fmtNum(p.total)}`;
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
  const mode = await ask("Verify", el("p", { text: "A quick check reads only the file table. A full check also decodes every entry; that reads the whole archive." }), [
    { label: "Quick", value: false },
    { label: "Full", value: true, primary: true },
    { label: "Cancel", value: null },
  ]);
  if (mode === null) return;
  setStatus("Verifying …");
  try {
    const r = await invoke("verify", { decode: mode });
    setStatus(`Verified: ${r.errors} errors, ${r.warnings} warnings`);
    const lines = [`${fmtNum(r.entries)} entries, ${r.decoded ? "all decoded" : "table only"}: ${r.errors} errors, ${r.warnings} warnings, ${r.infos} notes.`];
    const body = [el("p", { text: lines[0] })];
    if (r.issues.length) body.push(el("pre", { text: r.issues.join("\n") }));
    await ask(r.errors ? "Problems found" : "Archive checked", body, [{ label: "OK", value: true, primary: true }]);
  } catch (e) {
    setStatus("Verification failed");
    await note("Verification failed", errText(e), true);
  }
}

// ---------- wiring ----------

let searchTimer = 0;
$("btn-open").addEventListener("click", chooseArchive);
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
    const moves = { ArrowDown: cur + 1, ArrowUp: cur - 1, PageDown: cur + page, PageUp: cur - page, Home: 0, End: st.total - 1 };
    if (e.key in moves) {
      e.preventDefault();
      list.pick(cur < 0 && (e.key === "ArrowDown" || e.key === "PageDown") ? 0 : moves[e.key]);
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
  T.event.listen("tauri://drag-drop", (e) => {
    const p = e.payload?.paths?.[0];
    if (p) openPath(p);
  });
}

clearPreview();
syncViewBar();
buildKindFilter();

invoke("startup_path")
  .then(async (p) => {
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
