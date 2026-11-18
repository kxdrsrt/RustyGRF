# 07 – UI/UX

Die Oberfläche kommt ab M7 (nach Kern, CLI und MCP). Dieses Dokument legt die Richtung fest, damit Kern und Schnittstellen schon jetzt dazu passen.

## Technologie (Entscheidung D15: Tauri 2, umgesetzt in `apps/gui`)
| Option | Vorteile | Nachteile |
|---|---|---|
| **Tauri 2 + Web-Frontend** (TypeScript, z. B. Svelte oder React) | größte Gestaltungsfreiheit, Dark/Light per CSS (`prefers-color-scheme`), gute Listen-/Baumkomponenten, einfache Vorschau (Canvas/WebGL), Rust-Kern direkt nutzbar | nutzt die Web-Engine des Systems (WebView2/WebKit), Node im Build |
| egui / eframe | rein Rust, sehr schnelle Entwicklung, ein Binary | weniger poliertes "natives" Aussehen, Barrierefreiheit eingeschränkt |
| Slint | natives Aussehen, deklarativ | kleinere Community, Lizenzmodell prüfen |
| Iced | rein Rust | jünger, weniger Komponenten |

**Empfehlung:** Tauri 2. Grund: Design und Dark Mode sind dort am einfachsten, und Vorschau-Rendering (SPR/ACT/Karten/Modelle) geht gut im Browser (Canvas/WebGL). Der Rust-Kern bleibt unverändert, die GUI spricht dieselben Funktionen wie CLI und MCP an.

## Leitprinzipien
1. **Folgt dem System:** Hell/Dunkel standardmäßig nach Systemeinstellung (wie beim gebauten GRF Editor), zusätzlich manuell wählbar. Native Titelleiste passt sich an.
2. **Schnell bei 200.000+ Einträgen:** virtualisierte Listen und Bäume, Filter während des Tippens, kein Einfrieren (Hintergrund-Worker, Fortschritt, Abbrechen).
3. **Tastatur zuerst:** alle Aktionen per Tastatur und Befehlspalette (`Ctrl/Cmd+K`), Suche `Ctrl/Cmd+F`.
4. **Sicher:** jede Änderung ist ein Befehl mit Rückgängig/Wiederholen. Schreiben ist immer ein Entwurf, der erst mit "Speichern" auf die Platte geht (temporär, prüfen, dann umbenennen).
5. **Transparent:** Warnungen (defekter Eintrag, LZMA nicht für Standardclient lesbar, Namensrundlauf) sind sichtbar und erklärt, nicht versteckt.
6. **Lesbar:** Dateinamen mit wählbarer Kodierung (siehe Format-Notizen), koreanische Namen anzeigen und den Rohnamen zeigen.
7. **Barrierefrei und skalierbar:** Kontraste nach WCAG AA, Schriftgrößen/Skalierung, Fokusreihenfolge, Screenreader-Labels.
8. **Mehrsprachig:** Texte in Ressourcen, Start mit Deutsch und Englisch.

## Hauptansicht (Skizze)
```
┌ Menü / Befehlspalette ─────────────────────────────────────────────┐
│ [Öffnen] [Speichern] [Rückgängig] [Wiederholen]   [Filter……]  [⚙]  │
├───────────────┬──────────────────────────────┬─────────────────────┤
│ Baum          │ Liste (virtualisiert)         │ Vorschau / Details  │
│  data         │ Name | Typ | Größe | Methode  │  Bild / Sprite /    │
│   ├ sprite    │ …                             │  Text / Hex         │
│   ├ texture   │                               │  Metadaten, Hash    │
├───────────────┴──────────────────────────────┴─────────────────────┤
│ Status: 202.358 Einträge · zlib 100 % · Warnungen (1) · Fortschritt  │
└────────────────────────────────────────────────────────────────────┘
```
- Mehrere Archive im selben Fenster (Tabs oder Ebenen), Ladereihenfolge wie `DATA.INI` sichtbar (welches Archiv gewinnt).
- Drag & Drop aus und in das Archiv.
- "Vergleichen"-Ansicht für Diff/Merge (`rdata` über `data`).
- "Baue neu"-Assistent mit Kompressionswahl, Größenschätzung und Hinweis zur Client-Kompatibilität.

## Gestaltung
- **Design Tokens** (Farbe, Abstand, Radius, Typografie) zentral in einer Datei. Zwei Themes (hell, dunkel), aus denselben Tokens.
- Ruhige, dichte Oberfläche für Profis (wie ein Datei-Explorer/IDE), keine Marketing-Optik.
- Vorgehen: erst **Wireframes** (Markdown/HTML-Skizzen in `docs/ui/`), dann klickbarer Prototyp mit Fake-Daten, danach Anbindung an den Kern.
- Usability-Tests mit echten Aufgaben ("finde alle `.spr` unter `job4`", "baue `cdata` mit LZMA", "lege `rdata` über `data`").

## Schnittstelle Kern ↔ GUI
- Der Kern liefert **ereignisbasierten Fortschritt** und **abbrechbare** Operationen (nötig für die GUI, sinnvoll auch für CLI/MCP). Das jetzt schon in `rgrf-core` einplanen.
- Vorschau-Funktionen im Kern (z. B. "SPR-Frame als RGBA") statt in der GUI, damit CLI/MCP dieselben Bilder liefern können.
