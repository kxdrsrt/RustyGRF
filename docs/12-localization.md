# 12 – Lokalisierung (L0–L2 für die Hauptoberfläche umgesetzt, Beta)

## Stand 2026-11-20: umgesetzt
**Entscheidungen** (Nutzer: „ich folge allen Empfehlungen“): Fluent-Format, eine neutrale Spanisch-Fassung, Deutsch mit „du“, CLI bleibt Englisch, Dateien von Hand (Weblate erst, wenn das Projekt öffentlich wird), **maschinelle Entwürfe der KI, als Beta gekennzeichnet**, bis Muttersprachler sie gesehen haben.

**Technik:**
- `apps/gui/ui/i18n.js`: liest die benutzte **Teilmenge von Fluent** (Nachricht, Variablen `{ $n }`, Auswahl mit Mehrzahlkategorien und genauen Zahlen, Attribute `.title`/`.aria-label`/`.placeholder`/`.hint`, Kommentare). Keine Bibliothek; die Dateien bleiben echtes `.ftl` für Weblate und Co. Mehrzahl über `Intl.PluralRules`, Zahlen über `Intl.NumberFormat`. Fehlt ein Schlüssel, gilt **je Schlüssel** der englische Text.
- `apps/gui/ui/i18n/<sprache>.ftl`: `en` (Quelle), `de`, `es`, `pt-BR`, `ko`, `ja`, `fr`, `id`, `fil`; dazu die **Pseudo-Sprache** `qps` (aus `en` erzeugt: Akzente, Länge +40 %, Klammern) zum Aufspüren von Texten, die nicht über den Katalog laufen. Französisch mit geschützten Leerzeichen vor `: ; » `.
- HTML: `data-i18n="schlüssel"` (Text) und `data-i18n-attrs="schlüssel"` (Tooltip, Namen für Screenreader, Platzhalter); Skript: `t("schlüssel", { n })`.
- **Einstellungen → Sprache** (Standard: Systemsprache; Wechsel ohne Neustart, gespeichert); Beta-Sprachen tragen „(Beta)“, dazu ein Hinweis.
- **Native Menüleiste:** `set_menu_labels` baut sie beim Start und beim Sprachwechsel aus den `menu-*`-Schlüsseln neu auf. Die vom System gelieferten Einträge (Rückgängig, Kopieren, Beenden …) übersetzt das Betriebssystem selbst.
- **Prüfskript** `node tools/check-i18n.mjs` (im Ordner `apps/gui`): Leser-Tests (Mehrzahl, Attribute), **gleiche Schlüssel und Platzhalter in allen Katalogen**, keine Klammern nach dem Formatieren, jeder von Seite und Skript benutzte Schlüssel steht in `en.ftl`. Stand 2026-11-20: 1,031 Schlüssel, 8 Sprachen, alles grün.

**Abgedeckt (L0 teilweise):** Haupt- und Ansichtsleiste (alle Tooltips und Namen für Screenreader), Sortierung, Typ- und Größenfilter, Typnamen, leerer Zustand, Spaltenköpfe, Statuszeile (Zählung mit Mehrzahl), Speichern-Zähler, gemeinsame Knöpfe (OK, Abbrechen, Schließen, Fertig, Schnell, Vollständig), **Dialoge Prüfen, Erstellen, Zusammenführen, Extrahieren, Hinzufügen, Ersetzen, Entfernen, Änderungen, Speichern und der Fortschrittsdialog** (jeweils mit Status- und Fehlertiteln), **ganze Einstellungsseite mit den Erklärtexten zu Kompression und Format** (auch im Build-Dialog), Menüleiste.

**Auch Hex-Editor, beide Dialoge „Bilder verkleinern“, die Vorschau (Kopfzeile, Ordner, Animation, 3D-Modell und Karte mit allen Schaltern, Effekt, Video, Hex), „Bild speichern“, „Karte extrahieren“, der ACT-Editor (Ebenen, Anker, Zwischenbilder, Referenzen, Sound, Intervall) Text-/Lua-Editor, SPR-Bilder (ersetzen, hinzufügen, entfernen), Pixel-Editor, Paletten-Bearbeitung, Karten-Zellen-Editor (GAT), Passwort- und Entwurfsdialog sowie die Statuszeile beim Öffnen sind umgestellt. **Alle Texte der Oberfläche in `app.js` laufen jetzt über den Katalog.** (674 Schlüssel).**

**Info-Zeilen der Vorschau (2026-11-20):** Das Backend liefert sie als `Note`: entweder Klartext (Namen, Meldungen der Bibliotheken) oder ein Katalogschlüssel mit Werten (`{key: "pvw-n-spr", args: {…}}`, Schlüssel `pvw-n-*`). Die Oberfläche übersetzt sie in `noteText()`; die Namen der GAT-Zelltypen stehen als `gat-tn-*` im Katalog. Die Tests prüfen Schlüssel und Werte statt englischer Sätze. Zahlen werden mit ihrer Einheit aus dem Katalog gesetzt (`un-textures` usw.), damit „1 Textur“/„2 Texturen“ stimmt.

**Fehlermeldungen (2026-11-20):** Die ~50 Meldungen, die die App selbst erzeugt (Eingabeprüfungen in ACT-, Paletten-, Zellen-Editor, „keine Archive gewählt“, „kein Archiv offen“ …), kommen als kodierter Text (`\u0001` + JSON mit Schlüssel `err-*` und Werten, Funktion `coded`/Makro `err!` in `lib.rs`) und werden in `errText()`/`noteText()` übersetzt. Mehrzahlformen der Einheiten (`un-*`) stehen im Katalog; Stand 892 Schlüssel (2026-11-20, mit Menüs für Fenster, Speichern, Werkzeuge und Über).

**Noch nicht umgestellt (erscheint auf Englisch):** Fehlertexte, die aus den Bibliotheken (`rgrf-core`, `rgrf-formats`) durchgereicht werden (Dateiformat kaputt, Ein-/Ausgabefehler, falscher Schlüssel …), und Hinweise, die nur im Protokoll stehen; außerdem die Menüpunkte, die das Betriebssystem selbst liefert (die übersetzt es nach der Systemsprache).

**Nicht geprüft:** Aussehen aller Sprachen im echten Tauri-Fenster (nur im Browser mit dem echten Backend: Deutsch, Koreanisch, Pseudo, Japanisch, Filipino umgeschaltet, keine Skriptfehler); Mindestgröße 900 × 640 mit allen Sprachen und den Dialogen; Windows/Linux-Schriften für CJK; **die Übersetzungen selbst** (maschineller Entwurf; besonders die Erklärtexte, Verschlüsselung und Datenverlust gehören von Muttersprachlern gelesen).

**Neue Sprache hinzufügen:** `ui/i18n/<code>.ftl` anlegen (Schlüssel wie `en.ftl`), Eintrag in `LANGS` in `i18n.js`, `node tools/check-i18n.mjs`.

---

## Ursprünglicher Plan (Stand 2026-11-19)
 Ziel: die App (GUI) in diesen Sprachen, in dieser Reihenfolge der Wichtigkeit: **Deutsch, Spanisch, Portugiesisch (Brasilien), Koreanisch, Japanisch, Französisch, Indonesisch, Philippinisch**. Englisch bleibt die Ausgangssprache und der Rückfall.

## Was übersetzt wird und was nicht

| Teil | Übersetzen? | Warum |
|---|---|---|
| **GUI** (Fenster, Dialoge, Tooltips, Statuszeile, Fehlermeldungen der Oberfläche) | **ja, zuerst** | Hier arbeitet der Mensch. |
| **Native Menüleiste** (Rust) | ja | Gehört zur GUI; kommt aus demselben Katalog. |
| **Einstellungen mit Erklärungen** (0x300, 4 GB, Kompression …) | ja, mit Sorgfalt | Erklärtexte sind der schwierigste Teil: Fachbegriffe ohne Wortwörtlichkeit. |
| **Dokumentation für Nutzer** (Kurzanleitung, GUI-Handbuch) | später, Kurzanleitung zuerst | Die Entwicklungsdokumente in `docs/` sind deutsch und bleiben es. |
| **CLI-Meldungen und `--help`** | vorerst nein | Skripte und Fehlersuche verlassen sich auf stabile englische Texte; `--json` bleibt ohnehin sprachfrei. Später möglich über denselben Katalog. |
| **MCP-Werkzeugbeschreibungen** | **nein** | Sie richten sich an KI-Programme, Englisch ist dort der Standard. |
| **Dateinamen und Inhalte der Archive** | nein | Koreanische Pfade sind Daten, keine Oberfläche. |
| **Eigennamen und Fachwörter** (GRF, GPF, Thor, RGZ, zlib, LZMA, Gravity, Ragnarok, SPR/ACT/PAL/GAT/GND/RSW/RSM) | nein | Stehen in allen Sprachen gleich. |

## Wie groß ist die Aufgabe (grob gezählt, nicht genau)
Aus `app.js`, `index.html`, `viewer3d.js` und `lib.rs` per Suchmuster gezählt: etwa **500 bis 700 Texte**. Gezählt wurden Beschriftungen (`text:`, `title:`, `label:`), Dialogtitel, Texte im HTML und 18 Menüeinträge; die Zahl ist eine **Untergrenze**, weil viele Sätze aus Teilen zusammengesetzt werden. Zu bereinigen sind vorher:
- **22 Stellen mit „(s)“** („3 pending change(s)“): braucht echte Mehrzahlformen.
- **etwa 45 Sätze mit eingesetzten Werten** (`${n} files …`): Wortstellung ist in anderen Sprachen anders, deshalb ein Satz mit Platzhaltern statt zusammengesetzter Stücke.
- **Fehlertexte aus dem Rust-Teil** (etwa 85 Stellen im Backend): heute fertige englische Sätze. Der Weg dafür ist mit `Note` schon gelegt (Info-Zeilen laufen so); Fehler (`Result<_, String>`) müssten auf einen Fehlertyp mit Schlüssel umgestellt werden. Besser: stabile **Fehlercodes mit Parametern** (`{code: "name_collision", path: …}`), die die Oberfläche übersetzt. Die rohe Meldung bleibt für das Protokoll.

## Technik (Empfehlung, Entscheidung offen)

**Katalog:** eine Datei je Sprache, **von GUI und Menü gemeinsam genutzt** (`apps/gui/i18n/<sprache>.ftl`), Schlüssel statt Textstellen im Code.

| Format | Für | Gegen |
|---|---|---|
| **Fluent** (`.ftl`, Mozilla) | Mehrzahl und Fälle stehen **in der Übersetzung**, nicht im Code; Weblate und Crowdin lesen es; Rust-Crate und JS-Bündel existieren (Apache-2.0/MIT) | zwei kleine Abhängigkeiten (JS-Bündel einchecken, Rust-Crate); Übersetzer müssen die Syntax lernen |
| **JSON mit ICU-Kurzform** (`{n, plural, one{…} other{…}}`) | keine Abhängigkeit, mit `Intl.PluralRules` in rund 150 Zeilen selbst zu bauen | Grammatik (Geschlecht, Fälle) nur umständlich; eigener Parser, den wir pflegen |
| **gettext** (`.po`) | Standardwerkzeuge, gute Übersetzer-Programme | Rust und JS brauchen je ein Gerät, Platzhalter weniger ausdrucksstark |

**Empfehlung: Fluent** (die Sprachen hier haben sehr verschiedene Mehrzahlregeln: Koreanisch, Japanisch und Indonesisch keine, Französisch 0 und 1 als Einzahl, Philippinisch „one/other“ mit Besonderheiten; das sollen die Übersetzer lösen können, ohne Code zu ändern).

**Ablauf in der App**
- Sprache wählen: Einstellungen → „Language“ (Standard: Systemsprache, `navigator.language`; fällt ohne Katalog auf Englisch zurück, und **je Schlüssel** auf Englisch, wenn ein Text fehlt).
- Das HTML bekommt `data-i18n="schlüssel"`, die Skripte rufen `t("schlüssel", {n})`.
- Zahlen, Daten und Größen über `Intl` (Tausendertrennzeichen, Datum, Mehrzahl); Einheiten „KB/MB/GB“ bleiben.
- Die **native Menüleiste** wird beim Sprachwechsel neu aufgebaut (Tauri kann das zur Laufzeit), aus demselben Katalog.
- Sprachwechsel ohne Neustart; die Auswahl wird gespeichert.

## Besonderheiten je Sprache
| Sprache | Kennung | Zu beachten |
|---|---|---|
| Deutsch | `de` | Texte werden ~30 % länger (Leiste, Schaltflächen, Tooltips); Sie/du festlegen (Empfehlung: **du**, wie in Anwendungen dieser Art üblich) |
| Spanisch | `es` | **Entscheidung nötig:** ein neutrales Lateinamerikanisch/Spanien-verträgliches Spanisch oder `es-ES` und `es-419` getrennt; Empfehlung: eine neutrale Fassung (RO-Spielerschaft ist überwiegend lateinamerikanisch) |
| Portugiesisch | `pt-BR` | Brasilianisch, nicht europäisch; sehr große RO-Gemeinde |
| Koreanisch | `ko` | Spiel stammt aus Korea; **Begriffe der koreanischen Oberfläche des Clients** können als Glossar dienen; keine Mehrzahl; höflich-sachlicher Stil (합니다체) |
| Japanisch | `ja` | keine Mehrzahl, keine Leerzeichen zwischen Wörtern (Umbruch beachten); Schriftart des Systems (Hiragino/Yu Gothic/Noto) |
| Französisch | `fr` | ~20 % länger; geschütztes Leerzeichen vor `: ; ? !` |
| Indonesisch | `id` | keine Mehrzahlformen; Texte ~15 % länger |
| Philippinisch | `fil` | Kennung `fil` (nicht `tl`); Englisch-Tagalog-Mischungen sind bei Technikbegriffen üblich: Fachwörter oft **unübersetzt lassen** und im Glossar festlegen |

Keine Sprache braucht Rechts-nach-links-Schrift. **Schriftarten:** wir nehmen die Systemschrift (`system-ui`), die CJK auf macOS, Windows und gängigem Linux abdeckt; keine Schrift wird eingebaut.

## Qualitätssicherung
1. **Glossar zuerst** (je Sprache 40–60 Begriffe: Archiv, Eintrag, Vorschau, vormerken/staged, Entwurf, Palette, Sprite, Animation, Ebene, Anker, Kompression …). Ohne Glossar werden Übersetzungen uneinheitlich.
2. **Maschinenentwurf plus menschliche Durchsicht:** ich kann je Sprache einen ersten Entwurf liefern; **Muttersprachler müssen prüfen**, besonders die Erklärtexte der Einstellungen und alles zu Verschlüsselung und Datenverlust. Ein Entwurf ohne Prüfung wird in der App als **Beta** gekennzeichnet.
3. **Pseudo-Sprache** (`[Ŧéxt ~~~~]`, verlängert um 40 %, mit Akzenten und CJK-Zeichen) als Test: findet abgeschnittene Texte, überlappende Schaltflächen und Stellen, die nicht über den Katalog laufen. Die Mindestgröße 900 × 640 muss in **jeder** Sprache ohne Überlappung halten (Test wie bisher mit Bildschirmfoto und Messung).
4. **Automatische Prüfung:** gleiche Schlüssel und Platzhalter in allen Katalogen, keine übrig gebliebenen englischen Sätze im Code (Suchmuster in der Prüfung), Katalog-Lücken melden.
5. **Veröffentlichung:** eine Sprache wird erst als „fertig“ geführt, wenn alle Schlüssel übersetzt und von einem Muttersprachler gesehen sind.

## Schritte
| Schritt | Inhalt | Ergebnis |
|---|---|---|
| **L0 Vorbereitung** | Texte aus dem Code auf Schlüssel umstellen, „(s)“ und zusammengesetzte Sätze beseitigen, Fehlercodes im Backend | Englischer Katalog, App verhält sich wie heute |
| **L1 Technik** | Katalog laden, `t()`/`data-i18n`, Einstellung „Language“, Menü aus dem Katalog, Pseudo-Sprache, Prüfskript | Umschalten zwischen Englisch und Pseudo-Sprache |
| **L2 Welle 1: de, es, pt-BR** | Glossar, Entwurf, Durchsicht, Layout-Test | drei Sprachen |
| **L3 Welle 2: ko, ja** | Glossar mit Begriffen der Client-Oberfläche, Schrift- und Umbruchtest | zwei Sprachen |
| **L4 Welle 3: fr, id, fil** | Glossar, Entwurf, Durchsicht | drei Sprachen |
| **L5 Dokumentation** | Kurzanleitung in denselben Sprachen | ein Handbuch je Sprache |

Reihenfolge der Sprachen entspricht deiner Gewichtung; Koreanisch und Japanisch stehen vor Französisch, weil sie die Kerngemeinde dieser Werkzeuge sind.

## Offene Fragen an dich
1. **Format:** Fluent (Empfehlung) oder JSON mit ICU-Kurzform?
2. **Spanisch:** eine neutrale Fassung oder `es-ES` und `es-419` getrennt?
3. **Deutsch:** „du“ (Empfehlung) oder „Sie“?
4. **Muttersprachler:** Wer prüft welche Sprache? Ohne Prüfer führe ich die Sprache als Beta.
5. **Übersetzungsplattform:** Weblate (kostenlos für offene Projekte, setzt ein öffentliches Repository voraus) oder Dateien von Hand? Das hängt davon ab, ob das Projekt später öffentlich wird (bisher nur lokales Git, kein Push).
6. **CLI:** auch übersetzen oder dauerhaft englisch (Empfehlung: englisch)?
