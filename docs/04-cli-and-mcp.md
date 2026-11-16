# 04 – CLI und MCP

## CLI `rgrf`

Globale Optionen: `--json` (maschinenlesbar), `--names raw|cp949|utf8` (Anzeige, siehe [02-format-notes.md](02-format-notes.md)), `--quiet`, `--threads N`.
Exit-Codes: `0` ok, `1` Fehler, `2` Warnungen (z. B. defekter Eintrag toleriert), `3` Prüfung fehlgeschlagen.

| Befehl | Zweck |
|---|---|
| `rgrf info <grf>` | Version, Größe, Anzahl Einträge, Kompressionsmethoden, Warnungen |
| `rgrf ls <grf> [pattern] [--long]` | Einträge auflisten (Glob/Regex, Größe, Hash) |
| `rgrf find <grf> <pattern>` | Suchen (Name, Endung) |
| `rgrf cat <grf> <pfad>` | Eintrag auf stdout |
| `rgrf extract <grf> <ziel> [pattern]` | Entpacken (parallel, mit Namenmodus) |
| `rgrf verify <grf>` | Tabelle, Offsets, Überlappungen, Dekompression, Größen, Duplikate |
| `rgrf hash <grf> [pattern]` | CRC32/MD5 je Eintrag |
| `rgrf build <ordner> -o <grf> [--method zlib\|zopfli\|lzma] [--level N]` | GRF aus Verzeichnis bauen |
| `rgrf repack <grf> -o <grf> --method ...` | Neu komprimieren (Inhalt unverändert) |
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
