# RustyGRF

Ein modernes, plattformunabhängiges Werkzeug für Ragnarok-Online-Archive (GRF/GPF/Thor), inspiriert von Tokeiburus **GRF Editor**.

**Status:** M0 (Grundlagen), **M1 (Lesen, Prüfen, Extrahieren von GRF 0x200)** und **M1b (GRF 0x300)**, **M2 (`build`, `repack`)** und **M3 (`diff`, `merge`, `patch`)** und **M4 (MCP-Server `rgrf-mcp`, 11 Tools)** sind abgeschlossen: `rgrf info|ls|find|cat|extract|verify|hash|build|repack|diff|merge|patch` läuft auf allen Test-Archiven; Lesen stimmt mit GRF Editor überein, geschriebene Archive liest GRF Editor ohne Abweichung und die LZMA-Variante läuft im Dev-Client (siehe [docs/03-roadmap.md](docs/03-roadmap.md)). Nächster Schritt: M5 (Plattformen und Release) bzw. nach deiner Wahl (Punkt 33 in den Rückfragen). Lokales Git-Repo, **kein Remote, kein Push**. Golden Files aus der Windows-VM liegen vor (`tests/golden/`). Offene Fragen: [docs/rueckfragen/](docs/rueckfragen/README.md) (aktuell Version 7).

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
| [docs/rueckfragen/](docs/rueckfragen/README.md) | Rückfragen, Antworten, Entscheidungen (je Runde eine versionierte Datei; aktuell v6) |
| [docs/MASTERPROMPT.md](docs/MASTERPROMPT.md) | Prompt für eine neue Session, um sofort loszulegen |

## Verwandte Recherche
`RO/docs/grf-research/` (LZMA, data vs rdata, Messwerte, Roh-Transkripte der Forum-Threads).
