"use strict";

const T = window.__TAURI__;
const invoke = (cmd, args) => T.core.invoke(cmd, args);
const ROW_H = 24;
const OVERSCAN = 6;
const $ = (id) => document.getElementById(id);

const ICONS = { image: "▨", sprite: "◆", animation: "▶", palette: "◐", map: "▦", model: "◇", audio: "♪", text: "≡", effect: "✦" };

const S = {
  info: null,
  total: 0,
  offset: 0, // position of rows[0] in the filtered list
  rows: [],
  selected: null, // position in the filtered list
  query: "",
  loading: false,
  pending: false,
  previewToken: 0,
  pending: [], // changes waiting to be saved
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
    b.replaceChildren(...(Array.isArray(body) ? body : [body]));
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
    filters: [{ name: "GRF archives", extensions: ["grf", "gpf"] }],
  });
  if (path) await openPath(Array.isArray(path) ? path[0] : path);
}

async function openPath(path) {
  setStatus(`Opening ${path} …`);
  try {
    S.info = await invoke("open_archive", { path });
  } catch (e) {
    setStatus("Could not open the archive");
    await note("Could not open", errText(e), true);
    return;
  }
  document.title = `${S.info.file_name} - RustyGRF`;
  $("empty").hidden = true;
  $("viewport").hidden = false;
  for (const id of ["search", "btn-extract", "btn-verify", "names", "btn-add", "btn-changes"]) $(id).disabled = false;
  S.pending = [];
  updateEditBar();
  $("search").value = "";
  $("names").value = "cp949";
  S.query = "";
  await runSearch("");
  setStatus(
    `${S.info.file_name}: GRF ${S.info.format}, ${fmtNum(S.info.files)} files, ${fmtBytes(S.info.file_size)}`,
  );
  $("viewport").focus();
}

async function runSearch(q) {
  S.query = q;
  S.total = await invoke("search", { query: q });
  S.selected = null;
  updateEditBar();
  S.rows = [];
  S.offset = 0;
  $("spacer").style.height = `${S.total * ROW_H}px`;
  $("viewport").scrollTop = 0;
  $("count").textContent = q ? `${fmtNum(S.total)} of ${fmtNum(S.info.entries)}` : `${fmtNum(S.total)} entries`;
  clearPreview();
  await loadWindow();
}

function visibleRange() {
  const vp = $("viewport");
  const first = Math.max(0, Math.floor(vp.scrollTop / ROW_H) - OVERSCAN);
  const last = Math.min(S.total, Math.ceil((vp.scrollTop + vp.clientHeight) / ROW_H) + OVERSCAN);
  return [first, last];
}

async function loadWindow() {
  if (S.loading) {
    S.pending = true;
    return;
  }
  S.loading = true;
  try {
    do {
      S.pending = false;
      const [first, last] = visibleRange();
      const want = Math.max(last - first, 20);
      const offset = Math.max(0, first - want);
      const limit = want * 3;
      if (S.total === 0) {
        S.rows = [];
        S.offset = 0;
      } else {
        S.rows = await invoke("page", { offset, limit });
        S.offset = offset;
      }
      render();
    } while (S.pending);
  } catch (e) {
    setStatus(`Error: ${errText(e)}`);
  } finally {
    S.loading = false;
  }
}

function render() {
  const [first, last] = visibleRange();
  const rows = $("rows");
  rows.style.top = "0";
  const out = [];
  for (let pos = first; pos < last; pos++) {
    const r = S.rows[pos - S.offset];
    if (!r) continue;
    const cut = r.name.lastIndexOf("\\");
    const base = cut >= 0 ? r.name.slice(cut + 1) : r.name;
    const dir = cut >= 0 ? r.name.slice(0, cut) : "";
    const row = el(
      "div",
      { class: `row${pos === S.selected ? " selected" : ""}`, style: `top:${pos * ROW_H}px`, title: r.name, "data-pos": pos },
      el("span", { class: "icon", text: ICONS[r.kind] || "·" }),
      el("span", { class: "name" }, base, el("span", { class: "muted", text: dir ? `  ${dir}` : "" })),
      el("span", { class: "size", text: fmtBytes(r.size) }),
    );
    out.push(row);
  }
  rows.replaceChildren(...out);
}

function onScroll() {
  render();
  const [first, last] = visibleRange();
  const have = S.rows.length;
  const lo = S.offset;
  const hi = S.offset + have;
  const needLow = first < lo && lo > 0;
  const needHigh = last > hi && hi < S.total;
  if (needLow || needHigh) loadWindow();
}

function select(pos, scrollIntoView = true) {
  if (S.total === 0) return;
  pos = Math.max(0, Math.min(S.total - 1, pos));
  S.selected = pos;
  updateEditBar();
  const vp = $("viewport");
  if (scrollIntoView) {
    const top = pos * ROW_H;
    if (top < vp.scrollTop) vp.scrollTop = top;
    else if (top + ROW_H > vp.scrollTop + vp.clientHeight) vp.scrollTop = top + ROW_H - vp.clientHeight;
  }
  render();
  const r = S.rows[pos - S.offset];
  if (r) showPreview(r);
  else loadWindow().then(() => S.rows[pos - S.offset] && showPreview(S.rows[pos - S.offset]));
}

// ---------- preview ----------

function clearPreview() {
  $("preview-head").replaceChildren();
  $("preview-body").replaceChildren(el("p", { class: "placeholder", text: S.info ? "Select an entry to preview it." : "" }));
}

async function showPreview(row, action) {
  const token = ++S.previewToken;
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
  head.replaceChildren(el("div", { class: "title", text: details.name }), meta);
  body.replaceChildren(...renderPreview(preview, row, meta));
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
      return [note, pre];
    }
    case "frames":
      return [
        el(
          "div",
          { class: "frames" },
          p.frames.map((f) =>
            el("figure", {}, el("div", { class: "checker" }, el("img", { src: dataUrl("image/png", f.png), alt: f.label })), el("figcaption", { text: f.label })),
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
      );
      return [controls, el("div", { class: "checker" }, el("img", { src: dataUrl("image/gif", p.gif), width: p.width, height: p.height, alt: "animation" })), infoBlock(p.info), p.note ? el("div", { class: "info", text: `warning: ${p.note}` }) : null];
    }
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

async function doExtract() {
  if (!S.info) return;
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  const buttons = [];
  if (sel) buttons.push({ label: "Selected entry", value: "one", primary: true });
  buttons.push({ label: `All ${fmtNum(S.total)} in the list`, value: "all", primary: !sel });
  buttons.push({ label: "Cancel", value: null });
  const what = await ask("Extract", el("p", { text: S.query ? `The list is filtered by "${S.query}".` : "The list shows the whole archive." }), buttons);
  if (!what) return;
  const dest = await T.dialog.open({ directory: true, multiple: false, title: "Extract into folder" });
  if (!dest) return;
  const indices = what === "one" ? [sel.index] : [];
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
  $("btn-replace").disabled = S.selected == null;
  $("btn-remove").disabled = !S.info;
  $("btn-changes").textContent = `Changes (${S.pending.length})…`;
}

function setPending(rows) {
  S.pending = rows;
  updateEditBar();
}

async function doReplace() {
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  if (!sel) return;
  const file = await T.dialog.open({ multiple: false, directory: false, title: `File that replaces ${sel.name}` });
  if (!file) return;
  try {
    setPending(await invoke("stage_replace", { index: sel.index, file }));
    setStatus(`Replacement staged for ${sel.name}; save it with Changes…`);
  } catch (e) {
    await note("Could not stage the replacement", errText(e), true);
  }
}

async function doAdd() {
  const file = await T.dialog.open({ multiple: false, directory: false, title: "File to add" });
  if (!file) return;
  const sel = S.selected != null ? S.rows[S.selected - S.offset] : null;
  const folder = sel ? sel.name.slice(0, sel.name.lastIndexOf("\\") + 1) : "data\\";
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
  if (sel) buttons.push({ label: "Selected entry", value: "one", primary: true });
  buttons.push({ label: `All ${fmtNum(S.total)} in the list`, value: "all", primary: !sel });
  buttons.push({ label: "Cancel", value: null });
  const what = await ask("Remove from the saved archive", el("p", { text: "Nothing is deleted now: the entries are left out when you save the changes as a new archive." }), buttons);
  if (!what) return;
  try {
    setPending(await invoke("stage_remove", { indices: what === "one" ? [sel.index] : [] }));
    setStatus(`${what === "one" ? "1 entry" : `${fmtNum(S.total)} entries`} marked for removal`);
  } catch (e) {
    await note("Could not stage the removal", errText(e), true);
  }
}

async function doChanges() {
  if (!S.pending.length) {
    await note("Changes", "No pending changes. Use Replace…, Add file… or Remove; the open archive is never modified, the result is saved as a new archive.");
    return;
  }
  const list = el("ul", { class: "pending" }, ...S.pending.slice(0, 500).map((p) => el("li", {}, el("span", { class: "kind", text: p.kind }), el("span", { text: p.name }), p.file ? el("span", { class: "file", text: p.file }) : null)));
  const more = S.pending.length > 500 ? el("p", { class: "muted", text: `… and ${fmtNum(S.pending.length - 500)} more` }) : null;
  const act = await ask(`${fmtNum(S.pending.length)} pending change(s)`, [list, more], [{ label: "Save as new archive…", value: "save", primary: true }, { label: "Discard all", value: "discard" }, { label: "Close", value: null }]);
  if (act === "discard") {
    await invoke("discard_pending");
    setPending([]);
    setStatus("Changes discarded");
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
    if (open) await openPath(dst);
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
  const format = el("select", {}, el("option", { value: "0x200", text: "0x200 (every client)", selected: true }), el("option", { value: "0x300", text: "0x300 (64-bit offsets, for archives over 4 GB)" }));
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
  const picked = await T.dialog.open({ multiple: true, directory: false, title: "Archives to merge (at least two)", filters: [{ name: "GRF archives", extensions: ["grf", "gpf"] }] });
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
    $("search").value = "";
    await runSearch("");
  } catch (err) {
    setStatus(`Error: ${errText(err)}`);
  }
});
$("viewport").addEventListener("scroll", onScroll, { passive: true });
$("viewport").addEventListener("click", (e) => {
  const row = e.target.closest(".row");
  if (row) select(Number(row.dataset.pos), false);
});
$("viewport").addEventListener("keydown", (e) => {
  const page = Math.max(1, Math.floor($("viewport").clientHeight / ROW_H) - 1);
  const cur = S.selected ?? -1;
  const moves = { ArrowDown: cur + 1, ArrowUp: cur - 1, PageDown: cur + page, PageUp: cur - page, Home: 0, End: S.total - 1 };
  if (e.key in moves) {
    e.preventDefault();
    select(cur < 0 && e.key === "ArrowDown" ? 0 : moves[e.key]);
  }
});
window.addEventListener("resize", () => {
  render();
  onScroll();
});
window.addEventListener("keydown", (e) => {
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
  T.event.listen("job-progress", (e) => S.onJob?.(e.payload));
  T.event.listen("tauri://drag-drop", (e) => {
    const p = e.payload?.paths?.[0];
    if (p) openPath(p);
  });
}

clearPreview();

invoke("startup_path")
  .then((p) => p && openPath(p))
  .catch(() => {});
