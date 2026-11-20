// Translations for the window. Catalogs are Fluent files (`i18n/<language>.ftl`, https://projectfluent.org):
// this file reads the subset the app uses, so no library is needed:
//
//   key = text with { $variable }
//   key = { $n ->
//       [one] { $n } file
//      *[other] { $n } files
//   }
//   key = Label
//       .title = Tooltip          (attributes: .title, .aria-label, .placeholder)
//
// A key missing in a language falls back to English, per key.
// HTML: data-i18n="key" sets the text of a leaf element, data-i18n-attrs="key" sets its attributes.
// Script: t("key", { n: 3 }).
(() => {
  const LANGS = [
    // code, name in its own language, shown as Beta (machine draft, not yet read by a native speaker)
    ["en", "English", false],
    ["de", "Deutsch", true],
    ["es", "Español", true],
    ["pt-BR", "Português (Brasil)", true],
    ["ko", "한국어", true],
    ["ja", "日本語", true],
    ["fr", "Français", true],
    ["id", "Bahasa Indonesia", true],
    ["fil", "Filipino", true],
    ["qps", "Pseudo (test)", false],
  ];
  const state = { lang: "en", rules: new Intl.PluralRules("en"), cat: {}, en: {} };

  /** Splits catalog text into messages: `{key: "pattern source"}`; attributes become `key.attr`. */
  function parse(text) {
    const out = {};
    let key = null;
    let attr = null;
    let lines = [];
    const flush = () => {
      if (key == null) return;
      const body = lines.filter((l, i, a) => !(l.trim() === "" && (i === 0 || i === a.length - 1)));
      const indents = body.filter((l) => l.trim()).slice(1).map((l) => l.match(/^\s*/)[0].length);
      const cut = indents.length ? Math.min(...indents) : 0;
      const src = body.map((l, i) => (i === 0 ? l.trim() : l.slice(Math.min(cut, l.match(/^\s*/)[0].length)))).join("\n").trim();
      out[attr ? `${key}.${attr}` : key] = src;
      lines = [];
    };
    for (const raw of text.split(/\r?\n/)) {
      if (raw.startsWith("#")) continue;
      const msg = /^([A-Za-z][\w-]*)\s*=\s?(.*)$/.exec(raw);
      const att = /^\s+\.([\w-]+)\s*=\s?(.*)$/.exec(raw);
      if (msg) {
        flush();
        key = msg[1];
        attr = null;
        lines = [msg[2]];
      } else if (att && key != null) {
        const k = key;
        flush();
        key = k;
        attr = att[1];
        lines = [att[2]];
      } else if (key != null && /^\s+\S/.test(raw)) {
        lines.push(raw);
      } else if (key != null && raw.trim() === "") {
        lines.push(raw);
      }
    }
    flush();
    return out;
  }

  /** The `{ ... }` placeable starting at `i`: its inner text and the index after it. */
  function block(s, i) {
    let depth = 0;
    for (let j = i; j < s.length; j++) {
      if (s[j] === "{") depth++;
      else if (s[j] === "}" && --depth === 0) return [s.slice(i + 1, j), j + 1];
    }
    return [s.slice(i + 1), s.length];
  }

  const number = (n) => new Intl.NumberFormat(state.lang === "qps" ? "en" : state.lang).format(n);

  function format(src, vars) {
    let out = "";
    for (let i = 0; i < src.length; ) {
      if (src[i] !== "{") {
        out += src[i++];
        continue;
      }
      const [inner, next] = block(src, i);
      i = next;
      const sel = /^\s*\$([\w-]+)\s*->([\s\S]*)$/.exec(inner);
      if (sel) {
        out += format(variant(sel[1], sel[2], vars), vars);
        continue;
      }
      const v = /^\s*\$([\w-]+)\s*$/.exec(inner);
      if (v) {
        const x = vars?.[v[1]];
        out += typeof x === "number" ? number(x) : (x ?? "");
        continue;
      }
      const lit = /^\s*"([\s\S]*)"\s*$/.exec(inner);
      out += lit ? lit[1] : "";
    }
    return out;
  }

  function variant(name, body, vars) {
    const parts = [];
    const re = /^[ \t]*(\*?)\[([^\]]+)\][ \t]*/gm;
    let m;
    let last = null;
    while ((m = re.exec(body))) {
      if (last) last.text = body.slice(last.at, m.index);
      last = { star: !!m[1], key: m[2].trim(), at: re.lastIndex, text: "" };
      parts.push(last);
    }
    if (last) last.text = body.slice(last.at);
    const x = vars?.[name];
    const want = typeof x === "number" ? [String(x), state.rules.select(x)] : [String(x)];
    const hit = parts.find((p) => want.includes(p.key)) || parts.find((p) => p.star) || parts[parts.length - 1];
    return (hit?.text || "").trim();
  }

  const accents = { a: "á", e: "é", i: "í", o: "ö", u: "ü", c: "ç", n: "ñ", s: "š", A: "Å", E: "É", O: "Ø", U: "Ü" };
  const pseudo = (s) => {
    const body = s.replace(/[aeioucnsAEOU]/g, (c) => accents[c]);
    return `[${body} ${"~".repeat(Math.ceil(s.length * 0.4))}]`;
  };

  const raw = (key) => state.cat[key] ?? state.en[key];

  /** The text for `key`; the key itself when no catalog has it (visible, so a gap is noticed). */
  function t(key, vars) {
    const src = raw(key);
    if (src == null) return key;
    const s = format(src, vars);
    return state.lang === "qps" ? pseudo(s) : s;
  }
  const has = (key) => raw(key) != null;

  function load(code) {
    const get = (c) => {
      try {
        const x = new XMLHttpRequest();
        x.open("GET", `i18n/${c}.ftl`, false);
        x.send();
        return x.status === 200 || x.status === 0 ? parse(x.responseText) : {};
      } catch {
        return {};
      }
    };
    if (!Object.keys(state.en).length) state.en = get("en");
    state.cat = code === "en" || code === "qps" ? {} : get(code);
    state.lang = code;
    try {
      state.rules = new Intl.PluralRules(code === "qps" ? "en" : code);
    } catch {
      state.rules = new Intl.PluralRules("en");
    }
    document.documentElement.lang = code === "qps" ? "en" : code;
  }

  /** The catalog language for a setting (`system` follows the system language). */
  function resolve(setting) {
    if (LANGS.some((l) => l[0] === setting)) return setting;
    const want = (navigator.languages?.[0] || navigator.language || "en").toLowerCase();
    const base = want.split("-")[0];
    if (base === "pt") return "pt-BR";
    if (base === "tl" || base === "fil") return "fil";
    if (base === "in") return "id";
    return LANGS.find((l) => l[0] === base) && base !== "qps" ? base : "en";
  }

  /** Puts the catalog on the static page. */
  function apply(root = document) {
    for (const e of root.querySelectorAll("[data-i18n]")) e.textContent = t(e.dataset.i18n);
    for (const e of root.querySelectorAll("[data-i18n-attrs]")) {
      const key = e.dataset.i18nAttrs;
      for (const a of ["title", "aria-label", "placeholder"]) {
        if (has(`${key}.${a}`)) e.setAttribute(a, t(`${key}.${a}`));
      }
      // a button whose words are hidden in narrow windows keeps its name for screen readers and tooltips
      const word = e.tagName === "BUTTON" ? e.querySelector("span[data-i18n]:not(.sr)")?.textContent?.trim() : null;
      if (word && !has(`${key}.aria-label`)) e.setAttribute("aria-label", word.replace(/…$/, ""));
      if (word && !has(`${key}.title`)) e.setAttribute("title", word);
    }
  }

  /** All `menu-*` entries, for the native menu bar. */
  function menuLabels() {
    const out = {};
    for (const k of Object.keys({ ...state.en, ...state.cat })) if (k.startsWith("menu-")) out[k] = t(k);
    return out;
  }

  window.I18N = { LANGS, load, resolve, apply, t, has, menuLabels, parse, format, state };
  window.t = t;
})();
