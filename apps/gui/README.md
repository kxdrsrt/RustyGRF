# RustyGRF App (Tauri 2)

Desktop-Oberfläche auf dem Rust-Kern (`rgrf-core`, `rgrf-formats`). Eigenes Cargo-Workspace, damit Bibliothek, CLI und MCP-Server nicht von Tauri abhängen (Lizenzen siehe `deny.toml` und [docs/05-licensing.md](../../docs/05-licensing.md)).

## Aufbau
- `src-tauri/src/backend.rs`: alles, was die App mit einem Archiv tut (öffnen, suchen, blättern, Vorschau, extrahieren, prüfen), ohne Tauri-Typen, mit Tests.
- `src-tauri/src/lib.rs`: dünne Tauri-Befehle darum; schwere Arbeit läuft auf einem Arbeits-Thread, das Fenster bleibt bedienbar.
- `ui/`: reines HTML/CSS/JS (kein Build-Werkzeug). Die Liste ist virtualisiert (nur sichtbare Zeilen, Seiten vom Backend), Dark/Light per `prefers-color-scheme`. `ui/mock.js` ersetzt das Backend nur im normalen Browser, in der App ist es inaktiv.
- Namensanzeige: Standard **Koreanisch** (CP949, z. B. `유저인터페이스`), umschaltbar auf Latin-1 (so zeigt es GRF Editor) oder UTF-8. Gilt auch für die Suche; Extrahieren schreibt immer Latin-1-Namen (rundlauffähig mit `rgrf build`).
- Startpfad: `rustygrf-gui <datei.grf>` öffnet das Archiv direkt (Grundlage für „Öffnen mit“).
- **Build…** packt einen Ordner zu einer GRF (zlib/0x200 als Standard, Zopfli und LZMA nur ausdrücklich gewählt), mit Fortschrittsbalken, Abbrechen und anschließender Prüfung gegen die Dateien auf der Platte. Abbruch hinterlässt keine Datei. Ein Auftrag zur Zeit.
- **Merge…** führt mehrere GRFs zu einer neuen zusammen (Reihenfolge = Priorität, per ↑/↓ änderbar; „Replace only“: letzte GRF ist die Basis, die anderen fügen keine neuen Pfade hinzu), mit Fortschritt, Abbruch, Prüfung und Statistik je Eingabe. Die Eingaben bleiben unverändert, die Ausgabe darf keine der Eingaben sein.
- **Edit-Leiste** (Replace…, Add file…, Remove, Changes (n)…): Änderungen werden gesammelt und mit „Save as new archive…“ in eine **neue** GRF geschrieben, die geöffnete bleibt unberührt. Unveränderte Einträge behalten ihre Streams (kein Neukomprimieren, die GRF wächst nicht). **Gemessen** an einer echten 88-MB-GRF (4.661 Einträge, je 1 ersetzt/hinzugefügt/entfernt): 60 ms, Ergebnis 86,7 MB, geprüft. Gleicher Pfad mehrfach → die letzte Änderung zählt. Kern: `rgrf_core::edit::apply`.
- **Bearbeiten innerhalb von Einträgen:** in der Sprite-Vorschau „Replace…“ je Bild (PNG; bei Paletten-Sprites werden die Farben auf die Palette abgebildet, Transparenz → Index 0), in der Animationsvorschau „Set interval…“ je Aktion (ms, die Datei speichert Vielfache von 25 ms). Mehrere Änderungen am selben Eintrag bauen aufeinander auf und landen als **eine** Änderung in der Liste. Die Vorschau zeigt weiter das geöffnete Archiv, nicht die vorgemerkten Änderungen; erst nach dem Speichern und Öffnen sieht man das Ergebnis.
- Einziges npm-Paket: `@tauri-apps/cli` (Entwicklungsabhängigkeit).

## Starten und Bauen
```
cd apps/gui
npm install                  # holt nur die Tauri-CLI
npx tauri dev                # Fenster mit der App
npx tauri build              # Installer/Bundle für das eigene System
cargo test                   # Backend-Tests (Tests mit echten Archiven brauchen RGRF_TESTDATA)
```
Das Bauverzeichnis steht lokal in `.cargo/config.toml` (nicht im Git).
Layout ohne App ausprobieren: `python3 -m http.server -d ui 8765`, dann `http://127.0.0.1:8765` öffnen (Mock-Daten).

## Stand
Gemessen und geprüft siehe [docs/03-roadmap.md](../../docs/03-roadmap.md), Abschnitt M8. Noch nicht in der App: Bauen, Zusammenführen, Bearbeiten, Fortschritt/Abbruch, Einstellungen. Das Icon ist ein Platzhalter (`src-tauri/icons`, erzeugt aus einem 1024-px-PNG mit `npx tauri icon`).
