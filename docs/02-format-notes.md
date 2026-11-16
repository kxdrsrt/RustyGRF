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
- `zsize_aligned`: **gemessen (2026-11-16, 281.303 Einträge in `cdata`, `core`, `cpal`, `rdata`, `data`, ohne Ausnahme):** `zsize` aufgerundet auf ein Vielfaches von **8** (bei den 0x300-Archiven ist auch `zsize_aligned = zsize` üblich, siehe unten). Auf der Platte belegt der Eintrag `zsize_aligned` Bytes, gelesen werden `zsize`. Relevant für Verschlüsselung und exakte Rekonstruktion; `rgrf verify` meldet Abweichungen als Warnung.
- `flags` (nach GRF Editor `EntryType`, an den Archiven gemessen): `0x01` Datei; `0x00` **leerer Marker** (nur wenn auch `zsize` und `size` 0 sind, 1 Eintrag in der `data.grf` von 2024) **oder unkomprimiert gespeicherte Datei** (`zsize = size`, GRF Editor `RawDataFile`; alle 30 und 25 Flags-0-Einträge der beiden 0x300-Archive, siehe unten); `0x02` Header verschlüsselt (mit Datei: `0x03` = `FileAndHeaderCrypted`, 1 Eintrag in `rdata`); `0x04` Daten verschlüsselt (`0x05`); `0x20` mit GRF Editors eigener Verschlüsselung; `0x80` Gravitys Eintragsschutz (0x300, braucht den Schlüsselblock). RustyGRF behandelt jedes Verschlüsselungsbit als "verschlüsselt, nicht unterstützt" (listen, Warnung, kein Inhalt). Weitere GRF-Editor-Flags (z. B. `LzmaCompressed` = Bit 10, Wert 1025) sind **intern** und stehen nicht in der Datei.
- 0x200 hat 32-Bit-Offsets: Grenze ca. **4 GiB** (Forum 142790: "max around 4gb").
- Ältere Versionen 0x102/0x103: DES-basiert, nicht im Zielumfang von Version 1.

## Version 0x300 (gemessen am 2026-11-16)
- Quellcode von GRF Editor unterstützt 0x300 (Meldung "Expected 0x102, 0x103, 0x200 or 0x300"), `GRF.dll` enthält den String "Event Horizon".
- **Gemessen (2026-11-16, Rohbytes beider Archive, Quelltext von GRF Editor zum Gegenlesen):** Header wie 0x200 (46 Byte), aber: Magic `"Event Horizon\0"` + `63 00` (`c`), Tabellenoffset **u64** bei Byte 30..37, **Anzahl** u32 bei 38 (**ohne** Seed und ohne +7), Version `0x00000300` bei 42. Regel von GRF Editor (Fix 2024-10-25): Nur wenn die Bytes 35..37 null sind, gilt der 64-Bit-Offset; sonst wird wie bei 0x200 gelesen (32-Bit-Offset, Seed bei 34, Anzahl − Seed − 7). Eigene Archive mit Seed ≠ 0 in 0x300 kommen nicht vor (nur synthetisch getestet).
- **Tabellenblock:** `[key_block u32][zsize u32][size u32][zlib-Tabelle][Schlüsselblock]`. Der Schlüsselblock (`key_block` Bytes, höchstens 20 + 65535·64) liegt **hinter** der Tabelle und gehört zu Gravitys Eintragsschutz (Flag `0x80`); **in beiden vorliegenden 0x300-Archiven ist er 0**. Einträge: 21 Byte nach dem Namen (`zsize u32, zsize_aligned u32, size u32, flags u8, offset u64`), Offset relativ zum Ende des Headers (absolut = +46). Beide Archive parsen vollständig (Anzahl im Header = geparste Einträge; Tabelle endet exakt am Dateiende bzw. Schlüsselblock).
- **`zsize_aligned`:** In `data kro latest.grf` immer **gleich `zsize`** (261.359 Dateien, keine Aufrundung); in der Merge-GRF gemischt (193.009 gleich `zsize`, 160.550 auf 8 aufgerundet aus `official_data`). Beide Formen sind gültig, `verify` warnt nur bei anderen Werten. (Die 0x200-Archive von 2024 haben überall die Aufrundung.)
- **Gemessen:** `rgrf verify` beider Archive: 0 Fehler, 0 Warnungen; Hinweise: 30 bzw. 25 unkomprimiert gespeicherte Dateien, 79.764 bzw. 69.853 Einträge teilen sich einen Datenblock (compact).
- **Flags 0 mit Daten (unkomprimiert gespeicherte Dateien; Korrektur 2026-11-17):** In beiden 0x300-Archiven haben **alle** Einträge mit Flags `0x00` (30 bzw. 25) echte Daten: `zsize = size`, unkomprimiert (PNG-Dateien unter `illust`, zusammen 6,8 bzw. 6,0 MB); GRF Editor nennt sie `RawDataFile` und liefert ihren Inhalt (CRC32/MD5 identisch mit RustyGRF). Bis M1b hatte ich sie fälschlich als „Ordnermarker“ (leer) behandelt; das fiel erst beim Repack auf (die Summe der entpackten Größen wich um 5,99 MB ab). **Leere Marker** (Flags 0, `zsize` 0, `size` 0) gibt es nur einen: im 0x200-`data.grf` von 2024 (Offset roh `0xFFFFFFD2` = −46, absolut 0). RustyGRF liest, extrahiert, prüft und kopiert Flags-0-Einträge mit Daten als Dateien (`Entry::is_raw`).
- **Verschlüsselte Einträge:** In `rdata.grf` (2024) hat `data\luafiles514\lua files\msgstring_kr_s.lub` Flags `0x03`; die ersten Datenbytes sind `28 9c …` (weder zlib `78` noch LZMA `00`). **Gemessen (GRF Editor):** Er benennt das Flag `FileAndHeaderCrypted` und entpackt den Eintrag fehlerfrei (CRC32 `5c688728`, MD5 `0879093b…`, 33.224 B). **Gelesen/vermutet:** DES nur für die ersten Blöcke (Header). Eine Entschlüsselung ist nötig, um den Eintrag zu prüfen; in v1 listen wir ihn mit Warnung und prüfen ihn nicht (siehe `rueckfragen/v3-2026-11-16-archiv.md`, Punkt 12, und `rueckfragen/v4-2026-11-16-archiv.md`).
- Welcher Client 0x300 liest: siehe `RO/docs/grf-research/09-neue-grfs-2026-11-16.md` (alpha.exe und RagexeRE 2020: nur 0x200; Warpgate-Exe 2025-07-16: 0x300, kein `cps.dll`, also kein LZMA).

## Kompression
**Gemessen 2026-11-16 (Golden Files aus GRF Editor, Rohbytes am Mac gegengeprüft):** Die Archive im Dev-Client enthalten bereits LZMA-Einträge (erstes Byte `0x00`, dann `5d 00 00 30 00 00` = LZMA-Props + 3-MiB-Wörterbuch): `core.grf` 4.391 von 4.661, `cdata.grf` 185 von 3.973, `cpal.grf` 0 von 45.555, `rdata.grf` 0. Woher das stammt, ist **ungeklärt**: Der Inhaber hat die Dateien nach eigener Erinnerung mit GRF Editor gesichert, vermutlich also ein GRF-Editor-Repack (unbestätigt). Es ist daher **kein Beleg**, dass der Original-Client LZMA nutzt. Folge: **M1 muss LZMA-Einträge dekodieren**, sonst sind `core` und `cdata` nicht prüfbar. `GetDecompressedData()` von GRF Editor liest sie nur, wenn `Compression.CompressionAlgorithm = LzmaCompression` gesetzt ist.
| Methode | Erkennung | Status |
|---|---|---|
| zlib (Standard) | erstes Byte `0x78` | belegt (Tokei, Forum 101460) |
| LZMA (inoffiziell, LessGRF/Curiosity) | erstes Byte `0x00` | belegt (Forum 95086, 101460; in `cdata`-Test gemessen: 3.973/3.973 Einträge) |

- **LZMA-Aufbau (gemessen 2026-11-16 an 4.576 LZMA-Einträgen in `core` und `cdata`, alle Hashes identisch mit GRF Editor):** 1 Byte `0x00`, 1 Byte LZMA-Properties (`0x5d` = lc 3, lp 0, pb 2), 4 Byte Wörterbuchgröße little endian (`00 00 30 00` = 3 MiB), danach der rohe LZMA-Strom (beginnt mit `00`, ohne `.lzma`-Kopf, ohne gespeicherte Größe; die Größe kommt aus der Dateitabelle). Gelesen mit `lzma_rust2::LzmaReader::new_with_props(…, size, props, dict, None)`.
- Die **Dateitabelle** bleibt immer zlib (Tokei).
- zlib und LZMA lassen sich in **einem** Archiv mischen (Curiosity, belegt; `core`/`cdata` im Dev-Client sind gemischt: 270/4.391 bzw. 3.788/185).
- `compress()` der LZMA-`cps.dll` bleibt zlib, weil der Client damit Guild-Emblems an den Server schickt; `compress2()` kodiert LZMA (nur Tool-Seite).
- Der Client ignoriert Fehler beim Dekomprimieren (Curiosity, Tokei): Einträge wie `worm_tail_damage.wav` lesen mit der Original-`cps.dll`, nicht mit strengen Bibliotheken. RustyGRF liest **tolerant und meldet es**. **Gemessen:** `worm_tail_damage.wav` (`data.grf` 2024) ist ein zlib-Stored-Block, dessen Stream 11 Bytes zu kurz ist (`zsize` 1410, nötig 1421); wiederherstellbar sind 1.403 Bytes. GRF Editors Bibliothek liefert dafür leere Daten (Hash des leeren Inhalts), sein Extrahieren (`data-official`) schreibt 1.407 Bytes (die 1.403 plus 4 Nullbytes). RustyGRF liefert die 1.403 Bytes und die Warnung.
- Das Packen mit Standard-Werkzeug (`GrfCL`, Level 9) ergab bei `cdata` 17,02 MB, LZMA 13,56 MB (gemessen). Hochrechnung für `data.grf` siehe `RO/docs/grf-research/03-measurements.md`.

## Schreiben (M2, gemessen am 2026-11-16)
- **Beide Formate wurden von GRF Editor (GRF.dll, Windows-VM) gelesen:** Ausgabe von `rgrf repack` für `cdata` in fünf Varianten (0x200 zlib, 0x200 Zopfli, 0x200 LZMA, 0x300 LZMA, 0x300 zlib mit Inhalts-Dedupe): 0 Fehler, alle 3.973 Pfade, Größen, CRC32 und MD5 identisch mit dem Original, LZMA-Einträge als `LzmaCompressed` erkannt, geteilte Blöcke (739) erkannt. **Nicht** in einem Client getestet (siehe unten).
- **Header 0x200:** `"Master of Magic\0"`, Schlüssel `01..0E`, Tabellenoffset u32, Seed 0, Anzahl + 7, Version. **Header 0x300:** `"Event Horizon\0c\0"`, Schlüssel `01..0E`, Tabellenoffset u64, Anzahl (ohne +7), Version; Tabellenblock mit 4 Byte Schlüsselblockgröße (0). Beides wie in den gemessenen Originalarchiven.
- **Ausrichtung:** Standard wie im Original: 0x200 mit Aufrundung auf 8 (Auffüllen mit Nullbytes), 0x300 ohne (`zsize_aligned = zsize`). Umschaltbar (`--align`/`--no-align`). Ob ein bestimmter Client eine der Formen verlangt, ist **nicht** gemessen.
- **Leere Marker** (Flags 0, keine Daten) werden mit Größe 0 und Offset 0 geschrieben; das Original verwendet dort `0xFFFFFFD2` (0x200). Kein Client liest sie (vermutet). **Unkomprimiert gespeicherte Einträge (Flags 0 mit Daten) werden unverändert übernommen** (Bytes, Flags, Größen); sie werden nicht zu zlib/LZMA umgewandelt und nie mit komprimierten Einträgen gleichen Inhalts zusammengelegt.
- **Größengrenze 0x200:** Die ganze Datei (Daten + Tabelle) muss in 32 Bit passen; `repack` bricht sonst vor dem Umbenennen ab. (GRF Editor: Tabellenoffset über 4 GiB wird nicht gelesen.)
- **LZMA-Kodierer (`lzma-rust2`):** Eintrag `00`, Properties `5d` (lc 3, lp 0, pb 2), Wörterbuch 3 MiB, roher Strom ohne Endmarke (Layout wie die Referenzeinträge). In dieser Bibliothek sind die Presets 6 bis 9 **identisch** (`nice_len` 64). Gemessen an der Hälfte von `cdata` (40 MB entpackt): `nice_len` 128 −0,7 %, 273 −1,0 % (1,9-fache Zeit), Tiefenlimit und `lc`/`pb` ändern weniger als 0,15 %. RustyGRF-Preset 9 setzt deshalb `nice_len` 273; die Properties bleiben `5d` wie in den Referenzdateien.
- **Größen `cdata.grf` (17.352.923 B, gemischt zlib/LZMA, mit GrfCL-Referenzen aus der Recherche):** zlib 9 (miniz_oxide) 17.007.299 B (GrfCL zlib 9: 17,02 MB), Zopfli (10 Iterationen) 15.700.269 B (−7,7 % gegen zlib 9, 94 s), LZMA 13.108.591 B (GrfCL LZMA: 13,56 MB, −3,3 %), 0x300 + LZMA 13.096.532 B.
- **Nicht übertragen:** gespeicherte Streams werden nur übernommen, wenn der Codec gleich bleibt und der Stream nicht größer ist (`--recompress-all` schaltet das ab); beschädigte (`worm_tail_damage.wav`) und verschlüsselte Einträge werden unverändert kopiert.

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
