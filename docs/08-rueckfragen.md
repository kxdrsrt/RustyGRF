# 08 – Rückfragen, offene Punkte, Widersprüche

**Version 2** · 2026-11-16 · Status: alle 20 Punkte beantwortet. Offen bleiben nur Zuarbeit in der VM (8), Bestätigung der Auslegung von 20 (vor M4) und Messungen in späteren Meilensteinen (15).
Arbeit an M0/M1 darf beginnen.

Pflege: Jede Änderung erhöht die Version und erhält einen Eintrag im Änderungsprotokoll unten. Antworten werden **in dieser Datei** unter "Antwort" eingetragen (Datum), nicht gelöscht. Beschlossene Punkte wandern bei Bedarf in die passenden Dokumente (`01`–`07`).

Kennzeichnung: **gemessen** / **gelesen** (Quelle) / **vermutet**. Empfehlung = meine, nicht verbindlich.

## Übersicht
| # | Thema | Art | Blockiert | Status |
|---|---|---|---|---|
| 1 | Rust-Installation | Freigabe | M0 | **erledigt** |
| 2 | Context7/MCP läuft nicht in der Session | Technik | M0 | konfiguriert, neue Session nötig |
| 3 | Rust-Edition, MSRV, Zusatzwerkzeuge | Entscheidung | M0 | **entschieden** (2024, keine MSRV) |
| 4 | Projektlizenz | Entscheidung | M0 | **entschieden** (MIT OR Apache-2.0) |
| 5 | Namensprüfung "RustyGRF" | Prüfung | M0 | frei laut Inhaber |
| 6 | Lizenztabelle (Erlaubnis Tokeiburu) | Doku | nein | geklärt |
| 7 | Git: Verhalten im Dotfiles-Repo | Hinweis/Frage | nein | **erledigt** (lokal, kein Remote) |
| 8 | Golden Files: Umfang und Format | Zuarbeit VM | **M1-Abnahme** | Skript da, **Lauf in VM offen** |
| 9 | GRF 0x300 im Umfang von M1? | Widerspruch | **M1** | **entschieden** (M1b) |
| 10 | Neue kRO-`data.grf` fehlt in `0 GRFs/` | Widerspruch | nein | aufgelöst (`data kro latest.grf`) |
| 11 | Leerer Eintrag (`flags 0x00`) in `data.grf` | Frage | M1 | **entschieden**, Ordnermarker gemessen |
| 12 | Eintrag mit Flags `0x03` in `rdata.grf` (Verschlüsselung) | Frage | M1 | **entschieden**, gemessen |
| 13 | Zählung Merge: 205.609 oder 205.610? | Widerspruch | M3 | aufgelöst |
| 14 | Dateinamen auf macOS-Dateisystem (APFS) | Risiko/Frage | M1/M2 | **entschieden** |
| 15 | `DATA.INI`-Reihenfolge (0- oder 1-basiert) | offen aus Doku | M3 | Soll-Prio klar, Index messen (M3) |
| 16 | Windows-VM: Build-Tools jetzt oder später? | Entscheidung | nein | **entschieden** (M5) |
| 17 | GUI-Technik Tauri 2 | Bestätigung | M7 | Kernschnittstelle ja, GUI offen |
| 18 | Modellwahl und Review-Punkte | Prozess | nein | **entschieden** |
| 19 | LZMA-Parameter und Quellenlizenz (`curio-r`) | Frage | M2 | **entschieden** |
| 20 | MCP-Sicherheitsmodell (`rgrf-mcp`) | Entscheidung | M4 | Auslegung **vor M4 bestätigen** |

---

## M0-Voraussetzungen

### 1. Rust-Installation [blockiert]
**Stand (gemessen):** Auf dem Mac fehlen `rustup`, `cargo`, `rustc`. Vorhanden: Xcode Command Line Tools (`/Library/Developer/CommandLineTools`), Git 2.54 (Apple), Node unter `/opt/homebrew/bin`, 275 GiB frei, arm64.
**Frage:** Darf ich installieren?
- A) `rustup` (Standardprofil, Toolchain `stable`) + `rustfmt` + `clippy`. Cargo-Tools erst bei Bedarf.
- B) wie A, plus sofort `cargo-nextest`, `cargo-deny`.
- C) nichts, du installierst selbst.
**Empfehlung:** A, danach B bei M1-Start. Installationsort: Standard `~/.cargo` und `~/.rustup`. Das ist außerhalb des Dotfiles-Repos nicht versioniert (siehe 7).
**Antwort (2026-11-16):** A + B, "alles was du brauchst". **Erledigt (gemessen):** `rustc`/`cargo` 1.100.0 (stable, aarch64-apple-darwin), `rustfmt`, `clippy`, `cargo-nextest`, `cargo-deny` 0.21.0 in `~/.cargo/bin`. Mit `--no-modify-path` installiert, damit die Dotfiles (`~/.zshenv` usw.) unverändert bleiben. **`~/.cargo/bin` ist nicht im PATH deiner Shell**; VS Code/rust-analyzer finden es trotzdem. Für das Terminal: `. "$HOME/.cargo/env"` (oder in deine Dotfiles eintragen, wenn du willst).

### 2. Context7/MCP in der Session [blockiert]
**Stand:** Endpunkte erreichbar (gemessen 2026-11-16: Context7 HTTP 200 auf `initialize`, GitHub 401). In meiner Session gibt es **keine** `context7`-Tools. **Vermutet:** `RustyGRF` ist nicht als VS-Code-Workspace-Ordner geöffnet (geöffnet ist `1 CLIENT-SIDE/RO-client.code-workspace`), deshalb wird `.vscode/mcp.json` nicht geladen.
**Frage:** Bitte `RustyGRF` als Ordner öffnen, Servern vertrauen, neue Session starten. Wenn Remote-HTTP trotzdem nicht startet: darf ich in `mcp.json` auf stdio wechseln (`npx -y @upstash/context7-mcp`, Node ist da)?
**Empfehlung:** Erst Remote versuchen, stdio nur als Fallback.
**Antwort (2026-11-16):** Hinzufügen gewünscht (upstash/context7, Doku "MCP Clients"). **Erledigt:** `context7` (Remote `https://mcp.context7.com/mcp`, ohne API-Key, mit Rate-Limit) in `~/.copilot/mcp-config.json` und `~/Library/Application Support/Code/User/mcp.json` eingetragen (Backups `*.bak-20261116`); `.vscode/mcp.json` im Projekt hatte ihn schon. Gemessen: Server meldet `resolve-library-id` und `query-docs`. **Offen:** In einer bereits laufenden Session erscheinen die Tools erst nach Neustart/neuer Session. Bis dahin frage ich Context7 per HTTP direkt ab. Optional später: API-Key (höheres Limit) über `Authorization: Bearer`, nicht ins Repo.

### 3. Rust-Edition, MSRV, Zusatzwerkzeuge [blockiert]
**Stand:** `01-architecture.md` nennt "edition 2021 oder neuer", keine MSRV.
**Frage:**
- Edition: A) 2021, B) 2024 (aktuelle stabile Toolchain vorausgesetzt).
- MSRV: A) keine feste (immer aktuelles stable), B) fest (z. B. aktuelle stable minus 2).
- `cargo-llvm-cov` (Abdeckung): A) ja, B) später.
**Empfehlung:** Edition 2024, keine feste MSRV bis M5, Abdeckung später.
**Antwort (2026-11-16):** Empfehlung übernommen. **Erledigt:** `edition = "2024"`, `rust-toolchain.toml` (stable + rustfmt + clippy), keine MSRV, `cargo-llvm-cov` später.

### 4. Projektlizenz [blockiert]
**Stand:** `05-licensing.md`: offen (MIT, Apache-2.0 oder GPL). Tokeiburus Wunsch unbekannt (Tabelle leer). GRF-Editor-Repos haben keine Lizenzdatei (gelesen, GitHub-API, 2026-11-16).
**Frage:** Welche Lizenz? Wenn Code aus GRF Editor abgeleitet wird, kann das die Wahl einschränken.
- A) MIT/Apache-2.0 (Rust-üblich, Doppellizenz)
- B) GPL-3.0
- C) erst nach Rückfrage bei Tokeiburu entscheiden
**Empfehlung:** C für die Entscheidung, bis dahin **nur aus öffentlichem Format und Messungen bauen** (kein abgeleiteter Code), danach A, wenn er nichts dagegen hat. `cargo deny` braucht die Lizenz für die Prüfung.
**Antwort (2026-11-16):** Empfehlung übernommen; Tokeiburu hat freigegeben, sein Projekt kommt als Inspiration in die Credits (**kein Fork**). **Entscheidung:** `MIT OR Apache-2.0` (Dateien `LICENSE-MIT`, `LICENSE-APACHE`, Feld `license` im Workspace), Credits in `NOTICE` und `README.md`. Vor einer Veröffentlichung noch umkehrbar. Weiter gilt: bevorzugt aus öffentlichem Format und Messungen bauen, abgeleiteter Code wird im Dateikopf gekennzeichnet. Copyright-Zeile: "RustyGRF contributors" (bitte bei Bedarf auf deinen Namen ändern).

### 5. Namensprüfung "RustyGRF"
**Stand:** nicht geprüft (crates.io, GitHub, Marken).
**Frage:** Soll ich prüfen, bevor ich Crate-Namen festlege (`rgrf-core`, `rgrf-cli`, `rgrf-mcp`)? Das ist nur lesend (crates.io, GitHub-Suche).
**Empfehlung:** ja, jetzt. Veröffentlichung auf crates.io ist ausdrücklich noch nicht geplant.
**Antwort (2026-11-16):** Name ist frei, passt so (deine Aussage, von mir nicht geprüft). Crates bleiben `publish = false`.

### 6. Lizenztabelle (Erlaubnis Tokeiburu)
**Stand:** `05-licensing.md`, Tabelle "Bitte festhalten" leer. Die Erlaubnis ist laut dir erteilt (mündlich/Gespräch).
**Frage:** Kannst du Datum, Kanal und Wortlaut eintragen (oder ein Zitat/Link nennen)? Gilt sie auch für Veröffentlichung, für Übernahme von Patches, und wollte er eine bestimmte Namensnennung?
**Empfehlung:** kurz festhalten (Screenshot/Zitat), blockiert aber die Arbeit nicht.
**Antwort (2026-11-16):** Geklärt und freigegeben, keine weiteren Angaben nötig. In `05-licensing.md` vermerkt (Tabelle bleibt bewusst ohne Wortlaut).

### 7. Git im Dotfiles-Repo
**Stand (gemessen):** `~` ist ein Git-Repo. `Downloads/` steht darin in `.gitignore` (Zeile 110). Der RustyGRF-Ordner wird dort also nicht versehentlich committet. Ich hatte das vorher irrtümlich als Risiko genannt (Korrektur).
**Frage:** Wenn du später `git init` freigibst: eigenes Repo **im RustyGRF-Ordner** (verschachteltes Repo, vom Dotfiles-Repo ignoriert)? Oder Ordner verschieben?
**Empfehlung:** eigenes Repo im Ordner, Ordner nicht verschieben. Bis zu deiner Freigabe weiter kein `git init`.
**Antwort (2026-11-16):** `git init` im RustyGRF-Ordner erlaubt, GitHub später auf dein Zeichen. **Erledigt:** eigenes Repo im Ordner, **kein Remote, kein Push**. Erster Commit nur Doku, zweiter Commit Gerüst.

---

## M1 – Lesen und Prüfen

### 8. Golden Files: Umfang und Format [blockiert für die Abnahme]
**Stand:** Nur in der Windows-VM erzeugbar (GRF Editor/`GrfCL` als unabhängige Referenz). Auf dem Mac kann ich CRC32/MD5 aus den entpackten Ordnern `client-dev/grf/{cdata,core,cpal,data-official}` selbst berechnen; Eintragslisten aus meinen früheren Python-Skripten (`tools/grftable.py`) sind **keine** unabhängige Referenz.
**Frage:** Welche Archive und welche Felder?
- Archive: `cdata`, `core`, `cpal` (klein, schnell) und `data.grf` (202.358), `rdata.grf` (24.756, aus `client-kro`). Alle oder zuerst die kleinen?
- Felder je Eintrag: Pfad (Rohbytes als Hex **und** angezeigter Name), `zsize`, `zsize_aligned`, `size`, `flags`, Offset, CRC32, MD5.
- Format: CSV oder JSON Lines, UTF-8, sortiert nach Pfadbytes, eine Datei je Archiv unter `tests/golden/`.
**Empfehlung:** zuerst `cdata`, `core`, `cpal` + `rdata`; `data.grf` danach. `GrfCL` hat keinen Listen-Export, ich schreibe dafür ein kleines PowerShell-Skript (in der VM zu starten) und gebe es dir zum Prüfen. Zusätzlich je Archiv `shasum -a 256` der GRF-Datei, um sicherzustellen, dass VM und Mac dieselbe Datei meinen.
**Antwort (2026-11-16):** Empfehlung übernommen. **Erledigt:** `tools/golden/Export-GrfGolden.ps1` (TSV: `path_display`, `path_utf8_hex`, `zsize`, `zsize_aligned`, `size`, `flags`, `offset`, `crc32`, `md5`, `error`). **Ungetestet** (GRF.dll läuft nur unter Windows). **Zuarbeit für dich in der VM** (32-Bit-PowerShell, Arbeitsverzeichnis nicht auf `Z:`-UNC, Ausgabe nach `tests/golden/`):
`powershell -File Export-GrfGolden.ps1 -Grf <GRF> -Out <name>.tsv` für zuerst `cdata`, `core`, `cpal` (`client-dev/full/`) und `rdata` (`0 OTHER CLIENTS/client-kro/`), danach `data` (`client-kro/data.grf`, 202.358), später `data kro latest.grf` und die Merge-GRF (0x300). Zusätzlich `shasum -a 256` (Mac) der jeweiligen GRF in `tests/golden/SHA256SUMS`, damit VM und Mac dieselbe Datei meinen. Wenn das Skript scheitert, schick mir die Meldung.

### 9. GRF 0x300 im Umfang von M1? [blockiert]
**Widerspruch:** `02-format-notes.md`/`03-roadmap.md`: kRO-`data.grf` ist 0x200, 0x300 erst M6, Details "zu prüfen". `09-neue-grfs-2026-11-16.md` (gemessen): neue kRO-`data.grf` und `data (merged kro+chris).grf` sind **0x300** ("Event Horizon", 64-Bit-Offset, kein Seed, 4 Reserve-Bytes vor der Tabelle); der Parser in `grftable.py` wurde dafür angepasst.
**Frage:** Umfang M1:
- A) nur 0x200 (wie Roadmap), 0x300 nach M1 als eigener Schritt "M1b: 0x300 lesen" vorziehen (vor M2)
- B) 0x300 lesend schon in M1
- C) 0x300 bleibt M6
**Empfehlung:** A. Die M1-Abnahme ist 0x200-basiert. 0x300 lesen ist klein (zusätzlicher Headerzweig) und wird durch die Merge-Datei an echten Daten prüfbar. Als M1b festschreiben, sobald es 0x300-Golden-Files gibt. Roadmap/Format-Notizen danach aktualisieren.
**Antwort (2026-11-16):** Empfehlung übernommen (M1 = 0x200, **M1b = 0x300 lesen**, vor M2). Die Merge-GRF (`data (merged kro+chris).grf`) ist deine aktuellste Hauptdatei mit dem besten Inhalt; sie soll später (M2/M3) komprimiert/optimiert werden. In `03-roadmap.md` und `02-format-notes.md` eingetragen.

### 10. Neue kRO-`data.grf` fehlt
**Stand (gemessen 2026-11-16):** `0 GRFs/` enthält nur `data (merged kro+chris).grf` (5.731.443.579 B), `official_data (chris).grf` und `megadata/`. Die in der Recherche (`09`) genannte `data.grf` (3.667 MiB, 261.359 Einträge, MD5 `6a8d32b7…`) liegt dort **nicht** mehr. Der entpackte Warpgate-Client liegt unter `kro-latest/`.
**Frage:** Wurde sie verschoben/gelöscht? Brauche ich sie für Tests (0x300-Referenz, ohne Merge)?
**Empfehlung:** Für M1 nicht nötig. Falls sie in `kro-latest/` liegt, nenne mir den Pfad für M1b.
**Antwort (2026-11-16):** Sie liegt jetzt als `0 GRFs/data kro latest.grf`. **Gemessen:** 3.845.198.853 B (3.667 MiB), 0x300, 261.359 Dateien + 30 Ordnermarker, 19.696 MiB entpackt, identisch groß mit `kro-latest/data.grf`. Dient als reine 0x300-Referenz für M1b. Die Merge-GRF (5.731.443.579 B, 406.650 Dateien + 25 Ordnermarker, 0x300) ist ebenfalls vorhanden.

### 11. Leerer Eintrag in `data.grf` (flags `0x00`)
**Stand (gemessen, `03-measurements.md`):** `data.grf` (0x200): 202.357× Flags `0x01`, 1× `0x00` (leer). Merge-Zählung enthält diesen Eintrag als "+1 leerer Eintrag".
**Frage:** Wie soll `rgrf ls`/`verify` ihn behandeln?
- A) als normalen Eintrag der Größe 0 zählen und listen (Zählung 202.358 stimmt mit der Abnahme überein)
- B) ausblenden
**Empfehlung:** A, mit Hinweis in `verify` (kein Fehler). Abnahme verlangt 202.358 Einträge.
**Antwort (2026-11-16):** Empfehlung übernommen (A). **Neu gemessen:** Der Eintrag mit Flags `0x00` ist ein **Ordner-/Leermarker**: Name endet auf `.txt` (Unterordner `data\sprite\<korean>\…`), Größen 0, Offset `0xFFFFFFD2` (= −46, also absolut 0). Die 0x300-Archive haben 30 bzw. 25 solche Einträge mit Flags `0x0`. `ls` listet sie (Typ "leer"), `verify` meldet sie als Hinweis, nicht als Fehler, `extract` legt keine Datei an. In `02-format-notes.md` eingetragen.

### 12. Eintrag mit Flags `0x03` in `rdata.grf`
**Stand (gemessen):** `rdata.grf`: 24.755× `0x01`, 1× `0x03`. Format-Notizen (**zu prüfen**): Bit 1 = DES-Verschlüsselung (gemischt). Verschlüsselung gehört laut README nicht zu Version 1.
**Frage:** Verhalten von `ls/extract/verify` bei verschlüsselten Einträgen (v1 ohne `crypto`)?
- A) listen, Warnung, nicht extrahieren (Exit-Code 2)
- B) Fehler (Exit-Code 3)
- C) DES-Entschlüsselung schon in M1 einbauen
**Empfehlung:** A. Zuerst messen, um welchen Eintrag es geht und ob er wirklich verschlüsselt ist (die Bedeutung von `0x03` ist nur Annahme).
**Antwort (2026-11-16):** Empfehlung übernommen (A). **Zu deiner Rückfrage:** Das neue Warpgate-`data.grf` hat kein `rdata.grf` mehr, richtig. Die alte `rdata.grf` (Oktober 2024, `0 OTHER CLIENTS/client-kro/`) ist aber weiter vorhanden und bleibt **reine Testdatei** für M1/M3 (24.756 Einträge). **Gemessen:** Der Flag-`0x03`-Eintrag ist `data\luafiles514\lua files\msgstring_kr_s.lub` (10.464 B, entpackt 33.224 B), die ersten Bytes sind `28 9c d5 cc …` (**nicht** `78` und nicht `00`), das passt zu DES-Verschlüsselung (Annahme, nicht bewiesen). Verhalten in v1: listen, Warnung, nicht extrahieren (Exit-Code 2).

---

## Weitere Punkte

### 13. Zählung Merge (M3)
**Widerspruch:** `03-roadmap.md` (M3): "205.610 Einträge (202.358 + 3.252 neue)". `03-measurements.md` (gemessen): Merge **205.609** Dateien (+1 leerer Eintrag), Überschneidung 21.504, `rdata` 24.756 Einträge → 3.252 neue.
**Frage/Entscheidung:** Rechnung 202.358 + 3.252 = 205.610 (mit leerem Eintrag) vs. 205.609 Dateien. Wo ist die Differenz?
**Empfehlung:** vor M3 klären, wie der leere Eintrag gezählt wird (hängt an Punkt 11). Abnahme M3 nach der Festlegung korrigieren.
**Antwort (2026-11-16):** Empfehlung übernommen; **Widerspruch aufgelöst, kein Fehler:** 202.358 + 3.252 = 205.610 **Einträge** (inklusive des einen leeren Markers aus Punkt 11) = 205.609 **Dateien** + 1 Marker. `03-roadmap.md` (M3) präzisiert.

### 14. Dateinamen auf macOS (APFS)
**Stand:** Namen sind Bytes (CP949); GRF Editor zeigt sie als CP1252-Mojibake (gemessen, Rundlauf nur für `cdata` bestätigt). `data-official` mit 119.181 Nicht-ASCII-Namen ist noch nicht verifiziert. **Vermutet:** macOS-APFS ist standardmäßig case-insensitiv und normalisierungsunempfindlich (Unicode); das kann bei `extract` zu Kollisionen oder geänderten Bytes führen, und die extrahierten Ordner stammen aus der Windows-VM.
**Frage:** Wie soll `extract` mit nicht darstellbaren oder kollidierenden Namen umgehen?
- A) Standard `--names cp949`/`raw` wie dokumentiert, bei Kollision **Abbruch mit Meldung** (statt stillem Überschreiben), Option `--on-collision rename|skip`
- B) Immer Rohnamen mit Escapes
**Empfehlung:** A. Rundlauftest zuerst als Test mit synthetischen Namen auf dem Mac, dann an `data-official` (wegen 119.181 Namen; Ergebnis in `02-format-notes.md` eintragen).
**Antwort (2026-11-16):** Empfehlung übernommen (A): Abbruch bei Kollision statt stillem Überschreiben, Option `--on-collision rename|skip`; Rundlauftest zuerst synthetisch, dann `data-official`.

### 15. `DATA.INI`-Reihenfolge
**Stand:** `02-format-notes.md`: Wiki sagt "0 = höchste Priorität", das kRO-Beispiel ist 1-basiert; offen. Relevant für Merge-Semantik und GUI-Anzeige.
**Frage:** Hast du das schon am Dev-Client geprüft? (`client-dev/full/DATA.INI`, 57 B.)
**Empfehlung:** erst in M3 relevant. In der VM mit zwei GRFs und einer doppelten Testdatei prüfen (nur Kopie des Dev-Clients, `full/` nie ändern).
**Antwort (2026-11-16):** Für deinen Privatserver gilt: **eigene Custom-GRFs haben Vorrang, `data.grf` kommt zuletzt** (niedrigste Priorität). Das ist die Soll-Reihenfolge für Merge/Diff/GUI. Ob im Client Index 0 oder 1 die höchste Priorität ist, bleibt zu messen (VM, Kopie des Dev-Clients, M3).

### 16. Windows-VM: Build-Tools jetzt oder später?
**Stand (`06-tooling-and-workflow.md`):** In der VM fehlen Rust, Git, Node, C++-Build-Tools und Windows SDK.
**Frage:** Jetzt installieren oder erst zu M5 (Plattformen)?
**Empfehlung:** Erst zu M5. Für M0–M4 genügt die VM für Golden Files und Referenztests mit GRF Editor/`GrfCL`. Du brauchst dort jetzt nur die Skripte aus Punkt 8.
**Antwort (2026-11-16):** Empfehlung übernommen: erst zu M5.

### 17. GUI-Technik Tauri 2
**Stand:** `07-ui-ux.md`: Empfehlung Tauri 2 (Entscheidung vor M7). Der Kern soll Fortschritt/Abbruch schon jetzt unterstützen.
**Frage:** Bestätigt für die Kernplanung (Fortschritts-/Abbruch-Schnittstelle ab M1)? Der Rest kann bis M7 warten.
**Empfehlung:** ja, Kern mit `Progress`-Callback und Abbruch-Token ab M1, GUI-Entscheidung bleibt offen.
**Antwort (2026-11-16):** Empfehlung übernommen: Kern mit Fortschritts-Callback und Abbruch-Token ab M1.

### 18. Modellwahl und Reviews
**Stand:** `06-tooling-and-workflow.md`: Sonnet 5.5 high für Boilerplate; stärkeres Modell für schwierige Stellen und Review vor jedem Meilenstein (meine Einschätzung, nicht gemessen).
**Frage:** Soll ich Review-Läufe (Code-Review, Zweitmeinung) selbst als Subagent starten, oder startest du die Reviews manuell mit einem anderen Modell?
**Empfehlung:** Review-Subagent nach jedem Meilenstein (ohne Modelloverride), Zweitmeinung zum LZMA-Format und zum MCP-Sicherheitsmodell auf Anforderung.
**Antwort (2026-11-16):** Empfehlung übernommen: Review-Subagent nach jedem Meilenstein, Zweitmeinung bei den schwierigen Stellen.

### 19. LZMA-Format und Quelllizenz (M2)
**Stand:** `02-format-notes.md`: Aufbau "0x00 + 5 Byte Parameter + Daten" laut Research-Agent (**zu prüfen**), Wörterbuch 3 MiB wie `reduceSize` der `cps.dll`; `05-licensing.md` Regel 5: Lizenz von `curio-r/rag-cps-dll` vor Übernahme prüfen; Format aus Beobachtung nachbauen.
**Frage:** Reicht dir "Format aus echten Referenzdateien (`cdata` mit LZMA, 13,56 MB) ableiten, keinen Code übernehmen"?
**Empfehlung:** ja, ein reiner Beobachtungs-Nachbau. Betrifft erst M2.
**Antwort (2026-11-16):** Ja; Lizenz kein Hindernis (volle Freigabe laut dir). Format wird trotzdem aus Referenzdateien belegt (Messen statt Vermuten).

### 20. MCP-Sicherheitsmodell `rgrf-mcp`
**Stand:** `04-cli-and-mcp.md`: nur Lesen als Standard, Schreiben nur mit `--allow-write <ordner>`, Pfadprüfung gegen erlaubte Wurzeln.
**Frage:** Lesen nur in erlaubten Ordnern (ebenfalls `--allow-read`), oder darf das Lesen überall erfolgen?
**Empfehlung:** auch Lesen nur in erlaubten Wurzeln (Standard: Ordner aus dem Aufruf). Betrifft erst M4.
**Antwort (2026-11-16):** "überall". **Meine Auslegung (bitte vor M4 bestätigen):** Lesen darf überall erfolgen, **Schreiben nur in `--allow-write`-Ordnern** (Sicherheitsmodell aus `04-cli-and-mcp.md` bleibt für Schreiben). Falls du "überall" auch auf Schreiben beziehst, sag Bescheid; das wäre ein Sicherheitsrisiko, ich würde davon abraten.

---

## Hinweise zu MCP-Servern (Stand 2026-11-16)
| Server | Für | Stand in meiner Session |
|---|---|---|
| `context7` (`.vscode/mcp.json`, zusätzlich Nutzerebene) | Crate-APIs (`rmcp`, `clap`, `zopfli`, LZMA-Crates) | **konfiguriert** (Nutzerebene seit 2026-11-16), in meiner laufenden Session noch nicht geladen (siehe 2) |
| `github` (`.vscode/mcp.json`) | Issues/PRs/Actions, erst wenn Repo auf GitHub liegt | ein allgemeiner `github-mcp-server` ist vorhanden; ob es der aus `mcp.json` ist, ist unklar. Anmeldung nicht nötig bis Repo-Freigabe |
| Playwright / integrierter Browser | GUI-Tests, erst ab M7 | nicht nötig |
| eigener `rgrf-mcp` | ab M4 | existiert noch nicht |

## Änderungsprotokoll
| Version | Datum | Änderung |
|---|---|---|
| 1 | 2026-11-16 | Erste Sammlung (20 Punkte) |
| 2 | 2026-11-16 | Antworten eingetragen; Rust, Nextest, Deny installiert; Context7 in Nutzer-Konfigurationen; Gerüst, Lizenz, Golden-Skript angelegt; neue Messungen zu Punkt 10–12 |
