# Drittbibliotheken

Jede Abhängigkeit wird vor der Aufnahme auf Lizenz geprüft (`cargo deny check licenses`, Konfiguration in `deny.toml`) und hier eingetragen.

| Crate | Version | Lizenz | Zweck | Aufgenommen (Meilenstein) |
|---|---|---|---|---|
| `flate2` | 1.1.11 | MIT OR Apache-2.0 | zlib (Dateitabelle, Einträge), Rust-Backend `miniz_oxide` | M1 |
| `lzma-rust2` | 0.25.0 | Apache-2.0 | LZMA-Einträge lesen (M1) und kodieren (M2, Feature `encoder`) | M1/M2 |
| `zopfli` | 0.8.4 | Apache-2.0 | zlib-kompatible Kompression mit höherem Aufwand (`--method zopfli`) | M2 |
| `sha2` | 0.11.1 | MIT OR Apache-2.0 | SHA-256 für Inhalts-Dedupe und Rückprüfung beim Repack | M2 |
| `unicode-normalization` | 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | Dateinamen von macOS (NFD) beim Bauen in NFC bringen | M2 |
| `crc32fast` | 1.5.5 | MIT OR Apache-2.0 | CRC32 | M1 |
| `md-5` | 0.11.0 | MIT OR Apache-2.0 | MD5 | M1 |
| `rayon` | 1.12.0 | MIT OR Apache-2.0 | parallele Verarbeitung | M1 |
| `thiserror` | 2.0.21 | MIT OR Apache-2.0 | Fehlertypen | M1 |
| `encoding_rs` | 0.8.44 | (Apache-2.0 OR MIT) AND BSD-3-Clause | CP949-Namen (Korea) anzeigen/suchen | M1 |
| `clap` | 4.6.7 | MIT OR Apache-2.0 | CLI | M1 |
| `serde_json` | 1.0.154 | MIT OR Apache-2.0 | JSON-Ausgabe | M1 |
| `anyhow` | 1.0.104 | MIT OR Apache-2.0 | Fehlerbehandlung in der CLI | M1 |
| `png` | 0.18.1 | MIT OR Apache-2.0 | PNG lesen/schreiben (SPR-Frames exportieren/importieren) | M7a |
| `gif` | 0.14.2 | MIT OR Apache-2.0 | animiertes GIF aus gerenderten Frames (`act render --gif`); bringt `color_quant` (MIT) und `weezl` (MIT OR Apache-2.0) mit | M7b |

Stand 2026-11-16: `cargo deny check` (Lizenzen, Verbote, Quellen, Advisories) ohne Befund. Transitive Abhängigkeiten stehen in `Cargo.lock`; erlaubte Lizenzen in `deny.toml`.

## Lua 5.1.5 und 5.0.3 (Quelltext portiert in `crates/rgrf-formats/src/luacomp.rs` und `luacomp50.rs`)
Lexer, Parser und Codegenerator sind aus `llex.c`, `lparser.c` und `lcode.c` von Lua 5.1.5 (`luacomp.rs`) und von Lua 5.0.3 (`luacomp50.rs`) nach Rust übertragen. Lua 5.1 und 5.0 sind unter derselben MIT-Lizenz veröffentlicht (Lua 5.0.3: Copyright (C) 1994-2006 Tecgraf, PUC-Rio; Lua 5.1.5: Copyright (C) 1994-2012 Lua.org, PUC-Rio):

> Copyright (C) 1994-2012 Lua.org, PUC-Rio.
>
> Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

Das Prüfwerkzeug `tools/lua-verify` (nicht ausgeliefert) nutzt außerdem `mlua` (MIT) mit mitgebautem Lua 5.1 und lädt Lua 5.0.3 von lua.org.

