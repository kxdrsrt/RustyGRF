# Master-Prompt (in eine neue Session kopieren)

Zweck: Eine neue Session (Sonnet 5.5 high oder Opus 5.5) soll **sofort** an RustyGRF weiterarbeiten können, ohne Vorwissen aus dem Chat.

````markdown
# Aufgabe: RustyGRF bauen (Rust, cross-platform GRF-Werkzeug mit CLI und MCP)

## Projekt
RustyGRF ist ein modernes Werkzeug für Ragnarok-Online-Archive (GRF/GPF), inspiriert von Tokeiburus GRF Editor.
Ziele: Bibliothek + CLI (`rgrf`) + MCP-Server (`rgrf-mcp`), später Format-Crates und eine GUI (Tauri). Windows, macOS, Linux (x64/ARM64), keine native `cps.dll`.
Tokeiburu hat die Erlaubnis zur Portierung gegeben (Details in `docs/05-licensing.md`, vom Projektinhaber zu pflegen).

## Zuerst lesen (in dieser Reihenfolge)
1. `README.md`
2. `docs/01-architecture.md`, `docs/02-format-notes.md`, `docs/03-roadmap.md`
3. `docs/04-cli-and-mcp.md`, `docs/06-tooling-and-workflow.md`, `docs/07-ui-ux.md`, `docs/05-licensing.md`
4. Recherche: `RO/docs/grf-research/` (insbesondere `02-lzma.md`, `03-measurements.md`)

Danach fasse in 10 Zeilen zusammen, was du verstanden hast, und nenne offene Punkte, bevor du Code schreibst.

## Umgebung
- Projektordner: `RO/1 CLIENT-SIDE/0 GRFs/RustyGRF/` (macOS: `~/Downloads/RO/1 CLIENT-SIDE/0 GRFs/RustyGRF`; Windows-VM: `Z:\Downloads\RO\1 CLIENT-SIDE\0 GRFs\RustyGRF`).
- **Hauptplattform ist macOS** (Apple Silicon, VS Code auf dem Mac). Die Windows-ARM64-VM (Parallels) dient für Windows-Builds und für die Referenzwerkzeuge.
- **Bauverzeichnis außerhalb des Projekts, lokal:** macOS `CARGO_TARGET_DIR=~/build/rustygrf`, Windows `C:\build\rustygrf` (nicht auf dem Share `Z:`).
- Voraussetzungen auf macOS: Xcode Command Line Tools, `rustup`, später Node für die GUI. Auf der Windows-VM fehlen Rust, Git, Node und die C++-Build-Tools mit Windows SDK (siehe `docs/06-tooling-and-workflow.md`). **Frag vor jeder Installation.**
- Referenzwerkzeuge laufen nur unter Windows (`RO/1 CLIENT-SIDE/1 CLIENT TOOLS/5 GRF Editor/`, `GrfCL\GrfCL.exe` mit `-compressionMethod zlib|lzma`). Auf dem Mac stattdessen **Golden Files** unter `tests/golden/` verwenden (Eintragslisten und Hashes, in der VM erzeugt).
- Die Windows-Skripte in `RO/docs/grf-research/tools/` sind nur Beispiele (Windows-Pfade).

## Regeln (verbindlich)
1. **Kein `git init`, kein Push, kein Remote** ohne ausdrückliche Anweisung. Wenn ich `git init` erlaube: erster Commit nur Doku, Commit-Nachrichten kurz, ohne Spieldaten.
2. **Keine Spieldaten committen.** Keine GRFs, Sprites, Texturen, `.lub` aus Archiven. `.gitignore` schließt Archive aus.
3. **Nichts in `client-dev/full/` überschreiben.** Schreibtests nur in Temp-/Testordner.
4. **Messen statt vermuten.** Format-Details, die in `02-format-notes.md` als "zu prüfen" stehen, werden an echten Archiven bestätigt und dort eingetragen (mit Datum und Archiv).
5. **Quellen kennzeichnen.** Wenn Code aus GRF Editor abgeleitet ist, Herkunft im Dateikopf nennen (Regeln in `docs/05-licensing.md`). Bevorzugt aus dem öffentlichen Format und Messungen bauen.
6. **Tests zuerst** für die Abnahmekriterien des jeweiligen Meilensteins. Nach jedem Meilenstein: `cargo fmt`, `cargo clippy -D warnings`, `cargo nextest run`, `cargo deny check` (falls eingerichtet).
7. **Kompatibilität vor Größe:** Standardausgabe ist zlib. LZMA/Zopfli nur ausdrücklich.
8. **Dateinamen sind Bytes** (siehe Format-Notizen), Rundlauf prüfen.
9. **Keine neue Abhängigkeit ohne Lizenzprüfung** (Eintrag in `THIRD-PARTY`).
10. **Frage nach**, wenn eine Entscheidung den Plan ändert (Architektur, Lizenz, Plattformen). Sonst arbeite selbständig.

## Vorgehen
- Starte mit **M0** (Abschluss der Voraussetzungen), dann **M1** (Lesen und Prüfen). Nicht vorgreifen.
- Zerlege jeden Meilenstein in kleine, getestete Schritte. Kleine Commits (sobald Git freigegeben).
- Nutze für aktuelle Bibliotheks-APIs (`rmcp`, `clap`, `zopfli`, LZMA-Crates) **den MCP-Server `context7`** (eingerichtet in `.vscode/mcp.json`) oder docs.rs, nicht dein Gedächtnis.
- Für die schwierigen Stellen (LZMA-Eintragsformat, Writer/Rekonstruktion, Namensrundlauf, MCP-Sicherheit) plane eine zweite Meinung ein (Review durch ein stärkeres Modell oder Gegenprobe mit GRF Editor).
- Halte `docs/` aktuell: offene Fragen, getroffene Entscheidungen, Messwerte.

## Abnahme M1 (Kurzfassung, Details in `docs/03-roadmap.md`)
- Eintragsliste (Pfad, Größen, Flags) für `cdata.grf`, `core.grf`, `cpal.grf`, `data.grf` (202.358 Einträge) identisch mit GRF Editor/`GrfCL`.
- CRC32/MD5 aller Einträge identisch mit den extrahierten Dateien in `client-dev/grf/data-official`.
- `data.grf` öffnen < 2 s, Extraktion parallel. Meldet `worm_tail_damage.wav` als defekt, bricht aber nicht ab.

## Ausgabe, die ich von dir erwarte
- Nach jedem Meilenstein: Zusammenfassung (was gebaut, was getestet, Messwerte, offene Punkte), ohne Marketing.
- Bei Unsicherheit: konkrete Frage mit Optionen und deiner Empfehlung.
````

## Hinweise für den Projektinhaber
- Vor dem Start: Lizenztabelle in `docs/05-licensing.md` ausfüllen, Namen prüfen, Voraussetzungen installieren lassen (siehe `docs/06-tooling-and-workflow.md`).
- Modellwahl: siehe `docs/06-tooling-and-workflow.md` (Sonnet 5.5 high für den Großteil, stärkeres Modell für die schwierigen Stellen und Reviews).
