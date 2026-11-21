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
| Verschlüsselung (`Encryptor`) | DES (Flags 3/5, 0x102/0x103) wird gelesen; verschlüsselte Einträge werden beim Umpacken unverändert kopiert | ✅ DES lesen (CRC32 gegen GRF Editor gleich); DES schreiben (nur im Format 0x102/0x103); **GRF-Editor-Verschlüsselung ✅ (2026-11-19):** `--password`/`--key-file` öffnen verschlüsselte Archive (auch mit verschlüsselter Tabelle), `--encrypt` bei `build`/`repack`/`merge`/`patch`, `key create` (`.grfkey`), `client encrypt` (gepatchte EXE und DLL aus GRF Editors `cps.dll`-Vorlage), GUI fragt das Passwort ab und speichert wieder verschlüsselt, MCP über `RGRF_PASSWORD`; gegen GrfCL und `Encryption.dll` byteweise geprüft und mit einem verschlüsselten Client gestartet ([02-format-notes.md](02-format-notes.md)); **Gravity-Schutz (0x80) lesen ✅ (2026-11-19, kRO-Variante, aus GRF Editor portiert, alle 40 Testbeispiele GRF Editors wie dort; **jRO-Variante ✅ (2026-11-20):** 24 Einträge, mit GRF Editors eigenem Decoder erzeugt und geprüft, lesen gleich; echter jRO-Schlüsselblock ungeprüft. China-Variante ✅ lesen (2026-11-21, aus GRF Editors unfertigem Code, an echten Daten ungeprüft, siehe GSF-Absatz))**; beim Schreiben bleibt der Schutz in 0x300 erhalten, sonst wird entschlüsselt gespeichert |
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
| File › New › Gsf („Gravity Secure Patch“) | – | ➖ GRF Editor hat dafür nur einen Namen in einer Aufzählung, keinen Leser oder Schreiber; nichts zu übernehmen |
| Open…, Open recent, Close Grf | Öffnen, Zuletzt geöffnet (10, für alle Fenster), Archiv schließen | ✅ |
| (ein Fenster je Programm) | **mehrere Fenster** in einer App, gleiches Archiv holt sein Fenster nach vorn | ✅ besser (in der Windows-VM gemessen) |
| – | **Tabs (2026-11-21):** weitere Archive öffnen als Tab im selben Fenster (Einstellung „Archive in Tabs öffnen“, Standard an); Tab anklicken, mit Strg/Cmd+Tab oder Strg/Cmd+1…9 wechseln, mit × oder Mittelklick schließen, durch Ziehen umordnen, nach unten herausziehen = eigenes Fenster, auf die Oberseite eines anderen Fensters ziehen = in dessen Tabs; Rechtsklick: in neues Fenster, schließen, alle Fenster zu Tabs | ✅ auf allen Systemen gleich (Fenster einer Gruppe zeigen sich abwechselnd an derselben Stelle); in der Windows-VM mit dem echten Fenster geprüft (Wechsel, Herausziehen, Zusammenführen, Schließen, Ziehen per Zeiger); **auf dem Mac nicht geprüft** (Bildschirm war gesperrt); Linux: Tab-Verwaltung als Test grün, Fenster nicht gestartet |
| Save, Save as… | Speichern (an Ort und Stelle: daneben schreiben, prüfen, tauschen), Speichern unter (Container nach Endung) | ✅ |
| Save advanced › Defragment (quick), Compact (redirect indexes) | Erweitert speichern › Verdichten: schreibt das Archiv lückenlos neu, Daten werden kopiert | ✅ anderer Weg (immer vollständig neu geschrieben, geprüft) |
| Save advanced › Repack (recompress) | Erweitert speichern › Neu packen (zlib, zopfli, LZMA; Version wählbar) | ✅ |
| Save advanced › Encrypt File Table | Speichern-Dialog: „Nur die Dateitabelle verschlüsseln“ oder „Dateitabelle ebenfalls verschlüsseln“; CLI `rgrf crypt --encrypt-table/--decrypt-table`; MCP `grf_crypt`; Bibliothek `RepackOptions::encrypt_table`, `Archive::table_encrypted` | ✅ (2026-11-21): Zähler = Tabellengröße + 8 wie beim Lesen der Tabelle (Lesen war schon gegen GRF Editors Verfahren gebaut); Tests im Kern, CLI, MCP und GUI (falscher Schlüssel wird als solcher gemeldet). **Nicht gegen GRF Editor/GrfCL in der VM geprüft**; nur GRF 0x200/0x300 |
| About…, Quit | Hilfe › Über (Credits), Beenden | ✅ |
| Undo, Redo | Rückgängig, Wiederholen (200 Schritte, benannt), Verlauf per Rechtsklick und Bearbeiten › Änderungsverlauf, mehrere Schritte auf einmal; Cmd/Ctrl+Z, Shift+Cmd/Ctrl+Z, Ctrl+Y | ✅ (Verlauf 2026-11-20) |
| Navigate backward/forward (Auswahlverlauf) | Ansicht › Zurück/Vor, Alt+←/→, Cmd+[ / ], Maustasten 4/5 | ✅ (2026-11-20) |
| Merge, Subtract | Zusammenführen; Werkzeuge › Mit Archiv vergleichen (Unterschiede, als Thor- oder GRF-Patch speichern) | ✅ Subtrahieren über Vergleich |
| Extract Rgz | RGZ öffnen, Extrahieren | ✅ |
| Edit Grf/Thor settings (Format, Anzeigekodierung, Magic header) | Bearbeiten › Archiv-Einstellungen (Version, Kompression, Verschlüsselung; Namensanzeige in der Statusleiste) | ✅; Magic header ✅ (2026-11-20, auch CLI `--magic`); Thor-Ziel-GRF ✅ (2026-11-21: „In eine GRF einfügen“ und Ziel-GRF im Speichern-Dialog, wird beim Öffnen gelesen und beim Speichern beibehalten) |
| Tools › Grf validation | Werkzeuge › Archiv prüfen | ✅ |
| Open roaming data | App-Datenordner öffnen | ✅ |
| Import sprite…, Sprite editor | Sprite-Vorschau: Bilder hinzufügen, ersetzen, entfernen, Pixel-Editor | ✅ |
| Flat maps maker (Boden flach, Lightmap/Schatten/Farbe, Wasser, Objekte entfernen, Licht zurücksetzen) | `rgrf map flat`, MCP `grf_flat_maps`, GUI: 3D-Vorschau „Karte ändern…“ (vorgemerkt, rückgängig), Werkzeuge › Flache Karten (Patch-Archiv), „Karte extrahieren“ mit Änderungen | ✅ (2026-11-20) alle 1.029 kRO-Karten in 6,9 s; mehr als GRF Editor: Versionen bleiben, Wasser auch in GND ≥ 1.8, nur Modelle/Geräusche/Effekte entfernbar, keine Lightmap-Überläufe. Nicht übernommen: eigene Zelltexturen aus GAT-Typen, Quadranten-Schatten, Rinnen-Linien. Im Client nicht geprüft |
| Patch maker | Mit Archiv vergleichen › Als Patch speichern (Thor mit Löschanweisungen) | ✅ |
| Hash viewer | Werkzeuge › Prüfsummen (CRC32, MD5, SHA-256) | ✅ |
| Text encoding (Text zwischen Kodierungen umrechnen) | Werkzeuge › Textkodierung (z. B. als Latin-1 gelesenes Koreanisch zurück nach CP949); Namensanzeige Koreanisch/Latin-1/UTF-8 | ✅ (2026-11-20) |
| Advanced search (Text in Dateien, Regex, ganze Wörter, Groß/klein, Dateifilter) | Werkzeuge › In Dateien suchen (Cmd/Ctrl+Shift+F) | ✅ (kRO: 133 Textdateien in 0,56 s) |
| Image converter | „8-bit…“, Bilder verkleinern, CLI `image convert` | ✅ |
| GRF Command Line | `rgrf` (CLI) und MCP-Server | ✅ |
| OpenGL Logger | – | nicht nötig (WebGL im Webview) |
| Image finder | – | im Quelltext auskommentiert, auch bei GRF Editor ohne Funktion |
| GRF Encryption, Gravity Encryption, Set kRO/jRO/cnRO keys | Passwort-Verschlüsselung; Gravitys Schutz mit eingebauten kRO- und jRO-Schlüsseln | ✅ kRO, ✅ jRO (gegen GRF Editors Decoder geprüft, echter Schlüsselblock ungeprüft); cnRO ✅ lesen, an echten Daten **ungeprüft** (siehe unten; GRF Editor selbst kann es nicht) |
| Grf patcher | Zusammenführen (Patch über das Archiv legen) | ✅ |
| Settings | Einstellungen | ✅ |

### Kontextmenü der Eintragsliste (2026-11-21)

Rechtsklick, Menü-Taste oder Umschalt+F10 auf Dateien und Ordnern in Baum, Details, Ordneransicht (linker Teil) und Galerie. Pfeiltasten, Pos1/Ende, Esc (Fokus kehrt zurück); schließt bei Klick daneben, Größenänderung, Fensterwechsel.

| GRF Editor | RustyGRF | Stand |
| --- | --- | --- |
| Open file | Öffnen | ✅ |
| Select in explorer | Im Finder/Explorer zeigen (entpackt in temporären Ordner) | ✅ |
| Extract / Extract… | Extrahieren…, In letzten Ordner extrahieren | ✅ |
| Export selection… | In Archiv exportieren… (Auswahl oder Ordner als eigenes Archiv, Methode wählbar, geprüft; liest die gespeicherte Datei, nicht die vorgemerkten Änderungen) | ✅ |
| Export map files… | Karte extrahieren… (rsw/gnd/gat mit allem, was sie brauchen) | ✅ |
| Save map as… | Karte ändern… (flach machen) und Karte extrahieren | ◐ anderer Weg |
| Downgrade / Downgrade to… (Kartenversion) | Karte herabstufen… (Kontextmenü auf .rsw/.gnd/.gat), CLI `rgrf map downgrade`, MCP `grf_downgrade_maps`: RSM2-Modelle (2.2/2.3) → RSM 1.4 neben den alten, Weltdatei 2.1 (Wasser aus der Bodendatei), Bodendatei 1.7, Objekte mit neuer Position und negativem x-Maßstab | ✅ **Drehanimationen bleiben (2026-11-21):** Modelle mit Drehschlüsseln (`rsm_model::rsm1_nodes`) behalten sie in Millisekunden, feste Drehungen und Maßstäbe wandern in Eckpunkte und Kindknoten (anders als GRF Editors „anim“-Weg, der Positionen nur schätzt, ist es hier rechnerisch genau: gleiche Eckpunktlage zu jeder Zeit, Test mit Kette aus drehendem, festem und drehendem Knoten; 33 echte Modelle der kRO-`data.grf` mit größter Abweichung 0,027). **Verloren gehen weiter:** Positions-, Maßstabs- und Texturanimationen (RSM 1.4 hat keinen Platz dafür; Schlüssel 0 bleibt stehen; 49 der 82 drehenden Modelle haben solche mit). Gemessen: 151 Karten der kRO-`data.grf` mit 1.953 Modellen und 88.381 Objekten in 3,5 s, alle Ergebnisse lesen sich bis zum letzten Byte; die Umrechnung der Objekte ist rechnerisch gegen GRF Editors Zeichenmatrizen geprüft, **nicht in einem alten Client gesehen** |
| Delete | Entfernen (vormerken, rückgängig) | ✅ |
| Rename… | Umbenennen… (Datei und Ordner, ein Rückgängig-Schritt; prüft Namenskonflikte, Grenze 512 MB je Eintrag) | ✅ |
| Find usages… | Name in Dateien suchen… (Text und **kompilierte Lua-Dateien als dekompilierter Text**; kRO-`data.grf`: 517 `.lub` in 0,5 s) | ✅ (2026-11-21) |
| Copy NPC name | Name kopieren, Pfad kopieren | ◐ |
| Encryption ▸ Encrypt / Decrypt (je Eintrag) | Verschlüsseln… / Entschlüsseln im Kontextmenü (Dateien und Ordner, GRF 0x200): wird beim Speichern angewandt, rückgängig machbar, Passwort nur bei Archiven ohne Schlüssel; gemischte Archive (nur ein Teil verschlüsselt) werden gelesen und geschrieben | ✅ (2026-11-21; gleiche Chiffre wie die Ganz-Archiv-Verschlüsselung, die gegen GrfCL geprüft ist; gemischte Archive nicht gegen GRF Editor selbst geprüft; beim Speichern mit Auswahl werden Datenblöcke nicht geteilt, die Datei kann größer werden) |
| Properties | Eigenschaften… (Größen, Kodierung, Versatz, Flags, CRC32, MD5, SHA-256) | ✅ |
| – | Hex-Ansicht, Text bearbeiten, 8 Bit, Ersetzen, Datei hier hinzufügen | ✅ zusätzlich |

**Behoben (2026-11-21):** Bei schmalem Fenster (unter 900 px) lag die Liste nur auf halber Höhe neben einem 6 px breiten Vorschaustreifen, weil die Regel für die Ordneransicht mit Vorschau rechts die allgemeine Schmalfenster-Regel überstimmte. Jetzt bleibt die Vorschau bis 721 px rechts (schmalere Spalte), darunter liegt sie unter der Liste (60 %); im Browser (alle vier Ansichten) und im echten Windows-Fenster der VM (angedockt 960×540: ganze Liste sichtbar) gemessen.

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

### Klang bearbeiten (neu, 2026-11-21; GRF Editor kann Klänge nur abspielen)

| Funktion | RustyGRF | Stand |
| --- | --- | --- |
| Audio startet beim Auswählen (WAV/MP3/OGG) | Einstellung „Töne automatisch abspielen“ (Standard an); der vorige Klang wird gestoppt | ✅ im Browser (Chromium) und in WebView2 (Windows-VM, echter Klick: spielt bis zum Ende) geprüft; WKWebView (Mac) und Linux nicht |
| WAV bearbeiten: Zuschneiden, Ausschneiden, Stille, Lautstärke (dB), Normalisieren, Ein-/Ausblenden, Umkehren, Mono, Umrechnen der Abtastrate, Tempo, Echo, Hall, Tief-/Hochpass | GUI „Klang bearbeiten…“ (Wellenform, Auswahl ziehen, Schrittliste, Vorhören) im Kontextmenü und in der Vorschau; CLI `rgrf audio info|edit`; Bibliothek `rgrf_formats::audio` | ✅ (Kern 6 Tests, alle 3.332 WAV der kRO-`data.grf` lassen sich lesen und bearbeiten, GUI-Test, CLI-Test) |
| Ergebnis | 16-Bit-PCM-WAV (jeder Client spielt es); als vorgemerkte Änderung (rückgängig, Entwurf) | ✅ |
| MP3/OGG bearbeiten | Dieselben Schritte wie bei WAV; das Ergebnis bleibt MP3 bzw. Ogg (ähnliche Bitrate, bei jedem Speichern geht etwas Qualität verloren), `--to wav|mp3|ogg` wandelt um; Decoder Symphonia (rein Rust), Encoder LAME (MP3) und libvorbis mit aoTuV (Ogg), Merkmal `lossy-audio` (CLI, MCP und GUI haben es an) | ✅ (2026-11-21): alle 232 MP3 der Client-Musik (`BGM`) lesen sich, 20 davon gekürzt und als MP3 zurückgeschrieben (Länge und Bitrate geprüft), Tests in Formaten, CLI, MCP und GUI. Windows ARM64 (VM: C-Code von LAME und libvorbis mit MSVC gebaut, alle Tests grün) und Linux ARM64 (Docker: alle Tests mit den echten Daten grün) sind geprüft. **Nicht geprüft:** im Spiel, das GUI-Fenster mit MP3 unter Windows/Linux |
| MCP-Werkzeug für Klänge | `grf_audio_info`, `grf_audio_edit` (Datei oder Eintrag eines Archivs, Schritte als Text, schreibt eine neue Datei) | ✅ (Test) |

**GSF („Gravity Secure Patch“, 2026-11-21):** GRF Editor liest `.gsf` mit demselben Leser wie `.grf`/`.gpf` (`GrfContainerProvider`, `GrfHolder.Merge`: ein Fall `.gsf` neben `.grf` und `.gpf`; ein eigenes Format gibt es dort nicht). RustyGRF öffnet Archive nach ihrer Kennung, nicht nach der Endung, und speichert `.gsf` jetzt als GRF; „Öffnen“, „Zusammenführen“, „Vergleichen“, „Speichern unter“ und die Dateizuordnung kennen die Endung. Nicht geprüft: eine echte `.gsf` von Gravity (keine Beispieldatei vorhanden). **cnRO (2026-11-21):** `ProtectedCnDecoder.cs` aus GRF Editor ist **gelesen und nach `gravity.rs` portiert** (Kopfversion 2 mit 64-Byte-Signatur, Körperverschlüsselung 9 mit RC6 und IDEA-Maske, die beiden Schlüsselblöcke, die GRF Editor an ihrem SHA-256 erkennt, mit den dort eingebauten Schlüsseln). GRF Editor selbst kann diesen Weg nie ausführen (sein Kopfleser lehnt Version 2 ab, seine 64-Byte-Schlüssel scheitern an der 60-Byte-Prüfung); hier werden die ersten 60 Byte genommen, die Ed25519-Signatur wird nicht geprüft, aber der Hash des Inhalts: ein falsches Lesen ist ein Fehler, nie falsche Daten. **Ungeprüft an echten Daten** (es gibt kein cnRO-Archiv; getestet ist nur ein Hin-und-zurück mit einem Kodierer, der aus demselben Code geschrieben wurde). Die Schlüssel stehen nicht im Archiv, sondern nur für diese zwei bekannten Schlüsselblöcke in GRF Editor; ein anderer Block bleibt „unbekannt“. Schreiben: geschützte Einträge werden unverändert kopiert wie bei den anderen Varianten.

## Was RustyGRF anders machen soll
- Alles in **einer** App und **einer** Bibliothek (GRF Editor, ACT Editor und Palette Editor sind getrennte Programme).
- Dieselbe Funktion in CLI und MCP, damit ein Agent Sprites prüfen und Archive bauen kann.
- Läuft auf macOS, Linux und Windows (GRF Editor/ACT Editor nur Windows).

## Offene Fragen an dich
1. **Skripte im ACT Editor:** Wollen wir eine Skriptsprache (z. B. Lua) für Massenbearbeitung, oder reichen feste Funktionen (Palette tauschen, Offset verschieben, kopieren)? Empfehlung: erst feste Funktionen.
2. **Audio/Video/Hex-Ansicht:** brauchbar oder weglassen? Empfehlung: weglassen, bis Bedarf da ist.

## Vergleich mit dem Quelltext, 2. Durchgang (2026-11-21)
Gelesen: `EMenuInteraction.cs`, `ETreeViewInteraction.cs`, `EListViewInteraction.cs`, `SettingsDialog.xaml`, `GrfCL/CommandLineOptions.cs`, `WPF/PreviewTabs/*`, ACT Editors `changelog.txt` und `Core/Scripting/Scripts/*`. Ein erster, automatischer Durchlauf (Hilfsprogramm) nannte vieles als fehlend, das längst da ist (Herabstufen, Einträge verschlüsseln, „Verwendung suchen“, flache Karten); die Liste unten ist von Hand geprüft.

**Neu gebaut in diesem Durchgang:**

| Funktion in GRF/ACT Editor | RustyGRF | Stand |
| --- | --- | --- |
| „RSM2 to RSM1 (anim/flat)“ auf einzelnen Modellen | Kontextmenü „RSM2 → RSM1…“ auf `.rsm2` (eine Auswahl oder ein Ordner), schreibt `.rsm` daneben, ein Rückgängig-Schritt; Drehanimationen bleiben, sonst wie bei „flat“ | ✅ (Test auf echten Modellen) |
| ACT: „Mirror vertical/horizontal“ (Frame, Aktion) | Schaltflächen „Frame spiegeln ↔“, „Frame kippen ↕“, „Aktion spiegeln ↔“ (`Act::flip`, Regeln von `FlipCommand`, Halbpixel-Ausgleich bei ungerader Bildbreite aus der Sprite-Datei daneben) | ✅ (Kern- und GUI-Tests) |
| ACT: „Mirror action from left/right to right/left“ | „Auf die Gegenseite“ (1↔7, 2↔6, 3↔5 je 8, Anker mitgespiegelt) | ✅ |
| ACT: „Reverse animation“ | „Umkehren“ | ✅ |
| ACT: Anker „Set from file…“ und „Adjust from file…“ (z. B. Kopfbedeckung männlich ↔ weiblich) | „Anker aus Datei…“: kopieren, oder angleichen (erster Anker wie in der Datei, alle Ebenen um den Unterschied verschoben) | ✅ (Kern- und GUI-Test); die eingebauten Vorlagen `ref_head_m/f.act` fehlen, man wählt eine Datei |

**Nachgezogen (2026-11-21, 3. Durchgang; alles gebaut und geprüft):**

| Funktion in GRF/ACT Editor | RustyGRF | Stand |
| --- | --- | --- |
| ACT: „Duplicate frames…“, „Add sprite to all frames…“ (Animationen wie `0-2;5`, Bild, Versatz, Skalierung, Farbe, vor/hinter den Ebenen), „Edit frames…“ / „Edit actions…“ (Liste: umordnen, kopieren, entfernen, leere hinzufügen) | Menü „Skripte ▾“ im ACT-Editor; `Act::duplicate_frames`, `add_layer_to_frames`, `set_frame_order`, `set_action_order` (Kern, `act_ops.rs`) | ✅ Kern- und GUI-Tests; im Browser an einer echten ACT (Poring) durchgeklickt, Fehlermeldungen übersetzt |
| ACT: Richtungen kopieren („Setup“-Skripte: unten → unten rechts, unten rechts → unten, unten links → unten, unten → unten links) | vier Einträge im Menü „Skripte“, je in der Gruppe von 8 Aktionen der gewählten Aktion | ✅ Test mit den Abbildungsregeln aus `AnimationMenu.cs`. **Nicht gebaut:** der interaktive Dialog „Setup Headgear/Head“ (Ziehen und Platzieren, 859 Zeilen WPF); „Setup Garment“ ist in ACT Editor selbst abgeschaltet |
| ACT: Effekte Ein-/Ausblenden (Zielfarbe), Atmen, Farbpuls (multiplizierend) mit Beschleunigung, Frame-Bereich, fehlende Frames ergänzen | Menü „Skripte“ (`Act::apply_effect`, Kurve wie `GetEaseMethod`) | ✅ Tests; **nicht gebaut:** Umriss („Bleeding outline“), „Cut sprite“ und die addierende Farbüberlagerung (sie erzeugen Bilder im Sprite) |
| ACT: „New ACT“ aus Vorlagen (leer, NPC, Monster, Homunkulus, Waffe, Kopfbedeckung männlich/weiblich) | Bearbeiten → „Neue Animation aus Vorlage…“ legt eine `.act` im Archiv an; Form (Aktionen, Frames, Tempo, Monster-Sound) wie die `Resources/*.act` von ACT Editor (gemessen), Kopf-Anker eingebettet | ✅ Kern- und GUI-Test, Dialog im Browser |
| ACT: Standard-Anker männlicher/weiblicher Kopf (aus `ref_head_m/f.act`) | zwei Einträge im Menü „Skripte“ | ✅ |
| Sprite-Liste: Einfügen/Ersetzen für einen Bereich von Bildern | „Ersetzen…“ nimmt mehrere PNG und ersetzt ab diesem Bild die folgenden (hängt an, was überschreitet); neu „Davor einfügen…“ und „Danach einfügen…“ (mehrere PNG, die Bilder dahinter rücken, **die ACT daneben folgt**) | ✅ Kern- und GUI-Test, im Browser |
| ACT: Hintergrundfarbe der Vorschau | Einstellung „Vorschau-Hintergrund“ (Schachbrett, Schwarz, Weiß, Grau, Grün für Greenscreen, eigene Farbe); gilt für alle Vorschauen und den ACT-Editor | ✅ im Browser |
| Thor: „Add files to remove…“ (Löschanweisungen von Hand) | Bearbeiten → „Löschanweisungen hinzufügen (Thor)…“, ein Pfad je Zeile; als vorgemerkte Änderung (rückgängig, Entwurf), beim Speichern als `.thor` geschrieben. **Dabei behoben:** Speichern eines Thor aus dem GUI hat vorhandene Löschanweisungen verworfen, jetzt bleiben sie | ✅ Test; als `.grf` gespeichert gibt es eine klare Meldung (GPF kennt keine Löschanweisungen, in GRF Editor ist es auch nur für Thor) |
| Einstellungen: .lub beim Extrahieren dekompilieren, Ordner nach dem Extrahieren öffnen, Extraktionspfad festlegen, Hintergrund der Vorschau | Einstellungen → „Extrahieren und Vorschau“; die dekompilierte `.lub` behält den Namen und wird Lua-Text im koreanischen Zeichensatz (nicht vollständig lesbare bleiben wie sie sind) | ✅ Test (Kern `extract_with`, GUI); nicht gebaut: „Baumzustand merken“, „Thor-Hash anhängen“ im GUI (CLI `thor create --integrity` gibt es) |
| Baum: „New folder…“, „Extract root files only“ | Kontextmenü eines Ordners: „Neuer Ordner…“ (ein GRF hat keine leeren Ordner, also Pfad und Dateien wählen) und „Nur Dateien dieses Ordners extrahieren…“; Bearbeiten-Menü „Neuer Ordner…“ | ✅ Test, im Browser |
| GrfCL `hash` über einen Ordner, `hashComp` | `rgrf hashlist create|show|compare` (Dateien eines Ordners und Einträge der Archive darin, Quick-MD5, CRC32, MD5 oder Größe; Vergleich wie `HashObjectComparer`) | ✅ **gegen GrfCL in der Windows-VM geprüft:** GrfCL liest unsere Liste (`hashComp`: genau der eine geänderte Eintrag), wir lesen die von GrfCL, Pfade und Hashes gleich |
| GrfCL `thorPack`, `thorUnpack` | `rgrf thor pack|unpack` (Anordnung nach `ThorPacker.cs`) | ✅ gegen GrfCL geprüft: gleiche Größe und Anordnung, unser Auspacker liest die Datei von GrfCL (die zlib-Bytes selbst weichen ab). `thorUnpack` hat in GrfCL gar keine Verarbeitung |
| GrfCL `addFakeClientinfo` | `rgrf client definitions` (Text mit Blöcken `archiv.grf?`, Muster je Zeile, `#end` → komprimierte `__definitions` mit Quick-CRC32) | ◐ aus dem Quelltext portiert (Test der Bausteine); der Befehl in GrfCL heißt „Redirects the client info“, tut aber nur das und ist ohne Argumente nicht aufrufbar; wofür der Server die Datei braucht, ist unbekannt, **nicht gegen GrfCL geprüft** |
| Dateiformate: BSON/JSON, ICO, IMF, EBM, FNA, ThumbDB | Vorschau im GUI (BSON als JSON-Text, Symbole mit allen Größen, Emblem, IMF/FNA als Text, Thumbs.db mit den JPEG-Vorschaubildern) und `rgrf rare bson-to-json|json-to-bson|ebm-to-bmp|bmp-to-ebm|ico|thumbs|imf|fna`; BSON, EBM, FNA lassen sich zurückschreiben | ✅ an echten Dateien: alle 3 BSON der kRO (durch JSON zurück: **gleiche Bytes**), 304 IMF und 23 FNA der `data.grf` (FNA: gleiche Bytes), 143 Thumbs.db (2.074 Vorschaubilder, 1.888 mit JPEG; 18 haben nicht den Katalog der älteren Windows-Versionen und werden als solche gemeldet), Symbole; EBM nur mit einer selbst gebauten Datei (kein echtes Emblem vorhanden). Thumbs.db wird ohne Windows-Schnittstellen gelesen (eigener Leser für Compound-Dateien) |

**Geprüft auf allen drei Systemen (2026-11-21):** macOS (alle Tests mit den echten Daten), Linux ARM64 (Ubuntu 26.04 in Parallels: `cargo test --release --workspace` mit den echten Daten, alle grün; das GUI-Fenster aus den Quellen gebaut, ACT-Editor mit „Skripte ▾“ geöffnet, „Frames wiederholen“ angewendet: 4 → 8 Frames, Titel mit Stern, eine vorgemerkte Änderung; das Menü liegt im Dialog und ist unter WebKitGTK anklickbar) und Windows ARM64 (Tests ohne die echten Daten, grün). **Nicht geprüft:** das Windows-Fenster mit den neuen Dialogen, alle neuen Einstellungen im echten Fenster (im Browser geprüft), Ton.

**Weiterhin offen:** ACT: Umriss, „Cut sprite“, addierende Farbüberlagerung, der Dialog „Setup Headgear/Head“, Zoomen und Verschieben während der Wiedergabe; „Baumzustand merken“; `Effects2Str`; MCP-Werkzeuge für Hashlisten, Thor-Packen und die seltenen Formate (die CLI hat sie).

**Behoben (2026-11-21):** Bei schmalem Fenster (unter 900 px) lag die Liste nur auf halber Höhe neben einem 6 px breiten Vorschaustreifen, weil die Regel für die Ordneransicht mit Vorschau rechts die allgemeine Schmalfenster-Regel überstimmte. Jetzt bleibt die Vorschau bis 721 px rechts (schmalere Spalte), darunter liegt sie unter der Liste (60 %); im Browser (alle vier Ansichten) und im echten Windows-Fenster der VM (angedockt 960×540: ganze Liste sichtbar) gemessen.

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

### Klang bearbeiten (neu, 2026-11-21; GRF Editor kann Klänge nur abspielen)

| Funktion | RustyGRF | Stand |
| --- | --- | --- |
| Audio startet beim Auswählen (WAV/MP3/OGG) | Einstellung „Töne automatisch abspielen“ (Standard an); der vorige Klang wird gestoppt | ✅ im Browser (Chromium) und in WebView2 (Windows-VM, echter Klick: spielt bis zum Ende) geprüft; WKWebView (Mac) und Linux nicht |
| WAV bearbeiten: Zuschneiden, Ausschneiden, Stille, Lautstärke (dB), Normalisieren, Ein-/Ausblenden, Umkehren, Mono, Umrechnen der Abtastrate, Tempo, Echo, Hall, Tief-/Hochpass | GUI „Klang bearbeiten…“ (Wellenform, Auswahl ziehen, Schrittliste, Vorhören) im Kontextmenü und in der Vorschau; CLI `rgrf audio info|edit`; Bibliothek `rgrf_formats::audio` | ✅ (Kern 6 Tests, alle 3.332 WAV der kRO-`data.grf` lassen sich lesen und bearbeiten, GUI-Test, CLI-Test) |
| Ergebnis | 16-Bit-PCM-WAV (jeder Client spielt es); als vorgemerkte Änderung (rückgängig, Entwurf) | ✅ |
| MP3/OGG bearbeiten | Dieselben Schritte wie bei WAV; das Ergebnis bleibt MP3 bzw. Ogg (ähnliche Bitrate, bei jedem Speichern geht etwas Qualität verloren), `--to wav|mp3|ogg` wandelt um; Decoder Symphonia (rein Rust), Encoder LAME (MP3) und libvorbis mit aoTuV (Ogg), Merkmal `lossy-audio` (CLI, MCP und GUI haben es an) | ✅ (2026-11-21): alle 232 MP3 der Client-Musik (`BGM`) lesen sich, 20 davon gekürzt und als MP3 zurückgeschrieben (Länge und Bitrate geprüft), Tests in Formaten, CLI, MCP und GUI. Windows ARM64 (VM: C-Code von LAME und libvorbis mit MSVC gebaut, alle Tests grün) und Linux ARM64 (Docker: alle Tests mit den echten Daten grün) sind geprüft. **Nicht geprüft:** im Spiel, das GUI-Fenster mit MP3 unter Windows/Linux |
| MCP-Werkzeug für Klänge | `grf_audio_info`, `grf_audio_edit` (Datei oder Eintrag eines Archivs, Schritte als Text, schreibt eine neue Datei) | ✅ (Test) |

**GSF („Gravity Secure Patch“, 2026-11-21):** GRF Editor liest `.gsf` mit demselben Leser wie `.grf`/`.gpf` (`GrfContainerProvider`, `GrfHolder.Merge`: ein Fall `.gsf` neben `.grf` und `.gpf`; ein eigenes Format gibt es dort nicht). RustyGRF öffnet Archive nach ihrer Kennung, nicht nach der Endung, und speichert `.gsf` jetzt als GRF; „Öffnen“, „Zusammenführen“, „Vergleichen“, „Speichern unter“ und die Dateizuordnung kennen die Endung. Nicht geprüft: eine echte `.gsf` von Gravity (keine Beispieldatei vorhanden). **cnRO (2026-11-21):** `ProtectedCnDecoder.cs` aus GRF Editor ist **gelesen und nach `gravity.rs` portiert** (Kopfversion 2 mit 64-Byte-Signatur, Körperverschlüsselung 9 mit RC6 und IDEA-Maske, die beiden Schlüsselblöcke, die GRF Editor an ihrem SHA-256 erkennt, mit den dort eingebauten Schlüsseln). GRF Editor selbst kann diesen Weg nie ausführen (sein Kopfleser lehnt Version 2 ab, seine 64-Byte-Schlüssel scheitern an der 60-Byte-Prüfung); hier werden die ersten 60 Byte genommen, die Ed25519-Signatur wird nicht geprüft, aber der Hash des Inhalts: ein falsches Lesen ist ein Fehler, nie falsche Daten. **Ungeprüft an echten Daten** (es gibt kein cnRO-Archiv; getestet ist nur ein Hin-und-zurück mit einem Kodierer, der aus demselben Code geschrieben wurde). Die Schlüssel stehen nicht im Archiv, sondern nur für diese zwei bekannten Schlüsselblöcke in GRF Editor; ein anderer Block bleibt „unbekannt“. Schreiben: geschützte Einträge werden unverändert kopiert wie bei den anderen Varianten.

## Was RustyGRF anders machen soll
- Alles in **einer** App und **einer** Bibliothek (GRF Editor, ACT Editor und Palette Editor sind getrennte Programme).
- Dieselbe Funktion in CLI und MCP, damit ein Agent Sprites prüfen und Archive bauen kann.
- Läuft auf macOS, Linux und Windows (GRF Editor/ACT Editor nur Windows).

## Offene Fragen an dich
1. **Skripte im ACT Editor:** Wollen wir eine Skriptsprache (z. B. Lua) für Massenbearbeitung, oder reichen feste Funktionen (Palette tauschen, Offset verschieben, kopieren)? Empfehlung: erst feste Funktionen.
2. **Audio/Video/Hex-Ansicht:** brauchbar oder weglassen? Empfehlung: weglassen, bis Bedarf da ist.

## Vergleich mit dem Quelltext, 2. Durchgang (2026-11-21)
Gelesen: `EMenuInteraction.cs`, `ETreeViewInteraction.cs`, `EListViewInteraction.cs`, `SettingsDialog.xaml`, `GrfCL/CommandLineOptions.cs`, `WPF/PreviewTabs/*`, ACT Editors `changelog.txt` und `Core/Scripting/Scripts/*`. Ein erster, automatischer Durchlauf (Hilfsprogramm) nannte vieles als fehlend, das längst da ist (Herabstufen, Einträge verschlüsseln, „Verwendung suchen“, flache Karten); die Liste unten ist von Hand geprüft.

**Neu gebaut in diesem Durchgang:**

| Funktion in GRF/ACT Editor | RustyGRF | Stand |
| --- | --- | --- |
| „RSM2 to RSM1 (anim/flat)“ auf einzelnen Modellen | Kontextmenü „RSM2 → RSM1…“ auf `.rsm2` (eine Auswahl oder ein Ordner), schreibt `.rsm` daneben, ein Rückgängig-Schritt; Drehanimationen bleiben, sonst wie bei „flat“ | ✅ (Test auf echten Modellen) |
| ACT: „Mirror vertical/horizontal“ (Frame, Aktion) | Schaltflächen „Frame spiegeln ↔“, „Frame kippen ↕“, „Aktion spiegeln ↔“ (`Act::flip`, Regeln von `FlipCommand`, Halbpixel-Ausgleich bei ungerader Bildbreite aus der Sprite-Datei daneben) | ✅ (Kern- und GUI-Tests) |
| ACT: „Mirror action from left/right to right/left“ | „Auf die Gegenseite“ (1↔7, 2↔6, 3↔5 je 8, Anker mitgespiegelt) | ✅ |
| ACT: „Reverse animation“ | „Umkehren“ | ✅ |
| ACT: Anker „Set from file…“ und „Adjust from file…“ (z. B. Kopfbedeckung männlich ↔ weiblich) | „Anker aus Datei…“: kopieren, oder angleichen (erster Anker wie in der Datei, alle Ebenen um den Unterschied verschoben) | ✅ (Kern- und GUI-Test); die eingebauten Vorlagen `ref_head_m/f.act` fehlen, man wählt eine Datei |

**Noch offen (nach Nutzen):**

| Funktion in GRF/ACT Editor | Stand bei uns |
| --- | --- |
| ACT: „Setup Headgear/Head/Garment“ (Hilfen, die eine Animation für Kopfbedeckung, Kopf oder Umhang aus einer Vorlage aufbauen), „Duplicate frames…“, „Add sprite to all frames…“, „Edit frames/actions…“ (Dialoge mit Mehrfachauswahl), Effekte (Ein-/Ausblenden, Atmen, Umriss, Farbüberlagerung, „Cut sprite“) | ❌ |
| ACT: „New ACT“ aus Vorlagen; Sprite-Liste: Einfügen/Ersetzen für einen Bereich von Bildern; Hintergrundfarbe der Vorschau; Zoomen und Verschieben während der Wiedergabe | ❌ |
| Thor/GPF: „Add files to remove…“ / „Set as removed“ (Löschanweisungen von Hand hinzufügen) | ◐ `thor create --remove` und Vergleich→Thor schreiben sie; im GUI nicht von Hand |
| Baum: „New folder…“, „Extract root files only“ | ❌ (gering) |
| Einstellungen: .lub beim Extrahieren dekompilieren, Ordner nach dem Extrahieren öffnen, Hintergrund der Vorschau, Extraktionspfad überschreiben, Baumzustand merken, Thor-Hash anhängen | ❌ (`thor create --integrity` gibt es in der CLI) |
| GrfCL: `thorPack`/`thorUnpack` (Konfiguration in ein Thor-Patcher-Programm packen), `addFakeClientinfo`, `hash` über einen Ordner | ❌ (Patcher-Zubehör; `client` kennt Serveradresse und Hash je Eintrag) |
| Dateiformate: BSON/JSON, ThumbDB, ICO, IMF, EBM, FNA, `Effects2Str` | ❌ (kaum Nutzen für den Client-Bau) |
