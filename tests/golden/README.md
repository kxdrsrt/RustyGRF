# Golden Files

**Wofür:** Unabhängige Referenz. Sie stammen aus GRF Editor (`GRF.dll`, Windows-VM), nicht aus RustyGRF. Die Tests von `rgrf-core` vergleichen ihre Ausgabe damit: gleiche Einträge, gleiche Größen/Flags/Offsets, gleiche CRC32/MD5 der entpackten Inhalte. Stimmt beides überein, ist die Wahrscheinlichkeit hoch, dass RustyGRF das Format richtig liest und nicht nur sich selbst bestätigt. Sie enthalten nur Metadaten (Pfade, Größen, Hashes), keine Spieldaten.

Erzeugt am 2026-11-16 mit `tools/golden/run-in-vm.sh` (VM "Tiny 11 Pro", GRF Editor/`GRF.dll` aus `1 CLIENT TOOLS/5 GRF Editor`).

| Datei | Quelle (relativ zu `RO/1 CLIENT-SIDE`) | Einträge | LZMA | Fehler |
|---|---|---|---|---|
| `cdata.tsv.gz` | `client-dev/full/cdata.grf` | 3.973 | 185 | 0 |
| `core.tsv.gz` | `client-dev/full/core.grf` | 4.661 | 4.391 | 0 |
| `cpal.tsv.gz` | `client-dev/full/cpal.grf` | 45.555 | 0 | 0 |
| `rdata.tsv.gz` | `0 OTHER CLIENTS/client-kro/rdata.grf` | 24.756 | 0 | 0 |
| `data.tsv.gz` | `0 OTHER CLIENTS/client-kro/data.grf` (identisch mit `client-dev/full/data.grf`, byteweise verglichen) | 202.358 | 0 | 0 |

`SHA256SUMS` enthält die SHA-256 der Quellarchive (Pfad relativ zu `RO/1 CLIENT-SIDE`), damit klar ist, welche Datei gemeint ist. Prüfen: `cd "RO/1 CLIENT-SIDE" && shasum -a 256 -c "0 GRFs/RustyGRF/tests/golden/SHA256SUMS"`.

## Spalten (TSV, UTF-8, Kopfzeile, nach Pfad ordinal sortiert)
`path_display` (Name wie GRF Editor ihn anzeigt, Windows-1252), `path_utf8_hex`, `zsize`, `zsize_aligned`, `size`, `flags` (Dateiflag:Name laut GRF Editor), `offset` (**absolut**, also Archivoffset + 46), `crc32`, `md5` (über die entpackten Daten), `error`.

## Bekannte Besonderheiten (gemessen)
- Der Ordnermarker in `data` (Flags `0`) hat Offset `0`; roh steht dort `0xFFFFFFD2` (= −46). Er ist der einzige Eintrag, bei dem Roh-Offset + 46 nicht zum Golden-Wert passt.
- `data\wav\worm_tail_damage.wav`: Der Stream ist 11 Bytes zu kurz (Stored-Block, `zsize` 1410). GRF Editor meldet **keinen Fehler** und liefert **leere Daten** (CRC `00000000`, MD5 des leeren Strings). Ein tolerantes Dekodieren liefert 1.403 Bytes. Dieser Eintrag ist als Inhaltsreferenz ungeeignet; RustyGRF soll ihn als defekt melden.
- `rdata`: `msgstring_kr_s.lub` (Flags `3`, `FileAndHeaderCrypted`): GRF Editor entschlüsselt ihn (CRC `5c688728`).
- 12 Namen in `data` haben Bytes 0x80–0x9F (CP1252 ≠ Latin-1), ein Name enthält CP1252-undefinierte Bytes. Beim Vergleich Namen als **Bytes** behandeln oder CP1252 verwenden (nicht Latin-1).

## Abgleich mit dem Python-Tabellenparser (Mac)
Alle Felder (zsize, zsize_aligned, size, Flags, Offset) und alle Namen aller fünf Archive stimmen überein (202.358 + 24.756 + 45.555 + 4.661 + 3.973 Einträge), mit einer Ausnahme: dem Ordnermarker in `data` (siehe oben).
