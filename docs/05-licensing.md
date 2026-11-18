# 05 – Lizenz und Erlaubnis

## Stand
- Die Repositories **Tokeiburu/GRFEditor** und **Tokeiburu/ActEditor** haben auf GitHub **keine Lizenzdatei** und keine Lizenzangabe (geprüft am 16.11.2026, über die GitHub-API).
- **Tokeiburu hat dem Projektinhaber die Erlaubnis erteilt** und die Nutzung freigegeben (Stand 16.11.2026, mündlich/im Gespräch; vom Inhaber bestätigt, kein weiterer Nachweis nötig). Sein Projekt wird als **Inspiration** in den Credits genannt. RustyGRF ist **kein Fork**.
- **Projektlizenz (entschieden 2026-11-16):** `MIT OR Apache-2.0` (`LICENSE-MIT`, `LICENSE-APACHE`). Vor einer Veröffentlichung noch umkehrbar; Credits in `NOTICE` und `README.md`.

## Optional festhalten
Die Tabelle ist freiwillig; der Inhaber hat die Freigabe als geklärt gemeldet.

| Feld | Eintrag |
|---|---|
| Datum | |
| Kanal / Link (Issue, Mail, Discord) | |
| Wortlaut (Zitat) | |
| Gilt für | z. B. Portierung nach Rust, Nutzung von Formatkenntnissen, Veröffentlichung |
| Gilt **nicht** für | (leer lassen, falls nichts ausgeschlossen) |
| Lizenz für RustyGRF (Wunsch von Tokeiburu?) | |
| Namensnennung gewünscht? | |
| Patches aus GRF Editor mitziehen erlaubt? | |

## Regeln für das Projekt
1. **Namensnennung:** In README und `NOTICE`: "inspiriert von GRF Editor/ActEditor von Tokeiburu" (Form nach seinem Wunsch).
2. **Herkunft kennzeichnen:** Dateien, die aus seinem Code abgeleitet sind, erhalten einen Hinweis im Kopf. Eigenständige Eigenentwicklung (nach dem öffentlichen Format) wird nicht gekennzeichnet.
3. **Reine Formatkenntnis** stammt aus öffentlichen Quellen (Forum 57175 u. a.) und eigenen Messungen. Das ist der bevorzugte Weg für den Kern (M1–M5).
4. **Drittbibliotheken** (Rust-Crates): Lizenz jeder Abhängigkeit prüfen und in einer `THIRD-PARTY`-Liste führen. Kompatibilität mit der gewählten Projektlizenz sicherstellen.
5. **LZMA-Format** (Curiosity, GitHub `curio-r/rag-cps-dll`): Auch dort Lizenz/Erlaubnis vor Übernahme prüfen; das Format selbst wird aus Beobachtung an echten Dateien nachgebaut.
6. **Keine Spieldaten im Repo:** Keine GRFs, Sprites, Texturen oder sonstige Gravity-Inhalte committen (`.gitignore` schließt Archive aus). Testdaten bleiben lokal.
7. **Patches von GRF Editor mitziehen:** Nur nach bestätigter Erlaubnis (siehe Tabelle) und mit Quellenvermerk im Commit.

## Offen
- **Abhängigkeiten der GUI (`apps/gui`, Tauri 2):** der Baum enthält **MPL-2.0** (`cssparser`, `selectors`, `dtoa-short`, `option-ext`, über Tauri; Lizenz auf Dateiebene, die Crates werden unverändert eingebunden) und Zweitlizenzen (`0BSD`, `MIT-0`, `Unlicense`, `LGPL-2.1-or-later` nur als eine Option von Doppellizenzen, wir nutzen jeweils MIT/Apache). Die Bibliothek, CLI und der MCP-Server bleiben frei davon (eigener Workspace, eigene `apps/gui/deny.toml`). Wer die App weitergibt, muss den Quelltext dieser MPL-Dateien verfügbar halten (er liegt auf crates.io). Bestätigt am 2026-11-18: in Ordnung für dich (Empfehlung übernommen).
- Namensprüfung "RustyGRF": laut Inhaber frei (nicht von mir geprüft).
