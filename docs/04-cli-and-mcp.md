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
| `rgrf diff <a> <b> [--all]` | **umgesetzt (M3):** Pfade nur in A / nur in B / mit anderem Inhalt (SHA-256 des dekodierten Inhalts, Pfadvergleich wie der Client: Groß-/Kleinschreibung und `/`=`\` egal); Exit 0 identisch, 2 Unterschiede |
| `rgrf merge <grf> <grf>… -o <grf>` | **umgesetzt (M3):** das **erste** Archiv gewinnt je Pfad (wie der niedrigste Index in `DATA.INI`: eigene Custom-GRFs zuerst, `data.grf` zuletzt); Ausgabereihenfolge = Reihenfolge des Archivs mit der niedrigsten Priorität, dann neue Pfade der nächsten; **ohne `--method` werden alle Ströme unverändert kopiert** (nichts wird neu komprimiert, deshalb schnell; geprüft wird, dass jeder kopierte Strom byteidentisch ankommt), mit `--method` wird neu komprimiert und der neue Strom behalten, wo er kleiner ist; sonst wie `repack` (`--level`, `--format` Standard 0x300, wenn eine Quelle 0x300 ist, sonst 0x200, `--dedupe`, …); leere Marker werden nicht kopiert (gemeldet); je Quelle: Dateien / übernommen / überschrieben |
| `rgrf patch <alt> <neu> -o <grf>` | **umgesetzt (M3):** Archiv nur mit neuen und geänderten Dateien aus `<neu>` (Ströme unverändert kopiert, wie bei `merge`; `--method` komprimiert neu); in `DATA.INI` **vor** `<alt>` einreihen. In `<neu>` entfernte Pfade lassen sich in einem GRF nicht löschen (gemeldet) |

Verhalten:
- Schreibbefehle arbeiten in eine temporäre Datei, prüfen sie (`verify`) und benennen dann um.
- `--dry-run` zeigt, was passieren würde.
- `--json` hat ein stabiles Schema (Versionsfeld).

## MCP-Server `rgrf-mcp`

**Umgesetzt (M4).** Transport stdio (zeilenweises JSON-RPC 2.0, MCP-Version 2025-06-18; ältere 2025-03-26 und 2024-11-05 werden ausgehandelt), nur Tools. Start: `rgrf-mcp [--allow-write <ordner>]… [--allow-read <ordner>]…`. Eingebunden in `.vscode/mcp.json` als `rustygrf` (Schreibordner `~/build/rgrf-out`). Das offizielle Rust-SDK (`rmcp`, mit tokio) wird bewusst nicht genutzt: das Protokoll für reine Tools ist klein, und so bleiben die Abhängigkeiten gering.

| Tool | Eingabe | Ausgabe |
|---|---|---|
| `grf_info` | `path`, `codecs?` | Version, Größe, Einträge, Marker, verschlüsselt, geteilte Daten, Codecs |
| `grf_list` | `path`, `pattern?`, `ext?[]`, `offset?`, `limit?` (Standard 100, max. 1000), `names?` | Einträge seitenweise, `truncated`, `next_offset`; Suche = `pattern`/`ext` (Teilzeichenkette oder Glob, `/` = `\`) |
| `grf_read_text` | `path`, `entry`, `max_bytes?` (65.536, max. 1 MiB), `encoding?` (auto, utf8, cp949, cp1252) | Text und verwendete Kodierung; Binärdaten werden abgelehnt |
| `grf_read_summary` | `path`, `entry` | Größe, CRC32, MD5, Dateityp (SPR, ACT, RSW, GND, GAT, RSM, BMP, PNG, JPEG, WAV, Ogg, Lua, XML, Text), 64 Byte Hex |
| `grf_verify` | `path`, `decode?`, `max_issues?` | Zähler und Befunde (Fehler, Warnungen, Hinweise) |
| `grf_diff` | `a`, `b`, `limit?` | nur in A, nur in B, anderer Inhalt (je Gruppe `count`, `paths`, `truncated`) |
| `grf_extract` | `path`, `dest`, `pattern?`, `on_collision?`, `overwrite?` | geschriebene Dateien; **schreibend** |
| `grf_build` | `source_dir`, `output`, `method?`, `level?`, `format?`, `dedupe?`, `overwrite?`, `dry_run?` | Bauergebnis; **schreibend** |
| `grf_repack` | `path`, `output`, `method?`, `level?`, `format?`, `dedupe?`, `overwrite?`, `dry_run?` | Ergebnis; **schreibend** |
| `grf_merge` | `paths[]` (erstes gewinnt), `output`, `method?` (ohne: Ströme unverändert kopieren), sonst wie `grf_repack` | Ergebnis und je Quelle: Dateien / übernommen / überschrieben; **schreibend** |
| `grf_patch` | `old`, `new`, `output`, `method?` (ohne: Ströme kopieren), sonst wie `grf_repack` | Ergebnis mit `added`, `changed`, `removed_in_new`; **schreibend** |

Jedes Ergebnis kommt als `structuredContent` (JSON-Objekt) und als Text. Fehler einer Anfrage (Archiv fehlt, Pfad nicht erlaubt, falsches Argument) kommen als `isError: true` mit klarer Meldung, damit der Agent sich korrigieren kann; ein unbekanntes Tool oder eine unbekannte Methode ist ein JSON-RPC-Fehler. Argumente werden gegen dieselbe Tabelle geprüft, aus der das JSON-Schema erzeugt wird (unbekannte, fehlende, falsch typisierte Argumente werden abgelehnt).

Sicherheit (umgesetzt und getestet):
- **Nur Lesen als Standard.** Die fünf schreibenden Tools werden nur angeboten (und angenommen), wenn mindestens ein `--allow-write` gegeben ist.
- Schreiben nur unterhalb der erlaubten Ordner; `..` wird abgelehnt, Symlinks werden vor der Prüfung aufgelöst (kein Ausbruch über Ordner- oder Datei-Symlinks), ein Symlink als Ziel wird abgelehnt.
- Vorhandene Dateien werden nur mit `overwrite=true` ersetzt, Ordner nie; eine Ausgabe darf nie eines ihrer Eingabearchive sein; alle Schreibbefehle gehen über temporäre Datei, Rückprüfung und Umbenennen.
- Lesen geht überall (Entscheidung zu Punkt 20); `--allow-read` beschränkt es auf die genannten Ordner (plus Schreibordner).
- Fremde Archive: Einträge über 256 MiB werden nicht dekodiert, jede Antwort ist begrenzt (Seiten, `max_bytes`, `limit`) und mit `truncated` markiert; ein Absturz in einem Tool wird abgefangen und als Fehler gemeldet.
- Abbruch: `notifications/cancelled` stoppt einen laufenden Aufruf (gemessen: Zopfli-Build mit 150 Dateien wird in unter 4 s beendet, keine halbe Datei, keine temporäre Datei bleibt), er bekommt dann keine Antwort. Fortschritt kommt als `notifications/progress`, wenn der Client einen `progressToken` mitschickt.

Grenzen: Ein Aufruf läuft, bevor der nächste bearbeitet wird (ein Client kann während `grf_build` nicht gleichzeitig `grf_list` abfragen); sehr lange Aufrufe (Merge der 5,7-GB-Archive, Zopfli) können Zeitlimits mancher Clients überschreiten, Fortschrittsmeldungen helfen dort. Eine Eingabe pro Zeile; die Länge einer Zeile ist nicht begrenzt (der Client ist vertrauenswürdig, nur die Archive nicht).

Typische Anfragen an den Agenten: "Welche Dateien aus `rdata` überschreiben `data`?" (`grf_diff`), "Baue `cdata` mit LZMA und vergleiche die Größe" (`grf_build`, `grf_info`), "Suche in `data` alle Dateien mit `job4`" (`grf_list`).
