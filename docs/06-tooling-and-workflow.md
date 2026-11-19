# 06 – Werkzeuge, Modelle und Arbeitsweise

## Modellwahl (meine Einschätzung, nicht gemessen)
Ich kenne keine Benchmarks für genau diese Aufgabe. Die Empfehlung beruht auf der Art der Arbeit:

| Arbeit | Empfehlung |
|---|---|
| Boilerplate: CLI-Befehle (`clap`), JSON-Ausgabe, Tests, MCP-Tool-Gerüste, Doku, CI | Sonnet 5.5 mit hoher Denktiefe reicht in der Regel |
| Schwierige Stellen: LZMA-Eintragsformat nachbauen, Writer mit paralleler Kompression und exakter Rekonstruktion, Dateinamen-Rundlauf, Verschlüsselung/0x300, Sicherheitsmodell des MCP-Servers | stärkeres Modell (Opus 5.5) oder zumindest eine Zweitmeinung per Review |
| Architektur-Review und Code-Review vor jedem Meilenstein | stärkeres Modell |

Was den Modellunterschied klein macht:
- Dieser Plan hat **messbare Abnahmen** und Referenzdaten (eigene Archive, GRF Editor als Gegenprobe). Das Modell muss nicht raten, ob es richtig ist, die Tests sagen es.
- Kleine Schritte, ein Meilenstein nach dem anderen, immer mit Tests und Commit.
- Unklare Format-Details werden **gemessen**, nicht vermutet (siehe `02-format-notes.md`, Spalte "zu prüfen").

Praktisch: Mit Sonnet 5.5 starten und bei einem Fehlschlag oder Fehlentscheid an einer der "schwierigen Stellen" für genau diese Aufgabe das stärkere Modell nehmen. Opus ist nicht zwingend für das ganze Projekt nötig.

## Wo entwickeln
**Hauptplattform: macOS-VS-Code** (Projekt liegt unter `~/Downloads/RO/1 CLIENT-SIDE/0 GRFs/RustyGRF`). Die Windows-ARM64-VM (Parallels) dient nur für Windows-Builds und für die Referenzwerkzeuge (GRF Editor, `GrfCL`), die nur unter Windows laufen.

| | macOS (Hauptplattform) | Windows-VM (Test) |
|---|---|---|
| Compiler | Xcode Command Line Tools (`xcode-select --install`), `rustup` | `rustup` + VS Build Tools "Desktop development with C++" + Windows SDK |
| Build-Verzeichnis | `CARGO_TARGET_DIR=~/build/rustygrf` (nicht in einem synchronisierten Ordner) | `C:\build\rustygrf` (nicht auf `Z:`) |
| Referenz (GRF Editor/`GrfCL`) | nicht verfügbar, stattdessen **Golden Files** (Hashlisten) | verfügbar |
| Dev-Client-Test | nein | ja |

**VM-Steuerung vom Mac aus:** kein Parallels-MCP nötig, `prlctl` genügt (`prlctl exec "Tiny 11 Pro" …`, läuft als SYSTEM; Pfade über `\\Mac\Home\…` statt `Z:`; Beispiel `tools/golden/run-in-vm.sh`). Die Golden Files sind erzeugt (siehe `tests/golden/README.md`).

Golden Files (erledigt 2026-11-16, bei neuen Archiven wiederholen) (Eintragsliste mit Pfad, Größen, CRC32/MD5 für `cdata`, `core`, `cpal`, `data`, `rdata`) und im Repo unter `tests/golden/` ablegen.

## Vorher auf dem Rechner
**macOS (Stand 16.11.2026, installiert):** Rust 1.100.0 stable (`~/.cargo/bin`, per `--no-modify-path`, nicht im Shell-PATH: `. "$HOME/.cargo/env"`), rustfmt, clippy, cargo-nextest, cargo-deny. Bauverzeichnis `~/build/rustygrf` über die lokale, nicht versionierte `.cargo/config.toml`. Node (Homebrew) ist da, wird erst für die GUI gebraucht.

**Windows-ARM64-VM (Stand 16.11.2026 geprüft, Installation erst zu M5):**
Fehlt aktuell: **Rust**, **Git**, **Node.js**, **MSVC-Linker/C++-Build-Tools** und das **Windows SDK**. Vorhanden: VS 2022 Build Tools (nur .NET-Workload), .NET SDK 9, winget, ca. 219 GB frei auf C:.

| Was | Wofür | Installation (Vorschlag) |
|---|---|---|
| Visual Studio 2022 Build Tools: Workload **Desktop development with C++**, inkl. MSVC ARM64/x64 und Windows 11 SDK | Linker für `rustc` (MSVC-Target) | `winget`/VS-Installer, Komponente ergänzen |
| Rust (`rustup`), Target `aarch64-pc-windows-msvc` (+ optional `x86_64-pc-windows-msvc`) | Compiler | `winget install Rustlang.Rustup` |
| `rustfmt`, `clippy` | Format/Lint | über `rustup component add` |
| `cargo-nextest`, `cargo-deny`, `cargo-llvm-cov` (optional) | schnelle Tests, Lizenz-/Sicherheits-Check der Abhängigkeiten, Abdeckung | `cargo install ...` |
| Git | Versionskontrolle (siehe Regeln in `MASTERPROMPT.md`) | `winget install Git.Git` |
| Node.js LTS | nur für die GUI (Frontend), nicht für Kern/CLI/MCP | `winget install OpenJS.NodeJS.LTS` |

Wichtig:
- `CARGO_TARGET_DIR` auf eine **lokale** Platte setzen (z. B. `C:\build\rustygrf`). Bauen auf dem Mac-Share (`Z:`) ist langsam und hat sich schon bei MSBuild als unzuverlässig gezeigt.
- Quellcode darf auf `Z:` liegen (und ins Git), nur das Build-Verzeichnis nicht.
- Reihenfolge: erst Kern/CLI (braucht Rust + C++-Tools), GUI und Node erst ab M7.

## MCP-Server und Erweiterungen für die Entwicklung
**Eingerichtet** in `.vscode/mcp.json` (nur für diesen Workspace, ohne Geheimnisse):

| Server | Typ | Zweck |
|---|---|---|
| `context7` | Remote (`https://mcp.context7.com/mcp`) | aktuelle Doku zu Rust-Crates. Läuft ohne Node und ohne Schlüssel (mit Rate-Limit). Im Prompt "use context7" und die Bibliothek nennen |
| `github` | Remote (`https://api.githubcopilot.com/mcp/`) | Issues, PRs, Actions. Anmeldung per OAuth beim ersten Einsatz, erst nötig, wenn das Repo auf GitHub liegt |

Zum Aktivieren: Ordner `RustyGRF` in VS Code öffnen, bei der Nachfrage dem Workspace und den Servern vertrauen, dann in der Chat-Werkzeugliste (Werkzeug-Symbol) prüfen, dass `context7` Tools zeigt. Befehl "MCP: List Servers" zeigt den Status und Logs. Geprüft habe ich nur, dass der Context7-Endpunkt erreichbar ist (HTTP 400 auf einen absichtlich unvollständigen Testaufruf, also antwortet der Server) und GitHub eine Anmeldung verlangt (401). In VS Code selbst habe ich die Server nicht gestartet.

**Später (M7, GUI-Tests) ergänzen**, wenn Node installiert ist, in `mcp.json` unter `servers`:
```json
"playwright": { "type": "stdio", "command": "npx", "args": ["-y", "@playwright/mcp@latest"] }
```
(Paket `@playwright/mcp`, Version 0.0.85 am 16.11.2026 geprüft.)
Die stdio-Variante von Context7 wäre `npx -y @upstash/context7-mcp` (Paket v4.7.1, geprüft), falls der Remote-Server nicht erreichbar ist.

Wenn M4 fertig ist, kommt der eigene `rgrf-mcp` dazu (`"type": "stdio"`, `command` auf die gebaute Binärdatei).

Weitere Werkzeuge:

| Werkzeug | Nutzen | Priorität |
|---|---|---|
| **rust-analyzer** (`rust-lang.rust-analyzer`) | Typinformationen, Refactoring, Clippy beim Speichern | hoch |
| **Even Better TOML**, **CodeLLDB**, **Dependi** (`.vscode/extensions.json`) | Cargo.toml, Debugging, Abhängigkeitsversionen | mittel |
| **Browser/Playwright-Tools** | GUI-Tests und Screenshots ab M7 | später |
| Ein eigener **`rgrf-mcp`** (Projektziel) | Der Agent nutzt ihn selbst zum Untersuchen von Archiven | später |

## Arbeitsweise
1. Ein Meilenstein = ein Branch/Ordner, Plan aus `03-roadmap.md`, **Tests zuerst** für die Abnahmekriterien.
2. Jede Entscheidung über Format-Details mit Messung an echten Archiven belegen und in `02-format-notes.md` nachtragen.
3. Nach jedem Meilenstein: `cargo fmt`, `cargo clippy -D warnings`, `cargo nextest`, `cargo deny check`, danach Review (Code-Review, bei Sicherheit/Concurrency zusätzlich Zweitmeinung).
4. Nichts committen, was Spieldaten enthält. Keine Archive ins Repo.
5. Doku und Plan aktuell halten, offene Fragen in `03-roadmap.md` führen.

## Verschlüsselter Test-Client (2026-11-19)
`client-dev/full-encrypted` ist eine Kopie von `client-dev/full` ohne `datacomp.grf`, Screenshots und Replays: die vier GRFs (`cdata`, `core`, `cpal`, `data`) mit GRF Editors Verschlüsselung (Passwort in `PASSWORD.txt`), `alpha.exe` gepatcht, dazu die DLL mit dem Schlüssel (Name aus dem Schlüssel, derzeit `cwn.dll`). Erzeugt mit `rgrf --password … --encrypt repack <grf> -o …` und `rgrf --password … client encrypt alpha.exe -o …`. Starten in der VM wie jeder Client **von einer lokalen Kopie auf C:** (`C:\RO-local\full-encrypted`, aus der Freigabe geht Ctrl+S nicht). Gemessen: erreicht die Dienstauswahl mit allen Texturen. Hinweis: `repack` packt LZMA-Einträge in zlib um, `core.grf` wird dadurch größer (88 auf 97 MB).
