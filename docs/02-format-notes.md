# 02 – Format-Notizen

Kennzeichnung: **belegt** = in einer gelesenen Quelle oder selbst gemessen, **zu prüfen** = Annahme, im Prototyp gegen echte Archive zu bestätigen.
Quellen: `RO/docs/grf-research/` (Forum 57175 "Description of the GRF file format" von Tiera, 101460, LessGRF 95086), GRF-Editor-Quellcode (Ansicht der Struktur, siehe [05-licensing.md](05-licensing.md)).

## Container 0x200 (belegt)
```
Header (46 Byte)
  Magic[16]          "Master of Magic" + 0x00
  Key[14]            0x01..0x0E oder 0 (veraltet)
  FileTableOffset u32  relativ zum Ende des Headers, absolut = Wert + 46
  Seed u32           in der Praxis 0, aber kein Gesetz (Forum: Aeomin)
  FilesCount u32     echte Anzahl = FilesCount - Seed - 7
  Version u32        0x200
Daten                Einträge, jeweils einzeln komprimiert
Tabelle              [zsize u32][size u32][zlib-komprimierte Tabelle]
  je Eintrag:        Name (nullterminiert, Bytes) + 17 Byte:
                     zsize u32, zsize_aligned u32, size u32, flags u8, offset u32
```
- Entry-Offsets sind relativ zum Datenbereich, absolute Position = `46 + offset`.
- Auch bei "keine Kompression" gibt es einen zlib-Rahmen (Header `78 01`, Prüfsumme am Ende).
- `zsize_aligned`: Größe des auf die Blockgrenze aufgerundeten Eintrags (zu prüfen: 4 Byte). Wichtig für Verschlüsselung und exakte Rekonstruktion.
- `flags` (zu prüfen im Prototyp): Bit 0 = Datei, Bit 1 = gemischte DES-Verschlüsselung, Bit 2 = DES nur Header. GRF Editor nutzt zusätzlich interne Flags (z. B. `LzmaCompressed` als Bit 10), die **nicht** in der Datei stehen.
- 0x200 hat 32-Bit-Offsets: Grenze ca. **4 GiB** (Forum 142790: "max around 4gb").
- Ältere Versionen 0x102/0x103: DES-basiert, nicht im Zielumfang von Version 1.

## Version 0x300 (gemessen am 2026-11-16)
- Quellcode von GRF Editor unterstützt 0x300 (Meldung "Expected 0x102, 0x103, 0x200 or 0x300"), `GRF.dll` enthält den String "Event Horizon".
- **Gemessen** (`data kro latest.grf`, `data (merged kro+chris).grf`, Parser `grf-research/tools/grftable.py`): Magic "Event Horizon", Header 46 Byte, 64-Bit-Dateitabellenoffset an Position 30, danach 32-Bit-Anzahl, kein Seed (Anzahl direkt), 4 reservierte Bytes vor der Tabellengröße, Einträge 21 Byte (Offset 64 Bit). Beide Dateien parsen vollständig (Header-Anzahl = geparste Einträge).
- **Ordnermarker:** Einträge mit Flags `0x00` (0x300: 30 bzw. 25; 0x200 `data.grf` 2024: 1) haben Größen 0 und einen Offset, der absolut auf 0 zeigt (0x200: `0xFFFFFFD2` = −46). Sie zählen als Einträge (202.358 in der Abnahme).
- **Verschlüsselte Einträge:** In `rdata.grf` (2024) hat `data\luafiles514\lua files\msgstring_kr_s.lub` Flags `0x03`; die ersten Datenbytes sind `28 9c …` (weder zlib `78` noch LZMA `00`). **Vermutet:** DES (Bit 1). Nicht in v1 (kein `crypto`).
- Welcher Client 0x300 liest: siehe `RO/docs/grf-research/09-neue-grfs-2026-11-16.md` (alpha.exe und RagexeRE 2020: nur 0x200; Warpgate-Exe 2025-07-16: 0x300, kein `cps.dll`, also kein LZMA).

## Kompression
| Methode | Erkennung | Status |
|---|---|---|
| zlib (Standard) | erstes Byte `0x78` | belegt (Tokei, Forum 101460) |
| LZMA (inoffiziell, LessGRF/Curiosity) | erstes Byte `0x00` | belegt (Forum 95086, 101460; in `cdata`-Test gemessen: 3.973/3.973 Einträge) |

- LZMA-Aufbau (laut Research-Agent aus `curio-r/rag-cps-dll`, **zu prüfen**): 1 Byte Kennung `0x00`, 5 Byte LZMA-Parameter, danach die Daten. Das Ergebnis ist am echten Beispiel nachzumessen: `build-grfs.ps1 cdata -Method lzma` erzeugt eine Referenzdatei, die RustyGRF byteweise lesen können muss.
- Die **Dateitabelle** bleibt immer zlib (Tokei).
- zlib und LZMA lassen sich in **einem** Archiv mischen (Curiosity, belegt).
- `compress()` der LZMA-`cps.dll` bleibt zlib, weil der Client damit Guild-Emblems an den Server schickt; `compress2()` kodiert LZMA (nur Tool-Seite).
- Der Client ignoriert Fehler beim Dekomprimieren (Curiosity, Tokei): Einträge wie `worm_tail_damage.wav` (die letzten 7 Bytes beschädigt, kRO-`data.grf`) lesen mit der Original-`cps.dll`, nicht mit strengen Bibliotheken. RustyGRF muss **tolerant lesen und es melden**.
- Das Packen mit Standard-Werkzeug (`GrfCL`, Level 9) ergab bei `cdata` 17,02 MB, LZMA 13,56 MB (gemessen). Hochrechnung für `data.grf` siehe `RO/docs/grf-research/03-measurements.md`.

## Dateinamen (wichtig)
- Die Namen im Archiv sind **Bytes** (koreanisch, CP949). GRF Editor und `GrfCL` zeigen sie standardmäßig als Windows-1252-Zeichen (z. B. `À¯ÀúÀÎÅÍÆäÀÌ½º` für `유저인터페이스`). Der GRF-Editor-Downloadeintrag nennt ausdrücklich: Dateinamen werden automatisch in den passenden Zeichensatz gewandelt, die Kodierung ist wählbar (belegt).
- Entwurf:
  - Intern `GrfPath(Vec<u8>)`. Vergleiche und Suche auf Bytes, Groß-/Kleinschreibung wie der Client (ASCII-insensitiv, zu prüfen).
  - Anzeige-Modi: `raw` (Latin-1 bijektiv, verlustfrei), `cp949` (lesbar), `utf8` (nur wenn gültig).
  - Beim Extrahieren/Bauen Rundlauf prüfen: Namen → Datei → Namen müssen dieselben Bytes ergeben. Wenn nicht verlustfrei möglich, auf `raw` mit Escape ausweichen und warnen.
  - Pfadtrenner im Archiv ist `\`. Auf macOS/Linux beim Schreiben auf `/` abbilden.
- **Bereits gemessen:** `cdata` wurde mit dem Windows-1252-Verfahren extrahiert und neu gebaut, die Namen blieben byteidentisch. Für `data-official` (119.181 Dateien mit Nicht-ASCII-Namen) ist das Zurückpacken noch **nicht** verifiziert.

## Archive im Client
- `GPF` = identisches Format, nur andere Endung (Patches).
- `DATA.INI` bestimmt die Ladereihenfolge (Wiki: "0 = höchste Priorität", das kRO-Beispiel ist 1-basiert; offen). Der Client benötigt einen gehexten Exe für mehrere GRFs.
- `rdata.grf` überschreibt `data.grf` bei gleichem Pfad (kRO: 21.504 von 24.756 Einträgen; gemessen).
- Thor-Dateien (`.thor`) haben ein verwandtes Format, nicht im Umfang von Version 1.

## Quellformate für spätere Crates (rgrf-formats)
SPR (Sprites, Paletten), ACT (Animationen), PAL, GAT/GND/RSW (Karte), RSM/RSM2 (Modelle), STR (Effekte), IMF, Lua/Lub (dekompilieren). Genaue Beschreibungen kommen aus eigenen Messungen an Beispieldateien sowie, nach Klärung der Lizenz, den GRF-Editor-Parsern.
