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
    .title = Die ausstehenden Änderungen prüfen und als neues Archiv speichern (Cmd/Strg+S)

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
    [one] Die { $n } ausstehende Änderung als neues Archiv speichern
   *[other] Die { $n } ausstehenden Änderungen als neues Archiv speichern
 }
save-none = Die ausstehenden Änderungen als neues Archiv speichern
save-aria-n = { $n ->
    [one] { $n } ausstehende Änderung als neues Archiv speichern
   *[other] { $n } ausstehende Änderungen als neues Archiv speichern
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
menu-build = Erstellen...
menu-merge = Zusammenführen...
menu-extract = Extrahieren...
menu-verify = Prüfen
menu-changes-save = Änderungen als neues Archiv speichern...
menu-reduce-images = Bilder verkleinern...
menu-add = Datei hinzufügen...
menu-replace = Ersetzen...
menu-remove = Entfernen
menu-changes-review = Änderungen ansehen...
menu-layout-tree = Als Baum
menu-layout-folders = Als Ordner
menu-layout-list = Als Liste
menu-layout-details = Als Details
menu-layout-gallery = Als Galerie
menu-preview-right = Vorschau rechts
menu-preview-bottom = Vorschau unten
menu-preview-off = Vorschau ausblenden
