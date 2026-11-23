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
/**
 * The libraries report in English. Known messages (matched by their text, the translations are the
 * `le-*` keys) are shown in the user's language; what is not known stays as it is. Free text inside
 * a message (`reason`) is translated the same way.
 */
const LIB_ERRORS = [
  [/^I\/O error: ([\s\S]*)$/, "le-io", ["reason"]],
  [/^not a GRF archive: ([\s\S]*)$/, "le-not-grf", ["reason"]],
  [/^unsupported GRF version 0x([0-9a-f]+) \(supported: [^)]*\)$/, "le-grf-version", ["v"]],
  [/^corrupt file table: ([\s\S]*)$/, "le-corrupt-table", ["reason"]],
  [/^entry index (\d+) out of range$/, "le-no-entry", ["n"]],
  [/^entry (\d+) lies outside the archive \(offset (\d+), size (\d+), file size (\d+)\)$/, "le-entry-range", ["n", "offset", "size", "file"]],
  [/^unsafe or invalid path in archive: ([\s\S]*)$/, "le-unsafe-path", ["path"]],
  [/^target already exists: ([\s\S]*)$/, "le-exists", ["path"]],
  [/^cannot repack: ([\s\S]*)$/, "le-repack", ["reason"]],
  [/^this key does not belong to the archive \([^)]*\)$/, "le-wrong-key", []],
  [/^the file table is encrypted \(GRF Editor scheme\): a key is needed to open the archive$/, "le-table-encrypted", []],
  [/^verification of the written archive failed: ([\s\S]*)$/, "le-verify", ["reason"]],
  [/^magic "Master of Magic" or "Event Horizon" not found$/, "le-r-magic", []],
  [/^file has only (\d+) bytes$/, "le-r-small", ["n"]],
  [/^Thor magic not found$/, "le-r-thor", []],
  [/^file table (?:too large|claims \d+ bytes|of \d+ bytes)$/, "le-r-table-big", []],
  [/^(?:compressed )?file table (?:exceeds the file|lies outside the file)$/, "le-r-table-out", []],
  [/^file table does not decompress: ([\s\S]*)$/, "le-r-table-z", ["reason"]],
  [/^file table is (\d+) bytes, header says (\d+)$/, "le-r-table-size", ["got", "want"]],
  [/^(\d+) unexpected bytes after (\d+) entries$/, "le-r-trailing", ["n", "count"]],
  [/^entry (\d+): truncated$/, "le-r-entry-cut", ["n"]],
  [/^entry (\d+): name not terminated$/, "le-r-entry-name", ["n"]],
  [/^not an? ([A-Z]{3}) file \(missing "([A-Z]+)" signature\)$/, "le-sig", ["format", "sig"]],
  [/^not an? (BMP|STR|RIFF\/WAVE) file$/, "le-not-fmt", ["format"]],
  [/^([A-Z]{3}) data ends too early \(needed (\d+) bytes at offset (\d+), file has (\d+)\)$/, "le-early", ["format", "need", "at", "len"]],
  [/^GND data ends too early \(needed (\d+) bytes, file has (\d+)\)$/, "le-early-gnd", ["need", "len"]],
  [/^the (SPR|ACT|GND|STR|BMP|TGA)(?: file)? is truncated(?: or damaged)?$/, "le-truncated", ["format"]],
  [/^the bytecode is truncated or damaged$/, "le-bytecode-cut", []],
  [/^not Lua bytecode$/, "le-not-lua", []],
  [/^unsupported Lua bytecode: ([\s\S]*)$/, "le-lua-unsupported", ["reason"]],
  [/^unsupported (SPR|ACT|RSM|STR) version (\S+?)(?: \(0x94 is supported\))?$/, "le-version", ["format", "v"]],
  [/^world file: ([\s\S]*)$/, "le-prefix-world", ["reason"]],
  [/^ground file: ([\s\S]*)$/, "le-prefix-ground", ["reason"]],
  [/^model: ([\s\S]*)$/, "le-prefix-model", ["reason"]],
  [/^(.+) is not an RSM2 model \(version 2\.2 or 2\.3\)$/, "le-not-rsm2", ["name"]],
  [/^GND header is not valid \(([\s\S]*)\)$/, "le-gnd-header", ["reason"]],
  [/^GAT size (\d+) x (\d+) is not valid$/, "le-gat-size", ["w", "h"]],
  [/^(\d+) bytes of unused data after the cells$/, "le-gat-extra", ["n"]],
  [/^cell \((\d+), (\d+)\) is outside the (\d+) x (\d+) map$/, "le-gat-cell", ["x", "y", "w", "h"]],
  [/^the map would need more than 65 536 different lightmaps$/, "le-lightmaps", []],
  [/^shadows from the walk grid need the map's GAT$/, "le-gat-shadows", []],
  [/^object (\d+) has the unknown type (-?\d+)$/, "le-object-type", ["n", "kind"]],
  [/^the SPR has no (palette|true-colour) image (\d+) \((\d+) images of that kind\)$/, "le-spr-no-image", ["kind", "index", "count"]],
  [/^the SPR has no palette \(it holds no palette images\)$/, "le-spr-no-palette", []],
  [/^the sprite has no true-colour images$/, "le-spr-no-true", []],
  [/^the sprite has no palette images$/, "le-spr-no-pal-img", []],
  [/^indexed images need a palette$/, "le-spr-need-pal", []],
  [/^too many images \((?:(\d+), )?at most 65535 of (?:each|one) kind\)$/, "le-spr-too-many", ["n"]],
  [/^image is (\d+) x (\d+), SPR images are at most 65535 x 65535$/, "le-spr-big", ["w", "h"]],
  [/^the ACT has no action (\d+)(?: \((\d+) actions\))?$/, "le-act-no-action", ["index", "count"]],
  [/^action (\d+) has no frame (\d+)(?: \((\d+) frames\))?$/, "le-act-no-frame", ["action", "frame", "count"]],
  [/^action (\d+) has no frame after frame (\d+) to blend with$/, "le-act-no-next", ["action", "frame"]],
  [/^nothing selected: no layer matches$/, "le-act-nothing", []],
  [/^the sound name must be at most 39 Latin-1 characters$/, "le-act-sound-name", []],
  [/^unsupported WAV format tag (0x[0-9a-f]+)$/, "le-wav-tag", ["tag"]],
  [/^the WAV has no usable fmt or data chunk$/, "le-wav-chunks", []],
  [/^the ADPCM data is inconsistent$/, "le-adpcm", []],
  [/^unknown operation '([^']*)'$/, "le-audio-op", ["name"]],
  [/^the range (\d+)-(\d+) ms is empty or outside the sound \((\d+) ms long\)$/, "le-audio-range", ["a", "b", "len"]],
  [/^MP3 and Ogg Vorbis sounds can only be edited in a build with the feature lossy-audio$/, "le-no-lossy", []],
  [/^MP3 cannot hold (\d+) Hz \([^)]*\)$/, "le-mp3-rate", ["rate"]],
  [/^MP3 holds one or two channels$/, "le-mp3-channels", []],
  [/^not a readable sound: ([\s\S]*)$/, "le-lossy-read", ["reason"]],
  [/^the (?:TGA )?image has no pixels$/, "le-img-none", []],
  [/^image size does not match its pixel data$/, "le-img-size", []],
  [/^the file is too short for a TGA image$/, "le-tga-short", []],
  [/^unsupported TGA pixel depth (\d+)$/, "le-tga-depth", ["n"]],
  [/^unsupported TGA image type (\d+)$/, "le-tga-type", ["n"]],
  [/^unsupported BMP \(compression (\d+), (\d+) bits per pixel\)$/, "le-bmp-unsupported", ["compression", "bits"]],
  [/^the BMP has an invalid size$/, "le-bmp-size", []],
  [/^PNG (encoding|decoding) failed: ([\s\S]*)$/, "le-png-fail", ["how", "reason"]],
  [/^GIF encoding failed: ([\s\S]*)$/, "le-gif-fail", ["reason"]],
  [/^a PAL file is (\d+) bytes, this one has (\d+)$/, "le-pal-size", ["want", "got"]],
];

function libText(text) {
  if (typeof text !== "string" || !text) return text;
  const plain = text.trim();
  for (const [re, key, names] of LIB_ERRORS) {
    const m = re.exec(plain);
    if (!m) continue;
    const args = {};
    names.forEach((n, i) => (args[n] = n === "reason" ? libText(m[i + 1] ?? "") : (m[i + 1] ?? "")));
    return t(key, args);
  }
  // a message that names a file first ("/path/a.grf: not a GRF archive: …")
  const at = plain.indexOf(": ");
  if (at > 0) {
    const rest = plain.slice(at + 2);
    const translated = libText(rest);
    if (translated !== rest) return `${plain.slice(0, at)}: ${translated}`;
  }
  return text;
}

/** An error from the backend as text, in English where the backend has no translation; coded ones (a sentinel, then JSON) are translated. */
const plainErr = (e) => {
  const text = typeof e === "string" ? e : e?.message || JSON.stringify(e);
  return text.startsWith("\u0001") ? noteText(text) : text;
};

/** An error for the user: coded ones and the known messages of the libraries are translated. */
const errText = (e) => {
  const text = typeof e === "string" ? e : e?.message || JSON.stringify(e);
  return text.startsWith("\u0001") ? noteText(text) : libText(text);
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
    filters: [{ name: "GRF, GPF, GSF, Thor and RGZ archives", extensions: ["grf", "gpf", "gsf", "thor", "rgz"] }],
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
      if (!/key/i.test(plainErr(e)) || !/encrypted|CRC32/i.test(plainErr(e))) throw e;
      problem = /CRC32/.test(plainErr(e)) ? t("enc-wrong") : "";
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
const untitledKind = (path) => /untitled-\d+\.(grf|gpf|gsf|rgz|thor)$/i.exec(path || "")?.[1].toLowerCase() || null;

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
  renewAssets();
  Object.assign(NAV, { back: [], fwd: [], current: null });
  S.info = info;
  S.cryptAsked = false;
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
  await restoreTree();
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

/** The files shown before, for Back and Forward (as in GRF Editor and a web browser). */
const NAV = { back: [], fwd: [], current: null, moving: false };
function noteVisit(index) {
  if (NAV.moving || index === NAV.current) return;
  if (NAV.current != null) NAV.back.push(NAV.current);
  if (NAV.back.length > 100) NAV.back.shift();
  NAV.fwd = [];
  NAV.current = index;
}
async function navigate(back) {
  const from = back ? NAV.back : NAV.fwd;
  if (!S.info || !from.length) return;
  const to = from.pop();
  (back ? NAV.fwd : NAV.back).push(NAV.current);
  NAV.current = to;
  NAV.moving = true;
  try {
    await revealEntry(to);
  } finally {
    NAV.moving = false;
  }
}

const mainList = new VList(S, { vp: "viewport", spacer: "spacer", rows: "rows" }, 0, mainRow, (r) => {
  updateEditBar();
  if (!r.folder) noteVisit(r.index);
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

const TREE_KEY = "rustygrf-tree";
const treeStates = () => {
  try {
    return JSON.parse(localStorage.getItem(TREE_KEY) || "{}");
  } catch {
    return {};
  }
};

/** With the setting "Remember tree expansion": keeps which folders are open for this archive. */
async function rememberTree() {
  if (!SET.rememberTree || !S.info || S.untitled || S.query || V.layout !== "tree") return;
  try {
    const keys = await invoke("tree_state", { view: 0 });
    const all = treeStates();
    delete all[S.info.path];
    // newest last; only the latest 20 archives are kept
    const kept = Object.entries(all).slice(-19);
    kept.push([S.info.path, keys.slice(0, 5000)]);
    localStorage.setItem(TREE_KEY, JSON.stringify(Object.fromEntries(kept)));
  } catch {}
}

async function restoreTree() {
  if (!SET.rememberTree || S.untitled || V.layout !== "tree") return;
  const keys = treeStates()[S.info.path];
  if (!Array.isArray(keys)) return;
  try {
    const rows = await invoke("tree_restore", { view: 0, keys });
    await mainList.reset(rows);
  } catch {}
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
  if (list === mainList) rememberTree();
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
    rememberTree();
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
  stopAudio();
  $("preview-head").replaceChildren();
  $("preview-body").replaceChildren(el("p", { class: "placeholder", text: S.info ? t("pvw-select") : "" }));
}

async function showPreview(row, action) {
  const token = ++S.previewToken;
  stopAudio();
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
  // the text of a library error inside a message
  for (const k of ["error", "reason"]) if (typeof args[k] === "string") args[k] = libText(args[k]);
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

/**
 * Pictures of 3D previews come through the app's `rgrf` scheme, one request each, so the map is
 * drawn before its textures arrive. The tag changes with every opened archive and every change,
 * so the browser cache (shared by all windows) never gives an old picture.
 */
let ASSET_TAG = Date.now().toString(36);
const renewAssets = () => (ASSET_TAG = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`);
function assetUrl(kind, entry) {
  const name = `${kind}-${entry}-${ASSET_TAG}`;
  return T.core.convertFileSrc ? T.core.convertFileSrc(name, "rgrf") : `/rgrf/${name}`;
}
function withAssetUrls(p) {
  for (const tx of p.textures || []) tx.url = tx.entry != null ? assetUrl("tex", tx.entry) : null;
  if (p.water) p.water.frames = p.water.frames.map((e) => assetUrl("raw", e));
}

const infoBlock = (lines) => (lines?.length ? el("div", { class: "info" }, el("pre", { text: lines.map(noteText).join("\n") })) : null);

const PLAYER = { el: null };
function stopAudio() {
  PLAYER.el?.pause();
  PLAYER.el = null;
}

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
    case "audio": {
      const a = el("audio", { controls: true, src: dataUrl(p.mime, p.data) });
      stopAudio();
      PLAYER.el = a;
      // a click or an arrow key selected the entry, so the webview lets it start by itself
      if (SET.autoplay !== false) a.play().catch(() => {});
      const editable = ["wav", "mp3", "ogg"].includes(extOf(row.name).toLowerCase());
      return [editable ? el("div", { class: "controls" }, el("button", { type: "button", class: "small", icon: "edit", text: t("ctx-audio"), onclick: () => (stopAudio(), openAudioEditor(row)) })) : null, a];
    }
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
                    el("button", { type: "button", class: "small", text: t("spr-before"), title: t("spr-before-title"), onclick: () => replaceImage(row, i, "before") }),
                    el("button", { type: "button", class: "small", text: t("spr-after"), title: t("spr-after-title"), onclick: () => replaceImage(row, i, "after") }),
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
      withAssetUrls(p);
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
      withAssetUrls(p);
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
        el("button", { type: "button", class: "small", icon: "edit", text: t("pvw-flat-map"), title: t("pvw-flat-map-title"), onclick: () => doFlatMap(row, shown()) }),
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
async function entriesOf(row, view = 0) {
  return row.folder ? invoke("folder_entries", { view, path: row.name }) : [row.index];
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

/** The flat maps maker's options as ticks; flat ground implies no objects and no water, shadows from the walk grid replace light and shadows. */
const FLAT_KEYS = ["flatten", "remove_shadow", "remove_light", "remove_color", "gat_shadows", "reset_light", "remove_models", "remove_objects", "remove_water"];

function flatForm(preset = {}) {
  const boxes = Object.fromEntries(FLAT_KEYS.map((k) => [k, el("input", { type: "checkbox", checked: !!preset[k] })]));
  // implied ticks are shown ticked, replaced ones unticked, both locked; the own choice comes back when unlocked
  const own = {};
  const lock = (k, on, shown) => {
    const c = boxes[k];
    if (on && !c.disabled) own[k] = c.checked;
    if (!on && c.disabled) c.checked = own[k];
    c.disabled = on;
    if (on) c.checked = shown;
  };
  const sync = () => {
    for (const k of ["remove_models", "remove_objects", "remove_water"]) lock(k, boxes.flatten.checked, true);
    for (const k of ["remove_shadow", "remove_light"]) lock(k, boxes.gat_shadows.checked, false);
  };
  for (const k of ["flatten", "gat_shadows"]) boxes[k].addEventListener("change", sync);
  sync();
  const node = el("div", { class: "form" }, ...FLAT_KEYS.map((k) => el("label", {}, boxes[k], ` ${t(`flat-${k}`)}`)));
  const value = () => Object.fromEntries(FLAT_KEYS.map((k) => [k, boxes[k].checked && !boxes[k].disabled]));
  return { node, value, any: () => Object.values(value()).some(Boolean) };
}

/** Ticks that match what is switched off in the 3D view. */
function flatPreset(shown = {}) {
  const off = (k) => shown[k] === false;
  return { remove_shadow: off("lightmap"), remove_light: off("lightmap"), reset_light: off("lit"), remove_models: off("models"), remove_water: off("water") };
}

/** Changes the map of `row` in the archive (flat ground, no light, …); one step that Undo takes back. */
async function doFlatMap(row, shown = {}) {
  const form = flatForm(flatPreset(shown));
  const go = await ask(t("flat1-title"), [el("p", { text: t("flat1-intro") }), form.node, el("p", { class: "muted", text: t("flat-notes") })], [{ label: t("flat1-go"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  if (!form.any()) return note(t("flat1-title"), t("flat-none"));
  try {
    setPending(await invoke("stage_flat_map", { index: row.index, options: form.value() }));
    setStatus(t("flat1-status", { map: baseOf(row.name).replace(/\.[^.]+$/, "") }));
    refreshPreview();
  } catch (e) {
    await note(t("flat-failed"), errText(e), true);
  }
}

/** Tools › Flat Maps: a small archive with every map (or the selected ones) changed, for the client to read first. */
async function doFlatMaps() {
  if (!S.info) return;
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  const form = flatForm({ flatten: true, remove_shadow: true, remove_light: true });
  const buttons = [];
  if (sel) buttons.push({ label: sel.folder ? t("flats-in-folder") : t("flats-selected"), value: "one", primary: true });
  buttons.push({ label: t("flats-all"), value: "all", primary: !sel });
  buttons.push({ label: "Cancel", value: null });
  const what = await ask(t("flats-title"), [el("p", { text: t("flats-intro") }), form.node, el("p", { class: "muted", text: t("flat-notes") }), S.changes.length ? el("p", { text: t("flats-pending", { n: S.changes.length }) }) : null], buttons);
  if (!what) return;
  if (!form.any()) return note(t("flats-title"), t("flat-none"));
  const indices = what === "one" ? await entriesOf(sel) : [];
  if (what === "one" && !indices.length) return note(t("flats-title"), t("ext-empty-folder"));
  const base = S.info.file_name.replace(/\.[^.]+$/, "");
  const dst = await T.dialog.save({ title: t("flats-save-as"), defaultPath: `${base}-flat.grf`, filters: [{ name: "GRF archives", extensions: ["grf"] }] });
  if (!dst) return;
  const job = showJob(t("flats-running"));
  try {
    const r = await invoke("flat_maps", { dst, indices, options: form.value() });
    job.close();
    const lines = [el("p", { text: t("flats-summary", { changed: r.changed, maps: r.maps }) })];
    if (r.problems.length) lines.push(el("p", { text: t("flats-problems", { n: r.problems.length }) }), el("ul", { class: "pending" }, ...r.problems.slice(0, 30).map((m) => el("li", { text: m }))));
    if (!r.written) {
      setStatus(t("flats-nothing"));
      return ask(t("flats-nothing"), lines, [{ label: "OK", value: true, primary: true }], "info");
    }
    lines.push(el("p", { text: t("flats-written", { files: t("pvw-files", { n: r.files }), size: fmtBytes(r.file_bytes), dst }) }), el("p", { class: "muted", text: t("flats-how") }));
    setStatus(t("flats-status", { changed: r.changed, dst }));
    const open = await ask(t("flats-done"), lines, [{ label: t("dlg-open-it"), value: true, primary: true }, { label: "Close", value: false }], r.problems.length ? "warning" : "success");
    if (open) await openPath(dst, true);
  } catch (e) {
    job.close();
    const cancelled = /cancel/i.test(plainErr(e));
    setStatus(cancelled ? t("flats-cancelled") : t("flat-failed"));
    await note(cancelled ? t("flats-cancelled") : t("flat-failed"), cancelled ? t("nothing-written") : errText(e), !cancelled);
  }
}

const MAP_PARTS = () => ["models", "water", "sounds", "minimap", "effects"].map((k) => [k, t(`em-part-${k}`)]);

/** Writes the map of `row` (RSW, GAT or GND) with its files into a folder; `shown` are the switches of the 3D view. */
async function doExtractMap(row, shown = {}) {
  const boxes = Object.fromEntries(MAP_PARTS().map(([k]) => [k, el("input", { type: "checkbox", checked: shown[k] ?? true })]));
  const preset = flatPreset(shown);
  delete preset.remove_models;
  delete preset.remove_water;
  const form = flatForm(preset);
  const viewOff = Object.values(shown).some((v) => v === false);
  const strip = el("input", { type: "checkbox", checked: viewOff });
  const change = el(
    "details",
    { class: "flatbox", open: viewOff },
    el("summary", { text: t("em-change") }),
    el("label", {}, strip, ` ${t("em-strip")}`),
    form.node,
    el("p", { class: "muted", text: t("flat-notes") }),
  );
  const go = await ask(
    t("em-title"),
    [
      el("p", { text: t("em-intro") }),
      el("div", { class: "form" }, ...MAP_PARTS().map(([k, t]) => el("label", {}, boxes[k], ` ${t}`))),
      change,
    ],
    [{ label: t("em-choose-folder"), value: true, primary: true }, { label: "Cancel", value: null }],
  );
  if (!go) return;
  const skip = MAP_PARTS().map(([k]) => k).filter((k) => !boxes[k].checked);
  const flat = form.value();
  if (strip.checked) {
    flat.remove_models ||= skip.includes("models");
    flat.remove_sounds = skip.includes("sounds");
    flat.remove_effects = skip.includes("effects");
    flat.remove_water ||= skip.includes("water");
  }
  const dest = await T.dialog.open({ directory: true, multiple: false, title: t("em-dest") });
  if (!dest) return;
  setStatus(t("em-running"));
  try {
    let r;
    try {
      r = await invoke("extract_map", { index: row.index, dest, rename: false, skip, flat });
    } catch (e) {
      if (!/exists/i.test(plainErr(e))) throw e;
      const go = await ask(t("em-exists-title"), el("p", { text: t("em-exists-body", { error: errText(e) }) }), [{ label: t("em-keep-both"), value: true, primary: true }, { label: "Cancel", value: null }]);
      if (!go) return setStatus(t("em-nothing"));
      r = await invoke("extract_map", { index: row.index, dest, rename: true, skip, flat });
    }
    setStatus(t("em-status", { map: r.map, files: t("pvw-files", { n: r.files }), dest: r.dest }));
    const body = [el("p", { text: `${t("em-body", { files: t("pvw-files", { n: r.files }), map: r.map, dest: r.dest })}${r.skipped ? ` ${t("em-skipped", { files: t("pvw-files", { n: r.skipped }), parts: skip.map((k) => t(`em-part-${k}`)).join(", ") })}` : ""}${r.changed ? ` ${t("em-changed", { files: t("pvw-files", { n: r.changed }) })}` : ""}` })];
    if (r.missing.length) body.push(el("p", { text: t("em-missing", { files: t("pvw-files", { n: r.missing.length }) }) }), el("ul", { class: "pending" }, ...r.missing.slice(0, 50).map((m) => el("li", { text: m }))));
    if (r.problems.length) body.push(el("p", { class: "error", text: r.problems.slice(0, 10).join("\n") }));
    if (r.damaged) body.push(el("p", { class: "error", text: t("em-damaged", { files: t("pvw-files", { n: r.damaged }) }) }));
    await ask(r.missing.length || r.problems.length ? t("em-done-gaps") : t("em-done"), body, [{ label: "OK", value: true, primary: true }], r.missing.length || r.problems.length ? "warning" : "success");
  } catch (e) {
    setStatus(t("em-failed"));
    await note(t("em-failed"), errText(e), true);
  }
}

const LAST_EXTRACT_KEY = "rustygrf-last-extract";

/** The folder of the open archive and the separator its path uses, or null for a new archive. */
function archiveFolder() {
  const p = S.untitled ? "" : S.info?.path || "";
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return i > 0 ? { dir: p.slice(0, i), sep: p[i] } : null;
}

/**
 * Where to extract to, by the setting "Extract to": a folder named after the archive next to it
 * (`data-extracted`, the archive's paths kept), the archive's own folder (the client), a fixed
 * folder, or a question.
 */
async function pickExtractDest() {
  const at = archiveFolder();
  if (SET.extractMode === "fixed" && SET.extractTo) return SET.extractTo;
  if (at && SET.extractMode === "beside") return `${at.dir}${at.sep}${S.info.file_name.replace(/\.[^.]+$/, "")}-extracted`;
  if (at && SET.extractMode === "client") return at.dir;
  const dest = await T.dialog.open({ directory: true, multiple: false, title: t("ext-dest"), defaultPath: at?.dir });
  return Array.isArray(dest) ? dest[0] : dest;
}

/** Cmd/Ctrl+E: the selected file or folder goes straight to the usual place, without the question what. */
async function quickExtract() {
  if (!S.info) return;
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  if (!sel) return doExtract();
  const indices = await entriesOf(sel);
  if (!indices.length) return note(t("ext-title"), t("ext-empty-folder"));
  const dest = await pickExtractDest();
  if (dest) return extractIndices(indices, dest);
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
  const dest = await pickExtractDest();
  if (!dest) return;
  const indices = what === "one" ? await entriesOf(sel) : [];
  if (what === "one" && !indices.length) return note(t("ext-title"), t("ext-empty-folder"));
  return extractIndices(indices, dest);
}

/** Writes entries below a folder; when files exist there, offers to keep both. */
async function extractIndices(indices, dest) {
  localStorage.setItem(LAST_EXTRACT_KEY, dest);
  setStatus(t("ext-running"));
  let rename = false;
  for (;;) {
    try {
      const r = await invoke("extract", { indices, dest, rename, decompileLub: !!SET.decompileLub });
      setStatus(t("ext-done-status", { files: r.files, dest: r.dest }));
      if (SET.afterExtract === "reveal") invoke("reveal_path", { path: r.reveal || r.dest }).catch(() => {});
      else if (SET.afterExtract === "open") invoke("open_folder", { path: r.dest }).catch(() => {});
      // the file manager shows the result; the status line says where it went
      if (SET.afterExtract !== "nothing" && !r.damaged) return;
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
        HISTORY = { undo: h.undo || [], redo: h.redo || [] };
        for (const id of ["btn-undo", "act-undo"]) if ($(id)) $(id).disabled = !h.can_undo;
        for (const id of ["btn-redo", "act-redo"]) if ($(id)) $(id).disabled = !h.can_redo;
        $("btn-undo").title = HISTORY.undo.length ? `${t("btn-undo")}: ${stepLabel(HISTORY.undo[0])} (Cmd/Ctrl+Z)` : `${t("btn-undo")} (Cmd/Ctrl+Z)`;
        $("btn-redo").title = HISTORY.redo.length ? `${t("btn-redo")}: ${stepLabel(HISTORY.redo[0])} (Cmd/Ctrl+Shift+Z)` : `${t("btn-redo")} (Cmd/Ctrl+Shift+Z)`;
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
  invoke("tab_set_info", { title, dirty: edited }).catch(() => {});
}

function setPending(rows) {
  THUMB.reset();
  renewAssets();
  S.changes = rows;
  S.changed = new Map(rows.map((r) => [changeKey(r.name), r.kind]));
  updateTitle();
  updateEditBar();
  render();
}

/** A step of the history in words ("Replaced x.spr", "Removed 12 files"). */
function stepLabel(st) {
  if (!st || st.kind === "none") return "";
  return st.n > 1 ? t(`hist-${st.kind}-n`, { n: st.n }) : t(`hist-${st.kind}`, { name: baseOf(st.name) });
}

/** The last answer of `history`, for tooltips and the history list. */
let HISTORY = { undo: [], redo: [] };

/**
 * Cmd/Ctrl+Z and Shift+Z (and Ctrl+Y) from menu or keyboard: a text field undoes its typing, an
 * open pixel editor its strokes, otherwise the archive's changes step back.
 */
function editHistory(redo) {
  const a = document.activeElement;
  const typing = a && (a.isContentEditable || a instanceof HTMLTextAreaElement || (a instanceof HTMLInputElement && !["checkbox", "radio", "button", "color", "range", "file"].includes(a.type)));
  if (typing) {
    document.execCommand(redo ? "redo" : "undo");
    return;
  }
  if (S.undoHandler) return redo ? S.undoHandler.redo?.() : S.undoHandler.undo?.();
  const dlg = document.querySelector("dialog[open]");
  if (dlg && dlg.id !== "act-dlg") return;
  if (S.info) doHistory(redo);
}

/** Several steps at once, from the history list. */
async function jumpHistory(steps, redo) {
  try {
    setPending(await invoke("step_history", { steps, redo }));
    setStatus(redo ? t("history-redone-n", { n: steps }) : t("history-undone-n", { n: steps }));
    if ($("act-dlg").open) await loadActFrame();
    refreshPreview($("act-dlg").open ? E.action : undefined);
  } catch (e) {
    await note(t("history-failed"), errText(e), true);
  }
}

/** The list of steps under the undo or redo button; a click goes back (or forward) to that step. */
function showHistoryMenu(anchor, redo) {
  document.querySelector(".popmenu")?.remove();
  const steps = redo ? HISTORY.redo : HISTORY.undo;
  if (!steps.length) return;
  const menu = el("div", { class: "popmenu", role: "menu", "aria-label": redo ? t("btn-redo") : t("btn-undo") },
    ...steps.map((st, i) => el("button", { type: "button", role: "menuitem", text: stepLabel(st), onclick: () => { close(); jumpHistory(i + 1, redo); } })));
  const r = anchor.getBoundingClientRect();
  menu.style.left = `${Math.min(r.left, window.innerWidth - 300)}px`;
  menu.style.top = `${r.bottom + 4}px`;
  document.body.append(menu);
  const close = () => {
    menu.remove();
    document.removeEventListener("pointerdown", outside, true);
    document.removeEventListener("keydown", esc, true);
  };
  const outside = (e) => !menu.contains(e.target) && close();
  const esc = (e) => e.key === "Escape" && (e.preventDefault(), close(), anchor.focus());
  document.addEventListener("pointerdown", outside, true);
  document.addEventListener("keydown", esc, true);
  menu.querySelector("button")?.focus();
}

/** Edit › Change History: every step that can be undone or redone, in one list. */
async function openHistory() {
  if (!S.info) return;
  const row = (st, i, redo) => el("li", {}, el("button", { type: "button", class: "linklike", text: stepLabel(st), onclick: () => { $("dlg").close(); jumpHistory(i + 1, redo); } }));
  const body = [];
  if (HISTORY.redo.length) body.push(el("h4", { text: t("hist-redo-head") }), el("ol", { class: "history" }, ...HISTORY.redo.map((st, i) => row(st, i, true)).reverse()));
  body.push(el("h4", { text: t("hist-now") }));
  if (HISTORY.undo.length) body.push(el("h4", { text: t("hist-undo-head") }), el("ol", { class: "history" }, ...HISTORY.undo.map((st, i) => row(st, i, false))));
  else body.push(el("p", { class: "muted", text: t("hist-empty") }));
  body.push(el("p", { class: "hint", text: t("hist-hint") }));
  await ask(t("hist-title"), body, [{ label: "Close", value: null }], "info");
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

/** ACT Editor's range commands: one or several PNGs replace the images from `frame` on, or go in before or after it. */
async function replaceImage(row, frame, mode = "replace") {
  const picked = await T.dialog.open({ multiple: true, directory: false, title: t(`spr-${mode}-pick`, { frame, name: row.name }), filters: [{ name: t("spr-png-filter"), extensions: ["png"] }] });
  if (!picked) return;
  const pngs = Array.isArray(picked) ? picked : [picked];
  try {
    setPending(await invoke("stage_spr_images", { index: row.index, frame, pngs, mode }));
    setStatus(t(`spr-${mode}-status`, { frame, name: row.name, n: pngs.length }));
    refreshPreview();
  } catch (e) {
    await note(t(`spr-${mode}-failed`), errText(e), true);
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
  // the menu's Undo reaches the strokes while the editor is open
  S.undoHandler = { undo: () => undoBtn.click() };
  const go = await finish().finally(() => (S.undoHandler = null));
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
  const img = el("img", { src: dataUrl(f.mime || "image/png", f.png), alt: noteText(f.label) });
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
// what the frame view shows and how it plays; ACT Editor's View, Anchors and settings options
const ACT_VIEW_KEY = "rustygrf.actView";
const ACT_VIEW_DEFAULT = { anchors: true, lineH: true, lineV: true, grid: false, pixelated: true, adjacent: false, onion: false, base: false, keepLayer: true, speed: 1, accurate: false };
const E = { row: null, action: 0, frame: 0, view: null, busy: false, playing: false, timer: 0, sounds: new Map(), refs: loadActRefs(), layer: null, opt: loadActView(), optOpen: false };

function loadActView() {
  try {
    const r = JSON.parse(localStorage.getItem(ACT_VIEW_KEY) || "{}");
    const o = { ...ACT_VIEW_DEFAULT };
    for (const k of Object.keys(o)) if (typeof r?.[k] === typeof o[k]) o[k] = r[k];
    if (![0.25, 0.5, 1, 2].includes(o.speed)) o.speed = 1;
    return o;
  } catch {
    return { ...ACT_VIEW_DEFAULT };
  }
}
function saveActView() {
  try {
    localStorage.setItem(ACT_VIEW_KEY, JSON.stringify(E.opt));
  } catch {}
}

function loadActRefs() {
  try {
    const r = JSON.parse(localStorage.getItem(ACT_REFS_KEY) || "[]");
    return Array.isArray(r) ? r.filter((x) => typeof x?.path === "string").map((x) => ({ path: x.path, under: !!x.under, hidden: !!x.hidden })) : [];
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
      { class: r.hidden ? "refchip hiddenref" : "refchip", title: r.path },
      el("span", { class: "name", text: r.path.split("\\").pop() }),
      el("button", { type: "button", class: "small", text: r.hidden ? t("ref-show") : t("ref-hide"), title: t("ref-visible-title"), "aria-pressed": String(!r.hidden), onclick: () => { E.refs[i] = { ...r, hidden: !r.hidden }; saveActRefs(); loadActFrame(); } }),
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

/** The references that are shown, as the backend takes them ("Use body as base" turns the anchors round). */
const shownRefs = () => E.refs.filter((r) => !r.hidden).map((r) => ({ path: r.path, under: r.under, base: E.opt.base }));

async function loadActFrame() {
  try {
    const before = E.view;
    E.view = await invoke("act_frame", { index: E.row.index, action: E.action, frame: E.frame, references: shownRefs(), adjacent: E.opt.adjacent || E.opt.onion });
    if (!E.opt.keepLayer && before && (before.action !== E.view.action || before.frame !== E.view.frame)) E.layer = null;
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
const actPlayDelay = () => Math.max(16, (E.view.interval_ms * (E.opt.accurate ? 24 / 25 : 1)) / E.opt.speed);

async function actPlayStep() {
  if (!E.playing || !$("act-dlg").open) return stopActPlay();
  const started = performance.now();
  E.frame = (E.frame + 1) % E.view.frames;
  await loadActFrame();
  if (!E.playing) return;
  playActSound(E.view.sound);
  E.timer = setTimeout(actPlayStep, Math.max(16, actPlayDelay() - (performance.now() - started)));
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
  E.timer = setTimeout(actPlayStep, actPlayDelay());
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

/** ACT Editor's "Set anchors from file" and "Adjust anchors from file" (the latter moves the layers along). */
async function actAnchorsFromFile() {
  const mode = el("select", { class: "field" }, el("option", { value: "set", text: t("act-anchors-set") }), el("option", { value: "adjust", text: t("act-anchors-adjust") }));
  const go = await ask(t("act-anchors-title"), [el("p", { text: t("act-anchors-intro") }), mode], [{ label: t("act-anchors-pick"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  const path = await T.dialog.open({ multiple: false, filters: [{ name: "ACT", extensions: ["act"] }] });
  if (!path) return;
  if (E.busy) return;
  E.busy = true;
  stopActPlay();
  try {
    setPending(await invoke("stage_act_anchors", { index: E.row.index, path: Array.isArray(path) ? path[0] : path, adjust: mode.value === "adjust" }));
  } catch (e) {
    await note(t("act-anchors-title"), errText(e), true);
  }
  E.busy = false;
  await loadActFrame();
}


// ---------- ACT Editor's scripts ----------

const formRow = (text, input) => el("p", {}, el("label", {}, `${text} `, input));
const numberField = (value, opts = {}) => el("input", { type: "number", class: "narrow", value: String(value), step: opts.step ?? "1", min: opts.min, max: opts.max });
const colourValue = (input) => [parseInt(input.value.slice(1, 3), 16), parseInt(input.value.slice(3, 5), 16), parseInt(input.value.slice(5, 7), 16)];

/** Runs a script edit; the message of an error is shown in a dialog (the frame editor stays as it is). */
async function actScript(title, edit) {
  if (E.busy) return;
  stopActPlay();
  E.busy = true;
  try {
    setPending(await invoke("stage_act_edit", { index: E.row.index, action: E.action, frame: E.frame, edit }));
    if (edit.op === "action_order") Object.assign(E, { action: Math.min(E.action, edit.order.length - 1), frame: 0 });
    if (edit.op === "frame_order") E.frame = Math.min(E.frame, edit.order.length - 1);
  } catch (e) {
    E.busy = false;
    await note(title, errText(e), true);
    return loadActFrame();
  }
  E.busy = false;
  await loadActFrame();
}

/** A list of frames or actions to rearrange; resolves to the new order (old numbers, -1 = a new empty one). */
async function orderEditor(title, intro, count, itemLabel) {
  let order = Array.from({ length: count }, (_, i) => i);
  const list = el("div", { class: "orderlist", style: "max-height:45vh;overflow:auto" });
  const draw = () =>
    list.replaceChildren(
      ...order.map((o, i) => {
        const b = (text, tip, fn, off) => el("button", { type: "button", class: "small", text, title: tip, "aria-label": tip, disabled: !!off, onclick: fn });
        const swap = (j) => {
          [order[i], order[j]] = [order[j], order[i]];
          draw();
        };
        return el(
          "div",
          { class: "bar" },
          el("span", { class: o < 0 ? "muted" : "", style: "min-width:8em", text: o < 0 ? t("as-order-new") : itemLabel(o) }),
          b("↑", t("as-order-up"), () => swap(i - 1), i === 0),
          b("↓", t("as-order-down"), () => swap(i + 1), i === order.length - 1),
          b("⧉", t("as-order-copy"), () => (order.splice(i + 1, 0, o), draw())),
          b("✕", t("as-order-remove"), () => (order.splice(i, 1), draw()), order.length <= 1),
        );
      }),
    );
  draw();
  const add = el("button", { type: "button", class: "small", text: t("as-order-add"), onclick: () => (order.push(-1), draw()) });
  const go = await ask(title, [el("p", { text: intro }), list, add], [{ label: t("as-apply"), value: true, primary: true }, { label: "Cancel", value: null }]);
  return go ? order : null;
}

async function actScriptDuplicate() {
  const times = numberField(1, { min: "1", max: "1000" });
  const go = await ask(t("as-dup-title"), [el("p", { text: t("as-dup-intro", { action: E.action }) }), formRow(t("as-times"), times)], [{ label: t("as-apply"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (go) await actScript(t("as-dup-title"), { op: "duplicate_frames", times: Number(times.value) });
}

async function actScriptAddSprite(v) {
  const groups = el("input", { class: "field", value: `0-${Math.max(0, Math.floor((v.actions - 1) / 8))}` });
  const index = numberField(0, { min: "0" });
  const type = el("select", {}, el("option", { value: 0, text: t("act-type-palette", { n: v.indexed_images }) }), el("option", { value: 1, text: t("act-type-rgba", { n: v.rgba_images }) }));
  const ox = numberField(0);
  const oy = numberField(0);
  const scale = numberField(1, { step: "0.1" });
  const colour = el("input", { type: "color", value: "#ffffff" });
  const alpha = numberField(255, { min: "0", max: "255" });
  const front = el("select", {}, el("option", { value: "1", text: t("as-front") }), el("option", { value: "0", text: t("as-back") }));
  const go = await ask(
    t("as-addspr-title"),
    [el("p", { text: t("as-addspr-intro") }), formRow(t("as-animations"), groups), formRow(t("as-sprite-index"), index), formRow(t("as-sprite-type"), type), formRow("x", ox), formRow("y", oy), formRow(t("as-scale"), scale), formRow(t("as-colour"), colour), formRow("α", alpha), formRow(t("as-position"), front)],
    [{ label: t("as-apply"), value: true, primary: true }, { label: "Cancel", value: null }],
  );
  if (!go) return;
  await actScript(t("as-addspr-title"), { op: "add_sprite_to_frames", animations: groups.value, sprite_index: Number(index.value), sprite_type: Number(type.value), offset_x: Number(ox.value), offset_y: Number(oy.value), scale: Number(scale.value), color: [...colourValue(colour), Number(alpha.value)], front: front.value === "1" });
}

async function actScriptEffect(kind, v) {
  const title = t(`as-fx-${kind}`);
  const groups = el("input", { class: "field", value: String(Math.floor(E.action / 8)) });
  const layers = el("input", { class: "field", value: "", placeholder: "0" });
  const start = numberField(0, { min: "0" });
  const length = numberField(0, { min: "0" });
  const ease = numberField(0, { min: "-100", max: "100" });
  const loop = el("input", { type: "checkbox", checked: true });
  const empty = el("input", { type: "checkbox" });
  const colour = el("input", { type: "color", value: kind === "fade" ? "#000000" : "#ff0000" });
  const alpha = numberField(255, { min: "0", max: "255" });
  const sx = numberField(1.2, { step: "0.05" });
  const sy = numberField(1.2, { step: "0.05" });
  const body = [el("p", { text: t(`as-fx-${kind}-intro`) }), formRow(t("as-animations"), groups), formRow(t("as-layers"), layers), formRow(t("as-start"), start), formRow(t("as-length"), length), formRow(t("as-ease"), ease)];
  if (kind === "breathing") body.push(formRow("x", sx), formRow("y", sy));
  else body.push(formRow(t("as-colour"), colour), formRow("α", alpha));
  body.push(el("label", { class: "check" }, loop, ` ${t("as-loop")}`), el("br"), el("label", { class: "check" }, empty, ` ${t("as-add-empty")}`));
  const go = await ask(title, body, [{ label: t("as-apply"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  await actScript(title, { op: "effect", kind, animations: groups.value, layers: layers.value, start: Number(start.value), length: Number(length.value), ease: Number(ease.value), loop_missing: loop.checked, add_empty: empty.checked, color: [...colourValue(colour), Number(alpha.value)], scale_x: Number(sx.value), scale_y: Number(sy.value) });
}

/** The effects that make new pictures: the animation and the sprite next to it are staged together. */
async function actScriptImageFx(kind) {
  const title = t(`as-fx-${kind}`);
  const groups = el("input", { class: "field", value: String(Math.floor(E.action / 8)) });
  const layers = el("input", { class: "field", value: "", placeholder: "0" });
  const start = numberField(0, { min: "0" });
  const length = numberField(0, { min: "0" });
  const loop = el("input", { type: "checkbox", checked: true });
  const seed = numberField(1234, { min: "0" });
  const body = [el("p", { text: t(`as-fx-${kind}-intro`) }), formRow(t("as-animations"), groups), formRow(t("as-layers"), layers), formRow(t("as-start"), start), formRow(t("as-length"), length)];
  let fx;
  if (kind === "additive") {
    const colour = el("input", { type: "color", value: "#ff0000" });
    const alpha = numberField(255, { min: "0", max: "255" });
    const ease = numberField(0, { min: "-100", max: "100" });
    body.push(formRow(t("as-colour"), colour), formRow("α", alpha), formRow(t("as-ease"), ease));
    fx = () => ({ kind, color: [...colourValue(colour), Number(alpha.value)], ease: Number(ease.value) });
  } else if (kind === "cut") {
    const sx = numberField(0);
    const sy = numberField(0);
    const angle = numberField(0);
    const half = numberField(24);
    body.push(formRow(t("as-cut-source") + " x", sx), formRow(t("as-cut-source") + " y", sy), formRow(t("as-cut-angle"), angle), formRow(t("as-cut-half"), half), formRow(t("as-seed"), seed));
    fx = () => ({ kind, source_x: Number(sx.value), source_y: Number(sy.value), angle: Number(angle.value), half_angle: Number(half.value), seed: Number(seed.value) });
  } else {
    const min = numberField(0, { min: "0", max: "50" });
    const max = numberField(4, { min: "1", max: "50" });
    const dead = numberField(0.5, { step: "0.05", min: "0", max: "1" });
    const colour = el("input", { type: "color", value: "#fff0d1" });
    const alpha = numberField(220, { min: "0", max: "255" });
    body.push(formRow(t("as-outline-min"), min), formRow(t("as-outline-max"), max), formRow(t("as-outline-dead"), dead), formRow(t("as-colour"), colour), formRow("α", alpha), formRow(t("as-seed"), seed));
    fx = () => ({ kind, min_height: Number(min.value), max_height: Number(max.value), dead_rate: Number(dead.value), color: [...colourValue(colour), Number(alpha.value)], seed: Number(seed.value) });
  }
  body.push(el("label", { class: "check" }, loop, ` ${t("as-loop")}`));
  const go = await ask(title, body, [{ label: t("as-apply"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go || E.busy) return;
  stopActPlay();
  E.busy = true;
  try {
    const scope = { animations: groups.value, layers: layers.value, start: Number(start.value), length: Number(length.value), loop_missing: loop.checked };
    setPending(await invoke("stage_act_image_fx", { index: E.row.index, fx: fx(), scope }));
  } catch (e) {
    E.busy = false;
    await note(title, errText(e), true);
    return loadActFrame();
  }
  E.busy = false;
  await loadActFrame();
}

const SETUP_REF_KEY = "rustygrf-setup-reference";

/**
 * ACT Editor's "Setup Headgear/Head/Garment": every image of the reference (a head, or the body
 * for a garment) gets the image of this animation that belongs to it, and how far it lies from it;
 * the whole animation (all directions, frames and mirrored copies) is then built from that.
 */
async function actSetup(kind) {
  if (E.busy) return;
  stopActPlay();
  const garment = kind === "garment";
  const title = t(`as-setup-${kind}`);
  const female = /[\\/]여[_\\/]/.test(E.row.name);
  const refKey = garment ? `${SETUP_REF_KEY}-garment` : SETUP_REF_KEY;
  const last = localStorage.getItem(refKey);
  const ref = el("input", { type: "text", class: "field", list: "setup-presets", spellcheck: "false", value: last || REF_PRESETS()[(garment ? 0 : 2) + (female ? 1 : 0)][0] });
  const presets = el("datalist", { id: "setup-presets" }, (garment ? REF_PRESETS().slice(0, 2) : REF_PRESETS().slice(2)).map(([v, label]) => el("option", { value: v, label })));
  const table = el("div", { style: "max-height:50vh;overflow:auto" });
  const problem = el("p", { class: "error", role: "alert" });
  let info = null;
  const picture = (png, alt) => (png ? el("img", { src: dataUrl("image/png", png), alt, style: "image-rendering:pixelated;max-width:64px;max-height:64px;vertical-align:middle;background:var(--checker, #888)" }) : el("span", { class: "muted", text: "–" }));
  const rows = [];
  const draw = () => {
    rows.length = 0;
    const total = info.images.length + info.more_images;
    table.replaceChildren(
      el(
        "table",
        {},
        el("thead", {}, el("tr", {}, ...[garment ? "as-setup-th-ref-body" : "as-setup-th-ref", "as-setup-th-image", "", "x", "y"].map((k) => el("th", { text: k ? (k.length > 1 ? t(k) : k) : "" })))),
        el(
          "tbody",
          {},
          ...info.rows.map((r) => {
            const sel = el("select", {}, el("option", { value: "-1", text: t("as-setup-none"), selected: r.image < 0 }), ...Array.from({ length: total }, (_, i) => el("option", { value: String(i), text: String(i), selected: i === r.image })));
            const dx = numberField(r.dx);
            const dy = numberField(r.dy);
            const shown = el("span", {});
            const show = () => shown.replaceChildren(Number(sel.value) >= 0 ? picture(info.images[Number(sel.value)], "") : "");
            sel.addEventListener("change", show);
            show();
            rows.push({ sel, dx, dy });
            return el(
              "tr",
              {},
              el("td", {}, picture(r.reference_png, `${r.reference_image}`), " ", el("span", { class: "muted", text: r.usage ? t("as-setup-used", { n: r.reference_image, action: r.usage[0], frame: r.usage[1] + 1 }) : t("as-setup-unused", { n: r.reference_image }) })),
              el("td", {}, sel),
              el("td", {}, shown),
              el("td", {}, dx),
              el("td", {}, dy),
            );
          }),
        ),
      ),
    );
  };
  const load = async () => {
    problem.textContent = "";
    try {
      info = await invoke("act_setup_info", { index: E.row.index, reference: ref.value, garment });
      localStorage.setItem(refKey, ref.value.trim());
      draw();
    } catch (e) {
      info = null;
      table.replaceChildren();
      problem.textContent = errText(e);
    }
  };
  const loadBtn = el("button", { type: "button", class: "small", text: t("as-setup-load"), onclick: load });
  const body = [el("p", { text: t(`as-setup-${kind}-intro`) }), formRow(t(garment ? "as-setup-ref-body" : "as-setup-ref"), ref), presets, loadBtn, problem, table, el("p", { class: "muted", text: t("as-setup-note") })];
  const go = ask(title, body, [{ label: t("as-apply"), value: true, primary: true }, { label: "Cancel", value: null }]);
  load();
  if (!(await go) || !info) return;
  E.busy = true;
  try {
    const choices = rows.map((r, i) => ({ image: Number(r.sel.value), dx: Math.round(Number(r.dx.value) || 0), dy: Math.round(Number(r.dy.value) || 0), reference_image: info.rows[i].reference_image }));
    setPending(await invoke("stage_act_setup", { index: E.row.index, reference: ref.value, kind, choices }));
  } catch (e) {
    E.busy = false;
    await note(title, errText(e), true);
    return loadActFrame();
  }
  E.busy = false;
  await loadActFrame();
}

const SAVE_GARMENT_KEY = "rustygrf-save-garment";

/**
 * ACT Editor's "Save as garment": this animation becomes the animations of a garment (cape, wings)
 * for every job and both genders, as new entries of the archive. Optionally the animations of the
 * kRO garment with the closest sprite are taken and fitted to this sprite.
 */
async function actSaveGarment() {
  if (E.busy) return;
  stopActPlay();
  let saved = {};
  try {
    saved = JSON.parse(localStorage.getItem(SAVE_GARMENT_KEY) || "{}") || {};
  } catch {}
  const base = el("input", { type: "text", class: "field", value: saved.base || "custom_wing", spellcheck: "false", "aria-label": t("sg-name") });
  const folder = el("input", { type: "text", class: "field", value: saved.folder || "data\\sprite\\로브\\", spellcheck: "false", "aria-label": t("sg-folder") });
  const dx = numberField(saved.dx ?? 0);
  const dy = numberField(saved.dy ?? 0);
  const copy = el("input", { type: "checkbox", checked: !!saved.copy_spr });
  const guess = el("input", { type: "checkbox", checked: saved.guess !== false });
  const robe = el("select", { class: "field", "aria-label": t("sg-robe") }, el("option", { value: "", text: t("sg-robe-best") }));
  const hint = el("p", { class: "muted", text: t("pvw-loading") });
  const jobs = el("textarea", { class: "field", rows: "6", style: "width:100%;font-family:monospace", spellcheck: "false", "aria-label": t("sg-jobs") });
  jobs.value = await invoke("act_garment_jobs");
  const sync = () => (robe.disabled = !guess.checked);
  guess.addEventListener("change", sync);
  const row = (input, text) => el("label", { class: "check" }, input, ` ${text}`);
  const go = ask(
    t("sg-title"),
    [
      el("p", { text: t("sg-intro") }),
      formRow(t("sg-name"), base),
      formRow(t("sg-folder"), folder),
      el("p", {}, el("label", {}, `${t("sg-dx")} `, dx), " ", el("label", {}, `${t("sg-dy")} `, dy)),
      row(copy, t("sg-copy-spr")),
      row(guess, t("sg-guess")),
      formRow(t("sg-robe"), robe),
      hint,
      el("details", {}, el("summary", { text: t("sg-jobs") }), el("p", { class: "muted", text: t("sg-jobs-note") }), jobs),
    ],
    [{ label: t("sg-save"), value: true, primary: true }, { label: "Cancel", value: null }],
  );
  invoke("act_garment_matches", { index: E.row.index })
    .then((list) => {
      robe.append(...list.map((m) => el("option", { value: m.name, text: `${m.name} (${(m.ratio * 100).toFixed(1)} %)` })));
      hint.textContent = list.length ? t("sg-matches", { n: list.length }) : t("sg-no-matches");
      if (!list.length) {
        guess.checked = false;
        guess.disabled = true;
      }
      sync();
    })
    .catch((e) => (hint.textContent = errText(e)));
  if (!(await go)) return;
  const spec = { base: base.value, folder: folder.value, dx: Math.round(Number(dx.value) || 0), dy: Math.round(Number(dy.value) || 0), copy_spr: copy.checked, guess: guess.checked, robe: guess.checked && robe.value ? robe.value : null, jobs: jobs.value };
  E.busy = true;
  try {
    const [pending, report] = await invoke("stage_save_garment", { index: E.row.index, spec });
    setPending(pending);
    setStatus(t("sg-status", { n: report.files, name: spec.base }));
    localStorage.setItem(SAVE_GARMENT_KEY, JSON.stringify({ base: spec.base, folder: spec.folder, dx: spec.dx, dy: spec.dy, copy_spr: spec.copy_spr, guess: spec.guess }));
    if (report.robe) await note(t("sg-title"), t("sg-done-guess", { n: report.files, robe: report.robe, ratio: (report.ratio * 100).toFixed(1) }));
  } catch (e) {
    E.busy = false;
    await note(t("sg-title"), errText(e), true);
    return;
  }
  E.busy = false;
}

async function actDefaultAnchors(female) {
  if (E.busy) return;
  stopActPlay();
  E.busy = true;
  try {
    setPending(await invoke("stage_act_default_anchors", { index: E.row.index, female }));
  } catch (e) {
    await note(t("act-anchors-title"), errText(e), true);
  }
  E.busy = false;
  await loadActFrame();
}

function actScriptsMenu(ev, v) {
  const r = ev.currentTarget.getBoundingClientRect();
  const rot = (kind) => () => actScript(t("as-rot-title"), { op: "rotation_copy", kind });
  showContextMenu(
    r.left,
    r.bottom,
    [
      { label: t("as-edit-frames"), run: async () => {
        const order = await orderEditor(t("as-edit-frames"), t("as-order-intro"), v.frames, (n) => t("act-frame-label", { n: n + 1, total: v.frames }));
        if (order) await actScript(t("as-edit-frames"), { op: "frame_order", order });
      } },
      { label: t("as-edit-actions"), run: async () => {
        const order = await orderEditor(t("as-edit-actions"), t("as-order-intro"), v.actions, (n) => t("pvw-action-n", { n }));
        if (order) await actScript(t("as-edit-actions"), { op: "action_order", order });
      } },
      { label: t("as-dup"), run: actScriptDuplicate },
      { label: t("as-addspr"), run: () => actScriptAddSprite(v) },
      "-",
      { label: t("as-rot-b-br"), run: rot("b_br") },
      { label: t("as-rot-br-b"), run: rot("br_b") },
      { label: t("as-rot-bl-b"), run: rot("bl_b") },
      { label: t("as-rot-b-bl"), run: rot("b_bl") },
      "-",
      { label: t("as-fx-fade"), run: () => actScriptEffect("fade", v) },
      { label: t("as-fx-breathing"), run: () => actScriptEffect("breathing", v) },
      { label: t("as-fx-overlay"), run: () => actScriptEffect("overlay", v) },
      { label: t("as-fx-additive"), run: () => actScriptImageFx("additive") },
      { label: t("as-fx-cut"), run: () => actScriptImageFx("cut") },
      { label: t("as-fx-outline"), run: () => actScriptImageFx("outline") },
      "-",
      { label: t("as-setup-headgear"), run: () => actSetup("headgear") },
      { label: t("as-setup-head"), run: () => actSetup("head") },
      { label: t("as-setup-garment"), run: () => actSetup("garment") },
      { label: t("as-save-garment"), run: actSaveGarment },
      "-",
      { label: t("as-anchors-male"), run: () => actDefaultAnchors(false) },
      { label: t("as-anchors-female"), run: () => actDefaultAnchors(true) },
    ],
    ev.currentTarget,
    t("act-scripts"),
  );
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

const ACT_ZOOMS = [1, 2, 3, 4, 6, 8];
const actZoom = () => E.zoom || 1;

/** Zoom one step in or out (0 = back to 1:1); the zoom stays while playing and between frames. */
function setActZoom(step) {
  const i = Math.max(0, ACT_ZOOMS.indexOf(actZoom()));
  E.zoom = step === 0 ? 1 : ACT_ZOOMS[Math.max(0, Math.min(ACT_ZOOMS.length - 1, i + step))];
  drawActEditor();
}

/** The box around the selected layer, in screen pixels at the current zoom. */
function layerBox(b) {
  const z = actZoom();
  return el("div", { class: "layerbox", style: `left:${b[0] * z}px;top:${b[1] * z}px;width:${(b[2] - b[0]) * z}px;height:${(b[3] - b[1]) * z}px` });
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
      if (b) stage.append(layerBox(b));
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
    // frame by frame: Page up / Page down; Space plays and stops
    if (e.key === " " || e.key === "PageUp" || e.key === "PageDown") {
      e.preventDefault();
      if (e.key === " ") return toggleActPlay();
      stopActPlay();
      E.frame = Math.max(0, Math.min(v.frames - 1, E.frame + (e.key === "PageDown" ? 1 : -1)));
      return loadActFrame();
    }
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
    el("button", { type: "button", class: "small", text: t("act-flip-frame"), title: t("act-flip-frame-title"), onclick: () => actEdit({ op: "flip" }) }),
    el("button", { type: "button", class: "small", text: t("act-flip-frame-v"), title: t("act-flip-frame-v-title"), onclick: () => actEdit({ op: "flip", up_down: true }) }),
    el("button", { type: "button", class: "small", text: t("act-flip-action"), title: t("act-flip-action-title"), onclick: () => actEdit({ op: "flip", whole_action: true }) }),
    el("button", { type: "button", class: "small", text: t("act-mirror-action"), title: t("act-mirror-action-title"), onclick: () => actEdit({ op: "mirror_action" }) }),
    el("button", { type: "button", class: "small", text: t("act-reverse"), title: t("act-reverse-title"), disabled: v.frames <= 1, onclick: () => actEdit({ op: "reverse_frames" }) }),
    el("button", { type: "button", class: "small", text: `${t("act-scripts")} ▾`, title: t("act-scripts-title"), "aria-haspopup": "menu", onclick: (e) => actScriptsMenu(e, v) }),
    el("button", { type: "button", class: "small", text: t("act-anchors-file"), title: t("act-anchors-file-title"), onclick: () => actAnchorsFromFile() }),
    el("button", { type: "button", class: "small", text: t("act-remove-action"), disabled: v.actions <= 1, onclick: () => removeAction(v.action) }),
    el("button", { type: "button", class: "small", text: t("act-remove-frame"), disabled: v.frames <= 1, onclick: () => actEdit({ op: "remove_frame" }) }),
  );
  if (E.layer != null && E.layer >= v.layers.length) E.layer = null;
  const stage = el("div", { class: "stage checker", tabindex: "0", "data-act-stage": "", role: "group", "aria-roledescription": t("act-stage-role"), "aria-label": `${t("act-stage-aria", { n: v.frame + 1 })}${E.layer != null ? ` ${t("act-stage-sel", { n: E.layer })}` : ""}` });
  if (v.png) {
    const z = actZoom();
    const smooth = E.opt.pixelated ? "" : "image-rendering:auto";
    stage.append(el("img", { src: dataUrl("image/png", v.png), width: v.width * z, height: v.height * z, alt: "", draggable: "false", style: smooth }));
    // the neighbouring frames as a faint copy over the picture (onion skin)
    if (E.opt.onion) for (const png of [v.prev_png, v.next_png]) if (png) stage.append(el("img", { class: "ghost", src: dataUrl("image/png", png), width: v.width * z, height: v.height * z, alt: "", draggable: "false", style: smooth }));
    if (E.opt.grid) {
      let step = 1;
      while (v.width / v.scale / step > 50) step *= 2;
      const cell = step * v.scale * z;
      stage.append(el("div", { class: "pixgrid", style: `background-size:${cell}px ${cell}px;background-position:${(v.origin_x * z) % cell}px ${(v.origin_y * z) % cell}px` }));
    }
    if (E.opt.lineH) stage.append(el("div", { class: "cross-h", style: `top:${v.origin_y * z}px` }));
    if (E.opt.lineV) stage.append(el("div", { class: "cross-v", style: `left:${v.origin_x * z}px` }));
    if (E.opt.anchors) v.anchors.forEach((a, i) => stage.append(el("div", { class: "dot", title: t("act-anchor-title", { n: i }), "data-anchor": String(i), style: `left:${(v.origin_x + a.x * v.scale) * z}px;top:${(v.origin_y + a.y * v.scale) * z}px` })));
    const b = E.layer != null ? v.layers[E.layer]?.bounds : null;
    if (b) stage.append(layerBox(b));
    wireStage(stage, v);
  }
  const zoomBar = el(
    "div",
    { class: "bar" },
    el("button", { type: "button", class: "small", text: "−", title: t("act-zoom-out"), "aria-label": t("act-zoom-out"), disabled: actZoom() <= ACT_ZOOMS[0], onclick: () => setActZoom(-1) }),
    el("span", { class: "muted", text: `${actZoom() * 100}%`, role: "status" }),
    el("button", { type: "button", class: "small", text: "+", title: t("act-zoom-in"), "aria-label": t("act-zoom-in"), disabled: actZoom() >= ACT_ZOOMS[ACT_ZOOMS.length - 1], onclick: () => setActZoom(1) }),
    el("button", { type: "button", class: "small", text: "1:1", title: t("act-zoom-reset"), "aria-label": t("act-zoom-reset"), disabled: actZoom() === 1, onclick: () => setActZoom(0) }),
  );
  stage.addEventListener("wheel", (e) => {
    if (!e.ctrlKey && !e.metaKey) return;
    e.preventDefault();
    setActZoom(e.deltaY < 0 ? 1 : -1);
  }, { passive: false });
  // what the picture shows and how it plays; each choice stays between sessions
  const toggle = (key, label, title, reload) =>
    el("label", { class: "check", title }, el("input", { type: "checkbox", checked: E.opt[key], onchange: (e) => { E.opt[key] = e.target.checked; saveActView(); if (reload) loadActFrame(); else drawActEditor(); } }), ` ${label}`);
  const speed = el("select", { "aria-label": t("act-opt-speed"), onchange: () => { E.opt.speed = Number(speed.value); saveActView(); if (E.playing) { clearTimeout(E.timer); E.timer = setTimeout(actPlayStep, actPlayDelay()); } } }, [0.25, 0.5, 1, 2].map((s) => el("option", { value: String(s), text: `${s}×`, selected: s === E.opt.speed })));
  const viewOpts = el(
    "details",
    { class: "viewopts", open: E.optOpen, ontoggle: (e) => (E.optOpen = e.target.open) },
    el("summary", { text: t("act-opt-heading") }),
    el(
      "div",
      { class: "bar" },
      toggle("anchors", t("act-opt-anchors"), t("act-opt-anchors-title")),
      toggle("lineH", t("act-opt-line-h"), t("act-opt-line-title")),
      toggle("lineV", t("act-opt-line-v"), t("act-opt-line-title")),
      toggle("grid", t("act-opt-grid"), t("act-opt-grid-title")),
      toggle("pixelated", t("act-opt-pixelated"), t("act-opt-pixelated-title")),
      toggle("adjacent", t("act-opt-adjacent"), t("act-opt-adjacent-title"), true),
      toggle("onion", t("act-opt-onion"), t("act-opt-onion-title"), true),
      toggle("base", t("act-opt-base"), t("act-opt-base-title"), true),
      toggle("keepLayer", t("act-opt-keep"), t("act-opt-keep-title")),
      toggle("accurate", t("act-opt-accurate"), t("act-opt-accurate-title")),
      el("label", { class: "muted", text: `${t("act-opt-speed")} ` }, speed),
    ),
  );
  const side = (png, label) => (png ? el("figure", { class: "adjframe" }, el("img", { src: dataUrl("image/png", png), width: v.width * actZoom(), height: v.height * actZoom(), alt: label, draggable: "false", style: E.opt.pixelated ? "" : "image-rendering:auto" }), el("figcaption", { class: "muted", text: label })) : null);
  const adjacent = E.opt.adjacent && v.png && (v.prev_png || v.next_png) ? el("div", { class: "adjrow" }, side(v.prev_png, t("act-adj-prev", { n: (v.frame + v.frames - 1) % v.frames + 1 })), side(v.next_png, t("act-adj-next", { n: (v.frame + 1) % v.frames + 1 }))) : null;
  const left = el("div", {}, zoomBar, viewOpts, el("div", { class: "stagewrap" }, stage), adjacent, v.error ? el("p", { class: "muted", text: t("act-no-picture", { error: v.error }) }) : null, el("p", { class: "muted", text: t("act-stage-note") }));
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
  const scrolled = $("act-body").querySelector(".stagewrap");
  const [sl, st] = scrolled ? [scrolled.scrollLeft, scrolled.scrollTop] : [0, 0];
  $("act-body").replaceChildren(el("div", { class: "acted" }, bar, referenceRow(v), left, el("div", {}, layers, anchors)));
  const wrap = $("act-body").querySelector(".stagewrap");
  if (wrap) Object.assign(wrap, { scrollLeft: sl, scrollTop: st });
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

/** A GRF has no empty folders: "New folder" asks for the path and the files that go into it. */
async function doNewFolder(parent) {
  const row = S.selected != null ? S.rows[S.selected - S.offset] : null;
  const start = parent != null ? `${parent}\\` : row ? (row.folder ? `${row.name}\\` : row.name.slice(0, row.name.lastIndexOf("\\") + 1)) : "data\\";
  const input = el("input", { type: "text", class: "field", value: start, spellcheck: "false", "aria-label": t("nf-path") });
  const go = await ask(t("nf-title"), [el("p", { text: t("nf-intro") }), input], [{ label: t("nf-pick"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  const folder = input.value.trim().replace(/\//g, "\\").replace(/\\+$/, "");
  if (!folder) return note(t("nf-title"), t("err-enter-path"), true);
  const picked = await T.dialog.open({ multiple: true, directory: false, title: t("add-pick") });
  if (!picked) return;
  let added = 0;
  try {
    for (const file of Array.isArray(picked) ? picked : [picked]) {
      setPending(await invoke("stage_add", { path: `${folder}\\${file.split(/[\\/]/).pop()}`, file }));
      added += 1;
    }
    setStatus(t("nf-status", { n: added, folder }));
  } catch (e) {
    await note(t("add-failed"), errText(e), true);
  }
}

/** A new animation shaped like one of ACT Editor's templates, added to the archive. */
async function doNewAct() {
  const kinds = ["empty", "npc", "monster", "homunculus", "weapon", "headgear_male", "headgear_female"];
  const kind = el("select", { class: "field", "aria-label": t("tpl-kind") }, kinds.map((k) => el("option", { value: k, text: t(`tpl-${k}`) })));
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  const folder = sel ? (sel.folder ? `${sel.name}\\` : sel.name.slice(0, sel.name.lastIndexOf("\\") + 1)) : "data\\sprite\\";
  const path = el("input", { type: "text", class: "field", value: `${folder}new.act`, spellcheck: "false", "aria-label": t("tpl-path") });
  const withSpr = el("input", { type: "checkbox", checked: true });
  const go = await ask(t("tpl-title"), [el("p", { text: t("tpl-intro") }), formRow(t("tpl-kind"), kind), formRow(t("tpl-path"), path), el("label", { class: "check" }, withSpr, ` ${t("tpl-spr")}`), el("p", { class: "muted", text: t("tpl-note") })], [{ label: t("dlg-add"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  try {
    setPending(await invoke("stage_new_act", { path: path.value, kind: kind.value, spr: withSpr.checked }));
    setStatus(t("add-status", { path: path.value }));
  } catch (e) {
    await note(t("tpl-title"), errText(e), true);
  }
}

/** Delete directives of a Thor patch: paths the patch removes from the target when it is applied. */
async function doThorRemovals() {
  const text = el("textarea", { class: "field", rows: "8", style: "width:100%;font-family:monospace", spellcheck: "false", placeholder: "data\\texture\\old.bmp", "aria-label": t("tr-title") });
  const go = await ask(t("tr-title"), [el("p", { text: t("tr-intro") }), text], [{ label: t("dlg-add"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  try {
    setPending(await invoke("stage_removals", { text: text.value }));
    setStatus(t("tr-status", { n: text.value.split(/\n/).filter((l) => l.trim()).length }));
  } catch (e) {
    await note(t("tr-title"), errText(e), true);
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
    const cancelled = /cancel/i.test(plainErr(e));
    setStatus(cancelled ? t("save-cancelled") : t("save-failed"));
    await note(cancelled ? t("save-cancelled") : t("save-could-not"), cancelled ? t("nothing-written") : errText(e), !cancelled);
  }
}

const SAVE_TYPES = { grf: "GRF", gpf: "GPF", gsf: "GSF", thor: "Thor", rgz: "RGZ" };
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
  const tableEnc = !!info.table_encrypted;
  const method = el("select", {}, ["zlib", "zopfli", "lzma"].map((k) => el("option", { value: k, text: t(`method-${k}`) })));
  method.value = SET.method;
  const store = el("select", {}, el("option", { value: "copy", text: t("sa-store-copy") }), el("option", { value: "recompress", text: t("sa-store-recompress") }));
  store.value = preset === "repack" ? "recompress" : "copy";
  const format = el("select", {}, el("option", { value: "keep", text: t("sa-format-keep", { format: info.format }) }), ...["0x200", "0x300", "0x103", "0x102"].map((k) => el("option", { value: k, text: t(`format-${k}`) })));
  const enc = el("select", {}, encrypted || tableEnc ? el("option", { value: "keep", text: t("sa-enc-keep") }) : null, el("option", { value: "none", text: encrypted || tableEnc ? t("sa-enc-none") : t("sa-enc-plain") }), el("option", { value: "password", text: t("sa-enc-password") }), el("option", { value: "table", text: t("sa-enc-table") }));
  enc.value = preset === "encrypt" ? "password" : preset === "decrypt" ? "none" : encrypted || tableEnc ? "keep" : "none";
  // GRF Editor's "Encrypt File Table": the table can be encrypted along with the files
  const tbl = el("input", { type: "checkbox", checked: tableEnc });
  const tblRow = el("label", { class: "check" }, tbl, ` ${t("sa-enc-table-also")}`);
  const pass = el("input", { type: "password", class: "field", autocomplete: "new-password", placeholder: t("sa-password"), "aria-label": t("sa-password") });
  const pass2 = el("input", { type: "password", class: "field", autocomplete: "new-password", placeholder: t("sa-password-again"), "aria-label": t("sa-password-again") });
  const passRow = el("div", { class: "form" }, pass, pass2);
  // format and encryption are GRF things; a Thor or RGZ has neither
  const grfTarget = kind !== "thor" && kind !== "rgz";
  const sync = () => {
    method.disabled = store.value !== "recompress";
    passRow.hidden = !grfTarget || !["password", "table"].includes(enc.value);
    tblRow.hidden = !grfTarget || !(enc.value === "password" || (enc.value === "keep" && encrypted));
  };
  for (const c of [store, enc]) c.addEventListener("change", sync);
  sync();
  const facts = el("dl", { class: "facts" },
    el("dt", { text: t("as-path") }), el("dd", { text: S.untitled ? t("as-unsaved") : info.path }),
    el("dt", { text: t("as-type") }), el("dd", { text: info.format }),
    el("dt", { text: t("as-content") }), el("dd", { text: `${t("pvw-files", { n: info.files })} · ${fmtBytes(info.file_size)}` }),
    el("dt", { text: t("as-encryption") }), el("dd", { text: encrypted ? t("as-encrypted") : tableEnc ? t("as-table-encrypted") : t("as-plain") }),
    el("dt", { text: t("as-names") }), el("dd", { text: $("names").selectedOptions[0]?.textContent || "" }),
    info.magic ? el("dt", { text: t("as-magic") }) : null, info.magic ? el("dd", {}, el("code", { text: info.magic })) : null,
  );
  const fmtLabel = el("label", {}, el("span", { class: "fl", text: t("set-format") }), format);
  // GRF Editor's "Magic header": any 16 ASCII characters; an unusual one stays on save
  const magic = el("input", { type: "text", class: "field", value: info.magic || "", maxlength: "16", spellcheck: "false", "aria-label": t("as-magic") });
  const usualMagic = () => ((format.value === "keep" ? info.format : format.value).includes("300") ? "Event Horizon" : "Master of Magic");
  const magicReset = el("button", { type: "button", class: "small", text: t("as-magic-reset"), onclick: () => (magic.value = usualMagic()) });
  const magicLabel = el("label", {}, el("span", { class: "fl", text: t("as-magic") }), el("span", { class: "inline" }, magic, magicReset));
  const encLabel = el("label", {}, el("span", { class: "fl", text: t("sa-encryption") }), enc);
  for (const n of [fmtLabel, encLabel, passRow, magicLabel]) if (!grfTarget) n.hidden = true;
  // a Thor patch applies either to the client folder or inside a GRF
  const thorMerge = el("input", { type: "checkbox", checked: !!info.thor?.merge });
  const thorTarget = el("input", { type: "text", class: "field", value: info.thor?.target || "", placeholder: t("sa-thor-target-ph"), spellcheck: "false", "aria-label": t("sa-thor-target") });
  const thorHash = el("input", { type: "checkbox", checked: info.thor ? !!info.thor.integrity : !!SET.thorIntegrity });
  thorMerge.addEventListener("change", () => (thorTarget.disabled = !thorMerge.checked));
  thorTarget.disabled = !thorMerge.checked;
  const thorRow = el("div", { class: "form" },
    el("label", { class: "check" }, thorMerge, ` ${t("sa-thor-merge")}`),
    el("label", {}, el("span", { class: "fl", text: t("sa-thor-target") }), thorTarget),
    el("label", { class: "check" }, thorHash, ` ${t("sa-thor-hash")}`),
    el("p", { class: "hint", text: t("sa-thor-hint") }));
  thorRow.hidden = kind !== "thor";
  const body = [
    preset === "settings" ? facts : null,
    el("div", { class: "form buildform" },
      el("label", {}, el("span", { class: "fl", text: t("sa-store") }), store),
      el("label", {}, el("span", { class: "fl", text: t("set-compression") }), method),
      fmtLabel,
      encLabel,
      preset === "settings" || preset === "as" ? magicLabel : null,
    ),
    passRow,
    tblRow,
    thorRow,
    el("p", { class: "hint", text: t("sa-hint") }),
  ];
  const titles = { as: "sa-title", repack: "sa-title-repack", compact: "sa-title-compact", encrypt: "sa-title-encrypt", decrypt: "sa-title-decrypt", settings: "as-title" };
  const buttons = [];
  if (!S.untitled) buttons.push({ label: t("save-here"), value: "here", primary: preset !== "as" });
  buttons.push({ label: t("save-as-button"), value: "as", primary: preset === "as" || !!S.untitled }, { label: "Cancel", value: null });
  for (;;) {
    const act = await ask(t(titles[preset] || "sa-title"), body, buttons, preset === "settings" ? "info" : undefined);
    if (!act) return;
    if (["password", "table"].includes(enc.value) && (!pass.value || pass.value !== pass2.value)) {
      await note(t("sa-title-encrypt"), pass.value ? t("sa-password-mismatch") : t("err-password-empty"), true);
      continue;
    }
    const req = {
      dst: "",
      recompress: store.value === "recompress",
      method: method.value,
      format: format.value === "keep" ? null : format.value,
      encryption: enc.value,
      password: ["password", "table"].includes(enc.value) ? pass.value : null,
      encrypt_table: !tblRow.hidden ? tbl.checked : null,
      magic: magic.value && magic.value !== info.magic ? magic.value : null,
      thor_merge: kind === "thor" ? thorMerge.checked : null,
      thor_target: kind === "thor" ? thorTarget.value : null,
      thor_integrity: kind === "thor" ? thorHash.checked : null,
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
    const cancelled = /cancel/i.test(plainErr(e));
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
  const other = await T.dialog.open({ multiple: false, directory: false, title: t("cmp-pick"), filters: [{ name: "GRF, GPF, GSF, Thor, RGZ", extensions: ["grf", "gpf", "gsf", "thor", "rgz"] }] });
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

/** Shows an entry in the main list (clearing search and filters when they hide it) and previews it. */
async function revealEntry(index) {
  let at = await invoke("locate", { view: 0, entry: index, folder: null });
  if (at.line == null) {
    $("search").value = "";
    V.kinds = [];
    V.size = "";
    syncViewBar();
    await runSearch("");
    at = await invoke("locate", { view: 0, entry: index, folder: null });
  }
  if (at.line == null) return;
  await mainList.reset(at.rows, true);
  await select(at.line);
}

/** Tools › Search in Files (GRF Editor's "Advanced search"): text in the files, then jump to a hit. */
async function doSearchContents() {
  if (!S.info) return;
  const prev = SET.contentSearch || { query: "", files: "*.lua;*.lub;*.txt;*.xml", regex: false, case: false, word: false, inList: false };
  const query = el("input", { type: "search", class: "field", value: prev.query, placeholder: t("sc-query-ph"), "aria-label": t("sc-query"), spellcheck: "false", autocomplete: "off" });
  const files = el("input", { type: "text", class: "field", value: prev.files, placeholder: t("sc-files-ph"), "aria-label": t("sc-files"), spellcheck: "false" });
  const box = (key, label) => {
    const c = el("input", { type: "checkbox" });
    c.checked = !!prev[key];
    return [c, el("label", { class: "check" }, c, ` ${label}`)];
  };
  const [regex, regexL] = box("regex", t("sc-regex"));
  const [cased, caseL] = box("case", t("sc-case"));
  const [word, wordL] = box("word", t("sc-word"));
  const [inList, inListL] = box("inList", t("sc-in-list"));
  const results = el("div", { class: "sc-results" });
  const status = el("p", { class: "muted", role: "status" });
  const run = async () => {
    if (!query.value) return query.focus();
    SET.contentSearch = { query: query.value, files: files.value, regex: regex.checked, case: cased.checked, word: word.checked, inList: inList.checked };
    saveSettings();
    status.textContent = t("sc-running");
    results.replaceChildren();
    try {
      const r = await invoke("search_contents", { query: { query: query.value, regex: regex.checked, case: cased.checked, whole_word: word.checked, files: files.value, in_list: inList.checked } });
      status.textContent = r.hits.length ? t("sc-result", { lines: r.hits.length, files: t("pvw-files", { n: r.files_with_hits }), searched: t("pvw-files", { n: r.files_searched }) }) + (r.truncated ? ` ${t("sc-more")}` : "") : t("sc-none", { searched: t("pvw-files", { n: r.files_searched }) });
      let last = null;
      for (const h of r.hits) {
        if (h.name !== last) {
          results.append(el("div", { class: "sc-file", text: h.name }));
          last = h.name;
        }
        results.append(el("button", { type: "button", class: "sc-hit", title: t("sc-show"), onclick: async () => { $("dlg").close(); await revealEntry(h.index); } }, el("span", { class: "sc-line", text: String(h.line) }), el("span", { class: "sc-text", text: h.text })));
      }
    } catch (e) {
      status.textContent = errText(e);
    }
  };
  query.addEventListener("keydown", (e) => e.key === "Enter" && (e.preventDefault(), run()));
  files.addEventListener("keydown", (e) => e.key === "Enter" && (e.preventDefault(), run()));
  const body = [
    el("div", { class: "form" }, el("label", {}, el("span", { class: "fl", text: t("sc-query") }), query), el("label", {}, el("span", { class: "fl", text: t("sc-files") }), files)),
    el("div", { class: "sc-opts" }, regexL, caseL, wordL, inListL),
    el("div", { class: "controls" }, el("button", { type: "button", class: "primary", icon: "verify", text: t("sc-search"), onclick: run })),
    status,
    results,
  ];
  setTimeout(() => query.focus(), 0);
  await ask(t("sc-title"), body, [{ label: "Close", value: null }], "info");
}

/** Tools › Text Encoding (as in GRF Editor): text shown in a wrong encoding, read in the right one. */
async function doTextEncoding() {
  const ENC = [["windows-1252", "Windows-1252 (Latin-1)"], ["euc-kr", "CP949 (Korean)"], ["shift_jis", "Shift-JIS (Japanese)"], ["gbk", "GBK (Chinese, simplified)"], ["big5", "Big5 (Chinese, traditional)"], ["windows-1251", "Windows-1251 (Cyrillic)"], ["windows-874", "Windows-874 (Thai)"], ["utf-8", "UTF-8"], ["utf-16le", "UTF-16 LE"]];
  const pick = (value, label) => {
    const sel = el("select", { "aria-label": label }, ENC.map(([v, n]) => el("option", { value: v, text: n })));
    sel.value = value;
    return sel;
  };
  const prev = SET.textEncoding || { from: "windows-1252", to: "euc-kr" };
  const src = el("textarea", { class: "field mono", rows: "4", spellcheck: "false", placeholder: "À¯ÀúÀÎÅÍÆäÀÌ½º", "aria-label": t("te-source") });
  const out = el("textarea", { class: "field mono", rows: "4", readonly: "", "aria-label": t("te-result") });
  const hex = el("p", { class: "muted mono small" });
  const warn = el("p", { class: "error", hidden: "" });
  const from = pick(prev.from, t("te-from"));
  const to = pick(prev.to, t("te-to"));
  let timer = 0;
  const run = () => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      SET.textEncoding = { from: from.value, to: to.value };
      saveSettings();
      try {
        const r = await invoke("convert_text", { text: src.value, from: from.value, to: to.value });
        out.value = r.text;
        hex.textContent = r.hex ? t("te-bytes", { hex: r.hex }) : "";
        warn.hidden = !r.lossy;
        warn.textContent = t("te-lossy");
      } catch (e) {
        out.value = "";
        warn.hidden = false;
        warn.textContent = errText(e);
      }
    }, 120);
  };
  const swap = el("button", { type: "button", class: "small", text: t("te-swap"), onclick: () => { [from.value, to.value] = [to.value, from.value]; src.value = out.value; run(); } });
  for (const c of [src, from, to]) c.addEventListener("input", run);
  setTimeout(() => src.focus(), 0);
  await ask(t("te-title"), [
    el("p", { class: "hint", text: t("te-intro") }),
    el("label", { class: "stack" }, t("te-source"), src),
    el("div", { class: "form" }, el("label", {}, el("span", { class: "fl", text: t("te-from") }), from), el("label", {}, el("span", { class: "fl", text: t("te-to") }), to)),
    el("div", { class: "controls" }, swap),
    el("label", { class: "stack" }, t("te-result"), out),
    warn,
    hex,
  ], [{ label: "Close", value: null }], "info");
}

// A shortcut that is both a menu accelerator and a key the page handles must run once.
const LAST_RUN = {};
function once(id, f) {
  const now = Date.now();
  if (now - (LAST_RUN[id] || 0) < 400) return;
  LAST_RUN[id] = now;
  return f();
}

/** Help › Keyboard Shortcuts (Cmd/Ctrl+/). */
async function openShortcuts() {
  const mac = /Mac/.test(navigator.platform);
  const m = mac ? "⌘" : "Ctrl+";
  const sh = mac ? "⇧" : "Shift+";
  const rows = [
    [`${m}O`, "ks-open"],
    [`${m}F`, "ks-search"],
    ["↓ / Enter · Esc", "ks-search-go"],
    [`${sh}${m}F`, "ks-search-files"],
    ["a–z, 0–9", "ks-type"],
    ["↑ ↓ ← → · Home End", "ks-move"],
    [`${m}E`, "ks-extract"],
    ["F2", "ks-rename"],
    [mac ? "⌘⌫" : "Delete", "ks-remove"],
    [mac ? "⌘I" : "Alt+Enter", "ks-props"],
    [`${sh}${m}C`, "ks-copy-path"],
    [mac ? "⌥⌘R" : "Shift+Alt+R", "ks-reveal"],
    [`${m}Z · ${sh}${m}Z`, "ks-undo"],
    [`${m}S · ${sh}${m}S`, "ks-save"],
    [`${m}1 – ${m}4`, "ks-layout"],
    [`${sh}${m}P`, "ks-preview"],
    ["Ctrl+Tab", "ks-tabs"],
    [`${m},`, "ks-settings"],
    [`${m}/`, "ks-shortcuts"],
  ];
  const table = el("dl", { class: "facts shortcuts" }, ...rows.flatMap(([keys, key]) => [el("dt", {}, el("kbd", { text: keys })), el("dd", { text: t(key, { app: FILE_MANAGER() }) })]));
  await ask(t("ks-title"), table, [{ label: "OK", value: true, primary: true }], "info");
}

/**
 * Help › Getting Started: a few pages about what the app does and how to move in it.
 * Shown once on the first start; the keys are written for this system.
 */
async function openTour() {
  const mac = /Mac/.test(navigator.platform);
  const k = (s) => el("kbd", { text: s });
  const cmd = (key) => (mac ? `⌘${key}` : `Ctrl+${key}`);
  const item = (key, ...keys) => el("li", {}, ...t(key, { app: FILE_MANAGER() }).split(/(%\d)/).map((part) => {
    const m = /^%(\d)$/.exec(part);
    return m ? k(keys[Number(m[1])]) : part;
  }));
  const pages = [
    ["tour-1-title", [el("p", { text: t("tour-1-intro") }), el("ul", {}, item("tour-1-open", cmd("O")), item("tour-1-formats"), item("tour-1-safe"))]],
    ["tour-2-title", [el("p", { text: t("tour-2-intro") }), el("ul", {}, item("tour-2-search", cmd("F")), item("tour-2-type"), item("tour-2-views", cmd("1"), cmd("4")), item("tour-2-files", mac ? "⇧⌘F" : "Ctrl+Shift+F"))]],
    ["tour-3-title", [el("p", { text: t("tour-3-intro") }), el("ul", {}, item("tour-3-preview"), item("tour-3-edit", "F2", mac ? "⌘⌫" : "Delete"), item("tour-3-undo", cmd("Z")), item("tour-3-save", cmd("S")))]],
    ["tour-4-title", [el("p", { text: t("tour-4-intro") }), el("ul", {}, item("tour-4-extract", cmd("E")), item("tour-4-where"), item("tour-4-build"))]],
    ["tour-5-title", [el("p", { text: t("tour-5-intro") }), el("ul", {}, item("tour-5-keys", cmd("/")), item("tour-5-settings", cmd(",")), item("tour-5-again"))]],
  ];
  let at = 0;
  for (;;) {
    const [title, body] = pages[at];
    const buttons = [];
    if (at < pages.length - 1) buttons.push({ label: t("tour-next"), value: "next", primary: true });
    else buttons.push({ label: t("tour-done"), value: "done", primary: true });
    if (at > 0) buttons.push({ label: t("tour-back"), value: "back" });
    if (at < pages.length - 1) buttons.push({ label: t("tour-skip"), value: "done" });
    const go = await ask(t(title), [el("div", { class: "tour" }, el("p", { class: "tour-step", text: t("tour-step", { n: at + 1, total: pages.length }) }), ...body)], buttons, "info");
    SET.tourSeen = true;
    saveSettings();
    if (go === "next") at++;
    else if (go === "back") at--;
    else return;
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
    el("p", {}, "Kadir Sert · ", link("github.com/kxdrsrt", "https://github.com/kxdrsrt")),
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

// ---------- updates ----------

const UPDATE_REPO = "kxdrsrt/RustyGRF";

/** "v1.2.3-beta.1" → [1, 2, 3]; anything that is not a version gives []. */
const versionParts = (v) => (/^v?(\d+(?:\.\d+)*)/.exec(String(v || "").trim())?.[1] ?? "").split(".").filter(Boolean).map(Number);

/** True when version `a` is newer than `b`. */
function isNewer(a, b) {
  const x = versionParts(a);
  const y = versionParts(b);
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] || 0) - (y[i] || 0);
    if (d) return d > 0;
  }
  return false;
}

/** The installer of the latest release that fits this system, else the release page. */
function releaseDownload(release) {
  const platform = navigator.userAgentData?.platform || navigator.platform || "";
  const wanted = /Mac/i.test(platform) ? [/\.dmg$/i] : /Win/i.test(platform) ? [/\.msi$/i, /\.exe$/i] : [/\.AppImage$/i, /\.deb$/i];
  const assets = (release.assets || []).filter((a) => typeof a.browser_download_url === "string");
  for (const re of wanted) {
    const fits = assets.filter((a) => re.test(a.name));
    // with several builds (x64 and ARM) the page cannot tell which one this machine needs: let the release page choose
    if (fits.length === 1) return fits[0].browser_download_url;
    if (fits.length > 1) break;
  }
  return release.html_url;
}

/** Help › Check for Updates, and the same check once at start. Only a newer release shows anything at start. */
async function checkForUpdates(manual) {
  let current = "";
  try {
    current = (await T.app?.getVersion?.()) || "";
  } catch {}
  if (!current && !manual) return;
  let release;
  try {
    const r = await fetch(`https://api.github.com/repos/${UPDATE_REPO}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } });
    if (r.status === 404) {
      if (manual) await note(t("upd-title"), t("upd-none-yet"));
      return;
    }
    if (!r.ok) throw new Error(`HTTP ${r.status}`);
    release = await r.json();
  } catch (e) {
    if (manual) await note(t("upd-title"), t("upd-failed", { reason: String(e?.message || e) }), true);
    return;
  }
  const latest = String(release.tag_name || "").replace(/^v/i, "");
  if (!isNewer(latest, current || "0")) {
    if (manual) await note(t("upd-title"), t("upd-latest", { version: current || latest }));
    return;
  }
  if (!manual && SET.skipVersion === latest) return;
  const notes = String(release.body || "").trim();
  const body = [
    el("p", { text: t("upd-available", { latest, current: current || "?" }) }),
    notes ? el("pre", { class: "update-notes", text: notes.length > 1500 ? `${notes.slice(0, 1500)} …` : notes }) : null,
    el("p", { class: "muted", text: t("upd-hint") }),
  ];
  const choice = await ask(t("upd-title"), body, [
    { label: t("upd-download"), value: "get", primary: true },
    { label: t("upd-notes"), value: "notes" },
    { label: t("upd-skip"), value: "skip" },
    { label: t("upd-later"), value: null },
  ], "info");
  if (choice === "get") invoke("open_link", { url: releaseDownload(release) }).catch(() => {});
  else if (choice === "notes") invoke("open_link", { url: release.html_url }).catch(() => {});
  else if (choice === "skip") {
    SET.skipVersion = latest;
    saveSettings();
  }
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
    const cancelled = /cancel/i.test(plainErr(e));
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
    const cancelled = /cancel/i.test(plainErr(e));
    setStatus(cancelled ? t("build-cancelled") : t("build-failed"));
    await note(cancelled ? t("build-cancelled") : t("build-could-not"), cancelled ? t("nothing-written") : errText(e), !cancelled);
  }
}

async function doMerge() {
  const picked = await T.dialog.open({ multiple: true, directory: false, title: t("merge-pick"), filters: [{ name: "GRF, GPF, GSF, Thor and RGZ archives", extensions: ["grf", "gpf", "gsf", "thor", "rgz"] }] });
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
    const cancelled = /cancel/i.test(plainErr(e));
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
    const cancelled = /cancel/i.test(plainErr(e));
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
const DEFAULTS = { language: "system", theme: "system", accent: "orange", fontsize: "normal", density: "normal", plainIcons: false, motion: "system", names: "cp949", layout: "last", preview: "last", reopen: false, warnDiscard: true, autoplay: true, tabs: true, decompileLub: false, rememberTree: false, thorIntegrity: false, extractMode: "beside", afterExtract: "reveal", extractTo: "", previewBg: "checker", previewBgColor: "#808080", method: "zlib", format: "0x200", checkUpdates: true, skipVersion: "", tourSeen: false, lastArchive: "", recent: [] };
const SET = (() => {
  try {
    const saved = JSON.parse(localStorage.getItem(SETTINGS_KEY) || "{}");
    // settings of 0.1.0: a chosen folder meant "always there", the checkbox meant "open the folder"
    if (saved.extractMode == null && saved.extractTo) saved.extractMode = "fixed";
    if (saved.afterExtract == null && saved.openAfterExtract) saved.afterExtract = "open";
    delete saved.openAfterExtract;
    return { ...DEFAULTS, ...saved };
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
  flag("data-previewbg", SET.previewBg !== "checker", SET.previewBg);
  root.style.setProperty("--preview-bg", SET.previewBgColor || "#808080");
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
    if (key === "tabs") invoke("set_tabs_enabled", { on: value }).catch(() => {});
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
  const extractTo = el("span", { class: "bar" });
  const drawExtractTo = () =>
    extractTo.replaceChildren(
      ...[el("code", { text: SET.extractTo || t("set-extract-to-none") }),
      el("button", { type: "button", class: "small", text: t("set-extract-to-choose"), onclick: async () => {
        const dir = await T.dialog.open({ directory: true, multiple: false, title: t("ext-dest") });
        if (dir) {
          change("extractTo", Array.isArray(dir) ? dir[0] : dir);
          drawExtractTo();
        }
      } }),
      SET.extractTo ? el("button", { type: "button", class: "small", text: t("set-extract-to-clear"), onclick: () => (change("extractTo", ""), drawExtractTo()) }) : null,
      ].filter(Boolean),
    );
  drawExtractTo();
  const modeSel = pick("extractMode", t("set-extract-mode"), [["beside", t("set-extract-beside")], ["client", t("set-extract-client")], ["fixed", t("set-extract-fixed")], ["ask", t("set-extract-ask")]]);
  // the folder picker belongs to "always this folder" only
  modeSel.addEventListener("change", () => {
    const y = $("settings-body").scrollTop;
    openSettings();
    $("settings-body").scrollTop = y;
  });
  const bgColour = el("input", { type: "color", value: SET.previewBgColor || "#808080", "aria-label": t("set-bg-custom") });
  bgColour.addEventListener("input", () => change("previewBgColor", bgColour.value));
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
      row(t("set-audio"), check("autoplay", t("set-autoplay"))),
      row(t("set-tabs"), check("tabs", t("set-tabs-open"))),
      row(t("set-thor-hash"), check("thorIntegrity", t("set-thor-hash-on")), t("set-thor-hash-hint")),
      row(t("set-tree"), check("rememberTree", t("set-tree-remember")), t("set-tree-hint")),
      row(t("set-updates"), check("checkUpdates", t("set-updates-on")), t("set-updates-hint")),
      row(t("set-help"), el("span", { class: "bar" },
        el("button", { type: "button", class: "small", text: t("ks-title"), onclick: () => (dlg.close(), openShortcuts()) }),
        el("button", { type: "button", class: "small", text: t("tour-open"), onclick: () => (dlg.close(), openTour()) }),
      ), t("set-help-hint", { key: /Mac/.test(navigator.platform) ? "⌘/" : "Ctrl+/" })),
      el("h3", { text: t("set-extracting") }),
      row(t("set-decompile"), check("decompileLub", t("set-decompile-on")), t("set-decompile-hint")),
      row(t("set-extract-mode"), modeSel, t("set-extract-mode-hint")),
      SET.extractMode === "fixed" ? row(t("set-extract-fixed"), extractTo) : null,
      row(t("set-open-after"), pick("afterExtract", t("set-open-after"), [["reveal", t("set-after-reveal", { app: FILE_MANAGER() })], ["open", t("set-after-open")], ["nothing", t("set-after-nothing")]])),
      row(t("set-previewbg"), el("span", {}, pick("previewBg", t("set-previewbg"), [["checker", t("set-bg-checker")], ["black", t("set-bg-black")], ["white", t("set-bg-white")], ["gray", t("set-bg-gray")], ["green", t("set-bg-green")], ["custom", t("set-bg-custom")]]), " ", bgColour)),
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

// ---------- context menu (right click on an entry or folder; Menu key or Shift+F10) ----------

let ctxClose = null;

/** A menu at (x, y). Items: { label, icon, run, disabled } or "-" for a separator. Arrow keys, Home/End, Enter, Esc. */
function showContextMenu(x, y, items, returnFocus, label) {
  ctxClose?.(false);
  const menu = el("div", { class: "popmenu ctx", role: "menu", "aria-label": label || "" });
  const buttons = [];
  for (const it of items) {
    if (it === "-") {
      menu.append(el("hr", { role: "separator" }));
      continue;
    }
    const b = el("button", { type: "button", role: "menuitem", tabindex: "-1", disabled: !!it.disabled, icon: it.icon || "blank" }, el("span", { class: "mlabel", text: it.label }));
    b.addEventListener("click", () => {
      close(false);
      it.run();
    });
    menu.append(b);
    if (!it.disabled) buttons.push(b);
  }
  // inside the open dialog (if any): a modal dialog covers everything outside of it
  const open = document.querySelectorAll("dialog[open]");
  (open[open.length - 1] || document.body).append(menu);
  const box = menu.getBoundingClientRect();
  menu.style.left = `${Math.max(4, Math.min(x, window.innerWidth - box.width - 4))}px`;
  menu.style.top = `${Math.max(4, Math.min(y, window.innerHeight - box.height - 4))}px`;
  let at = -1;
  const focusAt = (i) => {
    at = (i + buttons.length) % buttons.length;
    buttons[at]?.focus();
  };
  const key = (e) => {
    if (e.key === "Escape") (e.preventDefault(), close(true));
    else if (e.key === "ArrowDown") (e.preventDefault(), focusAt(at + 1));
    else if (e.key === "ArrowUp") (e.preventDefault(), focusAt(at - 1));
    else if (e.key === "Home") (e.preventDefault(), focusAt(0));
    else if (e.key === "End") (e.preventDefault(), focusAt(buttons.length - 1));
    else if (e.key === "Tab") (e.preventDefault(), close(true));
  };
  const outside = (e) => !menu.contains(e.target) && close(false);
  const away = () => close(false);
  function close(restore) {
    document.removeEventListener("pointerdown", outside, true);
    document.removeEventListener("keydown", key, true);
    window.removeEventListener("blur", away);
    window.removeEventListener("resize", away);
    document.removeEventListener("wheel", away, true);
    menu.remove();
    ctxClose = null;
    if (restore) returnFocus?.focus?.();
  }
  ctxClose = close;
  document.addEventListener("pointerdown", outside, true);
  document.addEventListener("keydown", key, true);
  window.addEventListener("blur", away);
  window.addEventListener("resize", away);
  document.addEventListener("wheel", away, true);
  focusAt(0);
}

const FILE_MANAGER = () => (/Mac/.test(navigator.platform) ? "Finder" : /Win/.test(navigator.platform) ? "Explorer" : t("ctx-fm"));

/** The entries a row stands for, in the list it was clicked in. */
const entriesIn = (list, row) => entriesOf(row, list.view);

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const ta = el("textarea", { style: "position:fixed;opacity:0" });
    ta.value = text;
    document.body.append(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
  }
  setStatus(t("ctx-copied", { text }));
}

function entryMenu(list, row) {
  const items = [];
  const last = localStorage.getItem(LAST_EXTRACT_KEY);
  const pickAndExtract = async () => {
    const dest = await pickExtractDest();
    if (!dest) return;
    const indices = await entriesIn(list, row);
    if (!indices.length) return note(t("ext-title"), t("ext-empty-folder"));
    return extractIndices(indices, dest);
  };
  const guard = (fn) => () => Promise.resolve(fn()).catch((e) => note(t("open-failed"), errText(e), true));
  if (row.folder) {
    items.push({ label: t("ctx-extract-folder"), icon: "extract", run: guard(pickAndExtract) });
    items.push({ label: t("ctx-extract-root"), icon: "extract", run: guard(async () => {
      const dest = await pickExtractDest();
      if (!dest) return;
      const indices = await invoke("folder_entries", { view: list.view, path: row.name, rootOnly: true });
      if (!indices.length) return note(t("ext-title"), t("ext-empty-root"));
      return extractIndices(indices, dest);
    }) });
    if (last) items.push({ label: t("ctx-extract-last", { folder: baseOf(last.replace(/\//g, "\\")) || last }), icon: "extract", run: guard(async () => extractIndices(await entriesIn(list, row), last)) });
    items.push({ label: t("ctx-export-folder"), icon: "export", run: guard(() => exportRows(list, row)) });
    items.push("-", { label: t("ctx-add-here"), icon: "add", run: guard(doAdd) });
    items.push({ label: t("ctx-new-folder"), icon: "add", run: guard(() => doNewFolder(row.name)) });
    items.push({ label: t("ctx-rename-folder"), icon: "pencil", run: guard(() => renameRow(list, row)) });
    items.push({ label: t("ctx-delete-folder"), icon: "remove", run: guard(() => deleteRow(list, row)) });
    items.push(...cryptItems(list, row, guard));
    items.push("-", { label: t("ctx-copy-path"), icon: "copy", run: () => copyText(row.name) });
    items.push({ label: t("ctx-props"), icon: "info", run: guard(() => showProperties(list, row)) });
    return items;
  }
  const ext = extOf(row.name).toLowerCase();
  items.push({ label: t("ctx-open"), icon: "open", run: guard(async () => setStatus(t("ctx-opened", { path: await invoke("open_external", { index: row.index }) }))) });
  items.push({ label: t("ctx-reveal", { app: FILE_MANAGER() }), icon: "folder", run: guard(() => invoke("reveal_entry", { index: row.index })) });
  items.push("-", { label: t("ctx-extract"), icon: "extract", run: guard(pickAndExtract) });
  if (last) items.push({ label: t("ctx-extract-last", { folder: baseOf(last.replace(/\//g, "\\")) || last }), icon: "extract", run: guard(() => extractIndices([row.index], last)) });
  items.push({ label: t("ctx-export"), icon: "export", run: guard(() => exportRows(list, row)) });
  if (["rsw", "gnd", "gat"].includes(ext)) items.push({ label: t("ctx-map-extract"), icon: "map", run: guard(() => doExtractMap(row)) }, { label: t("pvw-flat-map"), icon: "edit", run: guard(() => doFlatMap(row)) }, { label: t("ctx-downgrade"), icon: "map", run: guard(() => doDowngradeMap(row)) });
  const edit = [];
  if (ext === "rsm2") edit.push({ label: t("ctx-rsm-downgrade"), icon: "map", run: guard(() => doDowngradeModels(list, row)) });
  if (row.kind === "text") edit.push({ label: t("ctx-edit"), icon: "edit", run: guard(() => openTextEditor(row)) });
  if (["wav", "mp3", "ogg"].includes(ext)) edit.push({ label: t("ctx-audio"), icon: "audio", run: guard(() => openAudioEditor(row)) });
  if (ext === "bmp") edit.push({ label: t("ctx-reduce"), icon: "image", run: guard(() => openReduceDialog(row)) });
  edit.push({ label: t("ctx-hex"), icon: "file", run: guard(async () => openHexEditor(row, (await invoke("details", { index: row.index })).name)) });
  items.push("-", ...edit, "-");
  items.push({ label: t("ctx-replace"), icon: "replace", run: guard(doReplace) });
  items.push({ label: t("ctx-rename"), icon: "pencil", run: guard(() => renameRow(list, row)) });
  items.push({ label: t("ctx-delete"), icon: "remove", run: guard(() => deleteRow(list, row)) });
  items.push(...cryptItems(list, row, guard));
  items.push("-", { label: t("ctx-usages"), icon: "search", run: guard(() => findUsages(row)) });
  items.push({ label: t("ctx-copy-name"), icon: "copy", run: () => copyText(baseOf(row.name)) });
  items.push({ label: t("ctx-copy-path"), icon: "copy", run: () => copyText(row.name) });
  items.push("-", { label: t("ctx-props"), icon: "info", run: guard(() => showProperties(list, row)) });
  return items;
}

/** The new path of a file, or of a folder (everything below it moves along). */
// ---------- sound editor ----------

/** Wave view, selection by dragging, steps applied by the backend; Apply stages the result. */
async function openAudioEditor(row) {
  const ops = [];
  let view = null;
  let sel = null; // { a, b } in ms of the sound as it is now
  const cv = el("canvas", { class: "wave", width: 900, height: 150, "aria-label": t("au-wave") });
  const info = el("div", { class: "muted" });
  const selInfo = el("div", { class: "muted" });
  const stepsList = el("ol", { class: "history" });
  const msg = el("p", { class: "error" });
  const player = el("audio", { controls: true });
  let playhead = 0;

  const draw = () => {
    const dpr = window.devicePixelRatio || 1;
    const w = cv.clientWidth || 900;
    const h = 150;
    cv.width = Math.round(w * dpr);
    cv.height = Math.round(h * dpr);
    const g = cv.getContext("2d");
    g.scale(dpr, dpr);
    const css = getComputedStyle(document.documentElement);
    g.fillStyle = css.getPropertyValue("--bg") || "#111";
    g.fillRect(0, 0, w, h);
    if (!view) return;
    const dur = Math.max(1, view.duration_ms);
    if (sel) {
      g.fillStyle = "rgba(255,140,0,0.28)";
      g.fillRect((sel.a / dur) * w, 0, ((sel.b - sel.a) / dur) * w, h);
    }
    g.strokeStyle = css.getPropertyValue("--accent") || "#f60";
    g.lineWidth = 1;
    g.beginPath();
    const n = view.peaks.length;
    for (let i = 0; i < n; i++) {
      const x = ((i + 0.5) / n) * w;
      const [lo, hi] = view.peaks[i];
      g.moveTo(x, h / 2 - Math.min(1, hi) * (h / 2 - 2));
      g.lineTo(x, h / 2 - Math.max(-1, lo) * (h / 2 - 2) + 1);
    }
    g.stroke();
    g.strokeStyle = css.getPropertyValue("--line") || "#444";
    g.beginPath();
    g.moveTo(0, h / 2);
    g.lineTo(w, h / 2);
    g.stroke();
    if (playhead > 0) {
      g.strokeStyle = "#fff";
      const x = (playhead / dur) * w;
      g.beginPath();
      g.moveTo(x, 0);
      g.lineTo(x, h);
      g.stroke();
    }
  };

  const chText = (n) => (n === 1 ? t("au-mono") : n === 2 ? t("au-stereo") : t("au-channels", { n }));
  const refreshText = () => {
    if (!view) return;
    const peak = Number.isFinite(view.peak_db) ? view.peak_db.toFixed(1) : "−∞";
    info.textContent = t("au-info", { rate: view.rate, channels: chText(view.channels), ms: fmtNum(view.duration_ms), peak });
    selInfo.textContent = sel ? t("au-sel", { a: fmtNum(sel.a), b: fmtNum(sel.b) }) : t("au-sel-none");
    stepsList.replaceChildren(...(ops.length ? ops.map((o) => el("li", {}, el("code", { text: o }))) : [el("li", { class: "muted", text: t("au-ops-none") })]));
  };

  const render = async () => {
    view = await invoke("audio_render", { index: row.index, ops, buckets: 900 });
    player.src = dataUrl("audio/wav", view.wav);
    if (sel) sel = sel.a >= view.duration_ms ? null : { a: sel.a, b: Math.min(sel.b, view.duration_ms) };
    refreshText();
    draw();
  };

  /** Adds a step; a step that fails is dropped again and says why. */
  const add = async (text, { structural = false } = {}) => {
    ops.push(text);
    msg.textContent = "";
    try {
      await render();
      if (structural) sel = null;
      refreshText();
      draw();
    } catch (e) {
      ops.pop();
      msg.textContent = errText(e);
      await render().catch(() => {});
    }
  };
  const needSel = () => {
    if (sel) return true;
    msg.textContent = t("au-needs-sel");
    return false;
  };
  const at = () => (sel ? `@${sel.a}-${sel.b}` : "");
  const range = () => `${sel.a}-${sel.b}`;
  const field = (label, value, attrs = {}) => {
    const i = el("input", { type: "number", class: "narrow", value: String(value), "aria-label": label, ...attrs });
    return [i, el("span", { class: "muted", text: label })];
  };
  const btn = (text, fn, extra = {}) => el("button", { type: "button", class: "small", text, onclick: fn, ...extra });
  const group = (title, ...kids) => el("div", { class: "audgroup" }, el("strong", { text: title }), el("div", { class: "controls" }, ...kids));

  // selection by dragging on the wave
  let drag = null;
  const msAt = (ev) => {
    const r = cv.getBoundingClientRect();
    return Math.round(Math.min(1, Math.max(0, (ev.clientX - r.left) / r.width)) * (view?.duration_ms || 0));
  };
  cv.addEventListener("pointerdown", (ev) => {
    if (!view) return;
    cv.setPointerCapture(ev.pointerId);
    drag = msAt(ev);
    sel = null;
    refreshText();
    draw();
  });
  cv.addEventListener("pointermove", (ev) => {
    if (drag == null) return;
    const m = msAt(ev);
    sel = m === drag ? null : { a: Math.min(drag, m), b: Math.max(drag, m) };
    refreshText();
    draw();
  });
  cv.addEventListener("pointerup", () => (drag = null));
  player.addEventListener("timeupdate", () => {
    playhead = player.paused && player.currentTime === 0 ? 0 : player.currentTime * 1000;
    draw();
  });
  player.addEventListener("ended", () => {
    playhead = 0;
    draw();
  });

  const [gain, gainL] = field(t("au-gain"), 3, { step: "0.5" });
  const [fin, finL] = field(t("au-ms"), 200, { min: "1" });
  const [fout, foutL] = field(t("au-ms"), 300, { min: "1" });
  const rate = el("select", { "aria-label": t("au-rate") }, ...[8000, 11025, 16000, 22050, 32000, 44100, 48000].map((r) => el("option", { value: r, text: `${r} Hz` })));
  const [speed, speedL] = field(t("au-speed-f"), 1.25, { step: "0.05", min: "0.25", max: "4" });
  const [eDelay, eDelayL] = field(t("au-echo-delay"), 250, { min: "10", max: "2000" });
  const [eFb, eFbL] = field(t("au-echo-fb"), 0.4, { step: "0.05", min: "0", max: "0.95" });
  const [eMix, eMixL] = field(t("au-mix"), 0.3, { step: "0.05", min: "0", max: "1" });
  const [rRoom, rRoomL] = field(t("au-room"), 0.5, { step: "0.1", min: "0", max: "1" });
  const [rMix, rMixL] = field(t("au-mix"), 0.25, { step: "0.05", min: "0", max: "1" });
  const [lp, lpL] = field("Hz", 4000, { min: "50" });
  const [hp, hpL] = field("Hz", 200, { min: "10" });

  const body = [
    el("p", { class: "muted", text: t("au-intro") }),
    cv,
    el("div", { class: "controls" }, info, selInfo, btn(t("au-sel-all"), () => ((sel = view ? { a: 0, b: view.duration_ms } : null), refreshText(), draw())), player),
    group(t("au-g-part"), btn(t("au-trim"), () => needSel() && add(`trim:${range()}`, { structural: true })), btn(t("au-cut"), () => needSel() && add(`cut:${range()}`, { structural: true })), btn(t("au-silence"), () => needSel() && add(`silence:${range()}`))),
    group(t("au-g-level"), gain, gainL, btn(t("au-apply-step"), () => add(`gain:${gain.value}${at()}`)), btn(t("au-normalize"), () => add("normalize:-1")), fin, finL, btn(t("au-fadein"), () => add(`fadein:${fin.value}${at()}`)), fout, foutL, btn(t("au-fadeout"), () => add(`fadeout:${fout.value}${at()}`))),
    group(t("au-g-shape"), btn(t("au-reverse"), () => add(`reverse${at()}`)), btn(t("au-mono-btn"), () => add("mono", { structural: false })), rate, btn(t("au-resample"), () => add(`resample:${rate.value}`)), speed, speedL, btn(t("au-speed"), () => add(`speed:${speed.value}`, { structural: true }))),
    group(t("au-g-fx"), eDelay, eDelayL, eFb, eFbL, eMix, eMixL, btn(t("au-echo"), () => add(`echo:${eDelay.value},${eFb.value},${eMix.value}`)), rRoom, rRoomL, rMix, rMixL, btn(t("au-reverb"), () => add(`reverb:${rRoom.value},${rMix.value}`))),
    group(t("au-g-filter"), lp, lpL, btn(t("au-lowpass"), () => add(`lowpass:${lp.value}${at()}`)), hp, hpL, btn(t("au-highpass"), () => add(`highpass:${hp.value}${at()}`))),
    el("h4", { text: t("au-ops") }),
    stepsList,
    el("div", { class: "controls" }, btn(t("au-undo"), async () => (ops.pop(), await render())), btn(t("au-reset"), async () => ((ops.length = 0), (sel = null), await render()))),
    msg,
  ];
  try {
    await render();
  } catch (e) {
    return note(t("au-failed"), errText(e), true);
  }
  const dlg = $("dlg");
  dlg.classList.add("wide");
  const go = await ask(t("au-title", { name: baseOf(row.name) }), body, [{ label: t("au-apply"), value: true, primary: true }, { label: "Cancel", value: null }], "info");
  dlg.classList.remove("wide");
  player.pause();
  if (!go || !ops.length) return;
  try {
    setPending(await invoke("stage_audio", { index: row.index, ops }));
    setStatus(t("au-status", { name: baseOf(row.name), n: ops.length }));
    refreshPreview();
  } catch (e) {
    await note(t("au-failed"), errText(e), true);
  }
}

/** GRF Editor's "Downgrade": the map for old clients (RSM2 models to RSM1, world 2.1, ground 1.7). */
async function doDowngradeMap(row) {
  const go = await ask(t("dg-title"), [el("p", { text: t("dg-intro") }), el("p", { class: "hint", text: t("dg-hint") })], [{ label: t("dg-go"), value: true, primary: true }, { label: "Cancel", value: null }], "warning");
  if (!go) return;
  try {
    const r = await invoke("stage_map_downgrade", { index: row.index });
    setPending(r.pending);
    setStatus(t("dg-status", { models: fmtNum(r.models), instances: fmtNum(r.instances) }));
    refreshPreview();
    const problems = [...r.missing.map((m) => `${t("dg-missing")}: ${m}`), ...r.failed];
    if (problems.length) await note(t("dg-title"), problems.slice(0, 12).join("\n") + (problems.length > 12 ? `\n… ${problems.length - 12}` : ""));
  } catch (e) {
    await note(t("dg-failed"), errText(e), true);
  }
}

/** GRF Editor's "RSM2 to RSM1": the selected models (or the one clicked) become RSM 1.4 next to the originals. */
async function doDowngradeModels(list, row) {
  try {
    const indices = await entriesIn(list, row);
    const r = await invoke("stage_model_downgrade", { indices });
    setPending(r.pending);
    setStatus(t("dgm-status", { n: fmtNum(r.models) }));
    refreshPreview();
    if (r.failed.length) await note(t("dg-title"), r.failed.slice(0, 12).join("\n"));
  } catch (e) {
    await note(t("dg-failed"), errText(e), true);
  }
}

// ---------- tabs ----------

/**
 * Tabs are windows of one group (see src-tauri/src/tabs.rs): every window draws the strip of its
 * group; a click shows that window in the same place. Dragging a tab out of the window moves it
 * into a window of its own, dropping it on the top of another window joins that window's tabs.
 */
const TABS = { list: [], drag: null };

async function refreshTabs() {
  try {
    TABS.list = await invoke("tab_list");
  } catch {
    TABS.list = [];
  }
  renderTabs();
}

function renderTabs() {
  const bar = $("tabbar");
  const show = TABS.list.length > 1;
  bar.hidden = !show;
  document.body.classList.toggle("has-tabs", show);
  if (!show) return bar.replaceChildren();
  const tabs = TABS.list.map((tab, i) => {
    const name = tab.title.replace(/^\*/, "").replace(/ - RustyGRF$/, "").replace(/ — Edited$/, "") || t("tab-empty");
    const x = el("button", { type: "button", class: "x", tabindex: "-1", title: t("tab-close"), "aria-label": t("tab-close"), text: "×", onclick: (ev) => (ev.stopPropagation(), invoke("tab_close", { label: tab.label })) });
    const b = el("div", { class: "tab", role: "tab", tabindex: tab.active ? "0" : "-1", "aria-selected": String(tab.active), title: name }, el("span", { class: "name" }, tab.dirty ? el("span", { class: "dirty", text: "● " }) : null, name), x);
    b.addEventListener("click", () => !tab.active && invoke("tab_activate", { label: tab.label }));
    b.addEventListener("auxclick", (ev) => ev.button === 1 && invoke("tab_close", { label: tab.label }));
    b.addEventListener("contextmenu", (ev) => {
      ev.preventDefault();
      showContextMenu(ev.clientX, ev.clientY, tabMenu(tab), b, t("tab-strip"));
    });
    b.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" || ev.key === " ") invoke("tab_activate", { label: tab.label });
    });
    b.addEventListener("pointerdown", (ev) => tabPointerDown(ev, tab, i));
    return b;
  });
  bar.replaceChildren(...tabs, el("button", { type: "button", class: "newtab", title: t("tab-new"), "aria-label": t("tab-new"), text: "+", onclick: () => chooseArchive() }));
}

function tabMenu(tab) {
  const items = [{ label: t("tab-new-window"), icon: "open", run: () => invoke("tab_drop", { label: tab.label, x: window.screenX + 120 + 40, y: window.screenY + 80 + 40 }) }];
  items.push({ label: t("tab-close"), icon: "remove", run: () => invoke("tab_close", { label: tab.label }) });
  items.push("-", { label: t("tab-merge"), icon: "merge", run: () => invoke("tab_merge_all") });
  return items;
}

/** Drags a tab with the pointer: along the strip it changes places, away from the window it leaves. */
function tabPointerDown(ev, tab, index) {
  if (ev.button !== 0 || ev.target.closest(".x")) return;
  const bar = $("tabbar");
  const node = ev.currentTarget;
  let dragging = false;
  const startX = ev.clientX;
  let at = index;
  node.setPointerCapture(ev.pointerId);
  const move = (e) => {
    if (!dragging && Math.abs(e.clientX - startX) + Math.abs(e.clientY) < 6) return;
    dragging = true;
    node.classList.add("dragging");
    const tabs = [...bar.querySelectorAll(".tab")];
    const to = tabs.findIndex((n) => e.clientX < n.getBoundingClientRect().right);
    const target = to < 0 ? tabs.length - 1 : to;
    if (e.clientY >= -10 && e.clientY < 60 && target !== at) {
      at = target;
      invoke("tab_move", { label: tab.label, to: target });
    }
  };
  const up = (e) => {
    node.removeEventListener("pointermove", move);
    node.removeEventListener("pointerup", up);
    node.removeEventListener("pointercancel", up);
    node.classList.remove("dragging");
    // dropped away from the strip: another window's top joins it, anything else is a new window
    if (dragging && (e.clientY < -24 || e.clientY > 90 || e.clientX < -24 || e.clientX > window.innerWidth + 24)) {
      invoke("tab_drop", { label: tab.label, x: e.screenX, y: e.screenY }).catch(() => {});
    }
  };
  node.addEventListener("pointermove", move);
  node.addEventListener("pointerup", up);
  node.addEventListener("pointercancel", up);
}

/** Ctrl/Cmd+Tab and Ctrl/Cmd+1..9 switch tabs. */
document.addEventListener("keydown", (ev) => {
  if (TABS.list.length < 2) return;
  const mod = /Mac/.test(navigator.platform) ? ev.metaKey : ev.ctrlKey;
  let next = null;
  const at = TABS.list.findIndex((x) => x.active);
  if (ev.ctrlKey && ev.key === "Tab") next = TABS.list[(at + (ev.shiftKey ? -1 : 1) + TABS.list.length) % TABS.list.length];
  else if (mod && !ev.shiftKey && !ev.altKey && /^[1-9]$/.test(ev.key)) next = TABS.list[Math.min(+ev.key, TABS.list.length) - 1];
  if (next && !next.active) {
    ev.preventDefault();
    invoke("tab_activate", { label: next.label });
  }
});

/** GRF Editor's per-entry encryption exists in GRF 0x200 archives only. */
function cryptItems(list, row, guard) {
  if (!/0x200/.test(S.info?.format || "") || S.untitled === "thor" || S.untitled === "rgz") return [];
  const items = ["-", { label: t("ctx-encrypt"), icon: "lock", run: guard(() => doCrypt(list, row, true)) }];
  if (S.info.encryption) items.push({ label: t("ctx-decrypt"), icon: "unlock", run: guard(() => doCrypt(list, row, false)) });
  return items;
}

async function doCrypt(list, row, on) {
  const indices = await entriesIn(list, row);
  if (!indices.length) return note(t("ext-title"), t("ext-empty-folder"));
  let password = null;
  if (on && !S.info.encryption && !S.cryptAsked) {
    const pass = el("input", { type: "password", class: "field", autocomplete: "new-password", placeholder: t("sa-password"), "aria-label": t("sa-password") });
    const pass2 = el("input", { type: "password", class: "field", autocomplete: "new-password", placeholder: t("sa-password-again"), "aria-label": t("sa-password-again") });
    setTimeout(() => pass.focus(), 50);
    for (;;) {
      const go = await ask(t("crypt-title"), [el("p", { text: t("crypt-intro") }), el("div", { class: "form" }, pass, pass2)], [{ label: t("crypt-go"), value: true, primary: true }, { label: "Cancel", value: null }]);
      if (!go) return;
      if (pass.value && pass.value === pass2.value) break;
      await note(t("crypt-title"), pass.value ? t("sa-password-mismatch") : t("err-password-empty"), true);
    }
    password = pass.value;
  }
  try {
    const r = await invoke("stage_crypt", { indices, on, password });
    if (password) S.cryptAsked = true;
    setPending(r.pending);
    setStatus(t(on ? "crypt-status-on" : "crypt-status-off", { files: t("pvw-files", { n: r.marked }) }));
  } catch (e) {
    await note(t("crypt-failed"), errText(e), true);
  }
}

async function renameRow(list, row) {
  const input = el("input", { type: "text", class: "field", value: row.name, spellcheck: "false", "aria-label": t(row.folder ? "ren-title-folder" : "ren-title") });
  input.addEventListener("keydown", (e) => e.key === "Enter" && (e.preventDefault(), $("dlg-actions").querySelector(".primary")?.click()));
  setTimeout(() => (input.focus(), input.select()), 50);
  const go = await ask(t(row.folder ? "ren-title-folder" : "ren-title"), [el("p", { text: t(row.folder ? "ren-intro-folder" : "ren-intro") }), input, el("p", { class: "hint", text: t("ren-hint") })], [{ label: t("ren-go"), value: true, primary: true }, { label: "Cancel", value: null }]);
  const to = input.value.trim();
  if (!go || !to || to === row.name) return;
  try {
    const indices = await entriesIn(list, row);
    setPending(await invoke("stage_rename", { indices, from: row.name, to }));
    setStatus(t("ren-status", { from: baseOf(row.name), to }));
    refreshPreview();
  } catch (e) {
    await note(t("ren-failed"), errText(e), true);
  }
}

async function deleteRow(list, row) {
  try {
    const indices = await entriesIn(list, row);
    if (!indices.length) return note(t("btn-remove"), t("ext-empty-folder"));
    setPending(await invoke("stage_remove", { indices }));
    setStatus(t("remove-status", { n: indices.length }));
    refreshPreview();
  } catch (e) {
    await note(t("remove-failed"), errText(e), true);
  }
}

/** The search dialog with the file's name (without extension) filled in. */
function findUsages(row) {
  SET.contentSearch = { ...(SET.contentSearch || {}), files: "*.lua;*.lub;*.txt;*.xml", query: baseOf(row.name).replace(/\.[^.]+$/, ""), regex: false, word: false, inList: false };
  return doSearchContents();
}

/** A new archive with only this entry, or everything below this folder. */
async function exportRows(list, row) {
  const indices = await entriesIn(list, row);
  if (!indices.length) return note(t("ext-title"), t("ext-empty-folder"));
  const method = el("select", { class: "field" }, ...["zlib", "lzma", "zopfli"].map((m) => el("option", { value: m, text: t(`exp-m-${m}`) })));
  const go = await ask(t("exp-title"), [el("p", { text: t("exp-intro", { n: indices.length }) }), el("div", { class: "form" }, el("label", {}, `${t("exp-method")} `, method)), el("p", { class: "muted", text: t("exp-note") })], [{ label: t("exp-choose"), value: true, primary: true }, { label: "Cancel", value: null }]);
  if (!go) return;
  const base = baseOf(row.name).replace(/\.[^.]+$/, "") || "export";
  const dst = await T.dialog.save({ title: t("exp-save-as"), defaultPath: `${base}.grf`, filters: [{ name: "GRF, GPF, GSF", extensions: ["grf", "gpf", "gsf"] }] });
  if (!dst) return;
  const job = showJob(t("exp-running"));
  try {
    const r = await invoke("export_selection", { indices, dst, method: method.value });
    job.close();
    setStatus(t("exp-status", { n: r.files, dst }));
    const open = await ask(t("exp-done"), el("p", { text: t("exp-done-body", { files: t("pvw-files", { n: r.files }), dst, size: fmtBytes(r.file_bytes), verified: r.verified ? t("verified-yes") : t("verified-no") }) }), [{ label: t("dlg-open-it"), value: true, primary: true }, { label: "Close", value: false }], "success");
    if (open) await openPath(dst, true);
  } catch (e) {
    job.close();
    const cancelled = /cancel/i.test(plainErr(e));
    setStatus(cancelled ? t("nothing-written") : t("exp-failed"));
    await note(cancelled ? t("nothing-written") : t("exp-failed"), cancelled ? "" : errText(e), !cancelled);
  }
}

/** Everything the archive knows about an entry (and its checksums), or the size of a folder. */
async function showProperties(list, row) {
  const line = (k, v) => [el("dt", { text: k }), el("dd", {}, el("code", { text: String(v) }))];
  if (row.folder) {
    await ask(t("prop-title"), el("dl", { class: "facts selectable" }, ...line(t("prop-path"), row.name), ...line(t("prop-files"), fmtNum(row.count)), ...line(t("prop-total"), `${fmtBytes(row.bytes)} (${fmtNum(row.bytes)} B)`)), [{ label: "OK", value: true, primary: true }], "info");
    return;
  }
  const d = await invoke("details", { index: row.index });
  const h = await invoke("entry_hashes", { index: row.index });
  await ask(t("prop-title"), el("dl", { class: "facts selectable" }, ...line(t("prop-path"), d.name), ...line(t("prop-type"), row.kind), ...line(t("prop-size"), `${fmtBytes(d.size)} (${fmtNum(d.size)} B)`), ...line(t("prop-stored"), `${fmtBytes(d.stored)} (${fmtNum(d.stored)} B)`), ...line(t("prop-codec"), d.codec), ...line(t("prop-offset"), fmtNum(d.offset)), ...line(t("prop-flags"), `0x${d.flags.toString(16).padStart(2, "0")}`), ...line("CRC32", h.crc32), ...line("MD5", h.md5), ...line("SHA-256", h.sha256)), [{ label: "OK", value: true, primary: true }], "info");
}

// ---------- wiring ----------

let searchTimer = 0;
$("btn-open").addEventListener("click", chooseArchive);
$("btn-settings").addEventListener("click", openSettings);
$("btn-open-2").addEventListener("click", chooseArchive);
$("btn-tour").addEventListener("click", () => openTour());
$("btn-keys").addEventListener("click", () => openShortcuts());
$("btn-merge").addEventListener("click", doMerge);
$("btn-replace").addEventListener("click", doReplace);
$("btn-add").addEventListener("click", doAdd);
$("btn-remove").addEventListener("click", doRemove);
$("btn-changes").addEventListener("click", doChanges);
$("btn-undo").addEventListener("click", () => doHistory(false));
$("btn-redo").addEventListener("click", () => doHistory(true));
for (const [id, redo] of [["btn-undo", false], ["btn-redo", true]]) {
  $(id).addEventListener("contextmenu", (e) => {
    e.preventDefault();
    showHistoryMenu($(id), redo);
  });
}
// the mouse's back and forward buttons
window.addEventListener("mouseup", (e) => {
  if ((e.button === 3 || e.button === 4) && !document.querySelector("dialog[open]")) {
    e.preventDefault();
    navigate(e.button === 3);
  }
});
// In the macOS app the menu's Undo/Redo entries take Cmd+Z and Shift+Cmd+Z. WebView2 keeps
// Ctrl+Z and Ctrl+Shift+Z for its own editing and never passes them to the menu (measured in the
// Windows VM: the page sees the key, the menu does not fire), so there, on Linux, in a browser and
// for Ctrl+Y (the Windows habit) the keys are handled here; in text fields they stay with the field.
const IN_APP = !!window.__TAURI_INTERNALS__;
const MENU_TAKES_Z = IN_APP && /Mac/.test(navigator.platform);
document.addEventListener("keydown", (e) => {
  if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
  const k = e.key.toLowerCase();
  if (k === "y" && !e.shiftKey && !e.metaKey) {
    e.preventDefault();
    return editHistory(true);
  }
  if (k !== "z" || MENU_TAKES_Z) return;
  const a = e.target;
  if ((a instanceof HTMLInputElement && !["checkbox", "color", "button"].includes(a.type)) || a instanceof HTMLTextAreaElement) return;
  e.preventDefault();
  editHistory(e.shiftKey);
}, true);
// in the app the browser's own menu (Reload, Inspect …) is of no use; text keeps its Copy
document.addEventListener("contextmenu", (e) => {
  if (IN_APP && !e.defaultPrevented && !e.target.closest("input, textarea, pre, .selectable, [contenteditable]")) e.preventDefault();
});
$("btn-build").addEventListener("click", doBuild);
$("btn-extract").addEventListener("click", doExtract);
$("btn-verify").addEventListener("click", doVerify);
$("search").addEventListener("input", (e) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => runSearch(e.target.value.trim()), 180);
});
// from the search field to the results: ↓ or Enter; Esc clears it, a second Esc goes to the list
$("search").addEventListener("keydown", async (e) => {
  if (e.isComposing || e.altKey || e.ctrlKey || e.metaKey) return;
  const toList = async () => {
    $("viewport").focus();
    if (!S.total || mainList.st.selected != null) return;
    // in a tree the first lines are the folders of the hits: the first file is what was searched for
    const first = S.rows.findIndex((r) => r && !r.folder);
    await select(first >= 0 ? S.offset + first : 0);
  };
  if (e.key === "Enter" || e.key === "ArrowDown") {
    e.preventDefault();
    if (e.key === "Enter") {
      clearTimeout(searchTimer);
      await runSearch(e.target.value.trim());
    }
    return toList();
  }
  if (e.key === "Escape") {
    e.preventDefault();
    if (!e.target.value) return toList();
    e.target.value = "";
    clearTimeout(searchTimer);
    await runSearch("");
  }
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
// type-to-select: the letters typed within a second form one prefix
const TYPE_AHEAD = { buf: "", at: 0, seq: 0, timer: 0 };
async function typeAhead(list, ch) {
  const now = Date.now();
  TYPE_AHEAD.buf = now - TYPE_AHEAD.at > 1000 ? ch : TYPE_AHEAD.buf + ch;
  TYPE_AHEAD.at = now;
  clearTimeout(TYPE_AHEAD.timer);
  TYPE_AHEAD.timer = setTimeout(() => (TYPE_AHEAD.buf = ""), 1000);
  const buf = TYPE_AHEAD.buf;
  // one letter, or the same letter again and again, steps to the next match; a longer prefix may stay
  const repeat = [...buf].every((c) => c.toLowerCase() === buf[0].toLowerCase());
  const cur = list.st.selected ?? -1;
  const seq = ++TYPE_AHEAD.seq;
  try {
    const line = await invoke("find_line", { view: list.view, prefix: repeat ? buf[0] : buf, start: Math.max(0, repeat ? cur + 1 : cur) });
    if (seq !== TYPE_AHEAD.seq) return;
    if (line != null) await list.pick(line);
    else setStatus(t("ta-none", { prefix: buf }));
  } catch {}
}

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
  vp.addEventListener("contextmenu", async (e) => {
    e.preventDefault();
    if (!S.info) return;
    const cell = e.target.closest(".row");
    let pos = cell ? Number(cell.dataset.pos) : list.st.selected;
    if (pos == null) return;
    let [x, y] = [e.clientX, e.clientY];
    if (!cell) {
      // the Menu key or Shift+F10: next to the selected line
      const b = (vp.querySelector(`.row[data-pos="${pos}"]`) || vp).getBoundingClientRect();
      [x, y] = [b.left + 32, b.bottom];
    }
    await list.pick(pos, false);
    const row = list.row(pos);
    if (row) showContextMenu(x, y, entryMenu(list, row), vp, rowLabel(row));
  });
  vp.addEventListener("dblclick", (e) => {
    const row = e.target.closest(".row");
    if (row && list.row(Number(row.dataset.pos))?.folder) toggleFolder(list, Number(row.dataset.pos));
  });
  vp.addEventListener("keydown", async (e) => {
    const st0 = list.st;
    const sel0 = st0.selected != null ? list.row(st0.selected) : null;
    const mac = /Mac/.test(navigator.platform);
    const cmd = mac ? e.metaKey : e.ctrlKey;
    // shortcuts on the selected line, as in Finder and Explorer
    if (sel0 && S.info) {
      const run = (f) => (e.preventDefault(), f());
      if (e.key === "F2" && !cmd && !e.altKey) return run(() => renameRow(list, sel0));
      if ((e.key === "Delete" && !cmd) || (mac && e.key === "Backspace" && e.metaKey)) return run(() => deleteRow(list, sel0));
      if ((mac && e.metaKey && e.key.toLowerCase() === "i") || (!mac && e.altKey && e.key === "Enter")) return run(() => showProperties(list, sel0));
      if (cmd && e.shiftKey && e.key.toLowerCase() === "c") return run(() => copyText(sel0.name));
      if (!sel0.folder && ((mac && e.metaKey && e.altKey && e.code === "KeyR") || (!mac && e.shiftKey && e.altKey && e.code === "KeyR"))) return run(() => invoke("reveal_entry", { index: sel0.index }).catch(() => {}));
    }
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    // type-to-select: letters jump to the next name that starts with them ("*" in a tree opens all)
    if (e.key.length === 1 && (e.key !== " " || TYPE_AHEAD.buf) && !(e.key === "*" && !TYPE_AHEAD.buf)) {
      e.preventDefault();
      return typeAhead(list, e.key);
    }
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
  // Back/Forward: Alt+arrows (Windows, Linux), Cmd+[ and Cmd+] (macOS)
  const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
  if (!typing && !document.querySelector("dialog[open]") && ((e.altKey && !e.ctrlKey && !e.metaKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) || (e.metaKey && (e.key === "[" || e.key === "]")))) {
    e.preventDefault();
    navigate(e.key === "ArrowLeft" || e.key === "[");
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
  if ((e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "e" && S.info && !document.querySelector("dialog[open]")) {
    e.preventDefault();
    once("extract-selection", quickExtract);
    return;
  }
  if ((e.metaKey || e.ctrlKey) && (e.key === "/" || e.key === "?") && !document.querySelector("dialog[open]")) {
    e.preventDefault();
    once("shortcuts", openShortcuts);
    return;
  }
  if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "f" && S.info && !document.querySelector("dialog[open]")) {
    e.preventDefault();
    doSearchContents();
    return;
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
  T.event.listen("tabs-changed", () => refreshTabs());
  T.event.listen("menu-command", (e) => runMenuCommand(e.payload));
  T.event.listen("tauri://drag-drop", (e) => {
    const p = e.payload?.paths?.[0];
    if (p) openPath(p);
  });
}

// the native menu bar offers every toolbar command; a disabled button means the command does not apply now
const MENU_BUTTONS = { open: "btn-open", build: "btn-build", merge: "btn-merge", extract: "btn-extract", verify: "btn-verify", add: "btn-add", replace: "btn-replace", remove: "btn-remove", changes: "btn-changes" };
const NEEDS_ARCHIVE = new Set(["extract-selection", "new-act", "new-folder", "thor-removals", "nav-back", "nav-forward", "history", "search-contents", "close-archive", "save", "save-as", "save-repack", "save-compact", "save-encrypt", "save-decrypt", "archive-settings", "hash", "compare", "reduce-images", "flat-maps"]);
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
  "check-updates": () => checkForUpdates(true),
  hash: doHashes,
  compare: doCompare,
  "search-contents": doSearchContents,
  "extract-selection": () => once("extract-selection", quickExtract),
  shortcuts: () => once("shortcuts", openShortcuts),
  tour: () => openTour(),
  "data-folder": () => invoke("open_data_folder"),
  history: openHistory,
  "text-encoding": doTextEncoding,
  "nav-back": () => navigate(true),
  "nav-forward": () => navigate(false),
  "recent-clear": clearRecent,
  "reduce-images": () => doReduceImages(),
  "flat-maps": () => doFlatMaps(),
  "new-act": doNewAct,
  "new-folder": () => doNewFolder(),
  "thor-removals": doThorRemovals,
};
function runMenuCommand(id) {
  if (id === "undo" || id === "redo") return editHistory(id === "redo");
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
invoke("set_tabs_enabled", { on: SET.tabs !== false }).catch(() => {});
refreshTabs();

// once per start, from the first window only, after the window has settled
if (SET.checkUpdates && T.window?.getCurrentWindow?.().label === "main") setTimeout(() => checkForUpdates(false).catch(() => {}), 4000);
// the first start shows the tour (not when a file was opened with the app, and only in the first window)
const FIRST_WINDOW = !IN_APP || T.window?.getCurrentWindow?.().label === "main";

invoke("startup_path")
  .then(async (p) => {
    if (!p && !SET.tourSeen && FIRST_WINDOW && !(SET.reopen && SET.lastArchive)) setTimeout(() => !document.querySelector("dialog[open]") && openTour(), 600);
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
