# 10 – Funktionsmatrix: GRF Editor und ACT Editor gegen RustyGRF (M7.0)

**Quelle und Grenzen:** gelesen aus den Quelltexten in `1 CLIENT TOOLS/5 GRF Editor/src` und `6 ACT Editor` (Ordner- und Dateinamen, `README`, `changelog.txt`, `doc/`). Daraus ist der **Umfang** abgeleitet, nicht jedes Verhalten; Details werden je Funktion beim Nachbauen aus dem Code gelesen und gegen das Programm in der Windows-VM geprüft. Stand der Tabelle: 2026-11-18.

Legende: ✅ umgesetzt und gemessen, 🔨 geplant (Meilenstein), ❓ Nutzen offen (Frage an dich).

## GRF Editor

| Funktion (laut Quelltext) | RustyGRF | Stand |
|---|---|---|
| Archive öffnen, Einträge listen, suchen, extrahieren | `info`, `ls`, `find`, `cat`, `extract` | ✅ M1 |
| Prüfen/Validieren (`GrfValidation`) | `verify` (Tabelle, Überlappung, Dekodieren), `lua check`, `client check` | ✅ M1, M7c; fehlende Ressourcen allgemein ❓ (nicht zuverlässig möglich) |
| Bauen, neu komprimieren, `Compact`/`GrfShrinker` | `build`, `repack` | ✅ M2 |
| Zusammenführen (`MergeDialog`), Subtrahieren (`SubtractDialog`), Patch | `merge`, `diff`, `patch` | ✅ M3 (Subtrahieren: `diff` + Auswahl, nicht eigener Befehl) |
| Hash (`HashDialog`) | `hash` | ✅ |
| GRF/GPF/RGZ/Thor lesen, `Patcher`/`TkPatch` | alle Befehle öffnen **GRF 0x102, 0x103, 0x200, 0x300, GPF, Thor (Modus 0x30 und 0x21) und RGZ** (Erkennung am Inhalt); `thor create`, `rgz create`; Patches anwenden mit `merge` | ✅ GRF 0x200/0x300 (Millionen Einträge gemessen), **0x102 an `event.grf`** (39 Dateien, Inhalt gegen 0x200-Umwandlung gleich); **Thor (0x30, 0x21), RGZ, GPF gegen Dateien von GRF Editors `GrfCL` geprüft** (byteidentischer Inhalt, GrfCL liest unsere Dateien); **0x102/0x103 schreiben** (`--format 0x102`/`0x103` bei `build`, `repack`, `merge`; GrfCL entpackt unsere `event.grf`-Umpackung in beiden Versionen mit identischem Inhalt, 39 Dateien); **Dateien Dritter** (`tools/golden/fetch-third-party-fixtures.sh`): 0x102/0x103/0x200 aus `L1nkZ/rpatchur` (Inhalt in allen drei Versionen identisch), 5 Thor-Dateien (Modus 0x30 und 0x21; `data.integrity` stimmt) und 5 GPF (178 Dateien in der größten) fehlerfrei; **RGZ echter Patcher ungemessen** (keine Datei gefunden) |
| Verschlüsselung (`Encryptor`) | DES (Flags 3/5, 0x102/0x103) wird gelesen; verschlüsselte Einträge werden beim Umpacken unverändert kopiert | ✅ DES lesen (CRC32 gegen GRF Editor gleich); DES schreiben (nur im Format 0x102/0x103); **GRF-Editor-Verschlüsselung ✅ (2026-11-19):** `--password`/`--key-file` öffnen verschlüsselte Archive (auch mit verschlüsselter Tabelle), `--encrypt` bei `build`/`repack`/`merge`/`patch`, `key create` (`.grfkey`), `client encrypt` (gepatchte EXE und DLL aus GRF Editors `cps.dll`-Vorlage), GUI fragt das Passwort ab und speichert wieder verschlüsselt, MCP über `RGRF_PASSWORD`; gegen GrfCL und `Encryption.dll` byteweise geprüft und mit einem verschlüsselten Client gestartet ([02-format-notes.md](02-format-notes.md)); Gravity-Schutz (0x80) ❌ (Schlüssel fehlen) |
| Dateinamen-Kodierung ansehen (`ViewEncoding`) | `--names cp1252|cp949|utf8|raw` | ✅ |
| Vorschau Bild/BMP/TGA/JPG (`PreviewImage`, `ImageConverter`) | GUI-Vorschau (BMP/JPG/PNG/GIF, TGA → PNG, Magenta durchsichtig) | ✅ (51.782 BMP, 9.063 TGA gemessen); Bilder umwandeln/optimieren 🔨 M7d |
| Vorschau und Bearbeiten **SPR** (`PreviewSpr`, `SpriteEditor`, `SpriteConverter`) | `spr info|export|build|replace|add|remove|set-palette` | ✅ Lesen/Schreiben/PNG (M7a), Bearbeiten (M7b); Oberfläche 🔨 M8 |
| Vorschau **ACT** (`PreviewAct`) | `act info`, `act render` (PNG) | ✅ Lesen/Schreiben/Rendern, Anker-Bindung, GIF (M7b) |
| Palette (`PreviewPalette`, `PaletteOperations`) | PAL lesen/schreiben, Palette tauschen (`spr set-palette`) | ✅; weitere Operationen 🔨 |
| Lua/Lub anzeigen, dekompilieren (`PreviewText`, `DecompilerSettings`) | `lua check` (Syntax), `cat` für Text | ✅ Syntaxprüfung (M7c); Dekompilieren Lua 5.1 ✅ (alle 504 kompilierten Dateien vollständig, mit lokalen Namen, Schleifen und `and`/`or`; Inhalt bei 445 von 504 Dateien gleich GRF Editor, Rest erklärt; mit echtem Lua 5.1 geprüft: alle 504 Texte vom Parser angenommen, alle 442 Lua-5.1- und alle 62 Lua-5.0-Dateien verhalten sich wie das Original, siehe 09-formats.md), Lua 5.0 ✅; **zurück zu `.lub` kompilieren ✅ (`lua compile`: dieselben Bytes wie das echte Lua 5.1.5, 442 von 442 Dateien und 1,5 Mio. Zufallsprogramme)**, **Lua 5.0 kompilieren ✅ (`lua compile --lua-version 5.0`: dieselben Bytes wie das echte Lua 5.0.3, 62 von 62 Dateien, 2 Mio. Zufallsprogramme und 1 Mio. beschädigte Eingaben ohne Abweichung; Original-Code bei 59 von 62 Dateien zurück; 5.1: alle Dateien aller Test-Archive geben den Original-Code zurück, auch Latin-1-Texte anderer Sprachen)** |
| Karten: GAT-Vorschau, Karten-Editor (Zellen), `MapExtractor` | `map info|png|check` | ✅ GAT/RSW lesen, schreiben, Bild, Ressourcen-Check (M7e); Zellen bearbeiten ✅ (`map gat-set`, Rechteck: Typ und/oder Höhe; GUI mit Mausauswahl 🔨), Karte extrahieren 🔨 |
| 3D: RSM, STR (`OpenGLViewport`) | `map info` (RSM-Struktur und Texturen), `map check` | ✅ RSM strukturell lesen (M7e); STR-Vorschau als GIF (vereinfacht) ✅; RSM-3D-Ansicht mit Knotenanimation ✅ (654 animierte Modelle); Karten-3D (Gelände, Modelle, Wasser) ✅; Wasser als Bildzyklus ✅, bewegte Modelle in Karten ✅, Wasserwellen 🔨 |
| Audio/Video (`PreviewWav`, `PreviewBink`) | GUI: WAV/MP3/OGG abspielen (ADPCM → PCM) | ✅ Audio; Bink-Video ❓ |
| `ThumbDb`, `TypeExplorer`, Raw-/Ordnerstruktur, Ressourcen-Indexer | `ls`, `info` teilweise | ❓ |
| Hex-Ansicht (Be.HexBox), `MagicEdit` | GUI: erste 512 Byte als Hexdump für unbekannte Dateien | ✅ Ansicht; Bearbeiten ❓ |
| GrfCL (Kommandozeile) | `rgrf` | ✅ (Vergleich in `BENCHMARKS.md`) |

## ACT Editor (Tokeiburu)

| Funktion (laut README, Changelog, Ordner) | RustyGRF | Stand |
|---|---|---|
| Aktionen, Frames, Layer, Anker lesen/schreiben | Bibliothek `rgrf-formats` | ✅ Versatz, Intervall, Aktion kopieren/entfernen; Layer-Feinbearbeitung 🔨 |
| Referenz-Sprite wählen (auch aus GRF) | – | 🔨 M7b |
| Interpolation (auch „erweitert“), Skripte (C#-Skript-Engine, `doc/ScriptGuide`) | – | ❓ Skripte: andere Sprache nötig (Frage) |
| GIF-Export | `act render --gif` | ✅ (Hintergrund wählen ❓) |
| Sound-Effekte je Frame | – | 🔨 M7b |
| Palette Editor (`PaletteEditorTool`, `PaletteSheetGenerator`) | – | 🔨 M7b |
| Sprite von indiziert nach BGRA konvertieren (und zurück) | `spr convert --to rgba|indexed [--act … --act-output …]` | ✅ (2026-11-19; ACT-Ebenen folgen; zurück verlustfrei bei höchstens 255 Farben, sonst Median-Cut mit Meldung; echter Sprite `C_Surf_Board_Poring` hin und zurück: gleiche Größe, 9 Bilder) |
| `GrfShellExplorer` (Windows-Explorer-Integration) | – | ❓ nur Windows |

## Was RustyGRF anders machen soll
- Alles in **einer** App und **einer** Bibliothek (GRF Editor, ACT Editor und Palette Editor sind getrennte Programme).
- Dieselbe Funktion in CLI und MCP, damit ein Agent Sprites prüfen und Archive bauen kann.
- Läuft auf macOS, Linux und Windows (GRF Editor/ACT Editor nur Windows).

## Offene Fragen an dich
1. **Skripte im ACT Editor:** Wollen wir eine Skriptsprache (z. B. Lua) für Massenbearbeitung, oder reichen feste Funktionen (Palette tauschen, Offset verschieben, kopieren)? Empfehlung: erst feste Funktionen.
2. **Audio/Video/Hex-Ansicht:** brauchbar oder weglassen? Empfehlung: weglassen, bis Bedarf da ist.
