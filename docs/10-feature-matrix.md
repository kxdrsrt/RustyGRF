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
| Verschlüsselung (`Encryptor`) | DES (Flags 3/5, 0x102/0x103) wird gelesen; verschlüsselte Einträge werden beim Umpacken unverändert kopiert | ✅ DES lesen (CRC32 gegen GRF Editor gleich); DES schreiben (nur im Format 0x102/0x103); **GRF-Editor-Verschlüsselung ✅ (2026-11-19):** `--password`/`--key-file` öffnen verschlüsselte Archive (auch mit verschlüsselter Tabelle), `--encrypt` bei `build`/`repack`/`merge`/`patch`, `key create` (`.grfkey`), `client encrypt` (gepatchte EXE und DLL aus GRF Editors `cps.dll`-Vorlage), GUI fragt das Passwort ab und speichert wieder verschlüsselt, MCP über `RGRF_PASSWORD`; gegen GrfCL und `Encryption.dll` byteweise geprüft und mit einem verschlüsselten Client gestartet ([02-format-notes.md](02-format-notes.md)); **Gravity-Schutz (0x80) lesen ✅ (2026-11-19, kRO-Variante, aus GRF Editor portiert, alle 40 Testbeispiele GRF Editors wie dort; jRO- und China-Variante ❌, keine Beispiele)**; beim Schreiben bleibt der Schutz in 0x300 erhalten, sonst wird entschlüsselt gespeichert |
| Dateinamen-Kodierung ansehen (`ViewEncoding`) | `--names cp1252|cp949|utf8|raw` | ✅ |
| Vorschau Bild/BMP/TGA/JPG (`PreviewImage`, `ImageConverter`) | GUI-Vorschau (BMP/JPG/PNG/GIF, TGA → PNG, Magenta durchsichtig) | ✅ (51.782 BMP, 9.063 TGA gemessen); **Umwandeln und Verkleinern ✅ (M7d, 2026-11-19):** `rgrf image info|convert|reduce`, GUI („8-bit…“ je BMP mit Vergleich, „Reduce Images…“ fürs Archiv), MCP `grf_image_reduce`; 24-Bit-BMP → 8 Bit mit PSNR-Schwelle, Magenta exakt (an allen ~26.000 verkleinerten BMPs von `data.grf` geprüft); bei 40 dB spart `data.grf` 275 MiB, `rdata.grf` 7,5 %. Mehr als GRF Editor: dessen „Convert Bgr24 textures in Indexed8“ ist im Quelltext ein leerer Stub. JPEG lesen ❌, PNG verlustfrei nachpacken ❌ |
| Vorschau und Bearbeiten **SPR** (`PreviewSpr`, `SpriteEditor`, `SpriteConverter`) | `spr info|export|build|replace|add|remove|set-palette` | ✅ Lesen/Schreiben/PNG (M7a), Bearbeiten (M7b); Oberfläche ✅ (M8: Bilder ersetzen, hinzufügen, entfernen, Palette tauschen, Pixel-Editor, True-Colour ↔ Palette) |
| Vorschau **ACT** (`PreviewAct`) | `act info`, `act render` (PNG) | ✅ Lesen/Schreiben/Rendern, Anker-Bindung, GIF (M7b) |
| Palette (`PreviewPalette`, `PaletteOperations`) | PAL lesen/schreiben, Palette tauschen (`spr set-palette`), `pal …` | ✅ |
| Lua/Lub anzeigen, dekompilieren (`PreviewText`, `DecompilerSettings`) | `lua check` (Syntax), `cat` für Text | ✅ Syntaxprüfung (M7c); Dekompilieren Lua 5.1 ✅ (alle 504 kompilierten Dateien vollständig, mit lokalen Namen, Schleifen und `and`/`or`; Inhalt bei 445 von 504 Dateien gleich GRF Editor, Rest erklärt; mit echtem Lua 5.1 geprüft: alle 504 Texte vom Parser angenommen, alle 442 Lua-5.1- und alle 62 Lua-5.0-Dateien verhalten sich wie das Original, siehe 09-formats.md), Lua 5.0 ✅; **zurück zu `.lub` kompilieren ✅ (`lua compile`: dieselben Bytes wie das echte Lua 5.1.5, 442 von 442 Dateien und 1,5 Mio. Zufallsprogramme)**, **Lua 5.0 kompilieren ✅ (`lua compile --lua-version 5.0`: dieselben Bytes wie das echte Lua 5.0.3, 62 von 62 Dateien, 2 Mio. Zufallsprogramme und 1 Mio. beschädigte Eingaben ohne Abweichung; Original-Code bei 59 von 62 Dateien zurück; 5.1: alle Dateien aller Test-Archive geben den Original-Code zurück, auch Latin-1-Texte anderer Sprachen)** |
| Karten: GAT-Vorschau, Karten-Editor (Zellen), `MapExtractor` | `map info|png|check` | ✅ GAT/RSW lesen, schreiben, Bild, Ressourcen-Check (M7e); Zellen bearbeiten ✅ (`map gat-set`, Rechteck: Typ und/oder Höhe; GUI mit Mausauswahl ✅), **Karte extrahieren ✅ (2026-11-19):** `rgrf map extract`, GUI-Knopf in der 3D-Karte, MCP `grf_map_files`; nimmt mehr mit als GRF Editors `MapExtractor` (zusätzlich Sounds, Wasserbilder, Minikarte); alle 1.029 Karten der kRO-`data.grf` in 0,6 s aufgelöst, Prontera als GRF zurückgebaut besteht `map check` |
| 3D: RSM, STR (`OpenGLViewport`) | `map info` (RSM-Struktur und Texturen), `map check` | ✅ RSM strukturell lesen (M7e); STR-Vorschau als GIF (vereinfacht) ✅; RSM-3D-Ansicht mit Knotenanimation ✅ (654 animierte Modelle); Karten-3D (Gelände, Modelle, Wasser) ✅; Wasser als Bildzyklus ✅, bewegte Modelle in Karten ✅, Wasserwellen ✅ (Formel mit roBrowser verglichen), Texturanimation RSM 2.3 ✅ |
| Audio/Video (`PreviewWav`, `PreviewBink`) | GUI: WAV/MP3/OGG abspielen (ADPCM → PCM); Bink: Kopfdaten und „mit dem System-Player öffnen“ | ✅ Audio; **Bink ✅ teilweise (2026-11-19):** Größe, Bildzahl, Bildrate und Länge aus dem Dateikopf (gemessen an `data\video\ad\01.bik`: 512 × 256, 450 Bilder, 30 fps), Öffnen mit dem Programm des Systems; **kein eigener Player** (Bink ist ein proprietäres Format, ein Decoder wäre Lizenz- und Pflegeaufwand) |
| Ordnerstruktur (Baumansicht), Sortierung, Filter | `ls` | ✅ GUI: Ansichten List, Details, Tree, Folders; Sortierung nach 7 Kriterien; Filter nach Typ und Größe; Vorschau rechts/unten/aus (siehe `apps/gui/README.md`) |
| `ThumbDb` (Vorschaubilder), `TypeExplorer`, Ressourcen-Indexer | **Galerie ✅ (2026-11-19):** fünfte Ansicht (Cmd/Ctrl+5), Raster mit Vorschaubildern für Bilder (BMP, TGA, PNG, JPG, GIF) und Sprites (erstes Bild), sichtbare Zellen werden nachgeladen, Pfeiltasten gehen im Raster; `ls`, `info` für die Typübersicht | ✅ Galerie: 120 Vorschaubilder aus der kRO-`data.grf` in 12 ms (alle Kerne), 18 KB je Bild; im Browser mit dem echten Backend bedient. **Cache nur im Speicher** (1.500 Bilder, geleert bei Änderungen); eine Datenbank auf der Platte wie ThumbDb gibt es nicht. Nicht gemessen: sehr große Archive mit schnellem Scrollen im echten Fenster |
| Hex-Ansicht (Be.HexBox), `MagicEdit` | GUI: „Hex…“ bei jeder Datei, 16 Byte je Zeile, Blättern, Springen zu einem Offset, einzelne Bytes ändern | ✅ (2026-11-19) Ansicht der ganzen Datei und Überschreiben einzelner Bytes (die Größe bleibt, Änderungen sind vorgemerkt und rückgängig zu machen); **Suchen ✅ (2026-11-20):** Hex-Bytes oder Text (UTF-8), „Weitersuchen“ springt zur Fundstelle (mit Umbruch an den Anfang) und markiert sie; Test im Backend, im Browser mit der echten `goldporing.act` bedient; Text nur UTF-8 (nicht CP949). Einfügen/Löschen von Bytes ❌ |
| GrfCL (Kommandozeile) | `rgrf` | ✅ (Vergleich in `BENCHMARKS.md`) |

## GRF Editor: jeder Menüpunkt im Vergleich (2026-11-20)

Aus `EditorMainWindow.xaml` und `EMenuInteraction.cs` gelesen, in der Windows-VM gegen GRF Editor angesehen.

| GRF Editor | RustyGRF | Stand |
|---|---|---|
| File › New › Grf, Gpf, Rgz, Thor | Datei › Neu › GRF, GPF, RGZ, Thor (Cmd/Ctrl+N) | ✅ leeres Archiv, Typ beim Speichern |
| File › New › Gsf („Gravity Secure Patch“) | – | ❌ Format unbekannt, keine Beispieldatei |
| Open…, Open recent, Close Grf | Öffnen, Zuletzt geöffnet (10, für alle Fenster), Archiv schließen | ✅ |
| (ein Fenster je Programm) | **mehrere Fenster** in einer App, gleiches Archiv holt sein Fenster nach vorn | ✅ besser (in der Windows-VM gemessen) |
| Save, Save as… | Speichern (an Ort und Stelle: daneben schreiben, prüfen, tauschen), Speichern unter (Container nach Endung) | ✅ |
| Save advanced › Defragment (quick), Compact (redirect indexes) | Erweitert speichern › Verdichten: schreibt das Archiv lückenlos neu, Daten werden kopiert | ✅ anderer Weg (immer vollständig neu geschrieben, geprüft) |
| Save advanced › Repack (recompress) | Erweitert speichern › Neu packen (zlib, zopfli, LZMA; Version wählbar) | ✅ |
| Save advanced › Encrypt File Table | Verschlüsseln (alle Einträge, GRF Editors Verfahren) | ✅ Einträge; nur die Tabelle allein ❌ |
| About…, Quit | Hilfe › Über (Credits), Beenden | ✅ |
| Undo, Redo | Rückgängig, Wiederholen | ✅ |
| Navigate backward/forward (Auswahlverlauf) | – | ❌ |
| Merge, Subtract | Zusammenführen; Werkzeuge › Mit Archiv vergleichen (Unterschiede, als Thor- oder GRF-Patch speichern) | ✅ Subtrahieren über Vergleich |
| Extract Rgz | RGZ öffnen, Extrahieren | ✅ |
| Edit Grf/Thor settings (Format, Anzeigekodierung, Magic header) | Bearbeiten › Archiv-Einstellungen (Version, Kompression, Verschlüsselung; Namensanzeige in der Statusleiste) | ✅; Magic header ändern ❌, Thor-Ziel-GRF in der App ❌ (CLI `thor create` kann es) |
| Tools › Grf validation | Werkzeuge › Archiv prüfen | ✅ |
| Open roaming data | App-Datenordner öffnen | ✅ |
| Import sprite…, Sprite editor | Sprite-Vorschau: Bilder hinzufügen, ersetzen, entfernen, Pixel-Editor | ✅ |
| Flat maps maker (Boden flach, Lightmap/Schatten/Farbe, Wasser, Objekte entfernen, Licht zurücksetzen) | Karte extrahieren mit Teilen abwählen | 🔨 Karten flach machen fehlt (GND/RSW schreiben nötig) |
| Patch maker | Mit Archiv vergleichen › Als Patch speichern (Thor mit Löschanweisungen) | ✅ |
| Hash viewer | Werkzeuge › Prüfsummen (CRC32, MD5, SHA-256) | ✅ |
| Text encoding (Text zwischen Kodierungen umrechnen) | Namensanzeige Koreanisch/Latin-1/UTF-8; Texte werden beim Öffnen erkannt | ❓ eigenes Werkzeug fehlt |
| Advanced search (Text in Dateien, Regex, ganze Wörter, Groß/klein, Dateifilter) | Werkzeuge › In Dateien suchen (Cmd/Ctrl+Shift+F) | ✅ (kRO: 133 Textdateien in 0,56 s) |
| Image converter | „8-bit…“, Bilder verkleinern, CLI `image convert` | ✅ |
| GRF Command Line | `rgrf` (CLI) und MCP-Server | ✅ |
| OpenGL Logger | – | nicht nötig (WebGL im Webview) |
| Image finder | – | im Quelltext auskommentiert, auch bei GRF Editor ohne Funktion |
| GRF Encryption, Gravity Encryption, Set kRO/jRO/cnRO keys | Passwort-Verschlüsselung; Gravitys Schutz mit eingebauten kRO-Schlüsseln | ✅ kRO; jRO/cnRO ❌ (keine Beispieldateien) |
| Grf patcher | Zusammenführen (Patch über das Archiv legen) | ✅ |
| Settings | Einstellungen | ✅ |

## ACT Editor (Tokeiburu)

| Funktion (laut README, Changelog, Ordner) | RustyGRF | Stand |
|---|---|---|
| Aktionen, Frames, Layer, Anker lesen/schreiben | Bibliothek `rgrf-formats` | ✅ Versatz, Intervall, Aktion kopieren/entfernen; Layer-Feinbearbeitung ✅ (`act layer`: Farbe, Skalierung, Drehung, Spiegeln, wählbar je Aktion/Frame/Layer; an einer echten ACT gerendert) |
| Referenz-Sprite wählen (auch aus GRF) | `act render --with` (Teile übereinander) | ✅ (2026-11-19) im ACT-Editor: beliebig viele Animationen des Archivs als Referenz, dahinter oder davor, am ersten Anker ausgerichtet wie im Client (Kopf und Kopfbedeckung am Körper); Vorschläge Körper/Kopf männlich/weiblich; bleibt gespeichert. Gemessen an `남_Crushed_Snow_Man.act` mit `초보자_남.act` und `1_남.act` |
| Interpolation (auch „erweitert“), Skripte (C#-Skript-Engine, `doc/ScriptGuide`) | **Zwischenbilder ✅ (2026-11-19):** „In-betweens…“ im ACT-Editor und `rgrf act interpolate`: 1–20 Zwischenframes je Paar, ein Paar oder die ganze Aktion, gleichmäßig oder weich, optional als Schleife (letzter Frame zurück zum ersten), mit Tempo-Erhalt (Frame-Zeit kürzer); Versatz, Skalierung, Drehung (kurzer Weg), Farbe und Anker werden gemischt, Bild und Spiegelung wechseln zur Hälfte, Sounds bleiben bei den Originalframes | ✅ Zwischenbilder: Tests (Bibliothek, GUI-Backend), echte ACT (`goldporing`, 8 → 24 Frames, Bilderfolge angesehen), Dialog im Browser. Mehr als gleichmäßige/weiche Kurven (z. B. eigene Kurven, „erweitert“) fehlt. Skripte: bewusst ❌ (CLI und MCP-Server decken die Automatisierung ab) |
| GIF-Export | `act render --gif` | ✅ (Hintergrund wählen ❓) |
| Sound-Effekte je Frame | `act sound` | ✅ (setzen/entfernen, Tabelle wächst; Eingabe im GUI ✅ (ACT-Editor), **Abspielen ✅ (2026-11-19):** „Play“ im ACT-Editor spielt die Aktion mit ihren Sounds, ein Knopf hört den Sound eines Frames vor; „atk“ wird als Treffer-Markierung erkannt) |
| Palette Editor (`PaletteEditorTool`, `PaletteSheetGenerator`) | `pal info/extract/png/from-png/set/adjust/sheet` | ✅ Palette ansehen, als Bild bearbeiten und zurückholen, Farben setzen, Farbton/Sättigung/Helligkeit, Vorschaublatt eines Sprites mit mehreren Paletten (an einem echten Sprite geprüft); Oberfläche ✅ (`.pal`-Vorschau: Farbe anklicken, Bereich anpassen; vorgemerkt wie jede Änderung); Pixel-Editor für Sprites ✅ (Paletten-Bilder: Stift, Radierer, Füllen, Pipette, Palette; `SpriteEditorTools`) |
| Sprite von indiziert nach BGRA konvertieren (und zurück) | `spr convert --to rgba|indexed [--act … --act-output …]` | ✅ (2026-11-19; ACT-Ebenen folgen; zurück verlustfrei bei höchstens 255 Farben, sonst Median-Cut mit Meldung; echter Sprite `C_Surf_Board_Poring` hin und zurück: gleiche Größe, 9 Bilder) |
| `GrfShellExplorer` (Windows-Explorer-Integration) | – | ❓ nur Windows |

## Was RustyGRF anders machen soll
- Alles in **einer** App und **einer** Bibliothek (GRF Editor, ACT Editor und Palette Editor sind getrennte Programme).
- Dieselbe Funktion in CLI und MCP, damit ein Agent Sprites prüfen und Archive bauen kann.
- Läuft auf macOS, Linux und Windows (GRF Editor/ACT Editor nur Windows).

## Offene Fragen an dich
1. **Skripte im ACT Editor:** Wollen wir eine Skriptsprache (z. B. Lua) für Massenbearbeitung, oder reichen feste Funktionen (Palette tauschen, Offset verschieben, kopieren)? Empfehlung: erst feste Funktionen.
2. **Audio/Video/Hex-Ansicht:** brauchbar oder weglassen? Empfehlung: weglassen, bis Bedarf da ist.
