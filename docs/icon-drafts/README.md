# Icon-Entwürfe

Beide Entwürfe sind SVG (1024 × 1024, Quelle im Text, frei änderbar).

- `draft-a-crate.svg`: **gewählt (2026-11-18, nach meiner Empfehlung)**. Eine isometrische Kiste (ein Archiv/Paket) auf rostorangem Grund; die kleinen Pixelquadrate stehen für die Sprite-Bilder darin.
- `draft-b-pixel-r.svg`: ein Pixel-„R“ in Rosttönen auf dunklem Grund. Klar lesbar, aber sagt weniger über den Zweck aus.

Gewählt ist A. Daraus entstand `apps/gui/src-tauri/icons/source/icon-1024.png` (Ecken transparent) und mit `npx tauri icon src-tauri/icons/source/icon-1024.png` alle Größen (`icon.icns`, `icon.ico`, PNGs). `npx tauri icon` legt auch `android/` und `ios/` an; die werden nicht gebraucht und wieder gelöscht.

Anderer Entwurf gewünscht? Sag es, dann tausche ich die Quelle und erzeuge die Größen neu.
