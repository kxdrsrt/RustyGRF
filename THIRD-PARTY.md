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
| `aes` | 0.9.4 | MIT OR Apache-2.0 | AES-128 für Gravitys geschützte Einträge (`gravity.rs`); bringt `cipher`, `inout`, `cpubits` (MIT OR Apache-2.0) mit | M6 (2026-11-19) |
| `twox-hash` | 2.1.5 | MIT | XXH32 und XXH3-128 für Gravitys geschützte Einträge (`xxhash-rust` wäre BSL-1.0 gewesen, nicht in `deny.toml`) | M6 (2026-11-19) |
| `lz4_flex` | 0.14.2 | MIT | LZ4-Blöcke in Gravitys geschützten Einträgen (nur Entpacken, `safe-decode`) | M6 (2026-11-19) |
| `symphonia` (mit `-core`, `-bundle-mp3`, `-codec-vorbis`, `-format-ogg`, `-metadata`, `-common`) | 0.6.1 | MPL-2.0 | MP3 und Ogg Vorbis lesen (Merkmal `lossy-audio`) | 2026-11-21 |
| `mp3lame-encoder` / `mp3lame-sys` | 0.3.0 / 0.2.0 | LGPL-3.0 (LAME) | MP3 schreiben, C-Quelltext wird mitgebaut (Merkmal `lossy-audio`) | 2026-11-21 |
| `vorbis_rs` (mit `aotuv_lancer_vorbis_sys`, `ogg_next_sys`) | 0.5.6 | BSD-3-Clause | Ogg Vorbis schreiben, C-Quelltext wird mitgebaut (Merkmal `lossy-audio`) | 2026-11-21 |

**Testdaten aus GRF Editor:** `crates/rgrf-core/tests/fixtures/protected-grf.bin` (59.280 Byte, SHA-256 `e6d1ff95…1c4f`) ist GRF Editors `Tests/fixtures/protected-grf.bin` unverändert: laut dessen `Tests/README.md` nur erzeugte Bytes, keine Client-Dateien; übernommen mit Tokeiburus Erlaubnis (siehe `docs/05-licensing.md`).

**Selbst erzeugte jRO-Testdaten:** `crates/rgrf-core/tests/fixtures/protected-grf-jro.bin` (166.638 Byte, SHA-256 `731b998c…20b7`): 29 Einträge im selben Aufbau, in der Windows-VM von einem kleinen Programm aus GRF Editors Decoder-Quellen (`ProtectedGrfDecoder.cs`, `ProtectedGrfCiphers.cs`, `ProtectedJroDecoder.cs`, `ProtectedCnDecoder.cs`, `JroArchiveProfiles.cs`, `ZlibNet`) mit BouncyCastle.Cryptography 2.7.0 (MIT), System.IO.Hashing 8.0.0 (MIT) und K4os.Compression.LZ4 1.3.8 (MIT) erzeugt und dort mit `ProtectedGrfDecoder.Decode` geprüft; nur Zufallsbytes, keine Client-Dateien. Das Programm liegt in `tools/jro-vectors` (eigener Code; GRF Editors Quellen kopiert `build.ps1` beim Bauen dazu).

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

