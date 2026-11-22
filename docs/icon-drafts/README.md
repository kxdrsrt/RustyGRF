# Icon-Entwürfe

Alle Entwürfe sind SVG (1024 × 1024, Quelle im Text, frei änderbar).

- `draft-c-crate-r.svg`: **gewählt (aktuelles Icon)**. Entwurf A, dazu ein großes „R“ aus Pixelblöcken auf der linken Seitenfläche der Kiste (4 × 5 Blöcke, wie die Fläche schräg gestellt, Steigung 156/272). Die Kiste ist um 15 % größer als in A, damit das R auch bei 32 px noch zu erkennen ist. Die Pixelquadrate auf der rechten Fläche bleiben.
- `draft-a-crate.svg`: der frühere Entwurf (gewählt am 2026-11-18). Eine isometrische Kiste (ein Archiv/Paket) auf rostorangem Grund; die kleinen Pixelquadrate stehen für die Sprite-Bilder darin. Ist die Grundlage von C.
- `draft-b-pixel-r.svg`: ein Pixel-„R“ in Rosttönen auf dunklem Grund. Klar lesbar, aber sagt weniger über den Zweck aus.

Gewählt ist C (A + Block-R). So entstand das Icon:

1. `draft-c-crate-r.svg` zu `apps/gui/src-tauri/icons/source/icon-1024.png` gerendert (1024 × 1024, Ecken transparent; z. B. mit `@resvg/resvg-js`). Die SVG liegt zusätzlich als `source/icon.svg` daneben.
2. Alle Größen mit `cd apps/gui && npx tauri icon src-tauri/icons/source/icon-1024.png` erzeugt (`icon.icns`, `icon.ico`, PNGs). `npx tauri icon` legt auch `android/` und `ios/` an; die werden nicht gebraucht und wieder gelöscht.
3. `apps/gui/ui/icon.png` (Icon im Über-Dialog, 128 × 128) aus derselben PNG verkleinert.

Anderer Entwurf gewünscht? Sag es, dann tausche ich die Quelle und erzeuge die Größen neu.
