# RustyGRF – Français (Beta : brouillon automatique, pas encore relu par des francophones).

tb-main = Principal
    .aria-label = Principal
tb-app = Application
    .aria-label = Application
tb-archive = Archive
    .aria-label = Archive
tb-edit = Édition
    .aria-label = Édition
tb-edit-mark = Édition
tb-view = Affichage et filtres
    .aria-label = Affichage et filtres

btn-settings = Réglages
    .title = Réglages (Cmd/Ctrl+,)
    .aria-label = Réglages
btn-open = Ouvrir…
    .title = Ouvrir une archive GRF (Cmd/Ctrl+O)
btn-build = Créer…
    .title = Empaqueter un dossier dans un nouveau GRF
btn-merge = Fusionner…
    .title = Fusionner plusieurs GRF en un nouveau
btn-extract = Extraire…
    .title = Extraire les entrées sélectionnées, ou toutes celles affichées
btn-verify = Vérifier
    .title = Rechercher des erreurs dans l'archive
btn-add = Ajouter un fichier…
    .title = Ajouter un fichier du disque sous un chemin de l'archive
btn-replace = Remplacer…
    .title = Remplacer l'entrée sélectionnée par un fichier du disque
btn-remove = Retirer
    .title = Laisser l'entrée sélectionnée hors de l'archive enregistrée
btn-undo = Annuler
    .title = Annuler la dernière modification en attente (Cmd/Ctrl+Z)
btn-redo = Rétablir
    .title = Rétablir (Maj+Cmd/Ctrl+Z)
btn-save = Enregistrer…
    .title = Revoir et enregistrer les modifications en attente dans une nouvelle archive (Cmd/Ctrl+S)

search = Recherche
    .placeholder = Recherche : un nom ou un motif comme *.spr
    .aria-label = Recherche

pv-group = Position de l'aperçu
    .aria-label = Position de l'aperçu
pv-right = Droite
    .title = Aperçu à côté de la liste
    .aria-label = Aperçu à droite
pv-bottom = Bas
    .title = Aperçu sous la liste
    .aria-label = Aperçu en bas
pv-off = Non
    .title = Masquer l'aperçu (Cmd/Ctrl+Maj+P)
    .aria-label = Masquer l'aperçu

view-group = Disposition de la liste
    .aria-label = Disposition de la liste
view-tree = Arbre
    .title = Arborescence des dossiers avec fichiers (Cmd/Ctrl+1)
view-folders = Dossiers
    .title = Dossiers à gauche, fichiers du dossier choisi à droite (Cmd/Ctrl+2)
view-list = Liste
    .title = Liste simple : nom, dossier, taille (Cmd/Ctrl+3)
view-details = Détails
    .title = Tableau simple à colonnes triables (Cmd/Ctrl+4)
view-gallery = Galerie
    .title = Galerie de miniatures d'images et de sprites (Cmd/Ctrl+5)

sort = Tri
sort-archive = Ordre enregistré
sort-name = Nom
sort-path = Chemin
sort-type = Type
sort-size = Taille
sort-stored = Taille compressée
sort-offset = Position dans le fichier
sort-dir = Sens du tri
    .title = Croissant ou décroissant
    .aria-label = Sens du tri

kind-filter-title = N'afficher que certains types de fichiers
kind-label-all = Type : tous
kind-label-pictures = Type : images
kind-label-one = Type : { $kind }
kind-label-n = Type : { $n } types
kind-all = Tous
kind-only-images = Images seulement
kind-image = Images
kind-sprite = Sprites
kind-animation = Animations
kind-palette = Palettes
kind-map = Cartes
kind-model = Modèles
kind-audio = Audio
kind-text = Texte et Lua
kind-effect = Effets
kind-file = Autres

size = Taille
size-any = toutes
size-over-1k = plus de 1 Ko
size-over-100k = plus de 100 Ko
size-over-1m = plus de 1 Mo
size-over-10m = plus de 10 Mo
size-under-1k = moins de 1 Ko
size-under-100k = moins de 100 Ko
size-under-1m = moins de 1 Mo

subfolders = Sous-dossiers
    .title = Lister aussi les fichiers des sous-dossiers
expand-all = Tout développer
    .title = Ouvrir tous les dossiers
collapse-all = Tout réduire
    .title = Fermer tous les dossiers
reset-filters = Réinitialiser
    .title = Effacer la recherche et les filtres de type et de taille

empty-hint = Ouvrez une archive GRF, ou déposez-la sur cette fenêtre.
open-archive = Ouvrir une archive…
folders-pane = Dossiers
    .aria-label = Dossiers
all-files = Tous les fichiers
    .title = Afficher les fichiers de tous les dossiers
col-name = Nom
col-type = Type
col-size = Taille
col-stored = Compressé
entries-list = Entrées
    .aria-label = Entrées
preview-pane = Aperçu
    .aria-label = Aperçu
split-folders = Redimensionner la liste des dossiers
    .aria-label = Redimensionner la liste des dossiers
split-preview = Redimensionner l'aperçu (flèches du clavier)
    .aria-label = Redimensionner l'aperçu (flèches du clavier)
    .title = Faites glisser, ou donnez le focus et utilisez les flèches du clavier

status-ready = Prêt
count-entries = { $n ->
    [one] { $n } entrée
   *[other] { $n } entrées
 }
count-filtered = { $n } sur { $total }
count-note-folders = (la recherche couvre tous les dossiers)
count-note-pictures = (images et sprites)
names = Noms
    .title = Comment les noms de fichiers sont affichés et recherchés. L'extraction écrit toujours des noms Latin-1, pour que le résultat corresponde à `rgrf build`.
names-korean = Coréen

save-n = { $n ->
    [one] Enregistrer la { $n } modification en attente dans une nouvelle archive
   *[other] Enregistrer les { $n } modifications en attente dans une nouvelle archive
 }
save-none = Enregistrer les modifications en attente dans une nouvelle archive
save-aria-n = { $n ->
    [one] Enregistrer { $n } modification en attente dans une nouvelle archive
   *[other] Enregistrer { $n } modifications en attente dans une nouvelle archive
 }
save-aria-none = Enregistrer les modifications

dlg-ok = OK
dlg-cancel = Annuler
dlg-close = Fermer
dlg-done = Terminé
dlg-quick = Rapide
dlg-full = Complète
dlg-edit-animation = Modifier l'animation
dlg-edit-pixels = Modifier les pixels
dlg-edit-text = Modifier le texte

verify-title = Vérifier
verify-body = La vérification rapide ne lit que la table des fichiers. La vérification complète décode aussi chaque entrée, ce qui lit toute l'archive.
verify-running = Vérification …
verify-status = Vérifié : { $errors } erreurs, { $warnings } avertissements
verify-summary-full = { $entries } entrées, toutes décodées : { $errors } erreurs, { $warnings } avertissements, { $infos } remarques.
verify-summary-quick = { $entries } entrées, table seulement : { $errors } erreurs, { $warnings } avertissements, { $infos } remarques.
verify-problems = Problèmes détectés
verify-ok = Archive vérifiée
verify-failed = Échec de la vérification

set-title = Réglages
    .aria-label = Réglages
set-appearance = Apparence
set-theme = Thème
set-theme-system = Suivre le système
set-theme-light = Clair
set-theme-dark = Sombre
set-accent = Couleur d'accentuation
set-accent-orange = orange
set-accent-blue = bleu
set-accent-green = vert
set-accent-purple = violet
set-textsize = Taille du texte
set-small = Petite
set-normal = Normale
set-large = Grande
set-density = Densité de la liste
set-compact = Compacte
set-comfortable = Aérée
set-fileicons = Icônes de fichiers
set-plainicons = Icônes simples (sans couleurs par type de fichier)
set-animations = Animations
set-animations-hint = Désactive la flèche et d'autres transitions.
set-motion-off = Non
set-language = Langue
set-language-system = Suivre le système
set-language-beta = { $name } (Bêta)
set-language-note = Les traductions marquées Bêta sont des brouillons automatiques qu'aucun locuteur natif n'a encore relus. Les textes pas encore traduits restent en anglais.
set-opening = À l'ouverture d'une archive
set-names = Noms de fichiers
set-names-cp949 = Coréen (CP949)
set-names-cp1252 = Latin-1 (comme GRF Editor les affiche)
set-names-utf8 = UTF-8
set-layout = Disposition de la liste
set-as-last = Comme la dernière fois
set-next-start = Pris en compte au prochain démarrage de l'application.
set-preview = Aperçu
set-startup = Démarrage
set-reopen = Rouvrir la dernière archive
set-changes = Modifications
set-warn-discard = Demander avant d'abandonner des modifications en attente
set-building = Création d'archives
set-compression = Compression
set-format = Format
set-building-note = Ce ne sont que les choix de départ de la boîte « Créer… » ; vous pouvez les changer là-bas pour chaque archive.
set-stored-note = Les réglages sont conservés uniquement dans cette application. Raccourci : { $key }+, ouvre cette fenêtre.
set-reset = Rétablir les valeurs par défaut

method-zlib = zlib (tous les clients)
    .hint = La norme. Tous les clients peuvent la lire et la création est rapide. Choisissez-la sauf raison contraire.
method-zopfli = zopfli (plus petit, lent)
    .hint = Écrit le même format zlib, donc tous les clients peuvent toujours le lire, mais compresse un peu plus les fichiers. La création est beaucoup plus longue ; la lecture est aussi rapide.
method-lzma = LZMA (nécessite la prise en charge du client)
    .hint = Plus petit que zlib, mais seul un client prenant en charge LZMA peut le lire (par exemple avec une cps.dll). Essayez l'archive dans votre client avant de la partager.
format-0x200 = 0x200 (tous les clients)
    .hint = Le format GRF standard. Tous les clients le lisent. Les positions de fichiers sont sur 32 bits : l'archive ne peut pas dépasser 4 Go.
format-0x300 = 0x300 (plus de 4 Go)
    .hint = Format plus récent avec des positions sur 64 bits : l'archive peut dépasser 4 Go. Seuls les clients récents le lisent (mesuré : un client de 2025 oui, ceux de 2020 non). À n'utiliser que si 4 Go ne suffisent pas.
format-0x103 = 0x103 (très anciens clients)
    .hint = Format très ancien, antérieur à 0x200 : la table des fichiers n'est pas compressée et les noms et données sont brouillés avec l'ancienne variante de DES. Pour les clients trop anciens pour lire 0x200. Même limite de 4 Go.
format-0x102 = 0x102 (très anciens clients)
    .hint = Identique à 0x103 (ils ne diffèrent que par le numéro de version). À utiliser si un ancien client demande exactement cette version.

menu-file = Fichier
menu-edit = Édition
menu-view = Affichage
menu-window = Fenêtre
menu-help = Aide
menu-settings = Réglages...
menu-open = Ouvrir...
menu-build = Créer...
menu-merge = Fusionner...
menu-extract = Extraire...
menu-verify = Vérifier
menu-changes-save = Enregistrer les modifications dans une nouvelle archive...
menu-reduce-images = Réduire les images...
menu-add = Ajouter un fichier...
menu-replace = Remplacer...
menu-remove = Retirer
menu-changes-review = Revoir les modifications...
menu-layout-tree = En arbre
menu-layout-folders = En dossiers
menu-layout-list = En liste
menu-layout-details = En détails
menu-layout-gallery = En galerie
menu-preview-right = Aperçu à droite
menu-preview-bottom = Aperçu en bas
menu-preview-off = Masquer l'aperçu
