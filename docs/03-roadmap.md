# 03 – Roadmap

Jeder Meilenstein hat eine **Abnahme** (messbar). Nichts gilt als fertig, bevor sie bestanden ist.

## M0 – Grundlagen (kein Format-Code)
- [x] Recherche, Messungen, Quellen (siehe `RO/docs/grf-research/`).
- [x] Erlaubnis von Tokeiburu (freigegeben, siehe [05-licensing.md](05-licensing.md)).
- [x] Name (laut Inhaber frei) und Lizenz gewählt: `MIT OR Apache-2.0` (2026-11-16).
- [x] Voraussetzungen auf dem Mac: Rust 1.100.0 stable, rustfmt, clippy, cargo-nextest, cargo-deny (2026-11-16). Windows-VM erst zu M5.
- [x] `CARGO_TARGET_DIR` lokal (`.cargo/config.toml`, nicht im Git → `~/build/rustygrf`). Context7 in den Nutzer-MCP-Konfigurationen eingetragen (neue Session nötig).
- [x] Toolchain-Entscheidung: Rust stable, Edition 2024, keine MSRV; Hauptplattform macOS, Windows-ARM64-VM für Windows-Builds und Referenztests.
- [x] Workspace-Gerüst (`rgrf-core`, `rgrf-cli`, `rgrf-mcp`), `deny.toml`, `NOTICE`, `THIRD-PARTY.md`; fmt/clippy/nextest/deny grün.
- [x] Lokales Git-Repo (kein Remote, kein Push bis zur Freigabe).
- [x] Golden Files für `cdata`, `core`, `cpal`, `rdata`, `data` in der Windows-VM erzeugt (2026-11-16, `tools/golden/run-in-vm.sh`) und gegen den Python-Parser geprüft, siehe `tests/golden/README.md`. 0x300-Archive folgen zu M1b.

## M1 – Lesen und Prüfen (`rgrf info|ls|find|cat|extract|verify`)
- 0x200 öffnen, Tabelle lesen, Einträge auflisten, einzeln und gesamt extrahieren.
- **Dekodieren von zlib und LZMA** (Lesen): `core.grf` hat 4.391 und `cdata.grf` 185 LZMA-Einträge (gemessen), siehe [02-format-notes.md](02-format-notes.md). Kodieren bleibt M2.
- **Abnahme:**
  - Eintragsliste (Pfad, Größen, Flags) für `cdata.grf`, `core.grf`, `cpal.grf`, `data.grf` (202.358 Einträge) identisch mit GRF Editor/`GrfCL`.
  - Hashes (CRC32, MD5) aller Einträge identisch mit den extrahierten Dateien aus `data-official`.
  - `data.grf` öffnen < 2 s (Tabelle) auf dem Entwicklungsrechner, Extraktion mit allen Kernen.
  - Meldet den bekannten defekten Eintrag `worm_tail_damage.wav`, bricht aber nicht ab.
  - Ordnermarker (Flags `0x00`, Größe 0) werden gelistet und mitgezählt (202.358), `verify` meldet sie nur als Hinweis. Verschlüsselte Einträge (Flags `0x03`, in `rdata.grf`: `msgstring_kr_s.lub`) werden gelistet, mit Warnung (Exit 2), nicht extrahiert.
  - Kern mit Fortschritts-Callback und Abbruch-Token (für CLI/MCP/GUI).

**Ergebnis M1 (2026-11-16, Apple M5 Pro, Release-Build, Archive im Dateicache):**
- [x] Eintragslisten von `cdata`, `core`, `cpal`, `rdata`, `data` (202.358) identisch mit den Golden Files aus GRF Editor (Pfad, `zsize`, `zsize_aligned`, `size`, Flags, Offset): 10 Tests in `crates/rgrf-core/tests/golden.rs`.
- [x] CRC32 und MD5 **aller** Einträge der fünf Archive identisch mit GRF Editor (inkl. 4.576 LZMA-Einträge), `data` in 4,8 s. Gegenprobe (Vergleich umgedreht): alle 3.973 `cdata`-Einträge schlagen dann fehl, es wird also jeder Eintrag verglichen.
- [x] Extraktion von `data.grf` (202.357 Dateien, 15,1 GiB) in 11 s; gegen den von GRF Editor entpackten Ordner `data-official`: **202.356 von 202.357 Dateien byteidentisch** (MD5), Namen nach Unicode-Normalisierung (NFC) und ohne Groß-/Kleinschreibung gleich. Abweichung: `worm_tail_damage.wav` (GRF Editor 1.407 Bytes, RustyGRF 1.403, siehe Format-Notizen); GRF Editor schreibt außerdem den Ordnermarker als leere Datei, RustyGRF überspringt Marker.
- [x] `data.grf` öffnen: **32 ms** (Abnahme < 2 s). `verify` von `data.grf` (16 GB entpackt) in 3,0 s, Exit 2: eine Warnung (`worm_tail_damage.wav`, 1.403 von 1.410 Bytes), ein Hinweis (Marker). `rdata`: eine Warnung (verschlüsselter Eintrag). `cdata`, `core`, `cpal`: sauber, Exit 0.
- [x] 53 Tests (Unit, synthetische Archive, Golden, CLI), `cargo fmt`, `cargo clippy -D warnings`, `cargo deny check` ohne Befund.
- Nicht getestet: **Lauf unter Windows und Linux** (`cargo check` für `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc` und `x86_64-unknown-linux-gnu` ist fehlerfrei, ausgeführt wurde nur auf macOS; Windows-Dateinamen mit unzulässigen Zeichen wie `?` sind nicht behandelt, siehe M5), große Archive über 4 GiB (0x300, M1b).

## M1b – GRF 0x300 lesen (vor M2)
- Header "Event Horizon", 64-Bit-Offset, 21-Byte-Einträge (gemessen, siehe [02-format-notes.md](02-format-notes.md)).
- **Abnahme:** `data kro latest.grf` (261.359 Dateien + 30 Ordnermarker) und `data (merged kro+chris).grf` (406.650 Dateien + 25 Marker) listen und prüfen; Eintragsliste und Hashes identisch mit den Golden Files aus der VM (GRF Editor liest 0x300).

## M2 – Bauen und Neu-Komprimieren (`rgrf build|repack`)
- Aus Verzeichnis bauen, bestehende GRF neu komprimieren: zlib, Zopfli, LZMA.
- **Abnahme:**
  - `build` aus `grf/cdata/data` ergibt ein Archiv, das GRF Editor/`GrfCL` und der Dev-Client lesen.
  - Reproduzierbar (zweimal bauen = byteidentisch).
  - LZMA-Variante von `cdata`: Größe in der Nähe der Referenz (13,56 MB), jeder Eintrag beginnt mit `0x00`, GRF Editor mit Custom-DLL liest sie.
  - Zopfli-Variante: gültig für den Standard-Client, Größe gegenüber zlib-9 gemessen (Ziel: ca. 4–8 % kleiner, Annahme).
  - Rundlauf für Namen auf `data-official` (119.181 Nicht-ASCII-Namen) bestanden.

## M3 – Diff, Merge, Patch (`rgrf diff|merge|patch`)
- Zwei Archive/Ordner vergleichen, `rdata` über `data` legen, Patch-GRF erzeugen.
- **Abnahme:** `merge data.grf rdata.grf` ergibt 205.610 Einträge (202.358 + 3.252 neue) = 205.609 Dateien + 1 leerer Marker; für jeden Pfad gewinnt `rdata`; Gegenprobe mit entpackten Ordnern. Priorität bei mehreren Archiven: eigene Custom-GRFs zuerst, `data.grf` zuletzt (Index-Zählung des Clients wird in der VM gemessen).

## M4 – MCP-Server (`rgrf-mcp`)
- Tools aus [04-cli-and-mcp.md](04-cli-and-mcp.md), Sicherheitsmodell (Schreiben nur in erlaubten Ordnern).
- **Abnahme:** Ein KI-Agent kann ohne Shell-Zugriff ein Archiv untersuchen, zwei vergleichen und ein Testarchiv bauen; alle Tools haben JSON-Schemas und Tests.

## M5 – Plattformen und Release
- CI für Windows, macOS, Linux (x64/ARM64), Binärdateien als Release, kurze Anleitung.
- **Abnahme:** Dieselben Tests laufen auf allen drei Systemen; Ergebnisse (Hashes) sind identisch.

## M6 – Entscheidung: Verschlüsselung, 0x300, Thor
- Nach Bedarf. Voraussetzungen: Format-Details belegen, Tests mit echten Archiven, Konflikt mit der einen `cps.dll` im Client beachten.

## M7 – Format-Crates und Oberfläche
- SPR → ACT → PAL → GAT/GND/RSW → RSM → STR. Zuerst Lesen und Rendern in eine Bildausgabe (CLI/MCP: "Vorschau als PNG"), dann eine GUI.
- **Abnahme je Format:** Vergleich mit Ausgaben von GRF Editor/ActEditor an festen Beispieldateien.

## Risiken
| Risiko | Gegenmaßnahme |
|---|---|
| Formatänderungen von Gravity (0x300, Verschlüsselung) | M6 getrennt, mit eigenem Test; Upstream (GRF Editor) beobachten |
| Dateinamen-Rundlauf (CP949 vs CP1252) | Bytes als Wahrheit, Rundlauftest auf `data-official` |
| Client lädt gebaute Archive nicht | Referenzvergleich mit GRF Editor, manueller Client-Test pro Meilenstein |
| LZMA: nur eine `cps.dll`, Konflikte mit Schutz-DLLs | Standard bleibt zlib, LZMA nur ausdrücklich |
| Viel Arbeit bei Vorschau/Editoren | erst nach M5, schrittweise |
| Lizenz/Erlaubnis | schriftlich festhalten (siehe Lizenzdatei) |

## Testdaten
Lokale Archive (nicht im Repo):
- `RO/1 CLIENT-SIDE/client-dev/full/` (`core.grf`, `cdata.grf`, `cpal.grf`, `data.grf`, `datacomp.grf`)
- `RO/1 CLIENT-SIDE/client-dev/grf/` (entpackt: `core`, `cdata`, `cpal`, `data-official`)
- `RO/1 CLIENT-SIDE/0 OTHER CLIENTS/client-kro/` (`data.grf` 26.10.2024, `rdata.grf`)
- `RO/1 CLIENT-SIDE/0 GRFs/` (eigene GRFs: `data (merged kro+chris).grf` = aktuellste Hauptdatei, `data kro latest.grf` = neue kRO-0x300-Referenz, `official_data (chris).grf`; später Komprimierung/Optimierung der Merge-GRF)
- `rdata.grf` (2024) gibt es im neuen kRO-Client nicht mehr; sie bleibt als Testdatei in `client-kro/`.
- Referenzwerkzeuge: `RO/1 CLIENT-SIDE/1 CLIENT TOOLS/5 GRF Editor/` (GRF Editor, `GrfCL` mit LZMA-Option)

Im Repo liegen nur Hashlisten (Golden Files), keine Archive.
