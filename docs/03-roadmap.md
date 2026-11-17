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
- 0x200 öffnen, Tabelle lesen, Einträge auflisten, einzeln und gesamt extrahieren. (0x300 folgt in M1b, ist aber im selben Code umgesetzt.)
- **Dekodieren von zlib und LZMA** (Lesen): `core.grf` hat 4.391 und `cdata.grf` 185 LZMA-Einträge (gemessen), siehe [02-format-notes.md](02-format-notes.md). Kodieren bleibt M2.
- **Abnahme:**
  - Eintragsliste (Pfad, Größen, Flags) für `cdata.grf`, `core.grf`, `cpal.grf`, `data.grf` (202.358 Einträge) identisch mit GRF Editor/`GrfCL`.
  - Hashes (CRC32, MD5) aller Einträge identisch mit den extrahierten Dateien aus `data-official`.
  - `data.grf` öffnen < 2 s (Tabelle) auf dem Entwicklungsrechner, Extraktion mit allen Kernen.
  - Meldet den bekannten defekten Eintrag `worm_tail_damage.wav`, bricht aber nicht ab.
  - Leere Marker (Flags `0x00`, Größe 0; nur einer in `data.grf`) werden gelistet und mitgezählt (202.358), `verify` meldet sie nur als Hinweis. Verschlüsselte Einträge (Flags `0x03`, in `rdata.grf`: `msgstring_kr_s.lub`) werden gelistet, mit Warnung (Exit 2), nicht extrahiert.
  - Kern mit Fortschritts-Callback und Abbruch-Token (für CLI/MCP/GUI).

**Ergebnis M1 (2026-11-16, Apple M5 Pro, Release-Build, Archive im Dateicache):**
- [x] Eintragslisten von `cdata`, `core`, `cpal`, `rdata`, `data` (202.358) identisch mit den Golden Files aus GRF Editor (Pfad, `zsize`, `zsize_aligned`, `size`, Flags, Offset): 10 Tests in `crates/rgrf-core/tests/golden.rs`.
- [x] CRC32 und MD5 **aller** Einträge der fünf Archive identisch mit GRF Editor (inkl. 4.576 LZMA-Einträge), `data` in 4,8 s. Gegenprobe (Vergleich umgedreht): alle 3.973 `cdata`-Einträge schlagen dann fehl, es wird also jeder Eintrag verglichen.
- [x] Extraktion von `data.grf` (202.357 Dateien, 15,1 GiB) in 11 s; gegen den von GRF Editor entpackten Ordner `data-official`: **202.356 von 202.357 Dateien byteidentisch** (MD5), Namen nach Unicode-Normalisierung (NFC) und ohne Groß-/Kleinschreibung gleich. Abweichung: `worm_tail_damage.wav` (GRF Editor 1.407 Bytes, RustyGRF 1.403, siehe Format-Notizen); GRF Editor schreibt außerdem den Ordnermarker als leere Datei, RustyGRF überspringt Marker.
- [x] `data.grf` öffnen: **32 ms** (Abnahme < 2 s). `verify` von `data.grf` (16 GB entpackt) in 3,0 s, Exit 2: eine Warnung (`worm_tail_damage.wav`, 1.403 von 1.410 Bytes), ein Hinweis (Marker). `rdata`: eine Warnung (verschlüsselter Eintrag). `cdata`, `core`, `cpal`: sauber, Exit 0.
- [x] 57 Tests (Unit, synthetische Archive, Golden, CLI), `cargo fmt`, `cargo clippy -D warnings`, `cargo deny check` ohne Befund.
- **Review (unabhängiger Code-Review nach M1, 2026-11-16), vier Befunde, alle behoben:** (1) doppelte oder nur in der Schreibweise verschiedene Pfade wurden bei paralleler Extraktion ohne feste Reihenfolge geschrieben → Einträge mit gleichem Ziel werden jetzt nacheinander in Archivreihenfolge geschrieben (bei `--overwrite` gewinnt der letzte, bei `rename`/`skip` der erste); (2) der Namensvergleich faltete auch Trailbytes zweibytiger CP949-Zeichen → `GrfPath::key()` lässt Doppelbyte-Zeichen unverändert (in den echten Archiven betrifft das 9 von 281.303 Namen, ohne Auswirkung; die Regel des Clients ist **nicht geprüft**); (3) kein Limit für die entpackte Größe eines Eintrags → `Archive::set_max_entry_size`, Standard 1 GiB (größter echter Eintrag: 21 MB), größere Angaben ergeben den Status `Corrupt`; (4) `--overwrite` folgte einem Symlink am Zielpfad und Ordner wurden vor der Symlink-Prüfung angelegt → beides geprüft, jeweils mit Test.
- **Bekannte Grenze:** `verify`, `hash` und `extract` halten jeden Eintrag vollständig im Speicher (bis 1 GiB je Thread). Für vertrauenswürdige Archive unkritisch; für den MCP-Server (M4, fremde Archive) ist Streaming oder ein Speicherbudget vorgesehen.
- Nicht getestet: **Lauf unter Windows und Linux** (`cargo check` für `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc` und `x86_64-unknown-linux-gnu` ist fehlerfrei, ausgeführt wurde nur auf macOS; Windows-Dateinamen mit unzulässigen Zeichen wie `?` sind nicht behandelt, siehe M5), große Archive über 4 GiB (0x300, M1b).

## M1b – GRF 0x300 lesen (vor M2)
- Header "Event Horizon", 64-Bit-Offset, 21-Byte-Einträge (gemessen, siehe [02-format-notes.md](02-format-notes.md)).
- **Abnahme:** `data kro latest.grf` (261.389 Dateien, davon 30 unkomprimiert gespeichert) und `data (merged kro+chris).grf` (406.675 Dateien, davon 25 unkomprimiert gespeichert) listen und prüfen; Eintragsliste und Hashes identisch mit den Golden Files aus der VM (GRF Editor liest 0x300).

**Ergebnis M1b (2026-11-16, Apple M5 Pro, Release-Build):**
- [x] Beide 0x300-Archive: Eintragstabelle (Pfad, Größen, Flags, Offset) **und CRC32/MD5 aller Dateien** identisch mit GRF Editor (261.359 bzw. 406.650 Dateien; `data kro latest.grf` 10 s, Merge-GRF 15 s im Golden-Test).
- [x] Öffnen: 30 ms (3,8 GB) bzw. 56 ms (5,7 GB). `rgrf verify`: 0 Fehler, 0 Warnungen in 5,7 s bzw. 5,8 s; Hinweise: 30/25 unkomprimiert gespeicherte Dateien und gemeinsame Datenblöcke (79.764/69.853).
- [x] Synthetische Tests: 64-Bit-Offsets und Tabelle jenseits 4 GiB (Sparse-Quelle), Schlüsselblock (Grenzen, `end()`), Gravity-Flag `0x80` als verschlüsselt, Seed-Regel von GRF Editor, absurde Offsets ohne Panic.
- [x] `verify` akzeptiert `zsize_aligned` = `zsize` oder aufgerundet auf 8 (gemessen: beide Formen kommen vor).
- Nicht getestet: ein 0x300-Archiv **mit** Schlüsselblock oder Einträgen mit Flag `0x80` (keines vorhanden; ältere und neuere kRO-Patches sollen sie nutzen, siehe `RO/docs/grf-research`), Entschlüsselung (M6).
- Extrahieren der 0x300-Archive nicht gegen einen Referenzordner verglichen (keiner vorhanden); die Hashes aller Einträge sind aber identisch, und die Extraktion nutzt denselben Lesepfad wie `hash`.

## M2 – Bauen und Neu-Komprimieren (`rgrf build|repack`)
**Stand 2026-11-17:** `repack` umgesetzt und gemessen (Ergebnisse und Größen: `docs/rueckfragen/v7-2026-11-17-aktuell.md`); `build` aus Ordner umgesetzt (cdata entpackt → gebaut: alle 3.973 Hashes identisch, von GRF Editor in 0x200/0x300 und zlib/LZMA ohne Abweichung gelesen); der Spieltest im Client ist bestanden (2026-11-17: `data.grf` der 2024er-Version als LZMA-0x200 läuft einwandfrei im Dev-Client mit LZMA-`cps.dll`; die echte Merge-GRF wird noch getestet, Punkt 31 in `rueckfragen/v7`).
- Aus Verzeichnis bauen, bestehende GRF neu komprimieren: zlib, Zopfli, LZMA.
- **Abnahme:**
  - `build` aus `grf/cdata/data` ergibt ein Archiv, das GRF Editor/`GrfCL` und der Dev-Client lesen.
  - Reproduzierbar (zweimal bauen = byteidentisch).
  - LZMA-Variante von `cdata`: Größe in der Nähe der Referenz (13,56 MB), jeder Eintrag beginnt mit `0x00`, GRF Editor mit Custom-DLL liest sie.
  - Zopfli-Variante: gültig für den Standard-Client, Größe gegenüber zlib-9 gemessen (Ziel: ca. 4–8 % kleiner, Annahme).
  - Rundlauf für Namen auf `data-official` (119.181 Nicht-ASCII-Namen) bestanden.

## M3 – Diff, Merge, Patch (`rgrf diff|merge|patch`)
**Stand 2026-11-17: umgesetzt und an echten Daten gemessen** (Apple M5 Pro, Release-Build, jede Ausgabe rückgeprüft).
- Zwei Archive vergleichen, Archive mit Priorität zusammenführen (**erstes gewinnt**, entspricht dem niedrigsten Index in `DATA.INI`: deine Annahme „eigene Custom-GRFs zuerst, `data.grf` zuletzt“), Patch-GRF mit nur Neuem/Geändertem erzeugen. `repack`, `merge` und `patch` teilen eine Schreib-Pipeline (Auswahl von Einträgen aus mehreren Quellen).
- **Abnahme 1 (`data kro latest` + `official_data (chris)`):** `merge "official_data (chris).grf" "data kro latest.grf"` ergibt 406.675 Dateien (186.507 aus Chris, 220.168 aus kRO, 41.221 kRO-Dateien überschrieben); `diff` gegen deine Merge-GRF `data (merged kro+chris).grf`: **406.675 identisch, 0 nur links, 0 nur rechts, 0 unterschiedlich**. Dauer Merge 78 s, `diff` zweier 4–5-GB-Archive 13 s. Das Ergebnis (0x300, zlib, 4,49 GB) ist damit dieselbe Datei im Inhalt, die du in GRF Editor gebaut hast.
- **Abnahme 2 (`rdata` über `data`, kRO-Client 2024):** 205.609 Dateien (24.756 aus `rdata`, 180.853 aus `data`, 3.252 neue Pfade, 21.504 überschrieben, davon 4.562 mit tatsächlich anderem Inhalt); der leere Marker von `data.grf` wird nicht kopiert (Roadmap-Erwartung 205.610 = 205.609 Dateien + 1 Marker, bestätigt). `diff` gegen `rdata`: 24.756 identisch, 0 unterschiedlich (jeder Pfad von `rdata` gewinnt). Der verschlüsselte Eintrag und `worm_tail_damage.wav` werden unverändert übernommen. Zuerst mit Neukomprimierung (zlib 9): 4.001.949.159 B, 63 s. **Seit dem Vergleich mit GrfCL (unten) kopiert `merge` ohne `--method` die Ströme unverändert:** 4.042.050.709 B (passt unter 4 GiB), 6,0 s auf dem Mac (mit Prüfung), Inhalt identisch (205.609 Einträge, `diff` 0 Abweichungen).
- Synthetische Tests (Core 6, CLI 1): Priorität und Reihenfolge, Duplikate in einem Archiv (später gewinnt), Groß-/Kleinschreibung und Separator beim Vergleich, 0x300-Quelle wählt 0x300, Dedupe über Archive, reproduzierbar, Patch über Alt = Neu, Ablehnung (kein Eingabearchiv als Ziel, kein vorhandenes Ziel, nichts zu patchen).
- **Nicht gemessen:** die Reihenfolge der `DATA.INI`-Indizes im Client (die Priorität „erstes Archiv gewinnt“ ist die Annahme aus deiner Aussage und gängiger Praxis; ein Spieltest mit zwei Archiven, die dieselbe Datei verschieden enthalten, würde es belegen); Ordner als Eingabe von `diff`/`merge` (stattdessen erst mit `build` ein Archiv erzeugen).
- Zwei Archive/Ordner vergleichen, `rdata` über `data` legen, Patch-GRF erzeugen.
- **Abnahme:** `merge data.grf rdata.grf` ergibt 205.610 Einträge (202.358 + 3.252 neue) = 205.609 Dateien + 1 leerer Marker; für jeden Pfad gewinnt `rdata`; Gegenprobe mit entpackten Ordnern. Priorität bei mehreren Archiven: eigene Custom-GRFs zuerst, `data.grf` zuletzt (Index-Zählung des Clients wird in der VM gemessen).

## M4 – MCP-Server (`rgrf-mcp`)
**Stand 2026-11-17: umgesetzt** (Tools, Sicherheitsmodell und Grenzen: [04-cli-and-mcp.md](04-cli-and-mcp.md)).
- 11 Tools: 6 lesende (`grf_info`, `grf_list` mit Suche, `grf_read_text`, `grf_read_summary`, `grf_verify`, `grf_diff`) und 5 schreibende (`grf_extract`, `grf_build`, `grf_repack`, `grf_merge`, `grf_patch`, nur mit `--allow-write`). Eigenes schlankes JSON-RPC über stdio statt `rmcp`.
- **Gemessen am echten Archiv** (Merge-GRF, 406.675 Einträge, über stdio wie ein Client): `grf_info` mit Codec-Zählung 0,74 s (405.746 LZMA, 904 zlib), `grf_list` 0,14 s, `grf_read_text` auf `cdata.grf` unter 10 ms, `grf_diff` gegen `data kro latest` 28 s mit demselben Ergebnis wie `rgrf diff` (145.286 nur links, 2.001 unterschiedlich), `grf_verify` 23 s ohne Fehler; ein Schreibversuch außerhalb des Schreibordners wurde abgelehnt.
- Tests (Server 9 Unit + 8 Integration + 3 mit der echten Binärdatei): Protokoll (Aushandeln der Version, Ping, unbekannte Methode/Tool, Fehler bei Argumenten, Parse-Fehler), jedes Tool, Seiten, Kodierungen (UTF-8, cp949, abgeschnittenes Zeichen), Pfadgrenzen (außerhalb, `..`, Symlink-Ausbruch, vorhandene Datei, Ausgabe = Eingabe, Lesebeschränkung), `dry_run`, Abbruch (Kern: Prüfung der Abbruchmarke je Eintrag, sonst griff er erst nach einem ganzen Stapel), Fortschritt, sauberes Ende bei EOF.
- **Nicht getestet:** der Betrieb in einem echten Agenten-Client (VS Code/Copilot); der Server ist dort als `rustygrf` in `.vscode/mcp.json` eingetragen, nach `cargo build --release` startbar. Parallele Anfragen (ein Aufruf nach dem anderen).
- Tools aus [04-cli-and-mcp.md](04-cli-and-mcp.md), Sicherheitsmodell (Schreiben nur in erlaubten Ordnern).
- **Abnahme:** Ein KI-Agent kann ohne Shell-Zugriff ein Archiv untersuchen, zwei vergleichen und ein Testarchiv bauen; alle Tools haben JSON-Schemas und Tests.

## Vergleich mit GRF Editor (`GrfCL`), 2026-11-17
**Messung in derselben Windows-ARM64-VM** (Parallels, 6 vCPU, 6 GB RAM), lokale Platte, gleiche Eingaben, Wall-Zeit des Prozesses und Spitzen-Arbeitsspeicher; Reihenfolge der Werkzeuge wechselt je Lauf. `rgrf.exe` ARM64 (Kreuz-Bau); `GrfCL.exe` aus `1 CLIENT TOOLS/5 GRF Editor` (.NET; der Prozessstart ist in den Zeiten enthalten). Kleine Aufgaben 3 Läufe (Median, in Klammern Streuung), große 1 Lauf. Jede Ausgabe beider Werkzeuge wurde mit `rgrf verify` gelesen (0 Fehler) und, wo vergleichbar, mit `rgrf diff` auf Inhalt verglichen. Rohdaten: `~/build/bench/results*.jsonl`, Skript `tools/cross/bench.ps1`.

| Aufgabe | `rgrf` | `GrfCL` | Größe der Ausgabe | Spitzen-RAM `rgrf` / `GrfCL` |
|---|---|---|---|---|
| Öffnen + Info `data.grf` (4 GB) | 0,08–0,13 s | 1,2–1,4 s | – | 42 MB / 260 MB |
| Entpacken `cdata` (103 MB, 3.973 Dateien) | **2,3 s** | 4,1 s | gleich | 20 / 145 MB |
| Entpacken `rdata` (1,2 GB, 24.756 Dateien) | 25,5 s | 26,3 s | gleich | 59 / 370 MB |
| Entpacken `data.grf` (16 GB, 202.357 Dateien) | 243 s (zweiter) / 385 s (erster) | 314 s / 316 s | gleich | 202 / 1.138 MB |
| Bauen `cdata` zlib 9 (mit Prüfung) | **1,0 s** (0,9–4,7) | 2,1 s | 17.009.178 / 17.012.174 B (−0,02 %) | 77 / 146 MB |
| Bauen `rdata` zlib 9 (mit Prüfung) | **11,7 s** (11,3–30,3) | 16,8 s | 287.083.129 / 287.126.228 B (−0,02 %) | 196 / 541 MB |
| Bauen `cdata` LZMA, `rgrf` Standard (Preset 9) | 10,5 s | **8,5 s** | **13,11 / 13,55 MB (−3,3 %)** | 147 / 151 MB |
| Bauen `cdata` LZMA, `rgrf --level 6` | **6,6 s** | 8,5 s | **13,28 / 13,55 MB (−2,1 %)** | 128 / 151 MB |
| Bauen `rdata` LZMA, `rgrf` Standard | 127 s | **74 s** | **233,6 / 237,4 MB (−1,6 %)** | 249 / 449 MB |
| Bauen `rdata` LZMA, `rgrf --level 6` | **67 s** | 74 s | **235,4 / 237,4 MB (−0,8 %)** | 273 / 449 MB |
| Merge `rdata` über `data` (Ströme kopiert) | 19,2 s (13,0 s ohne Prüfung) | 17,0 s (10,6–19,8) | 4.042.050.709 / 4.042.057.505 B | 242 / 377 MB |

**Lesart (gemessen, mit Vorbehalt):**
- `rgrf` ist beim Öffnen deutlich schneller (Prozessstart von .NET ist Teil der `GrfCL`-Zeit), beim zlib-Bauen schneller, braucht **durchweg weniger Arbeitsspeicher** (bis zu 6×) und erzeugt **gleich große bis kleinere** Dateien, bei LZMA 0,8–3,3 % kleiner.
- Beim Entpacken ist `rgrf` bei kleinen Archiven schneller und bei den großen **nicht eindeutig**: das 16-GB-Entpacken schwankte bei `rgrf` zwischen 243 s und 385 s je nach Reihenfolge (Dateisystem-Cache und Schreiblast der VM), `GrfCL` blieb bei 314–316 s. Ein Sieger lässt sich daraus nicht ableiten.
- **LZMA mit `rgrf`-Standard (Preset 9) ist langsamer als `GrfCL`** (bis 1,7×), mit `--level 6` schneller und trotzdem kleiner. Das Preset ist ein Zeit-gegen-Größe-Regler, kein Fehler. Standard bleibt Preset 9 (kleinste Dateien).
- **Gefunden und behoben durch diesen Vergleich:** `merge` brauchte zuerst 153–186 s (gegen 19 s), weil jeder Eintrag neu mit zlib 9 komprimiert wurde. Jetzt kopiert `merge`/`patch` ohne `--method` die Ströme (Auswahl `copy_streams`); ein Lauf in der VM zeigt 13–19 s, auf dem Mac 6 s. Der Nachteil: die Ausgabe ist 1 % größer als mit Neukomprimierung (4.042 gegen 4.002 MB).
- Die Prüfung der Ausgabe (Standard) kostet bei `rgrf` mal wenig, mal viel (Build `rdata` zlib: 11,7 s gegen 10,2 s ohne Prüfung, einmal 30 s); die Streuung der VM ist groß (derselbe Lauf 10 bis 44 s). Eine Ursache dafür habe ich nicht belegt.
- **Gleichwertigkeit:** `GrfCL` und `rgrf` erzeugen beim Merge gleich große Archive (Abweichung 6.800 Byte); die beiden unterscheiden sich im Inhalt nur in zwei Einträgen (`rgrf diff --all` auf den Ergebnissen des Kopier-Laufs): dem leeren Marker (`rgrf` kopiert Marker nicht) und dem **einen verschlüsselten Eintrag** `data\luafiles514\lua files\msgstring_kr_s.lub` aus `rdata`: `rgrf` kopiert ihn unverändert, die `GrfCL`-Kopie hat andere gespeicherte Bytes (warum, habe ich nicht untersucht; ob der Client sie liest, ist nicht geprüft).
- **Grenzen der Aussage:** eine VM, ein Rechner; `GrfCL` auf ARM64-Windows läuft vermutlich teilweise unter Emulation (nicht geprüft), `rgrf` nativ; GRF Editor selbst (GUI) nicht gemessen; `rgrf` hat viel weniger Funktionen als GRF Editor (Vorschau, Editoren, Thor, Verschlüsselung u. a.); keine Messung auf einem anderen Rechner; Zopfli nicht verglichen (hat `GrfCL` nicht).

## M5 – Plattformen und Release
**Stand 2026-11-17: Tests auf vier Plattformen bestanden; Release-Binärdateien und CI-Lauf stehen aus (brauchen den GitHub-Push).**
- **Gemessen, alle mit denselben echten Archiven** (`RGRF_REQUIRE_TESTDATA=1 RGRF_GOLDEN_FULL=1`; sieben Archive, über 950.000 Einträge, CRC32/MD5 gegen GRF Editor; CLI-Rundläufe `extract`/`build`/`repack` mit `cdata`):

| System | Ausführung | Ergebnis |
|---|---|---|
| macOS ARM64 | nativ (`cargo nextest`) | 135 Tests bestanden |
| Linux ARM64 | Docker `rust:1` (Rust 1.100.0), Quellen und Archive nur lesend eingebunden | alle bestanden (Core 91, Golden 14, CLI 10, MCP 9 + 8 + 3) |
| Windows ARM64 | Kreuz-Bau mit `cargo-xwin` auf dem Mac, ausgeführt in der Parallels-VM | alle bestanden (Core 88, Golden 14, CLI 10, MCP 8 + 3) |
| Windows x64 | wie oben, in der ARM64-VM unter Emulation | alle bestanden (gleiche Zahlen) |

Die Golden-Hashes (Pfad, Größe, CRC32, MD5 aller Einträge) sind also auf allen vier Systemen identisch mit GRF Editor. Core-Tests sind unter Windows weniger (Symlink-Tests nur unter Unix). Skripte: `tools/cross/`.
- **Beim Test gefunden und behoben** (Mac-Tests hatten es nicht gezeigt):
  1. **Linux:** `extract` legte bei Archivpfaden, die sich nur in der Groß-/Kleinschreibung unterscheiden, auf case-sensitiven Dateisystemen zwei Bäume an (`data/` und `DATA/`). Jetzt gilt für jede Pfadkomponente die erste Schreibweise in Archivreihenfolge, auf jedem System; Kollisionen laufen über `--on-collision`. (Wie viele Pfade der echten Archive davon betroffen sind, habe ich nicht gezählt. Test: `names_that_differ_only_in_case_end_up_in_one_tree_on_every_file_system`.)
  2. **Windows:** `canonicalize` liefert `\?\`-Pfade, in denen std `..` nicht als Elternverweis erkennt. Der MCP-Server prüft `..` jetzt zusätzlich im Text (der Pfad war schon vorher abgelehnt, aber über die falsche Prüfung).
  3. **Windows:** Namen, die Windows nicht speichern kann (`? * < > | " :`, Steuerzeichen, Punkt/Leerzeichen am Ende, Gerätenamen wie `nul`, `con`, `com1`), werden beim Extrahieren vorab abgelehnt statt mitten im Lauf zu scheitern oder Daten ins Gerät zu schreiben. Prüfung an allen sechs Test-Archiven: 0 solche Namen in 683.000 Einträgen. Dabei fing der Echtdaten-Test einen eigenen Fehler (Panic beim Zerschneiden eines Mehrbyte-Zeichens); behoben, Test mit Mehrbyte-Namen ergänzt.
- **Vorbereitet, nicht ausgeführt:** `.github/workflows/ci.yml` (fmt, clippy, Tests auf Linux x64/ARM64, macOS ARM64, Windows x64/ARM64, `cargo-deny`, Release-Binärdateien als Artefakte). Es lief nie, weil das Repo nicht auf GitHub liegt; Fehler darin sind daher möglich.
- **Nicht getestet:** Linux x64 und macOS x64 (nur ARM64), natives Bauen unter Windows (nur Kreuz-Bau), Windows-Dateisystem-Grenzen wie lange Pfade über 260 Zeichen, Extrahieren auf ein NTFS-Laufwerk in großem Umfang (nur `cdata` getestet), Signierung/Notarisierung der Binärdateien, Release-Veröffentlichung.

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
- `RO/1 CLIENT-SIDE/client-dev/full/` (`core.grf`, `cdata.grf`, `cpal.grf`, `data.grf`, `datacomp.grf`); **`cdata.grf` dort ist seit 2026-11-17 neu gebaut** (Serveradresse `<host-ip>`, alles zlib). Die Tests nutzen das Original `RO/1 CLIENT-SIDE/0 GRFs/reference/cdata-2024-original.grf` (SHA-256 in `tests/golden/SHA256SUMS`); diese Datei nicht ändern.
- `RO/1 CLIENT-SIDE/client-dev/grf/` (entpackt: `core`, `cdata`, `cpal`, `data-official`)
- `RO/1 CLIENT-SIDE/0 OTHER CLIENTS/client-kro/` (`data.grf` 26.10.2024, `rdata.grf`)
- `RO/1 CLIENT-SIDE/0 GRFs/` (eigene GRFs: `data (merged kro+chris).grf` = aktuellste Hauptdatei, `data kro latest.grf` = neue kRO-0x300-Referenz, `official_data (chris).grf`; später Komprimierung/Optimierung der Merge-GRF)
- `rdata.grf` (2024) gibt es im neuen kRO-Client nicht mehr; sie bleibt als Testdatei in `client-kro/`.
- Referenzwerkzeuge: `RO/1 CLIENT-SIDE/1 CLIENT TOOLS/5 GRF Editor/` (GRF Editor, `GrfCL` mit LZMA-Option)

Im Repo liegen nur Hashlisten (Golden Files), keine Archive.
