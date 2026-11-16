# 01 – Architektur

## Leitprinzipien
1. **Bytes zuerst:** Dateinamen und Daten sind rohe Bytes. Dekodieren ist nur Anzeige (siehe [02-format-notes.md](02-format-notes.md)).
2. **Kern ohne I/O-Politik:** Die Bibliothek liest über Traits, die CLI/MCP/GUI sind dünne Schichten.
3. **Nichts kaputt machen:** Schreiben geschieht immer in eine temporäre Datei, danach Prüfung, dann atomar umbenennen.
4. **Maschinenlesbar:** Jeder Befehl hat `--json`. Dasselbe Ergebnisformat nutzt der MCP-Server.
5. **Kompatibilität vor Größe:** Standardausgabe ist immer für den unveränderten Client lesbar (zlib). Alles andere ist eine ausdrückliche Option.

## Workspace (Cargo)
```
RustyGRF/
├─ crates/
│  ├─ rgrf-core/      Bibliothek: Container, Codecs, Namen, Prüfen, Bauen, Diff
│  ├─ rgrf-cli/       Binary `rgrf`
│  ├─ rgrf-mcp/       Binary `rgrf-mcp` (MCP über stdio)
│  ├─ rgrf-formats/   (später) SPR, ACT, PAL, GAT, GND, RSW, RSM, STR, IMF
│  └─ rgrf-gui/       (später) Oberfläche mit Vorschau
├─ docs/
├─ tests/             Integrationstests, Golden Files (Hashes, nicht die Archive selbst)
└─ Cargo.toml         workspace
```

## rgrf-core – Module
| Modul | Aufgabe |
|---|---|
| `container` | Header, Dateitabelle, Einträge, Versionen (0x102/0x103/0x200, später 0x300) |
| `codec` | Trait `Codec { compress, decompress }` mit `Zlib`, `Zopfli`, `Lzma` |
| `name` | `GrfPath(Vec<u8>)`, Normalisierung (`\` vs `/`, Groß/Klein), Anzeige-Kodierung |
| `reader` | Zugriff über `mmap` oder Stream, Lesen einzelner Einträge, Streaming-Extraktion |
| `writer` | Bauen aus Verzeichnis/Einträgen, parallele Kompression, atomares Schreiben |
| `verify` | Prüfen: Tabelle, Offsets, Überlappungen, Dekompression, Größen, Duplikate |
| `diff` | Vergleich zweier Archive/Ordner (Pfad, Größe, Hash), Patch-Erzeugung, Merge mit Vorrang |
| `hash` | CRC32 und MD5 je Eintrag (wie GRF Editor/GrfCL) |
| `crypto` (Feature `crypto`, später) | Verschlüsselung/Entschlüsselung, erst nach Entscheidung |

## Wichtige Entscheidungen

### Sprache und Bibliotheken (zu bestätigen im Prototyp)
- **Rust**, stabile Toolchain, `edition = 2021` oder neuer.
- zlib: `flate2` (Backend `zlib-rs` oder `miniz_oxide`) für Lesen, **Zopfli** (`zopfli`-Crate) für maximale zlib-kompatible Kompression.
- LZMA: Bibliothek (`lzma-rs` oder `liblzma`) hinter einem eigenen Wrapper, weil das GRF-Format von Curiosity ein eigenes Eintragsformat hat (siehe Format-Notizen).
- Parallelität: `rayon`. Dateizugriff: `memmap2`.
- CLI: `clap`. Ausgabe: `serde`/`serde_json`. Fehler: `thiserror`/`anyhow`.
- MCP: offizielles Rust-SDK für MCP (`rmcp`), Transport stdio.
(Konkrete Versionen und Lizenzen der Crates beim Anlegen prüfen.)

### Lesen
- Archiv wird per `mmap` geöffnet, die Dateitabelle einmal dekomprimiert und als Index im Speicher gehalten (200.000+ Einträge, kompakter Index statt `String`s).
- Einträge werden einzeln und nebenläufig dekomprimiert.
- Das Verhalten bei fehlerhaften Einträgen: **tolerant wie der Client** (zlib-Fehler am Ende ignorieren), aber melden. Beispiel: `worm_tail_damage.wav` in der kRO-`data.grf` ist defekt, der Client liest sie trotzdem.

### Schreiben
- Eintragsdaten werden parallel komprimiert, dann sequenziell in die Datei geschrieben (stabile Reihenfolge, reproduzierbare Ausgabe).
- Reproduzierbar: gleiche Eingabe + gleiche Optionen = byteidentische Datei (Test).
- Tabelle am Ende, danach Header mit finalem Offset. Dann Rückprüfung (`verify`) und atomares Umbenennen.

### Kompression
- Standard: **zlib** (wie Gravity/`cps.dll`) – Archiv für jeden Client lesbar.
- `--method zopfli`: zlib-kompatibel, kleiner, viel langsamer. Laut Recherche ca. 4–8 % kleiner (nicht auf GRFs getestet).
- `--method lzma`: nur mit der LZMA-fähigen `cps.dll` im Client. Dateitabelle bleibt zlib. Mischbar pro Eintrag.
- Bereits komprimierte Typen (`.png`, `.wav`, `.jpg` ...) werden nicht doppelt komprimiert, wenn das keinen Gewinn bringt (Eintragsebene prüfen und nur dann speichern).

### Plattformen
- Keine nativen Abhängigkeiten, nur reines Rust, damit `cargo build` auf Windows/macOS/Linux (x64/ARM64) ohne Zusatzinstallation läuft.
- Pfade: intern immer Bytes, Anzeige mit `/`. Auf der Festplatte beim Extrahieren dekodierte Namen, mit Option für Rohnamen (siehe Format-Notizen).

## Tests
- **Property-Tests:** Bauen → Lesen → Vergleichen (byteweise).
- **Referenzvergleich** mit GRF Editor/`GrfCL`: gleiche Eintragsliste, gleiche Hashes.
- **Golden Files:** kleine Archive (z. B. `cdata`, 17 MB) und Hashlisten der großen (`data.grf`).
- **Client-Test** (manuell): Dev-Client lädt die gebauten Archive.
- CI später (GitHub Actions) für alle drei Plattformen.
