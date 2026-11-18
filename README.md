# RustyGRF

Ein modernes, plattformunabhängiges Werkzeug für Ragnarok-Online-Archive (GRF/GPF/Thor), inspiriert von Tokeiburus **GRF Editor**.

**Status:** M0 (Grundlagen), **M1 (Lesen, Prüfen, Extrahieren von GRF 0x200)** und **M1b (GRF 0x300)**, **M2 (`build`, `repack`)** und **M3 (`diff`, `merge`, `patch`)** und **M4 (MCP-Server `rgrf-mcp`, 11 Tools)** sind abgeschlossen: `rgrf info|ls|find|cat|extract|verify|hash|build|repack|diff|merge|patch` läuft auf allen Test-Archiven; Lesen stimmt mit GRF Editor überein, geschriebene Archive liest GRF Editor ohne Abweichung und die LZMA-Variante läuft im Dev-Client (siehe [docs/03-roadmap.md](docs/03-roadmap.md)). **M5 (Plattformen)**: Tests bestanden auf macOS ARM64, Linux ARM64, Windows ARM64 und x64; CI-Konfiguration liegt bereit, läuft erst nach dem GitHub-Push. **M7a (SPR lesen/schreiben, PNG-Export) ist umgesetzt** (`rgrf spr`), siehe [docs/09-formats.md](docs/09-formats.md); **PAL und ACT (lesen, schreiben, als PNG rendern: `rgrf act`)** ebenfalls; nächster Schritt: Bearbeiten, GIF, Anker (M7b Rest). Lokales Git-Repo, **kein Remote, kein Push**. Golden Files aus der Windows-VM liegen vor (`tests/golden/`). Offene Fragen: [docs/rueckfragen/](docs/rueckfragen/README.md) (aktuell Version 7).

**Lizenz:** `MIT OR Apache-2.0`. **Inspiration/Credits:** GRF Editor und ActEditor von Tokeiburu (mit seiner Erlaubnis; RustyGRF ist kein Fork), LessGRF von Curiosity. Siehe `NOTICE`.

## Ziele
1. **Kern-Bibliothek + CLI** (`rgrf`): lesen, auflisten, prüfen, extrahieren, bauen, neu komprimieren, diff/merge/patch.
2. **MCP-Server**, damit KI-Agenten GRFs direkt untersuchen und bauen können.
3. **Cross-Platform:** Windows, macOS, Linux (x64 und ARM64), eine einzelne Binärdatei, keine native `cps.dll`-Abhängigkeit.
4. **Bessere Kompression** bei voller Client-Kompatibilität (Zopfli) und optional LZMA (inoffizielle LessGRF-Erweiterung).
5. Später: Format-Bibliotheken (SPR, ACT, PAL, GAT, GND, RSW, RSM, STR) und eine Oberfläche mit Vorschau.

## Nicht-Ziele (vorerst)
- Kein Ersatz der kompletten GRF-Editor-GUI in der ersten Version.
- Keine Verschlüsselung/Gravity-Schutz in Version 1 (erst nach Entscheidung, siehe [docs/03-roadmap.md](docs/03-roadmap.md)).

## Dokumente
| Datei | Inhalt |
|---|---|
| [docs/01-architecture.md](docs/01-architecture.md) | Aufbau, Crates, Datenfluss, Technologie-Entscheidungen |
| [docs/02-format-notes.md](docs/02-format-notes.md) | GRF-Format, Kompression, Dateinamen, was belegt/offen ist |
| [docs/03-roadmap.md](docs/03-roadmap.md) | Meilensteine mit Abnahmekriterien, Risiken, Testdaten |
| [docs/04-cli-and-mcp.md](docs/04-cli-and-mcp.md) | Befehle, Ausgabeformate, MCP-Tools, Sicherheit |
| [docs/05-licensing.md](docs/05-licensing.md) | Erlaubnis von Tokeiburu, Lizenz, Clean-Room-Regeln |
| [docs/06-tooling-and-workflow.md](docs/06-tooling-and-workflow.md) | Modellwahl, Voraussetzungen auf dem Rechner, MCPs (Context7 u. a.), Arbeitsweise |
| [docs/07-ui-ux.md](docs/07-ui-ux.md) | GUI-Technik (Tauri), UX-Prinzipien, Hauptansicht, Design Tokens |
| [docs/09-formats.md](docs/09-formats.md) | Dateiformate im Client (SPR, später ACT, PAL, ...), Aufbau und Messungen |
| [docs/10-feature-matrix.md](docs/10-feature-matrix.md) | Funktionen von GRF Editor und ACT Editor gegen RustyGRF (Plan M7) |
| [docs/08-translation.md](docs/08-translation.md) | Übersetzung (RO Translation Project) in `core.grf`/`cdata.grf` einspielen, Werkzeug und Messungen |
| [BENCHMARKS.md](BENCHMARKS.md) | Messungen gegen GRF Editor (`GrfCL`) und auf macOS, ehrlich mit Grenzen (Englisch, für Showcase) |
| [docs/rueckfragen/](docs/rueckfragen/README.md) | Rückfragen, Antworten, Entscheidungen (je Runde eine versionierte Datei; aktuell v7) |
| [docs/MASTERPROMPT.md](docs/MASTERPROMPT.md) | Prompt für eine neue Session, um sofort loszulegen |

## Verwandte Recherche
`RO/docs/grf-research/` (LZMA, data vs rdata, Messwerte, Roh-Transkripte der Forum-Threads).

## Schnellstart
```sh
cargo build --release                      # Ergebnis in target/ (oder dem Verzeichnis aus .cargo/config.toml)
rgrf info  data.grf                        # Version, Größe, Einträge
rgrf ls    data.grf "*.lub" --long         # suchen/auflisten
rgrf verify data.grf                       # Tabelle prüfen, jeden Eintrag dekodieren
rgrf extract data.grf ./out "data/sprite*" # entpacken (überschreibt nie ohne --overwrite)
rgrf build ./out -o neu.grf --method lzma --dedupe   # GRF aus Ordner (Ordner = Archivwurzel)
rgrf merge custom.grf data.grf -o merged.grf         # das ERSTE Archiv gewinnt (wie DATA.INI)
rgrf diff a.grf b.grf                      # Unterschiede nach Pfad und Inhalt
```
Alle Befehle: [docs/04-cli-and-mcp.md](docs/04-cli-and-mcp.md). Standard-Kompression ist zlib (jeder Client); LZMA nur mit LZMA-fähiger `cps.dll`, Zopfli nur ausdrücklich.
