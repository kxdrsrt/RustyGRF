# Release und Verpackung

Stand 2026-11-19. **Gebaut und geprüft** sind das macOS-Bundle, die Windows-Installer (ARM64, in der VM) und das Linux-`.deb` (ARM64); x64-Windows, Linux x64 und Intel-Mac sind beschrieben, aber nicht gebaut.

## Was es gibt

| Ziel | Befehl (in `apps/gui`) | Stand |
|---|---|---|
| macOS `.app` (Apple Silicon) | `npx tauri build --bundles app` | **gebaut** (9,9 MB, mit Icon, unsigniert, Dateizuordnung `.grf`/`.gpf` im `Info.plist` geprüft) |
| macOS `.dmg` | `npx tauri build --bundles dmg` | ✅ gebaut (arm64, 4,3 MB, `hdiutil verify` gültig; enthält `RustyGRF.app` und die Verknüpfung auf Programme, Dateitypen `grf`/`gpf` in der Info.plist); **ohne Signatur** (nur ad hoc), Gatekeeper warnt beim ersten Start; Intel-Mac nicht gebaut |
| Windows `.msi` / `.exe` (ARM64) | `npx tauri build` in der Windows-VM (Einrichtung siehe unten) | **gebaut und geprüft**: `RustyGRF_0.0.0_arm64_en-US.msi` (4,3 MB) und `RustyGRF_0.0.0_arm64-setup.exe` (NSIS, 2,8 MB). MSI still installiert (`msiexec /qn`, Exit 0) nach `C:\Program Files\RustyGRF`, Zuordnung `.grf`/`.gpf` → `RustyGRF.grf` mit `"…rustygrf-gui.exe" "%1"` in der Registry, sauber deinstalliert. NSIS still pro Benutzer (`/S`, Exit 0) nach `%LOCALAPPDATA%\RustyGRF`, Zuordnung unter `HKCU`. Im echten Fenster gestartet (siehe unten). x64 nicht gebaut |
| Linux `.deb` (ARM64) | in Docker (`rust:1`, Debian, `CARGO_BUILD_JOBS=2`, sonst Speichermangel), `npx tauri build --bundles deb` | **gebaut und geprüft** (4,7 MB): installiert sich in einem sauberen Container ohne Fehler, `ldd` findet alle Bibliotheken, Desktop-Datei mit `Exec=rustygrf-gui %F` und `MimeType`, MIME-Definition für `*.grf`/`*.gpf`/`*.thor`/`*.rgz` (`src-tauri/linux/`), Icon in drei Größen; **nicht gestartet** (kein Bildschirm im Container) |
| Linux x64, AppImage | `npx tauri build` unter Linux | nicht gebaut; braucht `webkit2gtk-4.1` und `libayatana-appindicator` (Workflow siehe unten) |
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
- **Icon:** Entwurf A (Kiste, Rostorange) ist eingebaut und im `.app` geprüft; Entwürfe und Vorgehen in [icon-drafts/](icon-drafts/README.md). Ein endgültiges Design ist deine Entscheidung.
- **Versionsnummer:** `0.0.0` in `tauri.conf.json` und den `Cargo.toml`; vor dem ersten Release festlegen.
- **Automatische Builds:** `.github/workflows/gui-bundles.yml` baut die Installer (Windows `.msi`/`.exe`, Linux `.deb`/AppImage x64 und ARM64, macOS `.app`/`.dmg`) bei Hand-Start oder Tag `v*`; läuft erst, wenn das Projekt auf GitHub liegt (bisher nur lokales Git, kein Push), Syntax geprüft (YAML), nicht ausgeführt.
- **Updater:** nicht eingebaut.

## Bekannte Grenzen der Pakete

- Zweiter Start bei laufender App: der erste Prozess bekommt die Datei (Single-Instance-Plugin), in der Linux-Variante über D-Bus. **Unter Windows geprüft:** der zweite Start beendet sich (Exit 0), die laufende App öffnet die übergebene `data.grf`; macOS und Linux nicht ausprobiert.
- Linux-Dateizuordnung steht in `tauri.conf.json` (`bundle.fileAssociations`) und in der MIME-Definition, ist aber nicht an einem Desktop geprüft.

## Windows-VM (ARM64)

Eingerichtet in der Parallels-VM „Tiny 11 Pro“ (Windows 11 ARM64, Build 26300): Visual Studio 2022 Build Tools (C++-Werkzeuge ARM64 und x64, Windows-11-SDK 26100), Rust über rustup (`aarch64-pc-windows-msvc`), Node als portables Archiv. Tauri lädt WiX und NSIS beim ersten Bau selbst. Die fertige App braucht nur WebView2 (in Windows 11 enthalten).

**Geprüft:** der ganze Workspace (`cargo test --release --workspace`) und die Tests der GUI-Crate laufen unter Windows grün, die GUI-Tests auch mit den echten kRO-Archiven (`RGRF_TESTDATA` auf die Mac-Freigabe `\\Mac\Home\…` gesetzt; Bildverkleinerung, Sounds, Referenz-Animationen eingeschlossen). Dabei gefunden und behoben: RGZ-Dateien ließen sich nicht öffnen, weil unter Windows ein Positions-Lesezugriff (`seek_read`) den Dateizeiger verschiebt. Im echten Fenster (Bildschirmfotos, Klicks und Tasten per Win32, Zustand per WebView2-DevTools-Protokoll) geprüft: Start mit Datei als Argument, Menüleiste (File, Edit, View, Window, Help mit allen Befehlen und Kürzeln; vorher fehlte View, weil ein nachträglich eingefügtes Untermenü unter Windows nicht erscheint), Ctrl+3 schaltet auf den Baum, Baum mit Pfeiltasten (öffnen, hinein, Auswahl, Vorschau mit koreanischem Text), Einstellungen (Fokus jetzt auf dem ersten Feld statt auf dem ganzen Bereich), Fenstertitel mit Archivnamen (vorher nur „RustyGRF“, auch unter macOS). **Nicht geprüft:** Ziehen und Ablegen von Dateien, Bildschirmleser, hohe DPI-Stufen, x64-Windows.
