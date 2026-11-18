# Release und Verpackung

Stand 2026-11-18. **Gebaut und geprüft** ist nur das macOS-Bundle; alles andere ist hier beschrieben, aber nicht ausgeführt.

## Was es gibt

| Ziel | Befehl (in `apps/gui`) | Stand |
|---|---|---|
| macOS `.app` (Apple Silicon) | `npx tauri build --bundles app` | **gebaut** (9,3 MB, unsigniert, Dateizuordnung `.grf`/`.gpf` im `Info.plist` geprüft) |
| macOS `.dmg` | `npx tauri build --bundles dmg` | nicht gebaut: das Tauri-Skript öffnet ein Finder-Fenster zum Anordnen der Symbole |
| Windows `.msi` / `.exe` | `npx tauri build` unter Windows (ARM64-VM oder x64) | nicht gebaut; braucht Rust, Node und die Visual-Studio-Build-Tools in der VM |
| Linux AppImage / `.deb` | `npx tauri build` unter Linux | nicht gebaut; braucht `webkit2gtk-4.1` und `libayatana-appindicator` |
| CLI `rgrf`, MCP-Server `rgrf-mcp` | `cargo build --release` (Hauptworkspace) | gebaut und getestet auf vier Plattformen (M5) |

`npx tauri build` ohne `--bundles` baut alle Formate des eigenen Systems (`bundle.targets = "all"`). Das Bauverzeichnis steht lokal in `apps/gui/.cargo/config.toml` (nicht im Git).

## Vorher prüfen

```
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --release   # Hauptworkspace
cd apps/gui && cargo test --release && cargo clippy --all-targets --features devserver -- -D warnings && cargo deny check
```

Die Tests mit echten Archiven brauchen `RGRF_TESTDATA` (siehe `apps/gui/.cargo/config.toml`). Die Oberfläche lässt sich ohne Fenster mit dem Devserver prüfen (`apps/gui/README.md`). Der Devserver ist ein eigenes Binary mit `required-features`; er landet nicht im Bundle.

## Offen, braucht deine Entscheidung

- **Signierung und Notarisierung** (macOS: Apple-Developer-Konto; Windows: Zertifikat). Ohne sie warnt das System beim ersten Start („nicht verifizierter Entwickler“, SmartScreen). Für den Eigengebrauch reicht Rechtsklick → Öffnen.
- **Icon:** Platzhalter, Entwürfe kann ich erzeugen (`npx tauri icon <1024-px-PNG>` macht daraus alle Größen).
- **Versionsnummer:** `0.0.0` in `tauri.conf.json` und den `Cargo.toml`; vor dem ersten Release festlegen.
- **Automatische Builds** (CI für macOS/Windows/Linux): erst sinnvoll, wenn das Projekt ein eigenes Repository hat (bisher nur lokales Git, kein Push).
- **Updater:** nicht eingebaut.

## Bekannte Grenzen der Pakete

- Zweiter Start bei laufender App: der erste Prozess bekommt die Datei (Single-Instance-Plugin), in der Linux-Variante über D-Bus. **Nicht an einer laufenden App ausprobiert**, weil der Fokuswechsel das Fenster auf dem Bildschirm nach vorn holt.
- Windows/Linux-Dateizuordnung steht in `tauri.conf.json` (`bundle.fileAssociations`), ist dort aber nicht geprüft.
