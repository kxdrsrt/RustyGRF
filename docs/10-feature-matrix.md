# 10 – Funktionsmatrix: GRF Editor und ACT Editor gegen RustyGRF (M7.0)

**Quelle und Grenzen:** gelesen aus den Quelltexten in `1 CLIENT TOOLS/5 GRF Editor/src` und `6 ACT Editor` (Ordner- und Dateinamen, `README`, `changelog.txt`, `doc/`). Daraus ist der **Umfang** abgeleitet, nicht jedes Verhalten; Details werden je Funktion beim Nachbauen aus dem Code gelesen und gegen das Programm in der Windows-VM geprüft. Stand der Tabelle: 2026-11-18.

Legende: ✅ umgesetzt und gemessen, 🔨 geplant (Meilenstein), ❓ Nutzen offen (Frage an dich).

## GRF Editor

| Funktion (laut Quelltext) | RustyGRF | Stand |
|---|---|---|
| Archive öffnen, Einträge listen, suchen, extrahieren | `info`, `ls`, `find`, `cat`, `extract` | ✅ M1 |
| Prüfen/Validieren (`GrfValidation`) | `verify` (Tabelle, Überlappung, Dekodieren) | ✅ M1; inhaltliche Prüfung (Lua, fehlende Dateien) 🔨 M7c/M7f |
| Bauen, neu komprimieren, `Compact`/`GrfShrinker` | `build`, `repack` | ✅ M2 |
| Zusammenführen (`MergeDialog`), Subtrahieren (`SubtractDialog`), Patch | `merge`, `diff`, `patch` | ✅ M3 (Subtrahieren: `diff` + Auswahl, nicht eigener Befehl) |
| Hash (`HashDialog`) | `hash` | ✅ |
| GRF/GPF/RGZ/Thor lesen, `Patcher`/`TkPatch` | GRF 0x200/0x300 | ✅ GRF; Thor/RGZ/GPF 🔨 M6 |
| Verschlüsselung (`Encryptor`) | – | 🔨 M6 (Entscheidung offen) |
| Dateinamen-Kodierung ansehen (`ViewEncoding`) | `--names cp1252|cp949|utf8|raw` | ✅ |
| Vorschau Bild/BMP/TGA/JPG (`PreviewImage`, `ImageConverter`) | – | 🔨 M7d |
| Vorschau und Bearbeiten **SPR** (`PreviewSpr`, `SpriteEditor`, `SpriteConverter`) | `spr info|export|build` | ✅ Lesen/Schreiben/PNG (M7a); Bearbeiten 🔨 M7b/M8 |
| Vorschau **ACT** (`PreviewAct`) | – | 🔨 M7b |
| Palette (`PreviewPalette`, `PaletteOperations`) | – | 🔨 M7b |
| Lua/Lub dekompilieren, Text (`PreviewText`, `DecompilerSettings`) | – | 🔨 M7c |
| Karten: GAT-Vorschau, Karten-Editor (Zellen), `MapExtractor` | – | 🔨 M7e |
| 3D: RSM, STR (`OpenGLViewport`) | – | 🔨 M7e |
| Audio/Video (`PreviewWav`, `PreviewBink`) | – | ❓ |
| `ThumbDb`, `TypeExplorer`, Raw-/Ordnerstruktur, Ressourcen-Indexer | `ls`, `info` teilweise | ❓ |
| Hex-Ansicht (Be.HexBox), `MagicEdit` | – | ❓ |
| GrfCL (Kommandozeile) | `rgrf` | ✅ (Vergleich in `BENCHMARKS.md`) |

## ACT Editor (Tokeiburu)

| Funktion (laut README, Changelog, Ordner) | RustyGRF | Stand |
|---|---|---|
| Aktionen, Frames, Layer, Anker bearbeiten | – | 🔨 M7b |
| Referenz-Sprite wählen (auch aus GRF) | – | 🔨 M7b |
| Interpolation (auch „erweitert“), Skripte (C#-Skript-Engine, `doc/ScriptGuide`) | – | ❓ Skripte: andere Sprache nötig (Frage) |
| GIF-Export, Hintergrund wählen | – | 🔨 M7b |
| Sound-Effekte je Frame | – | 🔨 M7b |
| Palette Editor (`PaletteEditorTool`, `PaletteSheetGenerator`) | – | 🔨 M7b |
| Sprite von indiziert nach BGRA konvertieren (und zurück) | – | 🔨 M7b |
| `GrfShellExplorer` (Windows-Explorer-Integration) | – | ❓ nur Windows |

## Was RustyGRF anders machen soll
- Alles in **einer** App und **einer** Bibliothek (GRF Editor, ACT Editor und Palette Editor sind getrennte Programme).
- Dieselbe Funktion in CLI und MCP, damit ein Agent Sprites prüfen und Archive bauen kann.
- Läuft auf macOS, Linux und Windows (GRF Editor/ACT Editor nur Windows).

## Offene Fragen an dich
1. **Skripte im ACT Editor:** Wollen wir eine Skriptsprache (z. B. Lua) für Massenbearbeitung, oder reichen feste Funktionen (Palette tauschen, Offset verschieben, kopieren)? Empfehlung: erst feste Funktionen.
2. **Audio/Video/Hex-Ansicht:** brauchbar oder weglassen? Empfehlung: weglassen, bis Bedarf da ist.
