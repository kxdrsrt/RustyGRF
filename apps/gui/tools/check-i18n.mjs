// Checks the translations: `node tools/check-i18n.mjs` (run from apps/gui).
//  - the reader (plurals, attributes, nesting) works,
//  - every catalog has exactly the keys and placeholders of en.ftl,
//  - every key the page and the script use exists in en.ftl.
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";

const ui = path.resolve("ui");
const stub = {
  documentElement: {},
  querySelectorAll: () => [],
};
class XHR {
  open(_m, url) { this.url = url; }
  send() {
    try { this.responseText = fs.readFileSync(path.join(ui, this.url), "utf8"); this.status = 200; } catch { this.status = 404; }
  }
}
const sandbox = { window: {}, document: stub, navigator: { language: "en", languages: ["en"] }, XMLHttpRequest: XHR, Intl, console };
sandbox.window = sandbox;
vm.createContext(sandbox);
vm.runInContext(fs.readFileSync(path.join(ui, "i18n.js"), "utf8"), sandbox);
const { I18N } = sandbox;

let failures = 0;
const fail = (m) => { failures++; console.error("FAIL " + m); };
const eq = (a, b, m) => { if (a !== b) fail(`${m}: got ${JSON.stringify(a)}, want ${JSON.stringify(b)}`); };

// reader
const cat = I18N.parse(`
# comment
a = Hello { $name }
b = { $n ->
    [one] { $n } file
   *[other] { $n } files
 }
c = Label
    .title = Tip
    .aria-label = Name
d = { $n ->
    [0] none
    [one] one
   *[other] many { $n }
 }
`);
eq(cat.a, "Hello { $name }", "plain");
eq(cat["c.title"], "Tip", "attribute");
eq(cat.c, "Label", "value before attributes");
I18N.state.cat = cat;
I18N.state.lang = "en";
eq(I18N.t("a", { name: "Bob" }), "Hello Bob", "variable");
eq(I18N.t("b", { n: 1 }), "1 file", "one");
eq(I18N.t("b", { n: 5 }), "5 files", "other");
eq(I18N.t("b", { n: 1234 }), "1,234 files", "number format");
eq(I18N.t("d", { n: 0 }), "none", "exact number beats category");
eq(I18N.t("missing-key"), "missing-key", "unknown key is shown as is");

// catalogs
const dir = path.join(ui, "i18n");
const read = (f) => I18N.parse(fs.readFileSync(path.join(dir, f), "utf8"));
const vars = (s) => [...new Set([...s.matchAll(/\$([\w-]+)/g)].map((m) => m[1]))].sort().join(",");
const en = read("en.ftl");
const codes = I18N.LANGS.map((l) => l[0]).filter((c) => c !== "en" && c !== "qps");
for (const code of codes) {
  const file = `${code}.ftl`;
  if (!fs.existsSync(path.join(dir, file))) { fail(`${file} is missing`); continue; }
  const c = read(file);
  for (const k of Object.keys(en)) {
    if (!(k in c)) fail(`${file}: missing ${k}`);
    else if (vars(c[k]) !== vars(en[k])) fail(`${file}: ${k} uses variables [${vars(c[k])}], English [${vars(en[k])}]`);
    else if (!c[k] && en[k]) fail(`${file}: ${k} is empty`);
  }
  for (const k of Object.keys(c)) if (!(k in en)) fail(`${file}: ${k} is not in en.ftl`);
  // every language must be able to produce every message
  I18N.load(code);
  for (const k of Object.keys(en)) {
    const out = I18N.t(k, { n: 3, total: 9, name: "x", kind: "y", key: "Ctrl", errors: 1, warnings: 2, infos: 3, entries: 4 });
    if (/\{|\}/.test(out)) fail(`${file}: ${k} still has braces after formatting: ${out}`);
  }
}

// keys used by the page and the script
const used = new Set();
const html = fs.readFileSync(path.join(ui, "index.html"), "utf8");
for (const m of html.matchAll(/data-i18n(?:-attrs)?="([^"]+)"/g)) used.add(m[1]);
const js = fs.readFileSync(path.join(ui, "app.js"), "utf8");
for (const m of js.matchAll(/\bt\("([\w.-]+)"/g)) used.add(m[1]);
for (const m of js.matchAll(/t\(`([\w.-]+)\$\{/g)) {
  for (const k of Object.keys(en)) if (k.startsWith(m[1])) used.add(k);
}
for (const k of used) if (!(k in en) && !(`${k}.title` in en) && !(`${k}.aria-label` in en) && !(`${k}.placeholder` in en)) fail(`used but not in en.ftl: ${k}`);

// the app's CSP must let the page load its catalogs (connect-src replaces default-src for them)
const conf = JSON.parse(fs.readFileSync(path.join(ui, "..", "src-tauri", "tauri.conf.json"), "utf8"));
const csp = conf.app?.security?.csp || "";
const connect = /(?:^|;)\s*connect-src([^;]*)/.exec(csp);
if (connect && !connect[1].split(/\s+/).includes("'self'")) fail(`the CSP's connect-src lacks 'self': the catalogs cannot be loaded (${csp})`);

console.log(`${Object.keys(en).length} keys, ${codes.length} languages checked, ${used.size} keys used by the page and script`);
if (failures) { console.error(`${failures} problem(s)`); process.exit(1); }
console.log("i18n OK");
