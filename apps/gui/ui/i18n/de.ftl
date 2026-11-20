# RustyGRF – Deutsch (Beta: maschineller Entwurf, noch nicht von Muttersprachlern geprüft). Anrede: du.

tb-main = Hauptleiste
    .aria-label = Hauptleiste
tb-app = App
    .aria-label = App
tb-archive = Archiv
    .aria-label = Archiv
tb-edit = Bearbeiten
    .aria-label = Bearbeiten
tb-edit-mark = Bearbeiten
tb-view = Ansicht und Filter
    .aria-label = Ansicht und Filter

btn-settings = Einstellungen
    .title = Einstellungen (Cmd/Strg+,)
    .aria-label = Einstellungen
btn-open = Öffnen…
    .title = Ein GRF-Archiv öffnen (Cmd/Strg+O)
btn-build = Erstellen…
    .title = Einen Ordner in ein neues GRF packen
btn-merge = Zusammenführen…
    .title = Mehrere GRFs zu einem neuen zusammenführen
btn-extract = Extrahieren…
    .title = Die gewählten Einträge extrahieren, oder alle angezeigten
btn-verify = Prüfen
    .title = Das Archiv auf Fehler prüfen
btn-add = Datei hinzufügen…
    .title = Eine Datei von der Festplatte unter einem Pfad im Archiv hinzufügen
btn-replace = Ersetzen…
    .title = Den gewählten Eintrag durch eine Datei von der Festplatte ersetzen
btn-remove = Entfernen
    .title = Den gewählten Eintrag im gespeicherten Archiv weglassen
btn-undo = Rückgängig
    .title = Die letzte vorgemerkte Änderung rückgängig machen (Cmd/Strg+Z)
btn-redo = Wiederholen
    .title = Wiederholen (Umschalt+Cmd/Strg+Z)
btn-save = Speichern…
    .title = Ausstehende Änderungen ansehen und speichern (Cmd/Strg+S speichert sofort)

search = Suche
    .placeholder = Suche: ein Name oder ein Muster wie *.spr
    .aria-label = Suche

pv-group = Position der Vorschau
    .aria-label = Position der Vorschau
pv-right = Rechts
    .title = Vorschau neben der Liste
    .aria-label = Vorschau rechts
pv-bottom = Unten
    .title = Vorschau unter der Liste
    .aria-label = Vorschau unten
pv-off = Aus
    .title = Vorschau ausblenden (Cmd/Strg+Umschalt+P)
    .aria-label = Vorschau ausblenden

view-group = Listenansicht
    .aria-label = Listenansicht
view-tree = Baum
    .title = Ordnerbaum mit Dateien (Cmd/Strg+1)
view-folders = Ordner
    .title = Links die Ordner, rechts die Dateien des gewählten Ordners (Cmd/Strg+2)
view-list = Liste
    .title = Flache Liste: Name, Ordner, Größe (Cmd/Strg+3)
view-details = Details
    .title = Flache Tabelle mit sortierbaren Spalten (Cmd/Strg+4)
view-gallery = Galerie
    .title = Vorschaubilder von Bildern und Sprites (Cmd/Strg+5)

sort = Sortieren
sort-archive = Gespeicherte Reihenfolge
sort-name = Name
sort-path = Pfad
sort-type = Typ
sort-size = Größe
sort-stored = Gepackte Größe
sort-offset = Position in der Datei
sort-dir = Sortierrichtung
    .title = Aufsteigend oder absteigend
    .aria-label = Sortierrichtung

kind-filter-title = Nur bestimmte Dateiarten anzeigen
kind-label-all = Typ: alle
kind-label-pictures = Typ: Bilder
kind-label-one = Typ: { $kind }
kind-label-n = Typ: { $n } Arten
kind-all = Alle
kind-only-images = Nur Bilder
kind-image = Bilder
kind-sprite = Sprites
kind-animation = Animationen
kind-palette = Paletten
kind-map = Karten
kind-model = Modelle
kind-audio = Audio
kind-text = Text und Lua
kind-effect = Effekte
kind-file = Sonstige

size = Größe
size-any = beliebig
size-over-1k = über 1 KB
size-over-100k = über 100 KB
size-over-1m = über 1 MB
size-over-10m = über 10 MB
size-under-1k = unter 1 KB
size-under-100k = unter 100 KB
size-under-1m = unter 1 MB

subfolders = Unterordner
    .title = Auch die Dateien der Unterordner auflisten
expand-all = Alle öffnen
    .title = Jeden Ordner öffnen
collapse-all = Alle schließen
    .title = Jeden Ordner schließen
reset-filters = Zurücksetzen
    .title = Suche, Typ- und Größenfilter löschen

empty-hint = Öffne ein GRF-Archiv oder ziehe es auf dieses Fenster.
open-archive = Archiv öffnen…
folders-pane = Ordner
    .aria-label = Ordner
all-files = Alle Dateien
    .title = Die Dateien aller Ordner anzeigen
col-name = Name
col-type = Typ
col-size = Größe
col-stored = Gepackt
entries-list = Einträge
    .aria-label = Einträge
preview-pane = Vorschau
    .aria-label = Vorschau
split-folders = Größe der Ordnerliste ändern
    .aria-label = Größe der Ordnerliste ändern
split-preview = Größe der Vorschau ändern (Pfeiltasten)
    .aria-label = Größe der Vorschau ändern (Pfeiltasten)
    .title = Ziehen, oder fokussieren und die Pfeiltasten benutzen

status-ready = Bereit
count-entries = { $n ->
    [one] { $n } Eintrag
   *[other] { $n } Einträge
 }
count-filtered = { $n } von { $total }
count-note-folders = (die Suche umfasst alle Ordner)
count-note-pictures = (Bilder und Sprites)
names = Namen
    .title = Wie Dateinamen angezeigt und gesucht werden. Beim Extrahieren werden immer Latin-1-Namen geschrieben, damit das Ergebnis zu `rgrf build` passt.
names-korean = Koreanisch

save-n = { $n ->
    [one] Die { $n } ausstehende Änderung ansehen und speichern
   *[other] Die { $n } ausstehenden Änderungen ansehen und speichern
 }
save-none = Eine Kopie mit anderen Einstellungen speichern
save-aria-n = { $n ->
    [one] { $n } ausstehende Änderung speichern
   *[other] { $n } ausstehende Änderungen speichern
 }
save-aria-none = Änderungen speichern

dlg-ok = OK
dlg-cancel = Abbrechen
dlg-close = Schließen
dlg-done = Fertig
dlg-quick = Schnell
dlg-full = Vollständig
dlg-edit-animation = Animation bearbeiten
dlg-edit-pixels = Pixel bearbeiten
dlg-edit-text = Text bearbeiten

verify-title = Prüfen
verify-body = Die schnelle Prüfung liest nur die Dateitabelle. Die vollständige Prüfung entpackt außerdem jeden Eintrag und liest dafür das ganze Archiv.
verify-running = Prüfe …
verify-status = Geprüft: { $errors } Fehler, { $warnings } Warnungen
verify-summary-full = { $entries } Einträge, alle entpackt: { $errors } Fehler, { $warnings } Warnungen, { $infos } Hinweise.
verify-summary-quick = { $entries } Einträge, nur die Tabelle: { $errors } Fehler, { $warnings } Warnungen, { $infos } Hinweise.
verify-problems = Probleme gefunden
verify-ok = Archiv geprüft
verify-failed = Prüfung fehlgeschlagen

set-title = Einstellungen
    .aria-label = Einstellungen
set-appearance = Darstellung
set-theme = Erscheinungsbild
set-theme-system = Dem System folgen
set-theme-light = Hell
set-theme-dark = Dunkel
set-accent = Akzentfarbe
set-accent-orange = Orange
set-accent-blue = Blau
set-accent-green = Grün
set-accent-purple = Violett
set-textsize = Textgröße
set-small = Klein
set-normal = Normal
set-large = Groß
set-density = Listendichte
set-compact = Kompakt
set-comfortable = Luftig
set-fileicons = Dateisymbole
set-plainicons = Einfarbige Symbole (keine Farben nach Dateityp)
set-animations = Animationen
set-animations-hint = Schaltet den Pfeil und andere Übergänge aus.
set-motion-off = Aus
set-language = Sprache
set-language-system = Dem System folgen
set-language-beta = { $name } (Beta)
set-language-note = Mit Beta gekennzeichnete Übersetzungen sind maschinelle Entwürfe, die noch kein Muttersprachler geprüft hat. Noch nicht übersetzte Texte erscheinen auf Englisch.
set-opening = Beim Öffnen eines Archivs
set-names = Dateinamen
set-names-cp949 = Koreanisch (CP949)
set-names-cp1252 = Latin-1 (wie GRF Editor sie zeigt)
set-names-utf8 = UTF-8
set-layout = Listenansicht
set-as-last = Wie zuletzt benutzt
set-next-start = Gilt ab dem nächsten Start der App.
set-preview = Vorschau
set-startup = Start
set-reopen = Das zuletzt geöffnete Archiv wieder öffnen
set-changes = Änderungen
set-warn-discard = Nachfragen, bevor ausstehende Änderungen verworfen werden
set-building = Archive erstellen
set-compression = Kompression
set-format = Format
set-building-note = Das sind nur die Startwerte des Dialogs „Erstellen…“; dort kannst du sie für jedes Archiv ändern.
set-stored-note = Die Einstellungen werden nur in dieser App gespeichert. Tastenkürzel: { $key }+, öffnet dieses Fenster.
set-reset = Auf Standard zurücksetzen

method-zlib = zlib (jeder Client)
    .hint = Der Standard. Jeder Client kann es lesen, und es wird schnell erstellt. Nimm es, wenn du keinen Grund für etwas anderes hast.
method-zopfli = zopfli (kleiner, langsam)
    .hint = Schreibt dasselbe zlib-Format, jeder Client kann es also weiterhin lesen, presst die Dateien aber etwas stärker. Das Erstellen dauert viel länger; das Lesen ist genauso schnell.
method-lzma = LZMA (braucht Client-Unterstützung)
    .hint = Kleiner als zlib, aber nur ein Client mit LZMA-Unterstützung kann es lesen (zum Beispiel einer mit einer cps.dll). Probiere das Archiv in deinem Client aus, bevor du es weitergibst.
format-0x200 = 0x200 (jeder Client)
    .hint = Das Standardformat von GRF. Jeder Client liest es. Dateipositionen sind 32 Bit groß, das Archiv kann also nicht über 4 GB wachsen.
format-0x300 = 0x300 (über 4 GB)
    .hint = Neueres Format mit 64-Bit-Dateipositionen, das Archiv kann also größer als 4 GB sein. Nur neuere Clients lesen es (gemessen: ein Client von 2025 ja, Clients von 2020 nein). Nimm es nur, wenn 4 GB nicht reichen.
format-0x103 = 0x103 (sehr alte Clients)
    .hint = Sehr altes Format aus der Zeit vor 0x200: Die Dateitabelle ist nicht komprimiert, Namen und Daten sind mit der alten DES-Variante verschlüsselt. Für Clients, die zu alt für 0x200 sind. Gleiche 4-GB-Grenze.
format-0x102 = 0x102 (sehr alte Clients)
    .hint = Wie 0x103 (sie unterscheiden sich nur in der Versionsnummer). Nimm es, wenn ein alter Client genau diese Version verlangt.

menu-file = Datei
menu-edit = Bearbeiten
menu-view = Ansicht
menu-window = Fenster
menu-help = Hilfe
menu-settings = Einstellungen...
menu-open = Öffnen...
menu-build = Aus Ordner erstellen...
menu-merge = Zusammenführen...
menu-extract = Extrahieren...
menu-verify = Archiv prüfen
menu-reduce-images = Bilder verkleinern...
menu-add = Datei hinzufügen...
menu-replace = Ersetzen...
menu-remove = Entfernen
menu-layout-tree = Als Baum
menu-layout-folders = Als Ordner
menu-layout-details = Als Details
menu-layout-gallery = Als Galerie
menu-preview-right = Vorschau rechts
menu-preview-bottom = Vorschau unten
menu-preview-off = Vorschau ausblenden

## Dialogs
dlg-choose-target = Ziel wählen…
dlg-open-it = Öffnen
dlg-add = Hinzufügen
job-starting = Starte …
job-cancelling = Breche ab …
job-progress = { $done } von { $total }
nothing-written = Es wurde nichts geschrieben.
verified-yes = geprüft
verified-no = nicht geprüft
build-pick = Ordner zum Packen (der Ordner, der data\ enthält …)
build-title = Archiv erstellen
build-pack = { $src } packen
build-share = Identische Dateien teilen
build-share-hint = Speichert Dateien mit gleichem Inhalt nur einmal, das spart Platz. In 0x102 und 0x103 nicht möglich.
build-verify-note = Das fertige Archiv wird mit den Dateien auf der Festplatte verglichen.
build-save-as = Archiv speichern unter
build-running = Archiv wird erstellt
build-status = Erstelle { $dst } …
build-done-status = { $dst } erstellt: { $files } Dateien
build-result = Dateien: { $files } in { $blocks } Blöcken ({ $from } → { $to }), { $verified }.
build-shared = Identische Dateien, die sich einen Block teilen: { $n }.
build-skipped = Übersprungene Elemente: { $n }
build-done-title = Archiv erstellt
build-cancelled = Erstellen abgebrochen
build-failed = Erstellen fehlgeschlagen
build-could-not = Erstellen nicht möglich
merge-pick = Zusammenzuführende Archive (mindestens zwei)
merge-need-two = Wähle mindestens zwei Archive.
merge-title = Archive zusammenführen
merge-higher = Höhere Priorität
merge-lower = Niedrigere Priorität
merge-intro = Reihenfolge = Priorität: Bei einem Pfad, der in mehreren Archiven vorkommt, gewinnt das oberste. Die Eingaben werden nicht verändert.
merge-replace-only = Nur ersetzen: Das letzte Archiv ist die Basis, die anderen fügen keine neuen Pfade hinzu
merge-save-as = Zusammengeführtes Archiv speichern unter
merge-running = Archive werden zusammengeführt
merge-status = Führe zusammen nach { $dst } …
merge-done-status = Zusammengeführt nach { $dst }: { $files } Dateien
merge-line = Dateien: { $files }, verwendet: { $used }, ersetzt: { $replaced }
merge-line-ignored = , neue Pfade ignoriert: { $n }
merge-result = Dateien: { $files }, { $size }, { $verified }.
merge-damaged = Beschädigte Blöcke, unverändert kopiert: { $n }.
merge-done-title = Archive zusammengeführt
merge-cancelled = Zusammenführen abgebrochen
merge-failed = Zusammenführen fehlgeschlagen
merge-could-not = Zusammenführen nicht möglich
ext-title = Extrahieren
ext-folder = Ordner ({ $n } Dateien)
ext-entry = Gewählter Eintrag
ext-all = Alle { $n } in der Liste
ext-filtered = Die Liste ist gefiltert; „alle“ meint die passenden Einträge.
ext-whole = Die Liste zeigt das ganze Archiv.
ext-dest = In Ordner extrahieren
ext-empty-folder = Dieser Ordner enthält keine Dateien.
ext-running = Extrahiere …
ext-done-status = { $files } Datei(en) nach { $dest } extrahiert
ext-done-title = Extrahiert
ext-done-body = { $files } Datei(en) nach { $dest } geschrieben.
ext-damaged = { $n } Einträge waren beschädigt; ihr rettbarer Teil wurde geschrieben.
ext-stopped = Extrahieren gestoppt
ext-could-not = Extrahieren nicht möglich
ext-rename = Neue Dateien umbenennen
replace-pick = Datei, die { $name } ersetzt
replace-status = Ersatz für { $name } vorgemerkt; speichere ihn mit „Speichern…“
replace-failed = Ersatz konnte nicht vorgemerkt werden
add-pick = Hinzuzufügende Datei
add-title = Datei hinzufügen
add-prompt = Pfad im Archiv (ein vorhandener Pfad wird ersetzt):
add-status = { $path } vorgemerkt; speichere es mit „Speichern…“
add-failed = Datei konnte nicht vorgemerkt werden
remove-title = Aus dem gespeicherten Archiv entfernen
remove-body = Jetzt wird nichts gelöscht: Die Einträge werden weggelassen, wenn du die Änderungen als neues Archiv speicherst.
remove-status = Zum Entfernen vorgemerkte Einträge: { $n }
remove-failed = Entfernen konnte nicht vorgemerkt werden
changes-title = Änderungen
changes-none = Keine ausstehenden Änderungen. Nutze „Ersetzen…“, „Datei hinzufügen…“ oder „Entfernen“; das geöffnete Archiv wird nie verändert, das Ergebnis wird als neues Archiv gespeichert.
changes-heading = { $n ->
    [one] { $n } ausstehende Änderung
   *[other] { $n } ausstehende Änderungen
 }
changes-more = … und { $n } weitere
changes-save = Als neues Archiv speichern…
changes-discard = Alle verwerfen
changes-discarded = Änderungen verworfen
save-pick = Geändertes Archiv speichern unter
save-running = Archiv wird gespeichert
save-status = Speichere { $dst } …
save-done-status = { $dst } gespeichert
save-result = Dateien: { $files }, { $size }, { $verified }: hinzugefügt { $added }, ersetzt { $replaced }, entfernt { $removed }.
save-note = Das Originalarchiv wurde nicht verändert. Die ausstehenden Änderungen bleiben, bis du sie verwirfst.
save-done-title = Archiv gespeichert
save-cancelled = Speichern abgebrochen
save-failed = Speichern fehlgeschlagen
save-could-not = Speichern nicht möglich
history-undone = Änderung rückgängig gemacht
history-redone = Änderung wiederholt
history-failed = Verlauf konnte nicht geändert werden

hex-title = Bytes von { $name }
hex-intro = Tippe zwei Hex-Ziffern, um ein Byte zu ändern. Die Dateigröße bleibt gleich; Änderungen werden wie alle anderen vorgemerkt und lassen sich rückgängig machen.
hex-bytes-aria = Bytes der Datei
hex-goto-ph = Offset, z. B. 0x400
hex-goto-aria = Zu Offset springen
hex-prev = ◀ Zurück
hex-next = Weiter ▶
hex-byte-aria = Byte bei Offset { $at }
hex-count = { $n ->
    [one] { $n } Byte geändert
   *[other] { $n } Bytes geändert
 }
hex-where = Offset { $from } – { $to } von { $size } ({ $human })
hex-apply = Änderungen übernehmen
hex-cannot = Bytes können nicht angezeigt werden
hex-status = Geänderte Bytes in { $name } (vorgemerkt): { $n }
hex-failed = Bytes konnten nicht geändert werden
hex-find-mode = Suchart
hex-mode-hex = Hex-Bytes
hex-mode-text = Text
hex-find-ph = Suche, z. B. 4D 5A
hex-find-btn = Weitersuchen
hex-found = Gefunden bei Offset { $at }
hex-not-found = Nicht gefunden
hex-bad-pattern = Gib Hex-Ziffern paarweise ein, z. B. 4D 5A
reduce-title = Bilder verkleinern
reduce-level-45 = 45 dB: beste Qualität, geringste Ersparnis
reduce-level-40 = 40 dB: kaum zu unterscheiden (empfohlen)
reduce-level-38 = 38 dB: kleiner, leichter Verlust in Verläufen
reduce-level-35 = 35 dB: am kleinsten, Verläufe können Streifen zeigen
reduce-intro = Schreibt eine Kopie dieses Archivs, in der jede True-Color-BMP mit 256 Farben gespeichert wird, sofern das Bild die unten gewählte Qualität hält und die gespeicherte Datei kleiner wird. Das geöffnete Archiv wird nicht verändert. Der Client lädt 8-Bit-BMPs (viele seiner eigenen sind welche).
psnr-hint = PSNR misst, wie weit sich die Farben verschoben haben: ab 40 dB ist der Unterschied kaum zu sehen, unter 35 dB zeigen glatte Verläufe Streifen.
reduce-notes = Magenta (durchsichtig) bleibt exakt, und keine andere Farbe wird durchsichtig. Bilder mit höchstens 256 Farben verlieren nichts.
reduce-quality = Niedrigste Qualität
reduce-pending = Die { $n } ausstehenden Änderungen werden auch in die Kopie geschrieben; diese Einträge werden nicht verkleinert.
reduce-save-as = Verkleinertes Archiv speichern unter
reduce-running = Bilder werden verkleinert
reduce-status = Verkleinere Bilder nach { $dst } …
reduce-why-already_8_bit = schon 8 Bit
reduce-why-below_min_psnr = unter der Qualität
reduce-why-not_smaller = gespeichert nicht kleiner
reduce-why-noisy_transparency = Rauschen um Magenta
reduce-why-alpha = mit Alpha
reduce-why-unreadable = nicht lesbar
reduce-summary = { $reduced } von { $matched } BMPs verkleinert ({ $lossless } verlustfrei{ $worst }), { $saved } kleiner.
reduce-worst = , schlechtester Wert { $db } dB
reduce-kept = Unverändert geblieben: { $list }.
reduce-nothing = Nichts zu verkleinern
reduce-files = Dateien: { $files }, { $size }, { $verified }.
reduce-done-status = Verkleinert nach { $dst }: { $saved } kleiner
reduce-done-title = Bilder verkleinert
reduce-cancelled = Verkleinern abgebrochen
reduce-failed = Verkleinern fehlgeschlagen
reduce-could-not = Verkleinern nicht möglich
reduce1-btn = 8 Bit…
reduce1-btn-title = Dieses Bild mit 256 Farben speichern (kleiner; Magenta bleibt durchsichtig). Zeigt vorher beide zum Vergleich (vorgemerkt)
reduce1-cannot = Dieses Bild kann nicht verkleinert werden
reduce1-title = Auf 8 Bit verkleinern?
reduce1-lossless = verlustfrei: Das Bild hat höchstens 256 Farben
reduce1-psnr = PSNR { $db } dB
reduce1-info = { $before } → { $after } entpackt, { $colors } Farben, { $quality }. Magenta (durchsichtig) bleibt exakt.
reduce1-original = Original
reduce1-8bit = 8 Bit
reduce1-go = Verkleinern
reduce1-status = { $name } auf 8 Bit verkleinert (vorgemerkt)

pvw-select = Wähle einen Eintrag für die Vorschau.
pvw-loading = Lade …
pvw-meta = { $size } entpackt, { $stored } gespeichert ({ $codec }), Offset { $offset }
pvw-staged-removed = vorgemerkt: entfernt, angezeigt wie beim Speichern
pvw-staged-added = vorgemerkt: hinzugefügt, angezeigt wie beim Speichern
pvw-staged-changed = vorgemerkt: geändert, angezeigt wie beim Speichern
pvw-hex-title = Jedes Byte dieser Datei anzeigen und einzelne Bytes ändern (vorgemerkt)
pvw-files = { $n ->
    [one] { $n } Datei
   *[other] { $n } Dateien
 }
pvw-frames = { $n ->
    [one] { $n } Frame
   *[other] { $n } Frames
 }
pvw-folder-meta = Ordner — { $files }, { $size } entpackt
pvw-open = Öffnen
pvw-folder-note = „Extrahieren…“ und „Entfernen“ wirken auf diesen Ordner, wenn er gewählt ist.
pvw-action = Aktion
pvw-action-n = Aktion { $n }
pvw-anim-meta = { $frames }, je { $ms } ms
pvw-anim-edit = Frames bearbeiten…
pvw-anim-edit-title = Ebenen, Anker und Frames dieser Animation bearbeiten (vorgemerkt)
pvw-anim-interval = Intervall setzen…
pvw-anim-interval-title = Die Frame-Zeit dieser Aktion ändern (vorgemerkt, wird mit „Speichern…“ gesichert)
pvw-warning = Warnung: { $note }
pvw-model-hint = { $n } Dreiecke — ziehen zum Drehen, Mausrad zum Zoomen, Doppelklick zum Zurücksetzen
pvw-lighting = Beleuchtung
pvw-lightmaps = Lightmaps
pvw-models = Modelle
pvw-water = Wasser
pvw-wireframe = Drahtgitter
pvw-animate = Animieren
pvw-all-title = Beleuchtung, Lightmaps, Modelle und Wasser zusammen ein- oder ausblenden
pvw-hide-all = Alle ausblenden
pvw-show-all = Alle einblenden
pvw-reset-view = Ansicht zurücksetzen
pvw-reset-view-title = Zurück zur Startansicht (auch: Doppelklick auf das Bild)
pvw-save-picture = Bild speichern…
pvw-save-picture-title = Die Ansicht so, wie sie jetzt ist (mit dem Ausgeschalteten), als PNG speichern, größer als auf dem Bildschirm
pvw-extract-map = Karte extrahieren…
pvw-extract-map-title = Diese Karte mit den benötigten Dateien in einen Ordner schreiben; hier ausgeschaltete Teile lassen sich weglassen
pvw-map-hint = Ziehen zum Drehen, mit rechter Maustaste oder Umschalt-Ziehen oder Pfeiltasten verschieben, Mausrad zum Zoomen, Doppelklick zum Zurücksetzen.
pvw-effect = { $frames } angezeigt mit { $fps } fps
pvw-video-open = Mit dem System-Player öffnen
pvw-video-open-title = Die Datei in einen temporären Ordner schreiben und mit dem Programm öffnen, das dein System dafür hat
pvw-video-opened = { $path } geöffnet. Falls nichts startete, hat dein System keinen Player für dieses Format (VLC und RAD Video Tools spielen Bink ab).
pvw-video-info = Bink-Video, { $w } × { $h }, { $frames } mit { $fps } fps ({ $secs } s), { $size }.
pvw-video-note = Dieses Fenster kann Bink, ein eigenes Format, nicht abspielen. Es bleibt unverändert im Archiv.
pvw-hex-head = { $n } Bytes, die ersten 512 angezeigt
sp-cannot = Bild kann nicht gespeichert werden
sp-not-ready = Die 3D-Ansicht ist noch nicht bereit.
sp-scale-1 = Wie auf dem Bildschirm
sp-scale-2 = 2 × (empfohlen)
sp-scale-4 = 4 ×
sp-title = Bild speichern
sp-intro = Die Ansicht, wie sie jetzt ist: gleicher Winkel und nur das Eingeschaltete. Ein größeres Bild wird in dieser Größe neu gezeichnet, nicht vergrößert.
sp-size = Größe
sp-choose-file = Datei wählen…
sp-save-as = Bild speichern unter
sp-filter = PNG-Bilder
sp-status = Bild gespeichert: { $path } ({ $w } × { $h }, { $size })
sp-failed = Bild konnte nicht gespeichert werden
em-part-models = Modelle und ihre Texturen
em-part-water = Wasserbilder
em-part-sounds = Sounds
em-part-minimap = Minikarte
em-part-effects = Effektskript und seine Texturen
em-title = Karte extrahieren
em-intro = Welt-, Boden- und Höhendateien sowie die Bodentexturen werden immer geschrieben. In der Ansicht ausgeschaltete Teile sind hier zunächst abgewählt. Weggelassene Dateien fehlen nur im Ordner; die Karte nennt sie weiter, außer du nimmst sie unten heraus.
em-choose-folder = Ordner wählen…
em-dest = Ordner für die Karte (die Dateien kommen darunter als data\...)
em-running = Karte wird extrahiert …
em-exists-title = Dateien sind schon vorhanden
em-exists-body = { $error }. Beide behalten, mit einer Nummer an den neuen Dateinamen?
em-keep-both = Beide behalten
em-nothing = Nichts extrahiert
em-status = Karte { $map }: { $files } nach { $dest } geschrieben
em-body = { $files } von { $map } nach { $dest } geschrieben.
em-skipped = { $files } weggelassen ({ $parts }).
em-missing = { $files }, die die Karte nennt, sind nicht in diesem Archiv (vielleicht in einer anderen GRF des Clients):
em-damaged = { $files } sind beschädigt und wurden so weit geschrieben, wie sie lesbar waren.
em-done-gaps = Karte extrahiert, mit Lücken
em-done = Karte extrahiert
em-failed = Karte konnte nicht extrahiert werden

open-status = { $name }: { $format }, { $files }, { $size }

act-title = Animation bearbeiten: { $name }
act-frame-label = Frame { $n } / { $total }
act-play = Abspielen
act-stop = Stopp
act-play-title = Die Aktion mit ihrer Frame-Zeit abspielen, mit den Sounds ihrer Frames
act-ms = ms/Frame
act-copy-frame = Frame kopieren
act-copy-frame-title = Eine Kopie nach diesem Frame einfügen
act-inbetween = Zwischenbilder…
act-inbetween-title = Frames hinzufügen, die weich von einem Frame zum nächsten überblenden
act-frame-earlier = Frame ←
act-frame-earlier-title = Den Frame eine Stelle nach vorn schieben
act-frame-later = Frame →
act-frame-later-title = Den Frame eine Stelle nach hinten schieben
act-copy-action = Aktion kopieren
act-copy-action-title = Eine Kopie dieser Aktion danach einfügen
act-action-up = Aktion ↑
act-action-up-title = Die Aktion eine Stelle nach vorn schieben
act-action-down = Aktion ↓
act-action-down-title = Die Aktion eine Stelle nach hinten schieben
act-remove-action = Aktion entfernen
act-remove-frame = Frame entfernen
act-stage-role = Frame-Editor
act-stage-aria = Frame { $n }. Klicke eine Ebene an, um sie zu wählen, und ziehe sie zum Verschieben; die Pfeiltasten verschieben die gewählte Ebene um 1 Pixel, mit Umschalt um 10.
act-stage-sel = Ebene { $n } ist gewählt.
act-anchor-title = Anker { $n }: zum Verschieben ziehen
act-no-picture = Kein Bild: { $error }
act-stage-note = Die Linien markieren den Ursprung; rote Punkte sind Anker. Ebenen werden der Reihe nach gezeichnet, die letzte oben.
act-th-type = Typ
act-th-image = Bild
act-th-mirror = Spiegeln
act-th-scale = Skalierung
act-th-scale-xy = Skalierung x / y
act-th-rot = Drehung
act-th-colour = Farbe
act-type-palette = Palette ({ $n })
act-type-rgba = Echtfarbe ({ $n })
act-layer-select = Diese Ebene wählen (auf dem Bild ziehen oder dort die Pfeiltasten benutzen)
act-layer-earlier = Früher zeichnen (darunter)
act-layer-later = Später zeichnen (darüber)
act-layer-remove = Die Ebene entfernen
act-add-layer = + Ebene
act-anchors-heading = Anker (wo andere Sprites ansetzen, z. B. Kopfbedeckungen)
act-add-anchor = + Anker
act-no-anchors = ACT { $version } hat keine Anker.
act-sound = Sound
act-sound-none = keiner
act-sound-title = Sounddatei, die mit diesem Frame abgespielt wird, z. B. effect\hit.wav (leer = keine)
act-sound-play-title = Diesen Sound abspielen (data\wav\ im Archiv)
act-sound-play-aria = Den Sound dieses Frames abspielen
act-interval-title = Intervall der Aktion { $n }
act-interval-prompt = Zeit pro Frame in Millisekunden (die Datei speichert Vielfache von 25 ms):
act-interval-set = Setzen
act-interval-status = Intervall der Aktion { $n } vorgemerkt; mit „Speichern…“ sichern.
act-interval-failed = Intervall konnte nicht gesetzt werden
act-remove-title = Aktion { $n } entfernen?
act-remove-body = Spätere Aktionen rücken um eins nach. Der Client wählt Aktionen nach Nummer (Richtung, Zustand), andere Dateien, die sie nutzen, können dadurch ihre Bedeutung ändern.
ip-title = Zwischenbilder hinzufügen
ip-intro = Fügt Frames hinzu, die Position, Skalierung, Drehung, Farbe und Anker in Schritten von einem Frame zum nächsten bewegen. Das Bild und die Spiegelung wechseln zur Hälfte, Sounds bleiben bei den Originalframes.
ip-count = Zwischenframes je Paar
ip-whole = Ganze Aktion (alle { $frames } Frames), nicht nur Frame { $frame } und der nächste
ip-smooth = Weich: am Anfang und Ende jedes Schritts langsam
ip-loop = Schleife: Der letzte Frame blendet auch zurück in den ersten
ip-keep = Tempo behalten: Die Frame-Zeit verkürzen, damit die Aktion gleich lang dauert (nur ganze Aktion)
ip-last = Das ist der letzte Frame: Ohne „Schleife“ oder „Ganze Aktion“ gibt es keinen nächsten Frame zum Überblenden.
ip-go = Frames hinzufügen
ref-title = Referenzanimation hinzufügen
ref-intro = Eine weitere Animation dieses Archivs, die mit der bearbeiteten gezeichnet wird, z. B. ein Körper und ein Kopf, um Kopfbedeckungen anzupassen. Ihr erster Anker trifft den ersten Anker des bearbeiteten Frames, so wie der Client Teile am Körper befestigt. Gleiche Aktions- und Framenummer; die Referenz wird nur angezeigt, nie verändert.
ref-animation = Animation (.act)
ref-behind-check = Hinter der bearbeiteten Animation
ref-preset-body-m = Körper, Anfänger (männlich)
ref-preset-body-f = Körper, Anfänger (weiblich)
ref-preset-head-m = Kopf 1 (männlich)
ref-preset-head-f = Kopf 1 (weiblich)
ref-label = Referenzen
ref-add-btn = Referenz hinzufügen…
ref-behind = Hinten
ref-front = Vorn
ref-toggle-title = Die Referenz hinter oder vor der bearbeiteten Animation zeichnen
ref-remove-aria = Die Referenz { $path } entfernen
te-title = { $name } bearbeiten
te-partial = Teile dieser Datei ließen sich nicht dekompilieren und bleiben als Auflistung; sie lassen sich nicht wieder kompilieren. Bearbeite nur, was der Text als Lua zeigt.
te-compile = Kompilieren und vormerken
te-stage = Vormerken
te-compiling = Kompiliere …
te-staging = Merke vor …
te-status = { $name } vorgemerkt. Mit „Speichern…“ sichern.
te-status-compiled = { $name } vorgemerkt (neu kompiliert). Mit „Speichern…“ sichern.

enc-title = Verschlüsseltes Archiv
enc-body = Dieses Archiv ist verschlüsselt (Verfahren von GRF Editor). Gib sein Passwort ein, um die Dateien zu lesen.
enc-wrong = Dieses Passwort gehört nicht zum Archiv.
enc-skip = Ohne Passwort öffnen
enc-needs = Die Dateitabelle dieses Archivs ist verschlüsselt; zum Öffnen wird ein Passwort gebraucht.
open-another-title = Anderes Archiv öffnen?
open-another-body = { $n ->
    [one] { $n } ausstehende Änderung des geöffneten Archivs ist noch in keinem Archiv gespeichert. Sie wird als Entwurf aufbewahrt und beim nächsten Öffnen dieses Archivs wieder angeboten.
   *[other] { $n } ausstehende Änderungen des geöffneten Archivs sind noch in keinem Archiv gespeichert. Sie werden als Entwurf aufbewahrt und beim nächsten Öffnen dieses Archivs wieder angeboten.
 }
open-running = Öffne { $path } …
open-failed-status = Archiv konnte nicht geöffnet werden
open-failed = Öffnen nicht möglich
draft-title = Nicht gespeicherte Änderungen wiederherstellen?
draft-body = { $n ->
    [one] { $n } Änderung vom { $when } wurde nicht in einem Archiv gespeichert. Sie wurde als Entwurf aufbewahrt (zum Beispiel, weil die App geschlossen wurde).
   *[other] { $n } Änderungen vom { $when } wurden nicht in einem Archiv gespeichert. Sie wurden als Entwurf aufbewahrt (zum Beispiel, weil die App geschlossen wurde).
 }
draft-archive-changed = Das Archiv selbst hat sich seitdem geändert. Prüfe die Änderungen, bevor du speicherst.
draft-missing = { $n ->
    [one] { $n } Ersatzdatei existiert nicht mehr auf der Festplatte und wird weggelassen.
   *[other] { $n } Ersatzdateien existieren nicht mehr auf der Festplatte und werden weggelassen.
 }
draft-more = { $n ->
    [one] { $n } älterer Entwurf dieses Archivs folgt, einer bei jedem Öffnen.
   *[other] { $n } ältere Entwürfe dieses Archivs folgen, einer bei jedem Öffnen.
 }
draft-later-note = „Später“ hebt diesen Entwurf für ein andermal auf; die Änderungen, die du jetzt machst, werden als eigener Entwurf gesichert.
draft-restore = Wiederherstellen
draft-discard = Verwerfen
draft-later = Später
draft-restored = { $n ->
    [one] { $n } Änderung wiederhergestellt
   *[other] { $n } Änderungen wiederhergestellt
 }
draft-restored-skipped = { $restored }, { $skipped } weggelassen
draft-discarded = Entwurf verworfen
draft-kept = Der Entwurf bleibt für ein andermal erhalten
draft-failed = Änderungen konnten nicht wiederhergestellt werden
txt-encoding = Kodierung:
txt-shortened = (gekürzt)
txt-edit = Bearbeiten…
txt-edit-title = Den Text bearbeiten; kompiliertes Lua wird neu kompiliert (vorgemerkt, wird mit „Speichern…“ gesichert)
pal-adjust = Anpassen…
pal-adjust-title = Farbton verschieben sowie Sättigung und Helligkeit eines Farbbereichs skalieren (vorgemerkt)
pal-click-hint = Klicke eine Farbe an, um sie zu ändern.
pal-click-title = Klicke eine Farbe an, um sie zu ändern
pal-colour = Farbe { $n }
pal-zero = Index 0 ist im Client die durchsichtige Farbe; ihre Farbe ist selten zu sehen.
pal-number = Farbnummer { $n } von 255:
pal-changed = Farbe { $n } von { $name } geändert (vorgemerkt); mit „Speichern…“ sichern.
pal-read-failed = Palette konnte nicht gelesen werden
pal-adjust-heading = Farben von { $name } anpassen
pal-range-note = Farbe 0 ist die durchsichtige, der Bereich beginnt daher bei 1.
pal-from = Von Farbe
pal-to = bis
pal-hue = Farbtonverschiebung (°)
pal-sat = Sättigung ×
pal-bri = Helligkeit ×
dlg-apply = Anwenden
pal-adjusted = Farben von { $name } angepasst (vorgemerkt); mit „Speichern…“ sichern.
pal-failed = Palette konnte nicht geändert werden
pal-pick = Palette für { $name }
pal-filter = Palette
pal-set-status = Palette von { $name } geändert (vorgemerkt); mit „Speichern…“ sichern.
pal-set-failed = Palette konnte nicht gesetzt werden
spr-add = Bild hinzufügen…
spr-add-title = Ein PNG als neues Bild anhängen (vorgemerkt)
spr-setpal = Palette setzen…
spr-setpal-title = Die Farben einer .pal-Datei verwenden (vorgemerkt)
spr-pixels = Pixel…
spr-pixels-title = Auf diesem Bild zeichnen: Stift, Radierer, Füllen und Pipette mit den Palettenfarben (vorgemerkt)
spr-replace = Ersetzen…
spr-replace-title = Dieses Bild durch ein PNG ersetzen (vorgemerkt, wird mit „Speichern…“ gesichert)
spr-remove-title = Dieses Bild entfernen; die Animation daneben folgt (vorgemerkt)
spr-replace-pick = PNG für Bild { $frame } von { $name }
spr-png-filter = PNG-Bild
spr-replace-status = Bild { $frame } von { $name } vorgemerkt (Farben werden auf die Palette des Sprites abgebildet); die Vorschau zeigt das Ergebnis. Mit „Speichern…“ sichern.
spr-replace-failed = Bild konnte nicht ersetzt werden
spr-add-pick = PNG, das zu { $name } hinzugefügt wird
spr-add-status = Bild zu { $name } hinzugefügt (vorgemerkt); mit „Speichern…“ sichern.
spr-add-failed = Bild konnte nicht hinzugefügt werden
spr-remove-heading = Bild { $n } entfernen?
spr-remove-body = Spätere Bilder rücken um eins nach. Ebenen der Animation neben diesem Sprite, die das Bild nutzen, werden leer, spätere folgen nach.
spr-remove-status = Bild { $frame } von { $name } entfernt (vorgemerkt); mit „Speichern…“ sichern.
spr-remove-failed = Bild konnte nicht entfernt werden
pix-error = Pixel
pix-canvas-aria = Bild { $frame }, { $w } mal { $h } Pixel. Pfeiltasten bewegen den Cursor, die Leertaste zeichnet.
pix-size = { $w } x { $h } px
pix-readout = { $w } x { $h } px — x { $x }, y { $y }: Farbe { $c } (#{ $hex })
pix-readout-t = { $w } x { $h } px — x { $x }, y { $y }: Farbe { $c } (#{ $hex }), durchsichtig
pix-pencil = Stift
pix-eraser = Radierer
pix-fill = Füllen
pix-picker = Pipette
pix-zoom = Zoom
pix-grid = Raster
pix-revert = Zurücksetzen
pix-palette = Palette
pix-palette-heading = Palette (Farbe anklicken; 0 ist durchsichtig)
pix-swatch-zero =  (durchsichtig)
pix-title = Pixel von Bild { $frame } — { $name }
pix-nochange = Kein Pixel geändert.
pix-status = Bild { $frame } von { $name } bearbeitet (vorgemerkt); mit „Speichern…“ sichern.
pix-failed = Pixel konnten nicht vorgemerkt werden
gat-alt = Karte, { $x } mal { $y } Zellen
gat-x-from = x von
gat-y-from = y von
gat-to = bis
gat-type-aria = Zelltyp
gat-type-nochange = Typ: keine Änderung
gat-height-ph = keine Änderung
gat-height = Höhe
gat-set = Zellen setzen
gat-set-title = Typ und/oder Höhe der gewählten Zellen setzen (vorgemerkt, wird mit „Speichern…“ gesichert)
gat-drag = Auf der Karte ziehen, um Zellen zu wählen.
gat-readout = x { $xa }–{ $xb }, y { $ya }–{ $yb }: { $cells }
gat-cells = { $n ->
    [one] { $n } Zelle
   *[other] { $n } Zellen
 }
gat-select-all = Alles wählen
gat-need = Wähle zuerst einen Zelltyp und/oder eine Höhe.
gat-status = { $cells } von { $name } geändert (vorgemerkt); mit „Speichern…“ sichern.
gat-failed = Zellen konnten nicht geändert werden

webgl-missing = WebGL 2 ist in diesem Fenster nicht verfügbar.

pvw-n-empty-marker = Leerer Markereintrag
pvw-n-sound-fail = Dieser Klang kann hier nicht abgespielt werden: { $error }
pvw-n-luac-fail = Kompiliertes Lua (Bytecode), das noch nicht angezeigt werden kann: { $error }
pvw-n-palette = Palette: 256 Farben
pvw-n-palette-label = 256 Farben (Index 0 ist transparent)
pvw-n-gat = GAT { $version } – { $w } x { $h } Zellen, Höhen { $lo } bis { $hi }
pvw-n-gat-type = Typ { $k } ({ $name }): { $cells }
pvw-n-gnd = GND { $version } – { $w } x { $h } Zellen, { $tiles }, { $lightmaps }, { $textures }
pvw-n-rsw = RSW { $version } – Boden { $ground }, { $models }, { $lights }, { $sounds }, { $effects }
pvw-n-instances = { $instances } aus { $models }
pvw-n-models-missing = { $n } nicht gefunden: { $names }
pvw-n-textures = { $n }
pvw-n-textures-missing = { $n ->
    [one] { $n } Textur fehlt
   *[other] { $n } Texturen fehlen
 }
pvw-n-rsm = RSM { $version } – { $nodes }, { $triangles }, { $textures }
pvw-n-rsm-loop = Die Animation wiederholt sich alle { $secs } s
pvw-n-node = Knoten { $name } ({ $v }, { $f })
pvw-n-node-moves = Knoten { $name } ({ $v }, { $f }), bewegt sich
pvw-n-str = STR – { $fps } fps, { $frames }, { $layers }, { $textures }
pvw-n-str-step = Es wird nur jedes { $step }. Bild gezeigt
pvw-n-str-missing = fehlende Texturen ({ $n }):
pvw-n-str-simple = Vereinfachte Vorschau: Mischmodi und Schlüsselanimation wie im Client, ohne Beleuchtung, auf dunklem Hintergrund.
pvw-n-spr = SPR { $version } – { $pal }, { $rgba }
pvw-n-warning = Warnung: { $text }
pvw-n-spr-limit = gezeigt werden die ersten { $shown } von { $total } Bildern
pvw-n-act = ACT { $version } – { $actions }, { $frames }, { $layers }
pvw-n-act-nospr = Die passende .spr-Datei ist nicht in diesem Archiv, daher gibt es nichts zu zeichnen.
pvw-n-act-none = Die Animation hat keine Aktionen
pvw-n-act-empty = Aktion { $action } zeichnet nichts
gat-tn-0 = begehbar
gat-tn-1 = nicht begehbar
gat-tn-2 = nicht begehbar, nicht beschießbar
gat-tn-3 = begehbar (Wasser)
gat-tn-4 = unbekannt
gat-tn-5 = nicht begehbar, beschießbar
gat-tn-6 = begehbar
gat-tn-weird = ungewöhnlich (höchstes Bit gesetzt)

un-tiles = { $n ->
    [one] { $n } Kachel
   *[other] { $n } Kacheln
 }
un-lightmaps = { $n ->
    [one] { $n } Lichtkarte
   *[other] { $n } Lichtkarten
 }
un-textures = { $n ->
    [one] { $n } Textur
   *[other] { $n } Texturen
 }
un-models = { $n ->
    [one] { $n } Modell
   *[other] { $n } Modelle
 }
un-lights = { $n ->
    [one] { $n } Licht
   *[other] { $n } Lichter
 }
un-sounds = { $n ->
    [one] { $n } Klang
   *[other] { $n } Klänge
 }
un-effects = { $n ->
    [one] { $n } Effekt
   *[other] { $n } Effekte
 }
un-nodes = { $n ->
    [one] { $n } Knoten
   *[other] { $n } Knoten
 }
un-triangles = { $n ->
    [one] { $n } Dreieck
   *[other] { $n } Dreiecke
 }
un-vertices = { $n ->
    [one] { $n } Eckpunkt
   *[other] { $n } Eckpunkte
 }
un-faces = { $n ->
    [one] { $n } Fläche
   *[other] { $n } Flächen
 }
un-palimg = { $n ->
    [one] { $n } Palettenbild
   *[other] { $n } Palettenbilder
 }
un-rgbaimg = { $n ->
    [one] { $n } Echtfarbbild
   *[other] { $n } Echtfarbbilder
 }
un-actions = { $n ->
    [one] { $n } Aktion
   *[other] { $n } Aktionen
 }
un-layers = { $n ->
    [one] { $n } Ebene
   *[other] { $n } Ebenen
 }
un-modelinst = { $n ->
    [one] { $n } Modellinstanz
   *[other] { $n } Modellinstanzen
 }
un-diffmodels = { $n ->
    [one] { $n } verschiedenem Modell
   *[other] { $n } verschiedenen Modellen
 }

err-target-open = Das Ziel darf nicht das geöffnete Archiv sein
err-alpha = Das Bild hat einen Alphakanal, den 8-Bit-BMPs nicht speichern können
err-saved-png = Das Bild wird als .png gespeichert
err-not-png = Kein PNG-Bild
err-no-palette = Der Sprite hat keine Palette
err-interval-positive = Das Intervall muss eine positive Zahl von Millisekunden sein
err-merge-two = Wähle mindestens zwei Archive
err-merge-target = Das Ziel darf keines der Eingabearchive sein
err-empty-marker = Dieser Eintrag ist ein leerer Marker
err-enter-path = Gib den Pfad im Archiv an, z. B. data\texture\a.bmp
err-name-chars = Der Pfad enthält Zeichen, die dieser Namensmodus nicht schreiben kann
err-marked-removal = Der Eintrag ist zum Entfernen vorgemerkt
err-no-spr = Die passende .spr-Datei ist nicht in diesem Archiv
err-no-changes = Es gibt keine Änderungen zu speichern
err-entry-encrypted = Der Eintrag ist verschlüsselt (Entschlüsseln wird noch nicht unterstützt)
err-map-large = Die Karte ist zu groß
err-search-len = Die Suche braucht 1 bis 256 Bytes
err-no-byte = Es wurde kein Byte geändert
err-no-draft = Für dieses Archiv gibt es keinen Entwurf
err-already-bits = Das Bild hat schon { $bits } Bit pro Pixel
err-not-file = { $path } ist keine Datei
err-file-4gib = { $path } ist größer als 4 GiB
err-text-chars = Der Text enthält Zeichen, die { $encoding } nicht darstellen kann
err-preview-large = { $mb } MB sind zu groß für eine Vorschau
err-map-part = „{ $part }“ ist kein Teil einer Karte, der weggelassen werden kann
err-number = { $what } muss eine sinnvolle Zahl sein
err-act-keep-action = Eine Animation behält mindestens eine Aktion
err-act-no-action = Diese Aktion gibt es nicht
err-act-interval = Das Intervall muss zwischen 1 und 60000 ms liegen
err-act-no-frame = Diesen Frame gibt es nicht
err-act-keep-frame = Eine Aktion behält mindestens einen Frame
err-act-no-frame-n = Frame { $n } gibt es nicht
err-act-image-number = Die Bildnummer ist -1 (leer) oder größer
err-act-image-type = Der Bildtyp ist 0 (Palette) oder 1 (Echtfarbe)
err-act-one-scale = Diese ACT-Version hat eine Skalierung für beide Richtungen
err-act-too-many-layers = Zu viele Ebenen
err-act-no-layer-n = Ebene { $n } gibt es nicht
err-act-no-layer = Diese Ebene gibt es nicht
err-act-no-anchors = Diese ACT-Version hat keine Anker
err-act-too-many-anchors = Zu viele Anker
err-act-no-anchor-n = Anker { $n } gibt es nicht
err-act-no-frames = Die Aktion hat keine Frames
err-gat-need = Wähle einen Typ und/oder eine Höhe zum Setzen
err-gat-height = Die Höhe muss eine sinnvolle Zahl sein
err-gat-type = Der Zelltyp ist 0 bis 6
err-pal-range = Die Farbnummern gehen von 0 bis 255
err-pal-order = Die erste Farbe liegt hinter der letzten
err-pal-numbers = Farbton, Sättigung und Helligkeit müssen Zahlen sein
err-pal-factors = Sättigung und Helligkeit sind Faktoren von mindestens 0
err-spr-size = Das Bild ist { $w } x { $h } Pixel groß, die Daten haben { $n } Werte
err-no-archive = Es ist kein Archiv geöffnet
err-job-running = Es läuft bereits ein anderer Auftrag

menu-new = Neu
menu-new-grf = GRF-Archiv...
menu-new-gpf = GPF-Patch...
menu-new-rgz = RGZ-Patch...
menu-new-thor = Thor-Patch...
menu-new-window = Neues Fenster
menu-recent = Zuletzt geöffnet
menu-recent-none = Keine zuletzt geöffneten Archive
menu-recent-clear = Liste leeren
menu-close-archive = Archiv schließen
menu-save = Speichern
menu-save-as = Speichern unter...
menu-save-advanced = Erweitert speichern
menu-save-repack = Neu packen (neu komprimieren)...
menu-save-compact = Verdichten (Daten kopieren)...
menu-save-encrypt = Verschlüsseln...
menu-save-decrypt = Entschlüsselt speichern...
menu-archive-settings = Archiv-Einstellungen...
menu-changes = Änderungen ansehen...
menu-tools = Werkzeuge
menu-hash = Prüfsummen...
menu-compare = Mit Archiv vergleichen...
menu-data-folder = App-Datenordner öffnen
menu-about = Über RustyGRF...
open-elsewhere = Dieses Archiv ist in einem anderen Fenster geöffnet.
err-open-elsewhere = Das Archiv ist in einem anderen Fenster geöffnet
untitled = Unbenannt
new-failed = Neues Archiv konnte nicht angelegt werden
close-title = Archiv schließen?
close-body = { $n } ungespeicherte Änderungen bleiben als Entwurf erhalten und werden beim nächsten Öffnen angeboten.
close-anyway = Schließen
save-here = Speichern
save-as-button = Speichern unter…
save-nothing = Nichts zu speichern: Es gibt keine Änderungen.
save-here-done = { $name } gespeichert: { $files }, { $size }, geprüft.
sa-title = Speichern unter
sa-title-repack = Neu packen (neu komprimieren)
sa-title-compact = Verdichten
sa-title-encrypt = Verschlüsseln
sa-title-decrypt = Entschlüsselt speichern
as-title = Archiv-Einstellungen
sa-store = Dateien
sa-store-copy = Gespeicherte Daten kopieren (schnell, verdichtet das Archiv)
sa-store-recompress = Jede Datei neu komprimieren
sa-format-keep = Beibehalten ({ $format })
sa-encryption = Verschlüsselung
sa-enc-keep = Passwort beibehalten
sa-enc-none = Verschlüsselung entfernen
sa-enc-plain = Keine
sa-enc-password = Mit Passwort verschlüsseln
sa-password = Passwort
sa-password-again = Passwort wiederholen
sa-password-mismatch = Die beiden Passwörter sind verschieden.
err-password-empty = Gib ein Passwort ein
sa-hint = „Speichern“ schreibt das Archiv neben sich, prüft es und ersetzt es dann. Verschlüsselung geht nur mit GRF 0x200; GRF Editor und Clients mit seiner DLL lesen sie. Thor und RGZ entstehen über den gewählten Dateinamen.
sa-filter = { $type }-Dateien
as-path = Datei
as-unsaved = noch nicht gespeichert
as-type = Typ
as-content = Inhalt
as-encryption = Verschlüsselung
as-encrypted = GRF-Editor-Passwort
as-plain = keine
as-names = Namen angezeigt als
err-save-same = Wähle eine andere Datei: „Speichern“ schreibt in das geöffnete Archiv
err-save-unverified = Das geschriebene Archiv bestand die Prüfung nicht; das Original ist unverändert
err-save-replace = { $path } konnte nicht ersetzt werden ({ $error }). Das gespeicherte Archiv liegt unter { $tmp }.
err-encrypt-format = Verschlüsseln geht nur bei GRF 0x200
err-patch-empty = Ein Patch braucht mindestens eine Datei
hash-title = Prüfsummen
hash-pick = Wähle zuerst eine Datei.
hash-size = Größe
cmp-pick = Mit welchem Archiv vergleichen?
cmp-running = Vergleiche …
cmp-failed = Vergleich nicht möglich
cmp-title = Unterschiede
cmp-same = Gleicher Inhalt
cmp-summary = Verglichen mit { $other }: { $here } nur hier, { $changed } geändert, { $there } nur dort, { $same } gleich.
cmp-only-here = Nur in diesem Archiv
cmp-changed = Geändert
cmp-only-there = Nur im anderen Archiv
cmp-patch-hint = Ein Patch enthält, was hier neu oder geändert ist; ein Thor-Patch löscht zusätzlich, was nur im anderen Archiv ist.
cmp-make-patch = Als Patch speichern…
cmp-patch-pick = Patch speichern als
cmp-patch-running = Schreibe den Patch …
cmp-patch-done = Patch geschrieben
cmp-patch-result = { $added } neu, { $changed } geändert, { $removed } entfernt; { $size }, geschrieben nach { $dst }.
about-title = Über RustyGRF
about-version = Version { $version }
about-tagline = Ein Werkzeug für Archive, Sprites, Animationen, Paletten und Karten von Ragnarok Online, auf macOS, Windows und Linux.
about-made-by = Gemacht von
about-thanks = Dank
about-tokeiburu = GRF Editor und ACT Editor, die Vorbilder; Formate mit seiner Erlaubnis übernommen
about-lua = Lua 5.0 und 5.1 (MIT), zum Prüfen und Kompilieren von Lua-Dateien
about-tauri = das Fenster um die App
about-license = Lizenz: MIT oder Apache-2.0. Keine Verbindung zu Gravity.
save-as-done = Gespeichert als { $dst }; dieses Fenster zeigt es jetzt.
save-unverified = Das gespeicherte Archiv hat die Prüfung nicht bestanden: { $dst }
menu-search-contents = In Dateien suchen...
sc-title = In Dateien suchen
sc-query = Text
sc-query-ph = was gesucht wird, z. B. prontera
sc-files = Dateien
sc-files-ph = *.lua;*.txt (leer: alle Dateien)
sc-regex = Regulärer Ausdruck
sc-case = Groß-/Kleinschreibung beachten
sc-word = Ganze Wörter
sc-in-list = Nur die Dateien der Liste
sc-search = Suchen
sc-running = Suche läuft …
sc-result = { $lines } Zeilen in { $files } (von { $searched } durchsucht).
sc-more = Es passen mehr Zeilen, als aufgeführt sind.
sc-none = Nichts gefunden in { $searched }.
sc-show = In der Liste zeigen
err-search-empty = Gib den gesuchten Text ein
err-bad-pattern = Der reguläre Ausdruck ist ungültig: { $error }
menu-undo = Rückgängig
menu-redo = Wiederholen
menu-history = Änderungsverlauf...
menu-nav-back = Zurück
menu-nav-forward = Vor
menu-text-encoding = Textkodierung...
hist-add = { $name } hinzugefügt
hist-replace = { $name } ersetzt
hist-edit = { $name } bearbeitet
hist-remove = { $name } entfernt
hist-discard = Änderung an { $name } verworfen
hist-mixed = { $name } geändert
hist-add-n = { $n } Dateien hinzugefügt
hist-replace-n = { $n } Dateien ersetzt
hist-edit-n = { $n } Dateien bearbeitet
hist-remove-n = { $n } Dateien entfernt
hist-discard-n = { $n } Änderungen verworfen
hist-mixed-n = { $n } Dateien geändert
history-undone-n = { $n } Schritte rückgängig gemacht
history-redone-n = { $n } Schritte wiederholt
hist-title = Änderungsverlauf
hist-redo-head = Kann wiederholt werden
hist-now = ▸ Jetzt
hist-undo-head = Kann rückgängig gemacht werden (neueste zuerst)
hist-empty = Nichts rückgängig zu machen.
hist-hint = Ein Klick geht zu diesem Schritt zurück (oder vor). Bis zu 200 Schritte je Archiv; der Verlauf endet mit dem Speichern, die Änderungen selbst überstehen einen Absturz als Entwurf. Rechtsklick auf Rückgängig oder Wiederholen zeigt dieselbe Liste.
te-title = Textkodierung
te-intro = Text, der in der falschen Kodierung gelesen wurde (etwa ein koreanischer Dateiname als Latin-1), wird lesbar: Er wird in der Kodierung zurückgeschrieben, in der er angezeigt wurde, und in der richtigen gelesen.
te-source = Text wie angezeigt
te-result = Ergebnis
te-from = Angezeigt als
te-to = Lesen als
te-swap = Tauschen
te-bytes = Bytes: { $hex }
te-lossy = Einige Zeichen gibt es in einer der beiden Kodierungen nicht; sie wurden ersetzt.
as-magic = Kennung (Magic)
as-magic-reset = Üblich
err-bad-magic = Die Kennung ist ungültig: bis zu 16 ASCII-Zeichen ({ $error })
menu-flat-maps = Flache Karten...
pvw-flat-map = Karte ändern…
pvw-flat-map-title = Flacher Boden, ohne Schatten, Licht, Objekte oder Wasser, in diesem Archiv (Rückgängig nimmt es zurück)
flat-flatten = Flacher Boden (keine Hügel oder Wände; auch keine Objekte und kein Wasser)
flat-remove_shadow = Schatten entfernen
flat-remove_light = Farbiges Licht entfernen
flat-remove_color = Kachelfarben entfernen
flat-gat_shadows = Schatten aus dem Laufraster (gesperrte Zellen dunkel; ersetzt Licht und Schatten)
flat-reset_light = Neutrales Sonnenlicht
flat-remove_models = Modelle entfernen
flat-remove_objects = Alle Objekte entfernen (Modelle, Lichter, Geräusche, Effekte)
flat-remove_water = Wasser entfernen
flat-notes = Die Regeln folgen dem Flat-Maps-Maker von GRF Editor. Die Dateiversionen bleiben; Wasser wird unter den Boden gelegt. Das Laufraster (wohin Spieler gehen können) ändert sich nicht.
flat-none = Nichts ist angekreuzt.
flat-failed = Die Karte konnte nicht geändert werden
flat1-title = Karte ändern
flat1-intro = Welt-, Boden- und Höhendatei dieser Karte werden im geöffneten Archiv geändert. Speichern behält die Änderung, Rückgängig nimmt sie zurück. In der Ansicht ausgeschaltete Teile sind schon angekreuzt.
flat1-go = Ändern
flat1-status = Karte { $map } geändert (noch nicht gespeichert)
flats-title = Flache Karten
flats-intro = Schreibt ein neues, kleines Archiv nur mit den geänderten Kartendateien. Vor die anderen Archive in DATA.INI gestellt, nimmt der Client diese Karten; das geöffnete Archiv bleibt unverändert.
flats-selected = Gewählte Karte
flats-in-folder = Karten im gewählten Ordner
flats-all = Alle Karten
flats-pending = Ausstehende Änderungen an Kartendateien (insgesamt { $n } Änderungen) werden berücksichtigt.
flats-save-as = Flache Karten speichern unter
flats-running = Karten werden geändert
flats-summary = Geänderte Karten: { $changed } von { $maps }.
flats-problems = Ausgelassen ({ $n }):
flats-nothing = Nichts geändert, nichts geschrieben
flats-written = { $files } nach { $dst } geschrieben ({ $size }).
flats-how = Zum Benutzen in DATA.INI vor die anderen Archive eintragen (zum Beispiel als 0=).
flats-status = { $changed } flache Karte(n) nach { $dst } geschrieben
flats-done = Flache Karten geschrieben
flats-cancelled = Flache Karten abgebrochen
em-change = Kartendateien ändern
em-strip = Oben weggelassene Teile auch aus der Weltdatei nehmen (damit der Client sie nicht sucht)
em-changed = { $files } geändert geschrieben.
err-flat-not-map = Das ist keine Kartendatei (.rsw, .gnd oder .gat)
err-flat-no-gnd = Die Bodendatei { $name } ist nicht in diesem Archiv
err-flat-failed = Die Karte lässt sich nicht ändern: { $error }
err-flat-nothing = Diese Optionen ändern an dieser Karte nichts
