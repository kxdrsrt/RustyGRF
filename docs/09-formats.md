# 09 – Dateiformate des Clients (SPR, ...)

Abgeleitet aus GRF Editor (Tokeiburu, mit Erlaubnis, siehe [05-licensing.md](05-licensing.md)): `GRF/FileFormats/SprFormat/*`, `Rle.cs`. Umsetzung: Crate `rgrf-formats`.

## SPR (Sprites) – M7a, umgesetzt

**Aufbau (gelesen aus dem Quelltext, an echten Dateien geprüft):**
- `"SP"`, Minor, Major (also `SP\x01\x02` = Version 2.1), `u16` Anzahl Palettenbilder, ab 2.0 `u16` Anzahl True-Color-Bilder.
- Palettenbild: `u16 Breite, u16 Höhe`, vor 2.1 rohe Pixel, ab 2.1 `u16 Größe` + RLE-Daten (nur Index 0 wird gepackt: `0` + Anzahl 1–255).
- True-Color-Bild: `u16 Breite, u16 Höhe`, dann `Breite × Höhe × 4` Bytes als **A, B, G, R**, Zeilen **von unten nach oben**.
- Palette: die letzten 1024 Bytes der Datei (256 × R, G, B, ungenutzt), Index 0 ist transparent. Dateien mit nur True-Color-Bildern tragen sie oft trotzdem.
- Bildreihenfolge: erst alle Palettenbilder, dann alle True-Color-Bilder (so zählt auch die ACT-Datei).
- Unterstützt: 1.0, 1.1, 2.0, 2.1. Version 2.2/3.2 (Größe als `u32`, zlib-gepackte True-Color-Bilder) kennt GRF Editor, aber in keiner unserer Dateien kommt sie vor; sie wird abgelehnt statt geraten.

**Gemessen (2026-11-18, kRO-`data.grf` 2024, 44.526 SPR-Dateien, `crates/rgrf-formats/tests/real_spr.rs`):**
- Versionen: 2.1 in 44.464 Dateien, 2.0 in 62. Version 1.x kommt in diesem Archiv nicht vor (nur synthetisch getestet).
- **Alle 44.526 Dateien lesen sich fehlerfrei** (658.431 Palettenbilder; 995 Dateien mit True-Color-Bildern).
- **Schreiben und erneut Lesen ergibt in allen Dateien dieselben Pixel und dieselbe Palette.**
- **Byteidentisch nach dem Zurückschreiben: 44.520 von 44.526.** Die 6 anderen: 4 Dateien (`NIGHT_WATCH_*` und `rebellion_*` unter `data\sprite\ÀÎ°£Á·\`) haben 13.660–15.670 ungenutzte Bytes zwischen Bildern und Palette (wir lesen mit Warnung, schreiben ohne); 2 Dateien (unter `data\sprite\¾Ç¼¼»ç¸®\`) bestehen nur aus 54 True-Color-Bildern und haben 822 Bytes Anhang hinter den Bildern (keine Palette); den Anhang schreiben wir nicht zurück.
- Sichtprüfung: Export eines Sprites (Surfboard-Poring) nach PNG, Farben und Transparenz stimmen.
- **Nicht gemessen:** Vergleich der Exporte mit GRF Editor/ActEditor (nur in der Windows-VM möglich).

**PNG:** Palettenbilder werden 8-Bit-indiziertes PNG (Palette in `PLTE`, Index 0 per `tRNS` transparent), True-Color als RGBA. Beim Zurückbauen bleibt jedes Bild unverändert; das ungenutzte vierte Palettenbyte geht dabei verloren (Pixel und Farben bleiben gleich).

**CLI:** `rgrf spr info|export|build`, siehe [04-cli-and-mcp.md](04-cli-and-mcp.md).

## PAL (Paletten) – M7b, umgesetzt
1024 Bytes: 256 × R, G, B, ungenutzt (Index 0 gilt als transparent). Gemessen (2026-11-18, kRO-`data.grf`): **alle 4.338 PAL-Dateien** sind genau 1024 Bytes und kommen byteidentisch zurück.

## ACT (Animationen) – M7b, Lesen/Schreiben/Rendern umgesetzt
**Aufbau (aus `ActConverter.cs`, an echten Dateien geprüft):**
- `"AC"`, Minor, Major, `u16` Anzahl Aktionen, 10 reservierte Bytes. Je Aktion `i32` Frames; je Frame 32 Bytes (zwei RECTs, Angriffsbereich, bleiben unverändert), `i32` Layer, die Layer, ab 2.0 `i32` Sound-Nr., ab 2.3 `i32` Anker zu je 16 Bytes.
- Layer: Versatz x, y (`i32`, ab 2.6 `f32`), Sprite-Nr., Spiegeln; ab 2.0 Farbe (R, G, B, A), Skalierung x (ab 2.4 auch y), Drehung in Grad, Sprite-Typ (0 Palette, 1 True-Color); ab 2.5 Breite und Höhe.
- Nach den Aktionen: ab 2.1 `i32` Sounds zu je 40 Bytes, ab 2.2 je Aktion ein `f32`-Intervall (Einheit 25 ms: 6 = 150 ms). Unbekannte Bytes am Ende bleiben erhalten.
- Unterstützt: 2.0 bis 2.6. Versionen unter 2.0 gibt es in den Dateien nicht; 2.6 (Versatz als Kommazahl) nur synthetisch getestet, kommt in `data.grf` nicht vor.

**Gemessen (2026-11-18, kRO-`data.grf` 2024, `crates/rgrf-formats/tests/real_act.rs`):**
- **71.647 ACT-Dateien**: Versionen 2.5 (68.386), 2.4 (2.643), 2.3 (373), 2.1 (242), 2.0 (3). **Alle lesen sich fehlerfrei und kommen byteidentisch zurück** (71.647 von 71.647).
- 5 Dateien (`NIGHT_WATCH_*`, `rebellion_*`) haben 1.280 ungenutzte Bytes am Ende (bleiben erhalten).
- Gegenprüfung mit dem SPR im selben Pfad: 44.252 ACT haben eins (27.395 nicht im selben Pfad). Von 18.252.639 Layern zeigen **226 auf ein Bild, das im SPR fehlt** (z. B. `frog_kagerou_¿©`: Aktion 100, Index 3 von 3); der Client ignoriert so etwas offenbar, wir zeichnen die Ebene nicht. Bei 380.761 Layern weicht die gespeicherte Breite/Höhe vom Bild ab; GRF Editor überschreibt sie beim Laden ebenfalls aus dem Bild, wir ignorieren sie beim Zeichnen.

**Rendern (`render.rs`):** Kette wie in `LayerDraw.cs` des ACT Editors: Spiegeln, Bild um `-(Breite+1)/2` bzw. `-(Höhe+1)/2` verschieben (ganzzahlig, Richtung 0), Skalieren, Drehen (Grad, im Uhrzeigersinn, Mittelpunkt bei ungeraden Größen um eine halbe Skalierung versetzt), Versatz; Nearest-Neighbour wie die Vorschau des ACT Editors; die Farbe multipliziert die Pixel (`ColorMultiply.hlsl`, aus dem Namen und dem Code gelesen, Shader nicht geprüft). Alle Frames einer Aktion teilen eine Leinwand.
- Sichtprüfung: Poporing, Aktion 0 (4 Frames), Körper und Auge korrekt zusammengesetzt, Squash-Animation sichtbar.
- Stichprobe: jede 40. ACT (1.111 Dateien), **346.419 Frames gerendert, kein Fehler, keine übergroße Leinwand**; 15.768 Aktionen enthalten kein Bild.
- **Noch nicht:** Anker-Bindung an eine andere ACT (Kopfbedeckung am Körper), Überlagerung mehrerer ACT/SPR, GIF-Export, Vergleich pixelgenau mit ACT Editor (nur in der Windows-VM; ob der Ursprung auf einer Pixelgrenze oder -mitte liegt, ist dort zu prüfen).

**CLI:** `rgrf act info|render`, siehe [04-cli-and-mcp.md](04-cli-and-mcp.md).

## Bearbeiten (M7b) – Bibliothek `edit.rs`, CLI `spr replace|add|remove|set-palette`, `act translate|interval|copy-action|remove-action`
- Bild ersetzen/anhängen, Palette tauschen, Bild entfernen **mit Nachführen der ACT** (Layer auf das entfernte Bild werden leer, spätere Indizes rücken nach, Layer der anderen Bildart bleiben), Versatz verschieben (Layer und Anker), Intervall setzen, Aktion kopieren/entfernen.
- PNG nach Palette: `quantize` nimmt je Pixel die nächste Farbe (RGB-Abstand) der Einträge 1 bis 255, Alpha unter 128 wird Index 0. Das ist eine einfache Abbildung ohne Dithering.
- Getestet: Einheitentests, ein CLI-Test, der alle Befehle durchspielt und die Ergebnisse zurücklesen lässt, und eine Sichtprüfung am Poporing (fremde Haar-Palette: die Augen wechseln von Rot zu Schwarz).
- **Nicht geprüft:** ob der Client oder GRF Editor die geänderten Dateien wie erwartet lädt (nur eigene Rückleseprüfung), und ob Schreiben in andere Versionen als die der Quelldatei sinnvoll ist (wir schreiben immer in der Version der Quelle).

## Anker, Kopfbedeckung und GIF (M7b)
- **Anker-Bindung** (`render::Part`): mehrere ACTs werden übereinander gezeichnet (Körper, Kopf, Kopfbedeckung). Ein angebundener Teil wird um `Anker0(Basis) − Anker0(Teil)` verschoben, wie `AnchoredTo` im ACT Editor; die Basis ist immer der erste Teil (auch für die Kopfbedeckung, ACT Editor überschreibt den Versatz ebenfalls mit dem der Basis). Ein Teil ohne die Aktion oder den Frame wird übersprungen, die Basis muss beides haben. Gleiche Aktions- und Frame-Nummer in allen Teilen angenommen.
- **Gemessen/gesehen (kRO-`data.grf`):** Novice (männlich) mit Kopf und „Cat Ears Cape“. Im Standbild sind alle drei Anker gleich (1, −56), die Bindung ändert nichts; in der Gehanimation (Aktion 8) unterscheiden sich die Bilder, mit Bindung sitzt der Kopf richtig (Gesicht sichtbar), ohne rutscht er um einen Pixel und verdeckt es.
- **GIF** (`gif_io.rs`): je Frame 256 Farben (NeuQuant), Alpha 0 wird transparent, halbtransparente Ränder werden hart. Frame-Zeit = Intervall × 2,5 Hundertstelsekunden, mindestens 2. Schleife unendlich. Gelesen und zurückdekodiert im Test; nicht in einem Browser oder Programm angesehen.
- **Nicht geprüft:** ob die Zuordnung „gleiche Aktionsnummer in Körper und Kopf“ für alle Aktionen stimmt (Kopfdateien haben Aktionen für Kopfdrehungen, der Client rechnet daraus um), pixelgenauer Vergleich mit dem ACT Editor.

## Lua (`.lub`/`.lua`) – M7c, Syntaxprüfung umgesetzt
- **Was:** `lua.rs` prüft die Lua-5.1-Grammatik (der Client bettet 5.1 ein) mit Zeilennummer, ohne Ausführung und ohne Syntaxbaum: Lexer (Strings mit Escapes, Lang-Strings und -Kommentare, Zahlen), rekursiver Abstieg für Anweisungen und Ausdrücke, die 5.1-Regel „mehrdeutig: `(` in neuer Zeile nach einem Aufruf“, kein `goto`, kein `//`. Fehler nennen die öffnende Zeile („to close 'function' at line 1“).
- **Gemessen (2026-11-18):** alle 427 `.lub` der aktuellen `core.grf`, alle 427 der alten (vor der Übersetzung) und alle 29 Text-Dateien aus der kRO-`data.grf` 2024 bestehen die Prüfung; 504 weitere Dateien in der kRO-`data.grf` sind kompiliert (Bytecode, werden gezählt, nicht geprüft). Dein 2018-Client hat in `data.grf` 223 kompilierte und ansonsten Text-Lua.
- **Gegenprobe:** in 420 echten Dateien das letzte `}` (sonst `end`) entfernt: **417 werden erkannt**; die drei übrigen haben das letzte `}` in einem Kommentar oder String, dort ändert das Entfernen nichts.
- **Grenze:** nur Syntax. Ein Laufzeitfehler (nicht definierte Variable, fehlender Tabelleneintrag wie bei `NPCIdentity` im ersten Übersetzungstest) bleibt unsichtbar; das kann keine Prüfung einer einzelnen Datei finden.

## Client-Prüfung (`rgrf client check`) – M7c
Liest einen Client-Ordner nach den gemessenen Regeln (niedrigster Index gewinnt, Archive schlagen lose Dateien, Stand 2018-06-20). Beispiel `client-dev/full` (2026-11-18, 6,8 s): eine lose `data\sclientinfo.xml` wird vom Client ignoriert (`cdata.grf` gewinnt), `datacomp.grf` steht nicht in der `DATA.INI`, 8.194 Dateien aus `data.grf` werden von höheren Archiven überdeckt (1.489 mit anderer Größe), alle drei Server der `sclientinfo.xml` (Live, Developers, <host-ip>) werden gelistet, 443 Lua-Dateien ohne Syntaxfehler. Beim Bau fiel ein Fehler im Prüfer selbst auf: eine `sclientinfo.xml` hat **mehrere** `<connection>`-Blöcke, es zählt also keine einzelne Adresse.
- **Nicht gemessen:** Index 0 in der `DATA.INI`; „Daten-Ordner zuerst lesen“ (kehrt die Reihenfolge um); ob `cps.dll` LZMA kann (nur Anwesenheit).

### Warum es keinen Ressourcen-Prüfer („Datei fehlt“) gibt
Versucht und gemessen: Bildnamen aus den Lua-Tabellen und aus dem Programm gegen die Archive zu prüfen. Von 1.445 verschiedenen Namen je Datei blieben nach Entfernen der Lua-Escapes 337 „fehlend“ in 14 Dateien, überwiegend **falsche Treffer**: der Basisordner hängt von der Tabelle ab (z. B. `petinfo.lub` nennt `ACC_*.act` ohne Ordner, `stateiconimginfo.lub` `.tga` ohne Ordner), und Namen im Programm entstehen oft erst zur Laufzeit (so auch `effect\p2.bmp`, der Name steht weder im Programm noch in den Lua-Dateien als Text). Ein brauchbarer Prüfer braucht je Tabelle eine eigene Regel; das ist nicht gebaut.

## Karten: GAT, GND, RSW – M7e (Teil 1), umgesetzt
- **GAT** (`gat.rs`): `"GRAT"`, Version, Breite, Höhe, dann je Zelle vier `f32`-Höhen und ein `i32`-Typ (Zeile 0 = Süden). Typen wie in GRF Editor: 0, 3, 6 begehbar (3 = Wasser), 1, 2, 5 nicht begehbar, 4 unbekannt; Werte mit gesetztem höchstem Bit kommen vor und bleiben erhalten. Bild: Norden oben.
- **RSW** (`rsw.rs`): Kopf, vier 40-Byte-Namen, Wasser (bis 2.5), Licht, Grenzen, ab 2.7 eine Zahlenliste, Objekte (Modelle, Lichter, Sounds, Effekte); was nach den Objekten folgt (Quadtree) bleibt als Bytes erhalten. Unbekannte Objekttypen werden als Fehler gemeldet statt übersprungen (GRF Editor überspringt sie, verliert damit aber die Ausrichtung).
- **GND** (`gnd.rs`): nur Kopf und Texturliste (reicht, um zu wissen, welche Texturen eine Karte braucht); Kacheln, Lightmaps und Würfelgitter werden **nicht** gelesen.
- **Gemessen (2026-11-18, kRO-`data.grf`, `crates/rgrf-formats/tests/real_map.rs`):**
  - **1.029 GAT** (Versionen 1.2: 903, 1.3: 126): alle fehlerfrei und **byteidentisch** zurückgeschrieben.
  - **1.029 RSW** (1.9: 2, 2.1: 812, 2.2: 57, 2.3: 2, 2.4: 6, 2.5: 24, 2.6: 101, 2.7: 25), **917.911 Objekte**: alle fehlerfrei und **byteidentisch** zurückgeschrieben.
  - **Genannte Dateien:** von 173.333 nachgeschlagenen Dateien (Boden, Höhen, GND-Texturen, Modelle, Sounds) fehlt **eine**: `air_evt.gnd` (die Karte `air_evt` hat keinen Boden). Der Karten-Check hat damit kaum Fehlalarme; anders als der allgemeine Ressourcen-Prüfer (siehe oben), weil die Ordner (`data\texture\`, `data\model\`, `data\wav\`) hier festliegen.
  - Auf deinem `client-dev/full` (7,6 s): 1.050 Karten, 174.988 Dateien, ein Problem (dasselbe `air_evt.gnd`).
  - Sichtprüfung: Prontera als Bild (Achteck, Brunnen, Schloss im Norden oben) ist erkennbar.
- **Nicht geprüft:** Texturen innerhalb der Modelle (RSM wird noch nicht gelesen), Effekte, ob GND-Kacheln und Lightmaps stimmen, GAT-Typen 4 und 6 (kaum vorhanden, nicht erklärt), Vergleich mit GRF Editor (nur in der Windows-VM).

## RSM/RSM2 (3D-Modelle) – M7e (Teil 2): strukturelles Lesen
- **Was:** `rsm.rs` liest Modelle (`.rsm`, `.rsm2`) bis zum letzten Byte und behält Namen, Texturlisten, Mesh-Zähler (Eckpunkte, Flächen) und die Zahl der Volumenboxen; die Geometrie selbst wird übersprungen. **Schreiben ist nicht möglich.** Das Lesen bis zum Dateiende ist der Nachweis für das Layout (`trailing` muss 0 sein).
- **Aufbau (nach `Rsm.cs`/`Mesh.cs`):** Version 1.x: 16 reservierte Bytes, Texturliste (40-Byte-Namen), Hauptmeshname; 2.2: Bildrate, Texturliste und Meshnamen mit Längenpräfix; 2.3: Texturen je Mesh (Längenpräfix), Texturanimationen. Je Mesh: Namen, Matrix, Eckpunkte, Textur-Eckpunkte, Flächen (ab 2.2 mit Längenfeld), Skalierungs- (ab 1.6), Rotations- und (ab 2.2) Positions-Keyframes.
- **Abweichung von GRF Editor (gemessen):** Modelle der **Version 1.5** (490 Dateien) tragen nach den globalen Skalierungs-Keyframes noch eine **zweite Liste (Positions-Keyframes des Modells)**, die GRF Editor nicht liest (es hält den Zähler für die Zahl der Volumenboxen und bricht nicht ab, weil es das Dateiende nie prüft). Erst mit dieser Liste enden alle 490 Dateien genau am Dateiende. Zwei falsche Anläufe (Positions-Keyframes je Mesh) wurden dabei an den Messdaten verworfen.
- **Gemessen (2026-11-18, kRO-`data.grf`):** **9.121 Modelle** (1.4: 6.560, 1.5: 490, 2.2: 402, 2.3: 1.669), 17.276 Meshes: **alle fehlerfrei und genau bis zum letzten Byte gelesen.** Von 17.831 Texturnamen fehlen 26 im Archiv (u. a. `ad\01.bik`, ein Video unter dem Namen einer Textur, und Colosseum-Texturen); das sind Lücken im Original-kRO.
- **`map check` prüft jetzt auch die Texturen in den Modellen:** auf `client-dev/full` 191.650 nachgeschlagene Dateien in 8,3 s, **2 Probleme**: `air_evt.gnd` fehlt, und das Modell `ad01.rsm` nennt `ad\01.bik`. Jede fehlende Modell-Textur wird nur einmal gemeldet (unter der ersten Karte, die das Modell nutzt).
- **Nicht geprüft:** ob die Dateien in anderen als den gemessenen Versionen (1.0 bis 1.3, 1.6 bis 2.1) stimmen (kommen in `data.grf` nicht vor, nur synthetisch getestet); Effekte (`.str`) und ihre Texturen; Vergleich mit GRF Editor.

## STR-Effekte, BMP, TGA, WAV – Vorschau (2026-11-18)
- **STR** (`str_effect.rs`): `"STRM"`, Version `0x94`, fps, letztes Frame, Ebenenzahl, 16 unbenutzte Bytes; je Ebene Texturliste (128-Byte-Namen, CP949) und Keys zu je 124 Byte (Frame, Art, Position, UV, vier Eck-x und vier Eck-y, Texturnummer, Animationsart, Verzögerung, Winkel in 1/1024 Umdrehung, Farbe 0–255, Quell- und Zielmischung, Preset). Art 0 setzt einen Zustand, Art 1 sagt, wie er sich je Frame danach ändert; Texturanimation Art 1–4 (normal, am Ende halten, wiederholen, rückwärts). Die Vertex-Reihenfolge (Dreiecksstreifen A B C D mit UV (0,0) (1,0) (0,1) (1,1)) und die Animationsregeln wurden am beobachtbaren Verhalten von roBrowsers `StrEffect` (GPL-3.0) geprüft, **ohne Code zu übernehmen**; die Umsetzung ist eigen. **Nur Lesen und Rendern, kein Schreiben.**
- **Texturen:** zuerst im Ordner der STR-Datei (neuere Effekte), sonst in `data\texture\effect\`. Gemessen: Von 30.788 Texturverweisen aller 3.230 STR der kRO-`data.grf` löst diese Regel **30.694** auf; mit nur `data\texture\effect\` wären es 1.789 gewesen. 94 fehlen im Original-kRO. BMP-Magenta (255, 0, 255) gilt als durchsichtig (Regel des Clients).
- **Gemessen:** alle **3.230 STR** lassen sich lesen. 156 Stichproben-Effekte (jeder zwanzigste) wurden gerendert (3,2 s mit Texturen-Dekodieren). Sichtprüfung: `heal.str` (Flügelstab mit Glanz) und `bash.str` (blaues Hexagramm) sind erkennbar; `astralstrike.str` hat 23 von 51 hellen Frames.
- **Vorschau in der GUI:** GIF mit höchstens 120 Frames bei halber Bühnengröße (320 × 240), dunkler Hintergrund; `min_astralstrike.str` (14 Ebenen, 40 Texturen) in unter 1 s. **Vereinfacht:** keine Beleuchtung, kein Nebel, keine 3D-Position, Preset-Werte (`mtpreset`) werden nicht ausgewertet. **Nicht geprüft:** Vergleich mit dem Client oder mit GRF Editor Bild für Bild.
- **BMP** (`bmp.rs`): 1/4/8/16/24/32 Bit unkomprimiert, oben oder unten beginnend. Gemessen: alle **51.782 BMP** der kRO-`data.grf` lesbar (RLE gibt es dort nicht; es wird als „nicht unterstützt“ gemeldet).
- **TGA** (`tga.rs`): alle **9.063 TGA** lesbar. **WAV** (`wav.rs`): PCM bleibt, IMA- und MS-ADPCM werden zu PCM gewandelt (6 von 3.332 Dateien, gegen `afconvert` Probe für Probe gleich).

## RSM-Geometrie und 3D-Ansicht der Modelle (2026-11-18)
- **`rsm_model.rs`:** liest zusätzlich zum strukturellen Leser (`rsm.rs`, bleibt der Maßstab für „bis zum letzten Byte“) die Geometrie: Knoten mit Eckpunkten, Texturkoordinaten, Flächen (Eckpunkt- und Texturindizes, Texturnummer, zweiseitig, Glättungsgruppe), Matrix (3 × 3), Versatz, Position/Drehung/Skalierung (bis 2.1 im Knoten, ab 2.2 aus den ersten Keyframes) und den Keyframe-Listen. **Statische Pose:** Knotenmatrix = Elternmatrix · Position · Drehung (Achse/Winkel oder erster Quaternion-Key) · Skalierung; Eckpunkte gehen dann durch Versatz (außer bei Ein-Knoten-Modellen) und die 3 × 3-Matrix. Das Verhalten wurde an roBrowsers Modell-Lader (GPL-3.0) beobachtet, Code nicht übernommen. Normalen: Fläche (flat), nach Glättungsgruppe gemittelt (smooth) oder nach oben (keine). Ausgabe: y nach oben, Modell auf x/z zentriert, Unterkante bei y = 0.
- **Gemessen (kRO-`data.grf`):** alle **9.121 Modelle** (1.4: 6.560, 1.5: 490, 2.2: 402, 2.3: 1.669) lesen mit Geometrie, stimmen in Eckpunkt- und Flächenzahl mit dem strukturellen Leser überein und haben eine endliche Pose (Größen 0,7 bis 1.018 Einheiten).
- **Sichtprüfung (WebGL in der GUI):** `프론테라\대성당.rsm` (Kathedrale, 1.4, 9 Texturen), `프론테라\시계탑.rsm` (Uhrenturm, **5 Knoten** mit Hierarchie, richtig zusammengesetzt) und `event\stringlights_d_02.rsm2` (2.2, Lichterkette) sehen richtig aus. Die Annahme für 2.2/2.3 (erste Keyframes statt Knotenwerte) ist damit an einem Modell bestätigt, aber nicht an allen.
- **Nicht umgesetzt:** Animation der Knoten (Drehung per Keyframe, z. B. Windrad), Texturanimation (2.3), Karten (GND/RSW als 3D-Szene), Lightmaps, Wasser. Texturen werden aus `data\texture\<Name>` geladen; BMP-Magenta ist durchsichtig, TGA-Alpha bleibt.
