// Stand-in for the Tauri backend so the page can be tried in a normal browser (layout, keyboard,
// scrolling). It is only active when `window.__TAURI__` is missing, so the app never uses it.
if (!window.__TAURI__) {
  const listeners = {};
  let pendingRows = [];
  const exts = ["spr", "act", "bmp", "txt", "gat", "rsw", "rsm", "wav", "lub", "pal", "tga"];
  const all = [];
  for (let i = 0; i < 24000; i++) {
    const ext = exts[i % exts.length];
    all.push({ index: i, name: `data\\${["sprite", "texture", "model", "wav", "luafiles514"][i % 5]}\\folder_${i % 37}\\item_${i}.${ext}`, size: (i * 7919) % 90000, kind: { spr: "sprite", act: "animation", bmp: "image", txt: "text", gat: "map", rsw: "map", rsm: "model", wav: "audio", lub: "text", pal: "palette", tga: "image" }[ext] });
  }
  let filtered = all;
  const views = [];
  // the visible lines of a mock tree (folders first, then files; same shape as the real backend)
  const mockLines = (v) => {
    const out = [];
    const files = v.spec.layout === "tree";
    const visit = (n, depth) => {
      out.push({ index: 4294967295, name: n.path, size: 0, kind: "folder", stored: 0, depth, folder: true, expanded: v.expanded.has(n.key), count: n.count, bytes: n.bytes });
      if (!v.expanded.has(n.key)) return;
      n.folders.forEach((c) => visit(c, depth + 1));
      if (files) n.files.forEach((r) => out.push({ ...r, stored: Math.floor(r.size / 2), depth: depth + 1, folder: false, expanded: false, count: 0, bytes: r.size }));
    };
    v.tree.root.folders.forEach((c) => visit(c, 0));
    if (files) v.tree.root.files.forEach((r) => out.push({ ...r, stored: Math.floor(r.size / 2), depth: 0, folder: false, expanded: false, count: 0, bytes: r.size }));
    v.lines = out;
  };

  const swatch = (w, h, color, grid) => {
    const c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    const g = c.getContext("2d");
    g.fillStyle = color;
    g.fillRect(0, 0, w, h);
    if (grid) {
      g.fillStyle = "rgba(255,255,255,0.7)";
      for (let i = 0; i < w; i += 4) g.fillRect(i, 0, 1, h);
    }
    return c.toDataURL("image/png").split(",")[1];
  };

  const preview = (i, action) => {
    const r = all[i];
    const ext = r.name.split(".").pop();
    switch (ext) {
      case "bmp":
      case "tga":
        return { kind: "image", mime: "image/png", data: swatch(48, 32, "#c2410c", true) };
      case "txt":
      case "lub":
        return { kind: "text", text: "-- sample text\nt = {\n  name = 'Poring',\n  hp = 50,\n}\n".repeat(30), encoding: "CP949", truncated: false };
      case "spr":
        return { kind: "frames", frames: [0, 1, 2, 3, 4, 5].map((n) => ({ png: swatch(40 + n * 6, 50, `hsl(${n * 50},70%,50%)`), width: 40 + n * 6, height: 50, label: `${n}: ${40 + n * 6} x 50` })), info: ["SPR 2.1 - 6 palette images, 0 true-colour images"] };
      case "act":
        return { kind: "animation", gif: "R0lGODlhAQABAIAAAP///wAAACH5BAEAAAAALAAAAAABAAEAAAICRAEAOw==", width: 120, height: 120, action: action ?? 0, actions: 8, frames: 4, interval_ms: 100, info: ["ACT 2.5 - 8 actions, 32 frames, 40 layers"], note: null };
      case "gat":
        return { kind: "map", png: swatch(240, 300, "#3f9d47", true), width: 240, height: 300, info: ["GAT 1.2 - 240 x 300 cells", "type 0 (walkable): 40000 cells"] };
      case "rsw":
      case "rsm":
        return { kind: "info", title: "World", lines: ["RSW 2.1 - ground x.gnd", "1304 models (63 files), 25 sounds", "Models:", "tree_01.rsm", "house_02.rsm"] };
      case "wav":
        return { kind: "message", text: "Audio preview needs the app (mock has no audio)." };
      default:
        return { kind: "hex", dump: "00000000  47 52 41 54 01 02 00 00 00 00 00 00 00 00 00 00  GRAT............\n".repeat(8), size: 4096 };
    }
  };

  window.__TAURI__ = {
    core: {
      invoke: async (cmd, a) => {
        await new Promise((r) => setTimeout(r, 15));
        switch (cmd) {
          case "startup_path":
          case "startup_select":
            return null;
          case "set_names":
            filtered = all;
            return all.length;
          case "open_archive":
            return { path: a.path, file_name: "mock.grf", format: "GRF 0x200", entries: all.length, files: all.length, file_size: 4155996888, encryption: null };
          case "set_view": {
            const sp = a.spec;
            let idx = all;
            const q = sp.query.toLowerCase().replaceAll("*", "");
            if (q) idx = idx.filter((r) => r.name.toLowerCase().includes(q));
            if (sp.kinds.length) idx = idx.filter((r) => sp.kinds.includes(r.kind === "text" && false ? "x" : r.kind));
            if (sp.min_size) idx = idx.filter((r) => r.size >= sp.min_size);
            if (sp.max_size) idx = idx.filter((r) => r.size <= sp.max_size);
            if (sp.folder && !q) idx = idx.filter((r) => (sp.recursive ? r.name.toLowerCase().startsWith(sp.folder.toLowerCase() + "\\") : r.name.toLowerCase().startsWith(sp.folder.toLowerCase() + "\\") && !r.name.slice(sp.folder.length + 1).includes("\\")));
            const base = (n) => n.slice(n.lastIndexOf("\\") + 1);
            const keys = { name: (r) => base(r.name).toLowerCase(), path: (r) => r.name.toLowerCase(), type: (r) => r.name.split(".").pop(), size: (r) => r.size, stored: (r) => Math.floor(r.size / 2), offset: (r) => r.index };
            idx = [...idx];
            if (keys[sp.sort]) idx.sort((x, y) => (keys[sp.sort](x) > keys[sp.sort](y) ? 1 : keys[sp.sort](x) < keys[sp.sort](y) ? -1 : x.index - y.index));
            if (sp.desc) idx.reverse();
            const v = (views[a.view] ||= { expanded: new Set() });
            v.spec = sp;
            if (sp.layout === "flat") {
              v.tree = null;
              v.flat = idx;
              if (a.view === 0) filtered = idx;
              return { rows: idx.length, matches: idx.length };
            }
            const nodes = new Map();
            const node = (path) => {
              if (!path) return root;
              const k = path.toLowerCase();
              if (!nodes.has(k)) {
                const cut = path.lastIndexOf("\\");
                const n = { path, key: k, folders: [], files: [], count: 0, bytes: 0 };
                nodes.set(k, n);
                node(cut >= 0 ? path.slice(0, cut) : "").folders.push(n);
              }
              return nodes.get(k);
            };
            const root = { path: "", key: "", folders: [], files: [], count: 0, bytes: 0 };
            for (const r of idx) node(r.name.includes("\\") ? r.name.slice(0, r.name.lastIndexOf("\\")) : "").files.push(r);
            const sum = (n) => {
              n.count = n.files.length;
              n.bytes = n.files.reduce((t, r) => t + r.size, 0);
              n.folders.sort((x, y) => (x.path.toLowerCase() > y.path.toLowerCase() ? 1 : -1));
              for (const c of n.folders) {
                sum(c);
                n.count += c.count;
                n.bytes += c.bytes;
              }
            };
            sum(root);
            v.tree = { root, nodes };
            if (sp.auto_expand) v.expanded = sp.layout === "tree" && idx.length <= 3000 ? new Set(nodes.keys()) : new Set();
            if (a.view === 0) filtered = idx;
            mockLines(v);
            return { rows: v.lines.length, matches: idx.length };
          }
          case "page_view": {
            const v = views[a.view] || { flat: all, tree: null };
            if (!v.tree) return (v.flat || all).slice(a.offset, a.offset + a.limit).map((r) => ({ ...r, stored: Math.floor(r.size / 2), depth: 0, folder: false, expanded: false, count: 0, bytes: r.size }));
            return v.lines.slice(a.offset, a.offset + a.limit);
          }
          case "tree_toggle": {
            const v = views[a.view];
            const k = a.path.toLowerCase();
            if (!v.expanded.delete(k)) v.expanded.add(k);
            mockLines(v);
            return v.lines.length;
          }
          case "tree_set_all": {
            const v = views[a.view];
            v.expanded = a.open ? new Set(v.tree.nodes.keys()) : new Set();
            mockLines(v);
            return v.lines.length;
          }
          case "locate": {
            const v = views[a.view];
            if (!v.tree) return { line: a.entry == null ? null : v.flat.findIndex((r) => r.index === a.entry), rows: v.flat.length };
            const path = a.entry != null ? all[a.entry].name : a.folder || "";
            const parts = path.split("\\");
            for (let i = 1; i < parts.length; i++) v.expanded.add(parts.slice(0, i).join("\\").toLowerCase());
            mockLines(v);
            return { line: v.lines.findIndex((r) => (a.entry != null ? r.index === a.entry : r.folder && r.name.toLowerCase() === path.toLowerCase())), rows: v.lines.length };
          }
          case "folder_entries": {
            const v = views[a.view];
            const n = v.tree.nodes.get(a.path.toLowerCase());
            const out = [];
            const walk = (x) => (out.push(...x.files.map((r) => r.index)), x.folders.forEach(walk));
            if (n) walk(n);
            return out;
          }
          case "search": {
            const q = a.query.toLowerCase();
            filtered = !q ? all : all.filter((r) => r.name.toLowerCase().includes(q.replaceAll("*", "")));
            return filtered.length;
          }
          case "page":
            return filtered.slice(a.offset, a.offset + a.limit);
          case "details":
            return { name: all[a.index].name, size: all[a.index].size, stored: Math.floor(all[a.index].size / 2), offset: 46 + a.index * 1000, codec: "zlib", flags: 1 };
          case "preview":
            return preview(a.index, a.action);
          case "extract":
            return { files: a.indices.length || filtered.length, damaged: 0, dest: a.dest };
          case "build_archive": {
            for (const done of [10, 55, 100]) {
              await new Promise((r) => setTimeout(r, 400));
              listeners["job-progress"]?.({ payload: { done, total: 100 } });
            }
            return { files: 100, blocks: 98, shared: 2, unpacked_bytes: 52428800, file_bytes: 18874368, verified: true, skipped: [] };
          }
          case "merge_archives": {
            for (const done of [20, 70, 100]) {
              await new Promise((r) => setTimeout(r, 300));
              listeners["job-progress"]?.({ payload: { done, total: 100 } });
            }
            return { files: 3, file_bytes: 2048, verified: true, damaged: 0, sources: a.req.sources.map((path, i) => ({ path, files: 2, used: i ? 1 : 2, overridden: i ? 1 : 0, ignored_new: 0 })) };
          }
          case "stage_replace":
            pendingRows = pendingRows.filter((p) => p.name !== all[a.index].name);
            pendingRows.push({ kind: "replace", name: all[a.index].name, file: a.file });
            return pendingRows;
          case "stage_add":
            pendingRows.push({ kind: "add", name: a.path.replace(/\//g, "\\"), file: a.file });
            return pendingRows;
          case "stage_remove":
            for (const i of a.indices) pendingRows.push({ kind: "remove", name: all[i].name, file: null });
            return pendingRows;
          case "stage_spr_image":
            pendingRows.push({ kind: "replace", name: all[a.index].name, file: `image ${a.frame} from x.png` });
            return pendingRows;
          case "pal_colors":
            return Array.from({ length: 256 }, (_, i) => (i * 65793).toString(16).padStart(6, "0").slice(-6));
          case "stage_pal_edit":
            pendingRows.push({ kind: "replace", name: all[a.index].name, file: `palette: ${a.edit.op}` });
            return pendingRows;
          case "stage_spr_add":
          case "stage_spr_remove":
          case "stage_spr_palette":
            pendingRows.push({ kind: "replace", name: all[a.index].name, file: cmd });
            return pendingRows;
          case "act_frame": {
            const c = document.createElement("canvas");
            c.width = 120;
            c.height = 100;
            const g = c.getContext("2d");
            g.fillStyle = "#6a9";
            g.fillRect(30 + a.frame * 6, 20, 50, 60);
            return { version: "2.5", actions: 8, frames: 4, action: a.action, frame: Math.min(a.frame, 3), interval_ms: 100, layers: [{ sprite_index: 0, sprite_type: 0, offset_x: 0, offset_y: 0, mirror: false, scale_x: 1, scale_y: 1, rotation: 0, color: [255, 255, 255, 255] }, { sprite_index: 1, sprite_type: 0, offset_x: 5, offset_y: -3, mirror: true, scale_x: 1, scale_y: 1, rotation: 90, color: [255, 128, 0, 200] }], anchors: [{ x: 4, y: -20 }], anchors_supported: true, separate_scale: true, sounds_supported: true, sound: "effect\\hit.wav", sounds: ["effect\\hit.wav", "effect\\miss.wav"], indexed_images: 6, rgba_images: 0, png: c.toDataURL().split(",")[1], width: 120, height: 100, origin_x: 60, origin_y: 80, scale: 4, error: null };
          }
          case "stage_act_edit":
            pendingRows.push({ kind: "replace", name: all[a.index].name, file: `action ${a.action} frame ${a.frame}: ${a.edit.op}` });
            return pendingRows;
          case "text_for_edit":
            return { text: "-- demo\nx = 1\n", format: "Lua 5.1 bytecode", complete: true };
          case "stage_text_edit":
            pendingRows.push({ kind: "replace", name: all[a.index].name, file: "Lua recompiled from edited text" });
            return pendingRows;
          case "stage_act_interval":
            pendingRows.push({ kind: "replace", name: all[a.index].name, file: `action ${a.action}: ${a.ms} ms per frame` });
            return pendingRows;
          case "history":
            return { can_undo: pendingRows.length > 0, can_redo: false };
          case "undo_change":
            pendingRows.pop();
            return pendingRows;
          case "redo_change":
            return pendingRows;
          case "pending":
            return pendingRows;
          case "discard_pending":
            pendingRows = [];
            return null;
          case "save_edited": {
            for (const done of [30, 100]) {
              await new Promise((r) => setTimeout(r, 300));
              listeners["job-progress"]?.({ payload: { done, total: 100 } });
            }
            const n = pendingRows.length;
            pendingRows = [];
            return { added: n, replaced: 0, removed: 0, files: all.length, file_bytes: 4096, verified: true };
          }
          case "cancel_job":
            return null;
          case "verify":
            return { entries: all.length, errors: 0, warnings: 1, infos: 2, issues: ["data\\wav\\worm_tail_damage.wav: stream ends early (1403 of 1410 bytes)"], decoded: a.decode };
        }
        throw new Error(`mock: unknown command ${cmd}`);
      },
    },
    dialog: { open: async (o) => (o?.multiple && !o?.directory && /merge/i.test(o.title || "") ? ["/mock/translation.grf", "/mock/core.grf"] : o?.directory ? "/tmp/extract-here" : "/mock/test.grf"), save: async (o) => `/tmp/${o?.defaultPath || "out.grf"}` },
    event: { listen: (name, fn) => void (listeners[name] = fn) },
  };
}
