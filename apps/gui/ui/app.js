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
const errText = (e) => (typeof e === "string" ? e : e?.message || JSON.stringify(e));

// ---------- dialogs ----------

/** The icon of a message: what the title and the buttons suggest. */
function dialogKind(title, buttons) {
  if (/^(could not|cannot|error)|failed|damaged/i.test(title)) return "error";
  if (/^(remove|open another|discard|overwrite|delete)/i.test(title)) return "warning";
  if (/encrypted|password/i.test(title)) return "lock";
  if (/^(extracted|saved|done|verified|written)/i.test(title)) return "success";
  return buttons.length > 1 ? "question" : "info";
}

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
  if (!force && S.changes.length && SET.warnDiscard) {
    const go = await ask("Open another archive?", el("p", { text: `${fmtNum(S.changes.length)} pending change(s) of the open archive are not saved in an archive yet. They are kept as a draft and offered again when you open this archive next time.` }), [{ label: "Open", value: true }, { label: "Cancel", value: null, primary: true }]);
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
  $("empty").hidden = true;
  $("lists").hidden = false;
  for (const id of ["search", "btn-extract", "btn-verify", "names", "btn-add", "btn-changes", "sort", "sort-dir", "size-filter"]) $(id).disabled = false;
  V.folder = null;
  buildKindFilter();
  syncViewBar();
  S.changes = [];
  S.changed = new Map();
  updateTitle();
  updateEditBar();
  $("search").value = "";
  $("names").value = SET.names;
  S.query = "";
  if (SET.names !== "cp949") await invoke("set_names", { mode: SET.names });
  SET.lastArchive = path;
  saveSettings();
  await runSearch("");
  setStatus(
    `${S.info.file_name}: ${S.info.format}, ${fmtNum(S.info.files)} files, ${fmtBytes(S.info.file_size)}`,
  );
  if (S.info.draft) await offerDraft(S.info.draft);
  $("viewport").focus();
}

/** Unsaved changes of an earlier session were found for this archive: restore or discard them. */
async function offerDraft(d) {
  const when = new Date(d.saved_at * 1000).toLocaleString();
  const body = [el("p", { text: `${fmtNum(d.changes)} change(s) from ${when} were not saved in an archive. They were kept as a draft (for example because the app closed).` })];
  if (d.archive_changed) body.push(el("p", { class: "error", text: "The archive itself has changed since then. Check the changes before you save." }));
  if (d.missing_files) body.push(el("p", { class: "muted", text: `${fmtNum(d.missing_files)} replacement file(s) no longer exist on disk and will be left out.` }));
  if (d.more) body.push(el("p", { class: "muted", text: `${fmtNum(d.more)} older draft(s) of this archive follow, one each time you open it.` }));
  body.push(el("p", { class: "muted", text: "Later keeps this draft for another time; the changes you make now are saved as a draft of their own." }));
  const act = await ask("Restore unsaved changes?", body, [{ label: "Restore", value: "restore", primary: true }, { label: "Discard", value: "discard" }, { label: "Later", value: null }]);
  try {
    if (act === "restore") {
      const r = await invoke("restore_draft");
      setPending(await invoke("pending"));
      refreshPreview();
      setStatus(`${fmtNum(r.restored)} change(s) restored${r.skipped ? `, ${fmtNum(r.skipped)} left out` : ""}`);
    } else if (act === "discard") {
      await invoke("discard_draft");
      setStatus("Draft discarded");
    } else {
      await invoke("defer_draft");
      setStatus("The draft is kept for another time");
    }
  } catch (e) {
    await note("Could not restore the changes", errText(e), true);
  }
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
      el("span", { class: `icon twisty${r.expanded ? " open" : ""}`, "data-twist": "1" }, svgIcon("chevron")),
      el("span", { class: "name" }, svgIcon(r.expanded ? "folder-open" : "folder", "folder-ic"), baseOf(r.name), el("span", { class: "count", text: fmtNum(r.count) })),
      el("span", { class: "size", text: fmtBytes(r.bytes) }),
    );
  }
  const key = changeKey(r.name);
  const state = S.changed.has(key) ? ` changed ${S.changed.get(key)}` : "";
  const base = baseOf(r.name);
  const dir = dirOf(r.name);
  const cls = `row${sel}${state}${V.layout === "details" || V.layout === "folders" ? " details" : ""}${tree ? " tree" : ""}`;
  const icon = kindIcon(r.kind);
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
  $("sort-dir-use").setAttribute("href", V.desc ? "#i-sort-desc" : "#i-sort-asc");
  $("size-filter").value = V.size;
  for (const b of document.querySelectorAll("#colhead button")) b.classList.toggle("on", b.dataset.sort === V.sort);
  const kinds = $("kind-label");
  kinds.textContent = V.kinds.length ? `Type: ${V.kinds.length === 1 ? (KINDS.find((k) => k[0] === V.kinds[0]) || [0, V.kinds[0]])[1] : `${V.kinds.length} kinds`}` : "Type: all";
  $("kind-filter").querySelector("summary").classList.toggle("active", V.kinds.length > 0);
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
        meta.textContent += `, ${img.naturalWidth} x ${img.naturalHeight} px${p.bits ? `, ${p.bits}-bit` : ""}`;
        if (img.naturalWidth <= 128) img.style.width = `${img.naturalWidth * 4}px`;
      });
      const reduce =
        p.bits > 8
          ? el(
              "div",
              { class: "controls" },
              el("button", { type: "button", class: "small", icon: "palette", text: "8-bit…", title: "Store this picture with 256 colours (smaller; magenta stays transparent). Shows both to compare first (staged)", onclick: () => openReduceDialog(row) }),
            )
          : null;
      return [reduce, el("div", { class: "checker" }, img)];
    }
    case "audio":
      return [el("audio", { controls: true, src: dataUrl(p.mime, p.data) })];
    case "text": {
      const pre = el("pre", { text: p.text });
      const note = el("div", { class: "muted" }, "Encoding: ", el("span", { class: "badge", text: p.encoding }), p.truncated ? " (shortened)" : "");
      const edit = el("button", { type: "button", class: "small", icon: "edit", text: "Edit…", title: "Edit the text; compiled Lua is compiled again (staged, saved with Changes…)", onclick: () => openTextEditor(row) });
      return [el("div", { class: "controls" }, edit), note, pre];
    }
    case "frames":
      return [
        row.name.toLowerCase().endsWith(".pal")
          ? el(
              "div",
              { class: "controls" },
              el("button", { type: "button", class: "small", icon: "palette", text: "Adjust…", title: "Shift hue, scale saturation and brightness of a range of colours (staged)", onclick: () => adjustPalette(row) }),
              el("span", { class: "muted", text: "Click a colour to change it." }),
            )
          : null,
        row.name.toLowerCase().endsWith(".spr")
          ? el(
              "div",
              { class: "controls" },
              el("button", { type: "button", class: "small", icon: "add", text: "Add image…", title: "Append a PNG as a new image (staged)", onclick: () => addImage(row) }),
              el("button", { type: "button", class: "small", icon: "palette", text: "Set palette…", title: "Use the colours of a .pal file (staged)", onclick: () => setPalette(row) }),
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
                    el("button", { type: "button", class: "small", icon: "pencil", text: "Pixels…", title: "Draw on this image: pencil, eraser, fill and colour picker on the palette colours (staged)", onclick: () => openPixelEditor(row, i) }),
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
        el("button", { type: "button", class: "small", icon: "edit", text: "Edit frames…", title: "Edit layers, anchors and frames of this animation (staged)", onclick: () => openActEditor(row, p.action) }),
        el("button", { type: "button", class: "small", icon: "animation", text: "Set interval…", title: "Change the frame time of this action (staged, saved with Changes…)", onclick: () => setInterval_(row, p.action, p.interval_ms) }),
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
        el("div", { class: "controls" }, el("span", { class: "muted", text: "drag = turn, right/shift-drag or arrows = move, wheel = zoom, double-click = reset" }), toggle("Lighting", "lit"), toggle("Lightmaps", "lightmap"), toggle("Models", "models"), toggle("Water", "water"), p.water?.frames?.length > 1 || p.models.some((m) => m.anim) ? toggle("Animate", "animate") : null, el("button", { type: "button", class: "small", icon: "extract", text: "Extract map…", title: "Write this map with every file it needs (ground, altitude, textures, models, sounds, water, minimap, effects) into a folder", onclick: () => doExtractMap(row) })),
        host,
        infoBlock(p.info),
      ];
    }
    case "effect":
      return [el("div", { class: "muted", text: `${p.frames} frame(s) shown at ${p.fps} fps` }), el("div", { class: "fxstage" }, el("img", { src: dataUrl("image/gif", p.gif), width: p.width, height: p.height, alt: "effect" })), infoBlock(p.info)];
    case "map":
      if (p.gat) return gatEditor(row, p);
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

/** Writes the map of `row` (RSW, GAT or GND) with all its files into a folder. */
async function doExtractMap(row) {
  const dest = await T.dialog.open({ directory: true, multiple: false, title: "Folder for the map (the files go below it as data\\...)" });
  if (!dest) return;
  setStatus("Extracting the map …");
  try {
    let r;
    try {
      r = await invoke("extract_map", { index: row.index, dest, rename: false });
    } catch (e) {
      if (!/exists/i.test(errText(e))) throw e;
      const go = await ask("Files exist already", el("p", { text: `${errText(e)}. Keep both, with a number added to the new file names?` }), [{ label: "Keep both", value: true, primary: true }, { label: "Cancel", value: null }]);
      if (!go) return setStatus("Nothing extracted");
      r = await invoke("extract_map", { index: row.index, dest, rename: true });
    }
    setStatus(`Map ${r.map}: ${fmtNum(r.files)} files written to ${r.dest}`);
    const body = [el("p", { text: `${fmtNum(r.files)} files of ${r.map} written to ${r.dest}.` })];
    if (r.missing.length) body.push(el("p", { text: `${fmtNum(r.missing.length)} file(s) the map names are not in this archive (they may be in another GRF of the client):` }), el("ul", { class: "pending" }, ...r.missing.slice(0, 50).map((m) => el("li", { text: m }))));
    if (r.problems.length) body.push(el("p", { class: "error", text: r.problems.slice(0, 10).join("\n") }));
    if (r.damaged) body.push(el("p", { class: "error", text: `${fmtNum(r.damaged)} file(s) are damaged and were written as far as they could be read.` }));
    await ask(r.missing.length || r.problems.length ? "Map extracted, with gaps" : "Map extracted", body, [{ label: "OK", value: true, primary: true }], r.missing.length || r.problems.length ? "warning" : "success");
  } catch (e) {
    setStatus("Could not extract the map");
    await note("Could not extract the map", errText(e), true);
  }
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
  const n = S.changes.length;
  $("changes-badge").textContent = n;
  $("changes-badge").hidden = n === 0;
  $("btn-changes").setAttribute("aria-label", n ? `Save ${n} pending changes as a new archive` : "Save changes");
  $("btn-changes").title = (n ? `Save the ${n} pending change${n === 1 ? "" : "s"} as a new archive` : "Save the pending changes as a new archive") + " (Cmd/Ctrl+S)";
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
  S.changes = rows;
  S.changed = new Map(rows.map((r) => [changeKey(r.name), r.kind]));
  updateTitle();
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

// ---------- pixel editor ----------

/** Draws on the palette indices of one sprite image; Apply stages the result. */
async function openPixelEditor(row, frame) {
  let v;
  try {
    v = await invoke("spr_image", { index: row.index, frame });
  } catch (e) {
    await note("Pixels", errText(e), true);
    return;
  }
  const { width: w, height: h, palette } = v;
  const px = Uint8Array.from(atob(v.pixels), (c) => c.charCodeAt(0));
  const original = px.slice();
  const undo = [];
  const st = { tool: "pencil", color: 1, zoom: Math.max(2, Math.min(24, Math.floor(480 / Math.max(w, h)))), down: false, last: null, grid: true };
  const rgb = palette.map((p) => [parseInt(p.slice(0, 2), 16), parseInt(p.slice(2, 4), 16), parseInt(p.slice(4, 6), 16)]);

  const canvas = el("canvas", { class: "pixcanvas", tabindex: "0", "aria-label": `Image ${frame}, ${w} by ${h} pixels. Arrow keys move the cursor, Space draws.` });
  const off = document.createElement("canvas");
  off.width = w;
  off.height = h;
  const readout = el("span", { class: "muted", text: `${w} x ${h} px` });
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
    readout.textContent = `${w} x ${h} px — x ${cursor.x}, y ${cursor.y}: colour ${px[i]} (#${palette[px[i]]})${px[i] === 0 ? ", transparent" : ""}`;
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
  const bar = el("div", { class: "controls wrap" }, tool("pencil", "pencil", "Pencil", "P"), tool("eraser", "eraser", "Eraser", "E"), tool("fill", "fill", "Fill", "F"), tool("picker", "picker", "Picker", "I"));
  const zoom = el("input", { type: "range", min: "1", max: "32", value: String(st.zoom), "aria-label": "Zoom" });
  zoom.addEventListener("input", () => {
    st.zoom = Number(zoom.value);
    draw();
  });
  const grid = el("input", { type: "checkbox", checked: true, onchange: () => {
    st.grid = grid.checked;
    draw();
  } });
  const undoBtn = el("button", { type: "button", class: "small", icon: "undo", text: "Undo", onclick: () => {
    if (undo.length) px.set(undo.pop());
    draw();
  } });
  const resetBtn = el("button", { type: "button", class: "small", icon: "reset", text: "Revert", onclick: () => {
    pushUndo();
    px.set(original);
    draw();
  } });
  bar.append(el("label", { class: "muted", text: "Zoom " }, zoom), el("label", { class: "muted" }, grid, " Grid"), undoBtn, resetBtn);

  const sw = el("div", { class: "swatches", role: "listbox", "aria-label": "Palette" });
  const swatches = () => {
    sw.replaceChildren(
      ...palette.map((p, i) => el("button", { type: "button", class: `swatch${i === st.color ? " cur" : ""}${i === 0 ? " zero" : ""}`, style: i === 0 ? "" : `background:#${p}`, title: `${i}: #${p}${i === 0 ? " (transparent)" : ""}`, role: "option", "aria-selected": String(i === st.color), "aria-label": `Colour ${i}`, onclick: () => {
        st.color = i;
        if (st.tool === "picker") setTool("pencil");
        swatches();
      } })),
    );
  };
  swatches();
  setTool("pencil");

  $("pix-title").textContent = `Pixels of image ${frame} — ${row.name}`;
  $("pix-body").replaceChildren(el("div", { class: "pixed" }, el("div", {}, bar, el("div", { class: "pixstage" }, canvas), readout), el("div", {}, el("h3", { class: "muted", text: "Palette (click a colour; 0 is transparent)" }), sw)));
  const dlg = $("pix-dlg");
  const actions = $("pix-actions");
  const finish = (go) =>
    new Promise((resolve) => {
      actions.replaceChildren(
        el("button", { type: "button", text: "Cancel", onclick: () => { dlg.close(); resolve(false); } }),
        el("button", { type: "button", class: "primary", icon: "changes", text: "Apply", onclick: () => { dlg.close(); resolve(true); } }),
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
  if (px.every((v, i) => v === original[i])) return setStatus("No pixel changed.");
  let bin = "";
  for (let i = 0; i < px.length; i += 8192) bin += String.fromCharCode(...px.subarray(i, i + 8192));
  try {
    setPending(await invoke("stage_spr_pixels", { index: row.index, frame, pixels: btoa(bin) }));
    setStatus(`Image ${frame} of ${row.name} edited (staged); save with Changes….`);
    refreshPreview();
  } catch (e) {
    await note("Could not stage the pixels", errText(e), true);
  }
}

// ---------- map cell editing ----------

/** A GAT as a picture on which a rectangle of cells can be chosen and set (mouse or number fields). */
function gatEditor(row, p) {
  const { cells_x: cx, cells_y: cy, types } = p.gat;
  const sel = { x0: 0, y0: 0, x1: 0, y1: 0, on: false };
  const img = el("img", { src: dataUrl("image/png", p.png), width: p.width, height: p.height, alt: `map, ${cx} by ${cy} cells`, draggable: "false" });
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
  const [lx0, ix0] = field("x from", "x0", cx - 1);
  const [lx1, ix1] = field("to", "x1", cx - 1);
  const [ly0, iy0] = field("y from", "y0", cy - 1);
  const [ly1, iy1] = field("to", "y1", cy - 1);
  const kind = el("select", { "aria-label": "Cell type" }, el("option", { value: "", text: "type: no change" }), ...types.map(([k, n]) => el("option", { value: String(k), text: `${k}: ${n}` })));
  const height = el("input", { type: "number", step: "0.5", placeholder: "no change", class: "narrow wide", "aria-label": "Height" });
  const apply = el("button", { type: "button", class: "small", icon: "edit", text: "Set cells", disabled: true, title: "Set the type and/or height of the chosen cells (staged, saved with Changes…)" });
  const readout = el("span", { class: "muted", text: "Drag on the map to choose cells." });
  const draw = () => {
    const [xa, xb, ya, yb] = [Math.min(sel.x0, sel.x1), Math.max(sel.x0, sel.x1), Math.min(sel.y0, sel.y1), Math.max(sel.y0, sel.y1)];
    box.hidden = !sel.on;
    if (sel.on) {
      Object.assign(box.style, { left: `${(xa / cx) * 100}%`, width: `${((xb - xa + 1) / cx) * 100}%`, top: `${((cy - 1 - yb) / cy) * 100}%`, height: `${((yb - ya + 1) / cy) * 100}%` });
      ix0.value = xa;
      ix1.value = xb;
      iy0.value = ya;
      iy1.value = yb;
      readout.textContent = `x ${xa}–${xb}, y ${ya}–${yb}: ${fmtNum((xb - xa + 1) * (yb - ya + 1))} cell(s)`;
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
    if (kind.value === "" && h === null) return note("Set cells", "Choose a cell type and/or a height first.");
    try {
      const r = await invoke("stage_gat_edit", { index: row.index, edit: { x0: sel.x0, y0: sel.y0, x1: sel.x1, y1: sel.y1, kind: kind.value === "" ? null : Number(kind.value), height: h } });
      setPending(r.pending);
      setStatus(`${fmtNum(r.changed)} cell(s) of ${row.name} changed (staged); save with Changes….`);
      refreshPreview();
    } catch (e) {
      await note("Could not change the cells", errText(e), true);
    }
  });
  const all = el("button", { type: "button", class: "small", text: "Select all", onclick: () => {
      Object.assign(sel, { x0: 0, y0: 0, x1: cx - 1, y1: cy - 1, on: true });
      draw();
    },
  });
  return [
    el("div", { class: "controls wrap" }, lx0, lx1, ly0, ly1, all),
    el("div", { class: "controls wrap" }, kind, el("label", { class: "muted", text: "height " }, height), apply, readout),
    stage,
    infoBlock(p.info),
  ];
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

const REF_PRESETS = [
  ["data\\sprite\\인간족\\몸통\\남\\초보자_남.act", "Body, novice (male)"],
  ["data\\sprite\\인간족\\몸통\\여\\초보자_여.act", "Body, novice (female)"],
  ["data\\sprite\\인간족\\머리통\\남\\1_남.act", "Head 1 (male)"],
  ["data\\sprite\\인간족\\머리통\\여\\1_여.act", "Head 1 (female)"],
];

async function addActReference() {
  const input = el("input", { type: "text", class: "field", list: "ref-presets", placeholder: "data\\sprite\\…\\name.act", spellcheck: "false", value: S.selected != null && /\.act$/i.test(S.rows[S.selected - S.offset]?.name || "") && S.rows[S.selected - S.offset].name !== E.row?.name ? S.rows[S.selected - S.offset].name : "" });
  const presets = el("datalist", { id: "ref-presets" }, REF_PRESETS.map(([v, t]) => el("option", { value: v, label: t })));
  const under = el("input", { type: "checkbox", checked: true });
  const go = await ask(
    "Add a reference animation",
    [
      el("p", { text: "Another animation of this archive drawn with the one you edit, e.g. a body and a head to fit headgear on. Its first anchor meets the first anchor of the edited frame, as the client attaches parts to the body. Same action and frame number; the reference is only shown, never changed." }),
      el("div", { class: "form" }, el("label", {}, "Animation (.act) ", input, presets), el("label", {}, under, " Behind the edited animation")),
      el("div", { class: "controls" }, ...REF_PRESETS.map(([v, t]) => el("button", { type: "button", class: "small", text: t, onclick: () => (input.value = v) }))),
    ],
    [{ label: "Add", value: true, primary: true }, { label: "Cancel", value: null }],
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
      el("button", { type: "button", class: "small", text: r.under ? "Behind" : "In front", title: "Draw the reference behind or in front of the edited animation", onclick: () => { E.refs[i] = { ...r, under: !r.under }; saveActRefs(); loadActFrame(); } }),
      el("button", { type: "button", class: "small", text: "✕", "aria-label": `Remove the reference ${r.path}`, onclick: () => { E.refs.splice(i, 1); saveActRefs(); loadActFrame(); } }),
      problem.get(r.path) ? el("span", { class: "muted", text: problem.get(r.path) }) : null,
    ),
  );
  return el("div", { class: "refs" }, el("span", { class: "muted", text: "References" }), ...chips, el("button", { type: "button", class: "small", text: "Add reference…", onclick: addActReference }));
}

async function openActEditor(row, action) {
  stopActPlay();
  // sounds are read again each time: a staged change may have replaced one
  Object.assign(E, { row, action, frame: 0, view: null, sounds: new Map() });
  $("act-title").textContent = `Edit animation: ${row.name}`;
  $("act-body").replaceChildren(el("p", { class: "muted", text: "Loading …" }));
  $("act-dlg").showModal();
  await loadActFrame();
}

$("act-dlg").addEventListener("close", () => {
  stopActPlay();
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

async function removeAction(i) {
  const go = await ask(`Remove action ${i}?`, el("p", { text: "Later actions move down by one. The client picks actions by number (direction, state), so other files that use them may change meaning." }), [{ label: "Remove", value: true, primary: true }, { label: "Cancel", value: null }]);
  if (go) await actEdit({ op: "remove_action" });
}

const hex2 = (n) => n.toString(16).padStart(2, "0");

function soundField(v) {
  const list = el("datalist", { id: "act-sounds" }, v.sounds.map((s) => el("option", { value: s })));
  const input = el("input", { type: "text", style: "width:13em", list: "act-sounds", value: v.sound ?? "", placeholder: "none", title: "Sound file played with this frame, e.g. effect\\hit.wav (empty = none)", size: "22" });
  input.addEventListener("change", () => actEdit({ op: "set_sound", name: input.value.trim() === "" ? null : input.value.trim() }));
  const note = el("span", { class: "muted", role: "status" });
  const hear = el("button", {
    type: "button",
    class: "small",
    icon: "audio",
    title: "Play this sound (data\\wav\\ in the archive)",
    "aria-label": "Play the sound of this frame",
    disabled: !v.sound,
    onclick: async () => {
      const s = await playActSound(v.sound);
      note.textContent = s?.problem ? `${s.path ?? v.sound}: ${s.problem}` : "";
    },
  });
  return el("span", { class: "soundfield" }, el("label", { class: "muted", text: "Sound " }, input, list), hear, note);
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
  for (let i = 0; i < v.actions; i++) actionSel.append(el("option", { value: i, text: `Action ${i}`, selected: i === v.action }));
  const bar = el(
    "div",
    { class: "bar" },
    el("label", { text: "Action " }, actionSel),
    el("button", { type: "button", class: "small", text: "◀", disabled: v.frame === 0, onclick: () => { E.frame -= 1; loadActFrame(); } }),
    el("span", { text: `Frame ${v.frame + 1} / ${v.frames}` }),
    el("button", { type: "button", class: "small", text: "▶", disabled: v.frame >= v.frames - 1, onclick: () => { E.frame += 1; loadActFrame(); } }),
    el("button", { type: "button", class: "small", text: E.playing ? "Stop" : "Play", "aria-pressed": String(E.playing), "data-act-play": "", title: "Play the action at its frame time, with the sounds of its frames", disabled: v.frames <= 1, onclick: toggleActPlay }),
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
  if (E.layer != null && E.layer >= v.layers.length) E.layer = null;
  const stage = el("div", { class: "stage checker", tabindex: "0", "data-act-stage": "", role: "group", "aria-roledescription": "frame editor", "aria-label": `Frame ${v.frame + 1}. Click a layer to select it and drag it to move it; the arrow keys move the selected layer by 1 pixel, with Shift by 10.${E.layer != null ? ` Layer ${E.layer} is selected.` : ""}` });
  if (v.png) {
    stage.append(el("img", { src: dataUrl("image/png", v.png), width: v.width, height: v.height, alt: "", draggable: "false" }), el("div", { class: "cross-h", style: `top:${v.origin_y}px` }), el("div", { class: "cross-v", style: `left:${v.origin_x}px` }));
    v.anchors.forEach((a, i) => stage.append(el("div", { class: "dot", title: `Anchor ${i}: drag to move it`, "data-anchor": String(i), style: `left:${v.origin_x + a.x * v.scale}px;top:${v.origin_y + a.y * v.scale}px` })));
    const b = E.layer != null ? v.layers[E.layer]?.bounds : null;
    if (b) stage.append(el("div", { class: "layerbox", style: `left:${b[0]}px;top:${b[1]}px;width:${b[2] - b[0]}px;height:${b[3] - b[1]}px` }));
    wireStage(stage, v);
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
      { class: i === E.layer ? "sel" : "" },
      el("td", {}, el("button", { type: "button", class: "small", text: String(i), title: "Select this layer (drag it on the picture or use the arrow keys there)", "aria-pressed": String(i === E.layer), onclick: () => { E.layer = i === E.layer ? null : i; drawActEditor(); } })),
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

// What the build options mean, shown next to the choice (Build dialog and Settings)
const METHODS = [
  ["zlib", "zlib (every client)", "The standard. Every client can read it and it builds fast. Choose this unless you have a reason not to."],
  ["zopfli", "zopfli (smaller, slow)", "Writes the same zlib format, so every client can still read it, but squeezes the files a little further. Building takes much longer; reading is just as fast."],
  ["lzma", "LZMA (needs client support)", "Smaller than zlib, but only a client with LZMA support can read it (for example one with a cps.dll). Try the archive in your client before you share it."],
];
const FORMATS = [
  ["0x200", "0x200 (every client)", "The standard GRF format. Every client reads it. File positions are stored in 32 bits, so the archive cannot grow beyond 4 GB."],
  ["0x300", "0x300 (over 4 GB)", "Newer format with 64-bit file positions, so the archive can be larger than 4 GB. Only newer clients read it (measured: a 2025 client does, 2020 clients do not). Use it only when 4 GB is not enough."],
  ["0x103", "0x103 (very old clients)", "Very old format from before 0x200: the file table is not compressed and names and data are scrambled with the old DES variant. For clients too old to read 0x200. Same 4 GB limit."],
  ["0x102", "0x102 (very old clients)", "Same as 0x103 (they differ only in the version number). Use it if an old client asks for exactly this version."],
];
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
  const src = await T.dialog.open({ directory: true, multiple: false, title: "Folder to pack (the folder that contains data\\ …)" });
  if (!src) return;
  const dedupe = el("input", { type: "checkbox" });
  const m = explained(METHODS, SET.method);
  const f = explained(FORMATS, SET.format, (v) => {
    const old = v === "0x102" || v === "0x103";
    dedupe.disabled = old;
    if (old) dedupe.checked = false;
  });
  const method = m.sel, format = f.sel;
  dedupe.disabled = format.value === "0x102" || format.value === "0x103";
  const body = [
    el("p", { text: `Pack ${src}` }),
    el("div", { class: "form buildform" },
      el("label", {}, el("span", { class: "fl", text: "Compression" }), method), m.hint,
      el("label", {}, el("span", { class: "fl", text: "Format" }), format), f.hint,
      el("label", {}, dedupe, " Share identical files"),
      el("div", { class: "hint", text: "Stores files with identical content only once, which saves space. Not possible in 0x102 and 0x103." })),
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

const PSNR_HINT = "PSNR measures how far the colours moved: from 40 dB on the difference is hard to see, below 35 dB smooth gradients show bands.";

/** One true-colour BMP next to its 8-bit version; staging replaces the entry. */
async function openReduceDialog(row) {
  let p;
  try {
    p = await invoke("reduce_preview", { index: row.index });
  } catch (e) {
    return note("Cannot reduce this picture", errText(e), true);
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
  const quality = p.psnr == null ? "lossless: the picture has no more than 256 colours" : `PSNR ${p.psnr.toFixed(1)} dB`;
  const go = await ask(
    "Reduce to 8 bits?",
    [
      el("p", { text: `${fmtBytes(p.bytes_before)} → ${fmtBytes(p.bytes_after)} unpacked, ${fmtNum(p.colors)} colours, ${quality}. Magenta (transparent) stays exact.` }),
      p.psnr == null ? null : el("p", { class: "muted", text: PSNR_HINT }),
      el("div", { class: "compare" }, pic(p.before, "Original"), pic(p.after, "8-bit")),
    ],
    [{ label: "Reduce", value: true, primary: true }, { label: "Cancel", value: null }],
  );
  if (!go) return;
  try {
    setPending(await invoke("stage_reduce", { index: row.index }));
    setStatus(`${row.name} reduced to 8 bits (staged)`);
    refreshPreview();
  } catch (e) {
    await note("Could not reduce", errText(e), true);
  }
}

/** A copy of the open archive (with the pending changes) whose true-colour BMPs are 8-bit. */
async function doReduceImages() {
  if (!S.info) return;
  const level = el(
    "select",
    { class: "field" },
    ...[
      [45, "45 dB: best, smallest savings"],
      [40, "40 dB: hard to tell apart (recommended)"],
      [38, "38 dB: smaller, slight loss in gradients"],
      [35, "35 dB: smallest, gradients may show bands"],
    ].map(([v, t]) => el("option", { value: String(v), text: t, selected: v === 40 })),
  );
  const pending = S.changes.length;
  const go = await ask(
    "Reduce images",
    [
      el("p", { text: "Writes a copy of this archive in which every true-colour BMP is stored with 256 colours, where the picture keeps the quality below and the stored file gets smaller. The open archive is not changed. The client loads 8-bit BMPs (many of its own are)." }),
      el("p", { class: "muted", text: `${PSNR_HINT} Magenta (transparent) stays exact, and no other colour becomes transparent. Pictures with at most 256 colours lose nothing.` }),
      el("div", { class: "form" }, el("label", {}, "Lowest quality ", level)),
      pending ? el("p", { text: `The ${fmtNum(pending)} pending change(s) are written into the copy too; those entries are not reduced.` }) : null,
    ],
    [{ label: "Choose target…", value: true, primary: true }, { label: "Cancel", value: null }],
  );
  if (!go) return;
  const base = S.info.file_name.replace(/\.[^.]+$/, "");
  const dst = await T.dialog.save({ title: "Save the reduced archive as", defaultPath: `${base}-8bit.grf`, filters: [{ name: "GRF archives", extensions: ["grf"] }] });
  if (!dst) return;
  const job = showJob("Reducing images");
  setStatus(`Reducing images into ${dst} …`);
  try {
    const r = await invoke("reduce_images", { dst, minPsnr: Number(level.value) });
    job.close();
    const why = { already_8_bit: "already 8-bit", below_min_psnr: "below the quality", not_smaller: "not smaller stored", noisy_transparency: "noisy around magenta", alpha: "with alpha", unreadable: "unreadable" };
    const kept = Object.entries(r.kept).map(([k, n]) => `${fmtNum(n)} ${why[k] || k}`).join(", ");
    const saved = r.stored_before - r.stored_after;
    const lines = [
      el("p", { text: `${fmtNum(r.reduced)} of ${fmtNum(r.matched)} BMPs reduced (${fmtNum(r.lossless)} lossless${r.worst_psnr != null ? `, worst ${r.worst_psnr.toFixed(1)} dB` : ""}), ${fmtBytes(saved)} smaller.` }),
      kept ? el("p", { class: "muted", text: `Kept as they are: ${kept}.` }) : null,
    ];
    if (!r.written) {
      setStatus("Nothing to reduce");
      return ask("Nothing to reduce", lines, [{ label: "OK", value: true, primary: true }], "info");
    }
    lines.push(el("p", { text: `${fmtNum(r.files)} files, ${fmtBytes(r.file_bytes)}, ${r.verified ? "verified" : "not verified"}.` }));
    setStatus(`Reduced into ${dst}: ${fmtBytes(saved)} smaller`);
    const open = await ask("Images reduced", lines, [{ label: "Open it", value: true, primary: true }, { label: "Close", value: false }]);
    if (open) await openPath(dst, true);
  } catch (e) {
    job.close();
    const cancelled = /cancel/i.test(errText(e));
    setStatus(cancelled ? "Reducing cancelled" : "Reducing failed");
    await note(cancelled ? "Reducing cancelled" : "Could not reduce", cancelled ? "Nothing was written." : errText(e), !cancelled);
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

// ---------- settings ----------

const SETTINGS_KEY = "rustygrf-settings";
const DEFAULTS = { theme: "system", accent: "orange", fontsize: "normal", density: "normal", plainIcons: false, motion: "system", names: "cp949", layout: "last", preview: "last", reopen: false, warnDiscard: true, method: "zlib", format: "0x200", lastArchive: "" };
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

/** Puts the settings on the page; called at start and after each change. */
function applySettings() {
  const root = document.documentElement;
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
  $("settings-title").replaceChildren(svgIcon("settings", "dlg-ic"), "Settings");
  const change = (key, value) => {
    SET[key] = value;
    saveSettings();
    applySettings();
  };
  const row = (label, control, hint) => el("div", { class: "srow" }, el("label", { text: label }), control, hint ? el("div", { class: "hint", text: hint }) : null);
  const pick = (key, options, hint) => {
    const sel = el("select", { "aria-label": key }, options.map(([v, t]) => el("option", { value: v, text: t, selected: SET[key] === v })));
    sel.addEventListener("change", () => change(key, sel.value));
    return sel;
  };
  const check = (key, text) => {
    const i = el("input", { type: "checkbox", checked: !!SET[key] });
    i.addEventListener("change", () => change(key, i.checked));
    return el("label", { class: "check" }, i, text);
  };
  const accents = el("div", { class: "accents", role: "group", "aria-label": "Accent colour" }, [["orange", "#ea580c"], ["blue", "#3b82f6"], ["green", "#22a553"], ["purple", "#8b5cf6"]].map(([name, col]) => {
    const b = el("button", { type: "button", title: name, "aria-label": name, "aria-pressed": String(SET.accent === name), class: SET.accent === name ? "on" : "", style: `background:${col}` });
    b.addEventListener("click", () => {
      change("accent", name);
      for (const x of accents.children) {
        x.classList.toggle("on", x === b);
        x.setAttribute("aria-pressed", String(x === b));
      }
    });
    return b;
  }));
  $("settings-body").replaceChildren(
    el(
      "div",
      { class: "settings" },
      el("h3", { text: "Appearance" }),
      row("Theme", pick("theme", [["system", "Follow the system"], ["light", "Light"], ["dark", "Dark"]])),
      row("Accent colour", accents),
      row("Text size", pick("fontsize", [["small", "Small"], ["normal", "Normal"], ["large", "Large"]])),
      row("List density", pick("density", [["compact", "Compact"], ["normal", "Normal"], ["comfortable", "Comfortable"]])),
      row("File icons", check("plainIcons", "Plain icons (no colours by file type)")),
      row("Animations", pick("motion", [["system", "Follow the system"], ["off", "Off"]]), "Turns off the arrow and other transitions."),
      el("h3", { text: "When opening an archive" }),
      row("File names", pick("names", [["cp949", "Korean (CP949)"], ["cp1252", "Latin-1 (as GRF Editor shows them)"], ["utf8", "UTF-8"]])),
      row("List layout", pick("layout", [["last", "As last used"], ["list", "List"], ["details", "Details"], ["tree", "Tree"], ["folders", "Folders"]]), "Takes effect the next time the app starts."),
      row("Preview", pick("preview", [["last", "As last used"], ["right", "Right"], ["bottom", "Bottom"], ["off", "Off"]]), "Takes effect the next time the app starts."),
      row("Start-up", check("reopen", "Open the last archive again")),
      row("Changes", check("warnDiscard", "Ask before pending changes are discarded")),
      el("h3", { text: "Building archives" }),
      ...[["Compression", "method", METHODS], ["Format", "format", FORMATS]].map(([label, key, table]) => {
        const e = explained(table, SET[key], (v) => change(key, v));
        e.sel.setAttribute("aria-label", label);
        return el("div", { class: "srow" }, el("label", { text: label }), e.sel, e.hint);
      }),
      el("p", { class: "muted", text: "These are only the starting choices of the Build… dialog; you can change them there for each archive." }),
      el("p", { class: "muted", text: `Settings are stored in this app only. Shortcut: ${/Mac/.test(navigator.platform) ? "Cmd" : "Ctrl"}+, opens this window.` }),
    ),
  );
  $("settings-actions").replaceChildren(
    el("button", { type: "button", icon: "reset", text: "Reset to defaults", onclick: () => {
      Object.assign(SET, DEFAULTS, { lastArchive: SET.lastArchive });
      saveSettings();
      applySettings();
      openSettings();
    } }),
    el("button", { type: "button", class: "primary", icon: "success", text: "Done", onclick: () => dlg.close() }),
  );
  if (!dlg.open) {
    dlg.showModal();
    // showModal focuses the first focusable element, which is the scrolling body itself
    $("settings-body").querySelector("select, input, button")?.focus();
  }
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
  if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key.toLowerCase() === "s") {
    e.preventDefault();
    if (S.info && !document.querySelector("dialog[open]")) doChanges();
  }
  if ((e.metaKey || e.ctrlKey) && e.key === ",") {
    e.preventDefault();
    openSettings();
  }
  if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && "1234".includes(e.key) && e.key.length === 1 && S.info) {
    e.preventDefault();
    setLayout(["list", "details", "tree", "folders"][Number(e.key) - 1]);
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
function runMenuCommand(id) {
  if (document.querySelector("dialog[open]")) return;
  if (id.startsWith("layout-")) return S.info && setLayout(id.slice(7));
  if (id.startsWith("preview-")) return setPreviewMode(id.slice(8));
  if (id === "reduce-images") return doReduceImages();
  const b = MENU_BUTTONS[id] && $(MENU_BUTTONS[id]);
  if (b && !b.disabled) b.click();
}

// buttons whose words are hidden in narrow windows keep their name for screen readers and tooltips
for (const b of document.querySelectorAll("button")) {
  const t = b.querySelector("span:not(.sr)")?.textContent?.trim();
  if (t && !b.hasAttribute("aria-label")) b.setAttribute("aria-label", t.replace(/…$/, ""));
  if (t && !b.title) b.title = t;
}
if (SET.layout !== "last") V.layout = SET.layout;
if (SET.preview !== "last") V.preview = SET.preview;
applySettings();
clearPreview();
syncViewBar();
buildKindFilter();

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
