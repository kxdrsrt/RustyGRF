# 09 – Dateiformate des Clients (SPR, ...)

Abgeleitet aus GRF Editor (Tokeiburu, mit Erlaubnis, siehe [05-licensing.md](05-licensing.md)): `GRF/FileFormats/SprFormat/*`, `Rle.cs`. Umsetzung: Crate `rgrf-formats`.

## SPR (Sprites) – M7a, umgesetzt

**Aufbau (gelesen aus dem Quelltext, an echten Dateien geprüft):**
- `"SP"`, Minor, Major (also `SP\x01\x02` = Version 2.1), `u16` Anzahl Palettenbilder, ab 2.0 `u16` Anzahl True-Color-Bilder.
- Palettenbild: `u16 Breite, u16 Höhe`, vor 2.1 rohe Pixel, ab 2.1 `u16 Größe` + RLE-Daten (nur Index 0 wird gepackt: `0` + Anzahl 1–255).
- True-Color-Bild: `u16 Breite, u16 Höhe`, dann `Breite × Höhe × 4` Bytes als **A, B, G, R**, Zeilen **von unten nach oben**.
- Palette: die letzten 1024 Bytes der Datei (256 × R, G, B, ungenutzt), Index 0 ist transparent. Dateien mit nur True-Color-Bildern tragen sie oft trotzdem.
- Bildreihenfolge: erst alle Palettenbilder, dann alle True-Color-Bilder (so zählt auch die ACT-Datei).
- Unterstützt: 1.0, 1.1, 2.0, 2.1. Version 2.2/3.2 (Größe als `u32`, zlib-gepackte True-Color-Bilder) kennt GRF Editor, aber in keiner unserer Dateien kommt sie vor; sie wird abgelehnt statt geraten.

**Gemessen (2026-11-18, kRO-`data.grf` 2024, 44.526 SPR-Dateien, `crates/rgrf-formats/tests/real_spr.rs`):**
- Versionen: 2.1 in 44.464 Dateien, 2.0 in 62. Version 1.x kommt in diesem Archiv nicht vor (nur synthetisch getestet).
- **Alle 44.526 Dateien lesen sich fehlerfrei** (658.431 Palettenbilder; 995 Dateien mit True-Color-Bildern).
- **Schreiben und erneut Lesen ergibt in allen Dateien dieselben Pixel und dieselbe Palette.**
- **Byteidentisch nach dem Zurückschreiben: 44.520 von 44.526.** Die 6 anderen: 4 Dateien (`NIGHT_WATCH_*` und `rebellion_*` unter `data\sprite\ÀÎ°£Á·\`) haben 13.660–15.670 ungenutzte Bytes zwischen Bildern und Palette (wir lesen mit Warnung, schreiben ohne); 2 Dateien (unter `data\sprite\¾Ç¼¼»ç¸®\`) bestehen nur aus 54 True-Color-Bildern und haben 822 Bytes Anhang hinter den Bildern (keine Palette); den Anhang schreiben wir nicht zurück.
- Sichtprüfung: Export eines Sprites (Surfboard-Poring) nach PNG, Farben und Transparenz stimmen.
- **Nicht gemessen:** Vergleich der Exporte mit GRF Editor/ActEditor (nur in der Windows-VM möglich).

**PNG:** Palettenbilder werden 8-Bit-indiziertes PNG (Palette in `PLTE`, Index 0 per `tRNS` transparent), True-Color als RGBA. Beim Zurückbauen bleibt jedes Bild unverändert; das ungenutzte vierte Palettenbyte geht dabei verloren (Pixel und Farben bleiben gleich).

**CLI:** `rgrf spr info|export|build`, siehe [04-cli-and-mcp.md](04-cli-and-mcp.md).
