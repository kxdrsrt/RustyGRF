# 04 – CLI und MCP

## CLI `rgrf`

Globale Optionen: `--json` (maschinenlesbar, Feld `schema_version`), `--names cp1252|cp949|utf8|raw` (Anzeige und Dateinamen beim Extrahieren, Standard `cp1252` wie GRF Editor, siehe [02-format-notes.md](02-format-notes.md)), `--quiet`, `--threads N`.
Exit-Codes: `0` ok, `1` Fehler, `2` Warnungen (z. B. defekter Eintrag toleriert, verschlüsselter Eintrag übersprungen), `3` Prüfung fehlgeschlagen (`verify` fand Fehler).

**Stand M1/M1b (umgesetzt und getestet):** liest GRF **0x200 und 0x300** (Version steht in `info`; `key_block` im JSON).

Befehle: `info`, `ls`, `find`, `cat`, `extract`, `verify`, `hash`. Muster ohne `*`/`?` sind eine Teilzeichenkette (ohne Groß-/Kleinschreibung, `/` = `\`), mit Platzhaltern ein Glob; reguläre Ausdrücke gibt es noch nicht. `info --codecs` zählt zlib/LZMA (liest ein Byte je Eintrag). `extract` überschreibt nie: Standard bricht bei vorhandenem Ziel ab (`--on-collision skip|rename`, `--overwrite`), verweigert `..`/absolute Pfade und Symlink-Ausbrüche, legt Ordner in Archivreihenfolge an (reproduzierbare Schreibweise auf case-insensitiven Dateisystemen), schreibt beschädigte Einträge mit dem wiederherstellbaren Teil (Warnung, Exit 2) und überspringt Marker und verschlüsselte Einträge.

| Befehl | Zweck |
|---|---|
| `rgrf info <grf>` | Version, Größe, Anzahl Einträge, Kompressionsmethoden, Warnungen |
| `rgrf ls <grf> [pattern] [--long]` | Einträge auflisten (Glob oder Teilzeichenkette, Größen, Flags, Offset) |
| `rgrf find <grf> [pattern] [--ext spr,act]` | Suchen (Name, Endung), Ausgabe wie `ls --long` |
| `rgrf cat <grf> <pfad>` | Eintrag auf stdout |
| `rgrf extract <grf> <ziel> [pattern]` | Entpacken (parallel, mit Namenmodus) |
| `rgrf verify <grf>` | Tabelle, Offsets, Überlappungen, Dekompression, Größen, Duplikate |
| `rgrf hash <grf> [pattern]` | CRC32/MD5 je Eintrag |
| `rgrf build <ordner> -o <grf>` | **umgesetzt (M2):** GRF aus Ordner (Ordner = Archivwurzel, enthält z. B. `data/`); gleiche Optionen wie `repack` (`--method`, `--level`, `--format` Standard 0x200, `--align/--no-align`, `--dedupe` = gleicher Inhalt teilt einen Block, `--no-verify`, `--overwrite`, `--dry-run`); Dateinamen NFC, `.DS_Store`/`._*`/`Thumbs.db`/`desktop.ini`/Symlinks werden übersprungen und gemeldet; Fehler bei nicht darstellbaren Namen, Groß-/Kleinschreibungs-Kollisionen, Dateien ≥ 4 GiB, Ausgabe im Quellordner |
| `rgrf repack <grf> -o <grf>` | **umgesetzt (M2):** neu komprimieren/umformatieren, Inhalt unverändert; `--method zlib\|zopfli\|lzma`, `--level`, `--format 0x200\|0x300`, `--align/--no-align`, `--dedupe none\|source\|content`, `--recompress-all`, `--no-verify`, `--overwrite`, `--dry-run` |
| `rgrf build <ordner> -o <grf> [--method zlib\|zopfli\|lzma] [--level N]` | GRF aus Verzeichnis bauen |
| `rgrf diff <a> <b>` | Unterschiede (neu/gelöscht/geändert) |
| `rgrf merge <basis> <overlay> -o <grf>` | Overlay hat Vorrang (z. B. `rdata` über `data`) |
| `rgrf patch <alt> <neu> -o <gpf>` | Patch erzeugen |

Verhalten:
- Schreibbefehle arbeiten in eine temporäre Datei, prüfen sie (`verify`) und benennen dann um.
- `--dry-run` zeigt, was passieren würde.
- `--json` hat ein stabiles Schema (Versionsfeld).

## MCP-Server `rgrf-mcp`

Transport: stdio. Tools (Beispiele, Namen mit Präfix `grf_`):

| Tool | Eingabe | Ausgabe |
|---|---|---|
| `grf_open_info` | `path` | Info wie `rgrf info` |
| `grf_list` | `path`, `pattern?`, `limit?`, `offset?` | Einträge (seitenweise, damit 200.000 Einträge nicht überfluten) |
| `grf_search` | `path`, `query`, `ext?` | Treffer |
| `grf_read_text` | `path`, `entry`, `max_bytes?` | Text-Dateien (Lua, TXT), Kodierung wählbar |
| `grf_read_binary_summary` | `path`, `entry` | Größe, Hash, Typ, Hex-Ausschnitt |
| `grf_extract` | `path`, `entries/pattern`, `dest` | geschriebene Dateien (nur in erlaubten Ordnern) |
| `grf_verify` | `path` | Prüfbericht |
| `grf_diff` | `a`, `b` | Unterschiede |
| `grf_build` | `source_dir`, `out`, `method`, `level` | Bauergebnis |
| `grf_merge` | `base`, `overlay`, `out` | Ergebnis |

Sicherheit:
- **Nur Lesen als Standard.** Schreibende Tools sind nur aktiv, wenn der Server mit `--allow-write <ordner>` gestartet wird.
- Alle Pfade werden gegen erlaubte Wurzeln geprüft (kein `..`, keine Symlink-Ausbrüche).
- Keine Archive überschreiben ohne ausdrücklichen Parameter; immer erst in temporäre Datei.
- Große Ergebnisse werden gekürzt und mit `truncated: true` markiert.

Typische Anfragen an den Agenten: "Welche Dateien aus `rdata` überschreiben `data`?", "Baue `cdata` mit LZMA und vergleiche die Größe", "Suche in `data` alle Dateien mit `job4`".
