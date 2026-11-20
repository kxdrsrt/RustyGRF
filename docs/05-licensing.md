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

- **Bilder auf 8 Bit (`quantize.rs`, `bmp.rs`):** eigener Code nach den bekannten Verfahren Median-Cut und k-Means. Aus roBrowsers `Texture.js` (GPL-3.0) wurde nur die Schwelle gelesen, ab der eine Farbe als Magenta durchsichtig gilt (R > 230, G < 20, B > 230); **kein Code übernommen**, die Datei nennt die Quelle.
- **STR-Effekte (`str_effect.rs`):** Dateiaufbau und Animationsregeln wurden am Verhalten von roBrowsers `StrEffect`/`Str` (GPL-3.0) geprüft und danach selbst geschrieben; es wurde **kein Code übernommen**, die Datei nennt die Quelle des Wissens.

## Herkunft abgeleiteten Codes (Stand 2026-11-19)
- **`crates/rgrf-core/src/gravity.rs` (2026-11-19):** Gravitys Schutz einzelner Einträge, portiert aus GRF Editors `GRF/Core/GrfCompression/ProtectedGrfDecoder.cs` und `ProtectedGrfCiphers.cs` (Ablauf, Konstanten, CHAM/ThreeWay/IDEA/RC6 als eigener Code wie dort) und die vier kRO-Standardschlüssel aus `GRF/Core/GrfHeader.cs` (`KRODefaultProtectedKeys`, byteweise gegen den Quelltext geprüft). Die Schlüssel stammen ursprünglich von Gravity; GRF Editor liefert sie öffentlich mit. Dazu das Testbündel `protected-grf.bin` (siehe `THIRD-PARTY.md`). Seit 2026-11-20 auch die jRO-Variante aus `ProtectedJroDecoder.cs` und `JroArchiveProfiles.cs` (Ablauf, TEA/XTEA/RC5 als eigener Code, die drei eingebauten jRO-Schlüssel und die 59 SHA-256-Werte bekannter Schlüsselblöcke), dazu das selbst erzeugte Testbündel `protected-grf-jro.bin` (in der Windows-VM aus GRF Editors Decoder-Quellen gebaut; nur erzeugte Bytes). Nicht portiert: `ProtectedCnDecoder.cs`.
Mit Erlaubnis von Tokeiburu nach GRF Editors Quelltext gelesen und in Rust neu geschrieben (Vermerk im Kopf der Datei): `crates/rgrf-core/src/des.rs` (`Utilities/DesDecryption.cs`: Tabellen und Ablauf), `container.rs` (0x102/0x103-Eintragsform nach `FileTable._load100`, 0x300-Header), `thor.rs` (`ThorFormat/*`), `rgz.rs` (`RgzFormat/*`) die Lua-Dekompilierung (`LubFormat`, nur als Gegenprobe) und `crates/rgrf-formats/src/flat_map.rs` (Regeln des Flat-Maps-Makers aus `GRFEditor/Tools/Map/MapEditor.cs`, GND-Wasserdaten aus `GndFormat/Gnd.cs`); `grfcrypt.rs` und `clientcrypt.rs` (Verschlüsselung: Ablauf aus `GRF/Core/*` und `EncryptorDialog.xaml.cs`, der Algorithmus aus der IL von `Encryption.dll`, die GRF Editor nur als Binärdatei mitbringt; die `cps.dll`-Vorlage (`GRFEditor/Files/cps.dll`, Binärdatei von Tokeiburu) ist in `crates/rgrf-core/assets/cps-template.dll` eingebaut; **Erlaubnis dafür hat der Projektinhaber am 2026-11-19 bestätigt** (Tokeiburu), Herkunft und MD5 stehen in `clientcrypt.rs` und `NOTICE`). Lua 5.0.3/5.1 (MIT, lua.org) dient dem Prüfwerkzeug `tools/lua-verify` (nicht ausgeliefert); `luacomp.rs` (5.1.5) und `luacomp50.rs` (5.0.3) sind Rust-Ports von Lexer, Parser und Codegenerator unter derselben MIT-Lizenz (siehe NOTICE, THIRD-PARTY.md).

