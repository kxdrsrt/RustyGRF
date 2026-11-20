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
    .title = Vérifier et enregistrer les modifications en attente (Cmd/Ctrl+S enregistre aussitôt)

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
    [one] Vérifier et enregistrer la { $n } modification en attente
   *[other] Vérifier et enregistrer les { $n } modifications en attente
 }
save-none = Enregistrer une copie avec d’autres réglages
save-aria-n = { $n ->
    [one] Enregistrer { $n } modification en attente
   *[other] Enregistrer { $n } modifications en attente
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
menu-build = Créer depuis un dossier...
menu-merge = Fusionner...
menu-extract = Extraire...
menu-verify = Vérifier l’archive
menu-reduce-images = Réduire les images...
menu-add = Ajouter un fichier...
menu-replace = Remplacer...
menu-remove = Retirer
menu-layout-tree = En arbre
menu-layout-folders = En dossiers
menu-layout-details = En détails
menu-layout-gallery = En galerie
menu-preview-right = Aperçu à droite
menu-preview-bottom = Aperçu en bas
menu-preview-off = Masquer l'aperçu

## Dialogs
dlg-choose-target = Choisir la destination…
dlg-open-it = L'ouvrir
dlg-add = Ajouter
job-starting = Démarrage …
job-cancelling = Annulation …
job-progress = { $done } sur { $total }
nothing-written = Rien n'a été écrit.
verified-yes = vérifié
verified-no = non vérifié
build-pick = Dossier à empaqueter (celui qui contient data\ …)
build-title = Créer l'archive
build-pack = Empaqueter { $src }
build-share = Partager les fichiers identiques
build-share-hint = Ne stocke qu'une fois les fichiers de contenu identique, ce qui économise de la place. Impossible en 0x102 et 0x103.
build-verify-note = L'archive terminée est vérifiée par rapport aux fichiers du disque.
build-save-as = Enregistrer l'archive sous
build-running = Création de l'archive
build-status = Création de { $dst } …
build-done-status = { $dst } créée : { $files } fichiers
build-result = Fichiers : { $files } en { $blocks } blocs ({ $from } → { $to }), { $verified }.
build-shared = Fichiers identiques partageant un bloc : { $n }.
build-skipped = Éléments ignorés : { $n }
build-done-title = Archive créée
build-cancelled = Création annulée
build-failed = Échec de la création
build-could-not = Impossible de créer
merge-pick = Archives à fusionner (au moins deux)
merge-need-two = Choisissez au moins deux archives.
merge-title = Fusionner des archives
merge-higher = Priorité plus haute
merge-lower = Priorité plus basse
merge-intro = Ordre = priorité : pour un chemin présent dans plusieurs archives, celle du haut l'emporte. Les entrées ne sont pas modifiées.
merge-replace-only = Remplacer seulement : la dernière archive sert de base, les autres n'ajoutent aucun nouveau chemin
merge-save-as = Enregistrer l'archive fusionnée sous
merge-running = Fusion des archives
merge-status = Fusion dans { $dst } …
merge-done-status = Fusionné dans { $dst } : { $files } fichiers
merge-line = Fichiers : { $files }, utilisés : { $used }, remplacés : { $replaced }
merge-line-ignored = , nouveaux chemins ignorés : { $n }
merge-result = Fichiers : { $files }, { $size }, { $verified }.
merge-damaged = Blocs endommagés copiés tels quels : { $n }.
merge-done-title = Archives fusionnées
merge-cancelled = Fusion annulée
merge-failed = Échec de la fusion
merge-could-not = Impossible de fusionner
ext-title = Extraire
ext-folder = Dossier ({ $n } fichiers)
ext-entry = Entrée sélectionnée
ext-all = Les { $n } de la liste
ext-filtered = La liste est filtrée ; « tous » désigne les entrées qui correspondent.
ext-whole = La liste affiche toute l'archive.
ext-dest = Extraire dans le dossier
ext-empty-folder = Ce dossier ne contient aucun fichier.
ext-running = Extraction …
ext-done-status = { $files } fichier(s) extrait(s) dans { $dest }
ext-done-title = Extrait
ext-done-body = { $files } fichier(s) écrit(s) dans { $dest }.
ext-damaged = { $n } entrées étaient endommagées ; leur partie récupérable a été écrite.
ext-stopped = Extraction arrêtée
ext-could-not = Impossible d'extraire
ext-rename = Renommer les nouveaux fichiers
replace-pick = Fichier qui remplace { $name }
replace-status = Remplacement en attente pour { $name } ; enregistrez-le avec « Enregistrer… »
replace-failed = Impossible de mettre le remplacement en attente
add-pick = Fichier à ajouter
add-title = Ajouter un fichier
add-prompt = Chemin dans l'archive (un chemin existant est remplacé) :
add-status = { $path } en attente ; enregistrez avec « Enregistrer… »
add-failed = Impossible de mettre le fichier en attente
remove-title = Retirer de l'archive enregistrée
remove-body = Rien n'est supprimé maintenant : les entrées sont omises quand vous enregistrez les modifications dans une nouvelle archive.
remove-status = Entrées marquées pour retrait : { $n }
remove-failed = Impossible de mettre le retrait en attente
changes-title = Modifications
changes-none = Aucune modification en attente. Utilisez « Remplacer… », « Ajouter un fichier… » ou « Retirer » ; l'archive ouverte n'est jamais modifiée, le résultat est enregistré dans une nouvelle archive.
changes-heading = { $n ->
    [one] { $n } modification en attente
   *[other] { $n } modifications en attente
 }
changes-more = … et { $n } de plus
changes-save = Enregistrer dans une nouvelle archive…
changes-discard = Tout abandonner
changes-discarded = Modifications abandonnées
save-pick = Enregistrer l'archive modifiée sous
save-running = Enregistrement de l'archive
save-status = Enregistrement de { $dst } …
save-done-status = { $dst } enregistré
save-result = Fichiers : { $files }, { $size }, { $verified } : ajoutés { $added }, remplacés { $replaced }, retirés { $removed }.
save-note = L'archive d'origine n'a pas été modifiée. Les modifications en attente sont conservées jusqu'à ce que vous les abandonniez.
save-done-title = Archive enregistrée
save-cancelled = Enregistrement annulé
save-failed = Échec de l'enregistrement
save-could-not = Impossible d'enregistrer
history-undone = Modification annulée
history-redone = Modification rétablie
history-failed = Impossible de modifier l'historique

hex-title = Octets de { $name }
hex-intro = Saisissez deux chiffres hexadécimaux pour modifier un octet. La taille du fichier ne change pas ; les modifications sont mises en attente comme les autres et peuvent être annulées.
hex-bytes-aria = Octets du fichier
hex-goto-ph = décalage, p. ex. 0x400
hex-goto-aria = Aller au décalage
hex-prev = ◀ Précédent
hex-next = Suivant ▶
hex-byte-aria = Octet au décalage { $at }
hex-count = { $n ->
    [one] { $n } octet modifié
   *[other] { $n } octets modifiés
 }
hex-where = Décalage { $from } – { $to } sur { $size } ({ $human })
hex-apply = Appliquer les modifications
hex-cannot = Impossible d'afficher les octets
hex-status = Octets modifiés dans { $name } (en attente) : { $n }
hex-failed = Impossible de modifier les octets
hex-find-mode = Type de recherche
hex-mode-hex = Octets hex
hex-mode-text = Texte
hex-find-ph = rechercher, p. ex. 4D 5A
hex-find-btn = Suivant
hex-found = Trouvé au décalage { $at }
hex-not-found = Introuvable
hex-bad-pattern = Saisissez les chiffres hex par paires, p. ex. 4D 5A
reduce-title = Réduire les images
reduce-level-45 = 45 dB : meilleure qualité, gain minimal
reduce-level-40 = 40 dB : difficile à distinguer (recommandé)
reduce-level-38 = 38 dB : plus petit, légère perte dans les dégradés
reduce-level-35 = 35 dB : le plus petit, les dégradés peuvent montrer des bandes
reduce-intro = Écrit une copie de cette archive où chaque BMP en couleurs vraies est stocké avec 256 couleurs, si l'image garde la qualité choisie ci-dessous et que le fichier stocké devient plus petit. L'archive ouverte n'est pas modifiée. Le client charge des BMP 8 bits (beaucoup des siens le sont).
psnr-hint = Le PSNR mesure l'écart des couleurs : à partir de 40 dB la différence est difficile à voir, sous 35 dB les dégradés doux montrent des bandes.
reduce-notes = Le magenta (transparent) reste exact et aucune autre couleur ne devient transparente. Les images de 256 couleurs ou moins ne perdent rien.
reduce-quality = Qualité minimale
reduce-pending = Les { $n } modifications en attente sont aussi écrites dans la copie ; ces entrées ne sont pas réduites.
reduce-save-as = Enregistrer l'archive réduite sous
reduce-running = Réduction des images
reduce-status = Réduction des images dans { $dst } …
reduce-why-already_8_bit = déjà en 8 bits
reduce-why-below_min_psnr = sous la qualité requise
reduce-why-not_smaller = pas plus petits une fois stockés
reduce-why-noisy_transparency = bruit autour du magenta
reduce-why-alpha = avec alpha
reduce-why-unreadable = illisibles
reduce-summary = { $reduced } BMP sur { $matched } réduits ({ $lossless } sans perte{ $worst }), { $saved } de moins.
reduce-worst = , pire { $db } dB
reduce-kept = Laissés tels quels : { $list }.
reduce-nothing = Rien à réduire
reduce-files = Fichiers : { $files }, { $size }, { $verified }.
reduce-done-status = Réduit dans { $dst } : { $saved } de moins
reduce-done-title = Images réduites
reduce-cancelled = Réduction annulée
reduce-failed = Échec de la réduction
reduce-could-not = Impossible de réduire
reduce1-btn = 8 bits…
reduce1-btn-title = Stocker cette image avec 256 couleurs (plus petite ; le magenta reste transparent). Affiche d'abord les deux pour comparer (en attente)
reduce1-cannot = Impossible de réduire cette image
reduce1-title = Réduire à 8 bits ?
reduce1-lossless = sans perte : l'image a 256 couleurs au plus
reduce1-psnr = PSNR { $db } dB
reduce1-info = { $before } → { $after } décompressé, { $colors } couleurs, { $quality }. Le magenta (transparent) reste exact.
reduce1-original = Original
reduce1-8bit = 8 bits
reduce1-go = Réduire
reduce1-status = { $name } réduit à 8 bits (en attente)

pvw-select = Sélectionnez une entrée pour l'aperçu.
pvw-loading = Chargement …
pvw-meta = { $size } décompressé, { $stored } stocké ({ $codec }), décalage { $offset }
pvw-staged-removed = en attente : retiré, affiché tel qu'il sera enregistré
pvw-staged-added = en attente : ajouté, affiché tel qu'il sera enregistré
pvw-staged-changed = en attente : modifié, affiché tel qu'il sera enregistré
pvw-hex-title = Afficher chaque octet de ce fichier et modifier des octets isolés (en attente)
pvw-files = { $n ->
    [one] { $n } fichier
   *[other] { $n } fichiers
 }
pvw-frames = { $n ->
    [one] { $n } image
   *[other] { $n } images
 }
pvw-folder-meta = Dossier — { $files }, { $size } décompressé
pvw-open = Ouvrir
pvw-folder-note = « Extraire… » et « Retirer » agissent sur ce dossier lorsqu'il est sélectionné.
pvw-action = Action
pvw-action-n = Action { $n }
pvw-anim-meta = { $frames }, { $ms } ms chacune
pvw-anim-edit = Modifier les images…
pvw-anim-edit-title = Modifier les calques, ancres et images de cette animation (en attente)
pvw-anim-interval = Définir l'intervalle…
pvw-anim-interval-title = Modifier la durée d'image de cette action (en attente, enregistrée avec « Enregistrer… »)
pvw-warning = avertissement : { $note }
pvw-model-hint = { $n } triangles — glisser pour tourner, molette pour zoomer, double-clic pour réinitialiser
pvw-lighting = Éclairage
pvw-lightmaps = Lightmaps
pvw-models = Modèles
pvw-water = Eau
pvw-wireframe = Fil de fer
pvw-animate = Animer
pvw-all-title = Afficher ou masquer ensemble l'éclairage, les lightmaps, les modèles et l'eau
pvw-hide-all = Tout masquer
pvw-show-all = Tout afficher
pvw-reset-view = Réinitialiser la vue
pvw-reset-view-title = Retour à la vue de départ (aussi : double-clic sur l'image)
pvw-save-picture = Enregistrer l'image…
pvw-save-picture-title = Enregistrer la vue telle qu'elle est (avec ce qui est désactivé) en PNG, plus grande qu'à l'écran
pvw-extract-map = Extraire la carte…
pvw-extract-map-title = Écrire cette carte avec les fichiers dont elle a besoin dans un dossier ; les parties désactivées ici peuvent être omises
pvw-map-hint = Glisser pour tourner, glisser avec le clic droit ou Maj, ou les flèches, pour déplacer ; molette pour zoomer ; double-clic pour réinitialiser.
pvw-effect = { $frames } affichées à { $fps } i/s
pvw-video-open = Ouvrir avec le lecteur du système
pvw-video-open-title = Écrire le fichier dans un dossier temporaire et l'ouvrir avec le programme que votre système a pour lui
pvw-video-opened = { $path } ouvert. Si rien ne s'est lancé, votre système n'a pas de lecteur pour ce format (VLC et RAD Video Tools lisent Bink).
pvw-video-info = Vidéo Bink, { $w } × { $h }, { $frames } à { $fps } i/s ({ $secs } s), { $size }.
pvw-video-note = Cette fenêtre ne peut pas lire Bink, un format à part. Il reste inchangé dans l'archive.
pvw-hex-head = { $n } octets, 512 premiers affichés
sp-cannot = Impossible d'enregistrer l'image
sp-not-ready = La vue 3D n'est pas prête.
sp-scale-1 = Comme à l'écran
sp-scale-2 = 2 × (recommandé)
sp-scale-4 = 4 ×
sp-title = Enregistrer l'image
sp-intro = La vue telle qu'elle est : même angle, et seulement ce qui est activé. Une image plus grande est redessinée à cette taille, pas agrandie.
sp-size = Taille
sp-choose-file = Choisir le fichier…
sp-save-as = Enregistrer l'image sous
sp-filter = Images PNG
sp-status = Image enregistrée : { $path } ({ $w } × { $h }, { $size })
sp-failed = Impossible d'enregistrer l'image
em-part-models = Modèles et leurs textures
em-part-water = Images d'eau
em-part-sounds = Sons
em-part-minimap = Mini-carte
em-part-effects = Script d'effets et ses textures
em-title = Extraire la carte
em-intro = Les fichiers de monde, de sol et d'altitude et les textures du sol sont toujours écrits. Les parties désactivées dans la vue sont décochées ici. Les fichiers laissés de côté manquent seulement dans le dossier ; la carte les cite toujours, sauf si vous les retirez plus bas.
em-choose-folder = Choisir le dossier…
em-dest = Dossier pour la carte (les fichiers vont dessous sous la forme data\...)
em-running = Extraction de la carte …
em-exists-title = Les fichiers existent déjà
em-exists-body = { $error }. Garder les deux, avec un numéro ajouté aux noms des nouveaux fichiers ?
em-keep-both = Garder les deux
em-nothing = Rien extrait
em-status = Carte { $map } : { $files } écrits dans { $dest }
em-body = { $files } de { $map } écrits dans { $dest }.
em-skipped = { $files } omis ({ $parts }).
em-missing = { $files } que la carte cite ne sont pas dans cette archive (peut-être dans un autre GRF du client) :
em-damaged = { $files } sont endommagés et ont été écrits dans la mesure où ils étaient lisibles.
em-done-gaps = Carte extraite, avec des manques
em-done = Carte extraite
em-failed = Impossible d'extraire la carte

open-status = { $name } : { $format }, { $files }, { $size }

act-title = Modifier l'animation : { $name }
act-frame-label = Image { $n } / { $total }
act-play = Lire
act-stop = Arrêter
act-play-title = Lire l'action à sa durée d'image, avec les sons de ses images
act-ms = ms/image
act-copy-frame = Copier l'image
act-copy-frame-title = Insérer une copie après cette image
act-inbetween = Intermédiaires…
act-inbetween-title = Ajouter des images qui se fondent en douceur d'une image à la suivante
act-frame-earlier = Image ←
act-frame-earlier-title = Déplacer l'image d'une place plus tôt
act-frame-later = Image →
act-frame-later-title = Déplacer l'image d'une place plus tard
act-copy-action = Copier l'action
act-copy-action-title = Insérer une copie de cette action après elle
act-action-up = Action ↑
act-action-up-title = Déplacer l'action d'une place plus tôt
act-action-down = Action ↓
act-action-down-title = Déplacer l'action d'une place plus tard
act-remove-action = Retirer l'action
act-remove-frame = Retirer l'image
act-stage-role = éditeur d'images
act-stage-aria = Image { $n }. Cliquez sur un calque pour le sélectionner et faites-le glisser pour le déplacer ; les flèches déplacent le calque sélectionné d'1 pixel, avec Maj de 10.
act-stage-sel = Le calque { $n } est sélectionné.
act-anchor-title = Ancre { $n } : faites-la glisser pour la déplacer
act-no-picture = Pas d'image : { $error }
act-stage-note = Les lignes marquent l'origine ; les points rouges sont des ancres. Les calques sont dessinés dans l'ordre, le dernier au-dessus.
act-th-type = Type
act-th-image = Image
act-th-mirror = Miroir
act-th-scale = Échelle
act-th-scale-xy = Échelle x / y
act-th-rot = Rot.
act-th-colour = Couleur
act-type-palette = palette ({ $n })
act-type-rgba = couleurs vraies ({ $n })
act-layer-select = Sélectionner ce calque (le faire glisser sur l'image ou y utiliser les flèches)
act-layer-earlier = Dessiner plus tôt (dessous)
act-layer-later = Dessiner plus tard (dessus)
act-layer-remove = Retirer le calque
act-add-layer = + Calque
act-anchors-heading = Ancres (où se fixent d'autres sprites, p. ex. les couvre-chefs)
act-add-anchor = + Ancre
act-no-anchors = ACT { $version } n'a pas d'ancres.
act-sound = Son
act-sound-none = aucun
act-sound-title = Fichier son joué avec cette image, p. ex. effect\hit.wav (vide = aucun)
act-sound-play-title = Lire ce son (data\wav\ dans l'archive)
act-sound-play-aria = Lire le son de cette image
act-interval-title = Intervalle de l'action { $n }
act-interval-prompt = Durée par image en millisecondes (le fichier stocke des multiples de 25 ms) :
act-interval-set = Définir
act-interval-status = Intervalle de l'action { $n } en attente ; enregistrez avec « Enregistrer… ».
act-interval-failed = Impossible de définir l'intervalle
act-remove-title = Retirer l'action { $n } ?
act-remove-body = Les actions suivantes reculent d'une place. Le client choisit les actions par numéro (direction, état), donc d'autres fichiers qui les utilisent peuvent changer de sens.
ip-title = Ajouter des images intermédiaires
ip-intro = Ajoute des images qui font évoluer position, échelle, rotation, couleur et ancres par étapes d'une image à la suivante. L'image et le miroir changent à mi-chemin, et les sons restent sur les images d'origine.
ip-count = Images intermédiaires par paire
ip-whole = Toute l'action (les { $frames } images), pas seulement l'image { $frame } et la suivante
ip-smooth = Doux : lent au début et à la fin de chaque pas
ip-loop = Boucle : la dernière image se fond aussi dans la première
ip-keep = Garder la vitesse : raccourcir la durée d'image pour que l'action dure autant qu'avant (action entière seulement)
ip-last = C'est la dernière image : sans « Boucle » ou « Toute l'action », il n'y a pas d'image suivante avec laquelle fondre.
ip-go = Ajouter les images
ref-title = Ajouter une animation de référence
ref-intro = Une autre animation de cette archive dessinée avec celle que vous modifiez, p. ex. un corps et une tête pour ajuster des couvre-chefs. Sa première ancre rejoint la première ancre de l'image modifiée, comme le client fixe les parties au corps. Même numéro d'action et d'image ; la référence est seulement affichée, jamais modifiée.
ref-animation = Animation (.act)
ref-behind-check = Derrière l'animation modifiée
ref-preset-body-m = Corps, novice (homme)
ref-preset-body-f = Corps, novice (femme)
ref-preset-head-m = Tête 1 (homme)
ref-preset-head-f = Tête 1 (femme)
ref-label = Références
ref-add-btn = Ajouter une référence…
ref-behind = Derrière
ref-front = Devant
ref-toggle-title = Dessiner la référence derrière ou devant l'animation modifiée
ref-remove-aria = Retirer la référence { $path }
te-title = Modifier { $name }
te-partial = Des parties de ce fichier n'ont pas pu être décompilées et restent sous forme de listing ; elles ne peuvent pas être recompilées. Ne modifiez que ce que le texte montre comme Lua.
te-compile = Compiler et mettre en attente
te-stage = Mettre en attente
te-compiling = Compilation …
te-staging = Mise en attente …
te-status = { $name } en attente. Enregistrez avec « Enregistrer… ».
te-status-compiled = { $name } en attente (recompilé). Enregistrez avec « Enregistrer… ».

enc-title = Archive chiffrée
enc-body = Cette archive est chiffrée (méthode de GRF Editor). Saisissez son mot de passe pour lire les fichiers.
enc-wrong = Ce mot de passe ne correspond pas à l'archive.
enc-skip = Ouvrir sans mot de passe
enc-needs = La table des fichiers de cette archive est chiffrée ; un mot de passe est nécessaire pour l'ouvrir.
open-another-title = Ouvrir une autre archive ?
open-another-body = { $n ->
    [one] { $n } modification en attente de l'archive ouverte n'est pas encore enregistrée dans une archive. Elle est conservée comme brouillon et reproposée à la prochaine ouverture de cette archive.
   *[other] { $n } modifications en attente de l'archive ouverte ne sont pas encore enregistrées dans une archive. Elles sont conservées comme brouillon et reproposées à la prochaine ouverture de cette archive.
 }
open-running = Ouverture de { $path } …
open-failed-status = Impossible d'ouvrir l'archive
open-failed = Ouverture impossible
draft-title = Restaurer les modifications non enregistrées ?
draft-body = { $n ->
    [one] { $n } modification du { $when } n'a pas été enregistrée dans une archive. Elle a été conservée comme brouillon (par exemple parce que l'application s'est fermée).
   *[other] { $n } modifications du { $when } n'ont pas été enregistrées dans une archive. Elles ont été conservées comme brouillon (par exemple parce que l'application s'est fermée).
 }
draft-archive-changed = L'archive elle-même a changé depuis. Vérifiez les modifications avant d'enregistrer.
draft-missing = { $n ->
    [one] { $n } fichier de remplacement n'existe plus sur le disque et sera omis.
   *[other] { $n } fichiers de remplacement n'existent plus sur le disque et seront omis.
 }
draft-more = { $n ->
    [one] { $n } brouillon plus ancien de cette archive suit, un à chaque ouverture.
   *[other] { $n } brouillons plus anciens de cette archive suivent, un à chaque ouverture.
 }
draft-later-note = « Plus tard » conserve ce brouillon pour une autre fois ; les modifications que vous faites maintenant sont enregistrées dans un brouillon à part.
draft-restore = Restaurer
draft-discard = Abandonner
draft-later = Plus tard
draft-restored = { $n ->
    [one] { $n } modification restaurée
   *[other] { $n } modifications restaurées
 }
draft-restored-skipped = { $restored }, { $skipped } omis
draft-discarded = Brouillon abandonné
draft-kept = Le brouillon est conservé pour une autre fois
draft-failed = Impossible de restaurer les modifications
txt-encoding = Encodage :
txt-shortened = (raccourci)
txt-edit = Modifier…
txt-edit-title = Modifier le texte ; le Lua compilé est recompilé (en attente, enregistré avec « Enregistrer… »)
pal-adjust = Ajuster…
pal-adjust-title = Décaler la teinte et mettre à l'échelle la saturation et la luminosité d'une plage de couleurs (en attente)
pal-click-hint = Cliquez sur une couleur pour la modifier.
pal-click-title = Cliquez sur une couleur pour la modifier
pal-colour = Couleur { $n }
pal-zero = L'indice 0 est la couleur transparente dans le client ; sa couleur est rarement visible.
pal-number = Couleur numéro { $n } sur 255 :
pal-changed = Couleur { $n } de { $name } modifiée (en attente) ; enregistrez avec « Enregistrer… ».
pal-read-failed = Impossible de lire la palette
pal-adjust-heading = Ajuster les couleurs de { $name }
pal-range-note = La couleur 0 est la transparente, la plage commence donc à 1.
pal-from = De la couleur
pal-to = à
pal-hue = Décalage de teinte (°)
pal-sat = Saturation ×
pal-bri = Luminosité ×
dlg-apply = Appliquer
pal-adjusted = Couleurs de { $name } ajustées (en attente) ; enregistrez avec « Enregistrer… ».
pal-failed = Impossible de modifier la palette
pal-pick = Palette pour { $name }
pal-filter = Palette
pal-set-status = Palette de { $name } modifiée (en attente) ; enregistrez avec « Enregistrer… ».
pal-set-failed = Impossible de définir la palette
spr-add = Ajouter une image…
spr-add-title = Ajouter un PNG comme nouvelle image (en attente)
spr-setpal = Définir la palette…
spr-setpal-title = Utiliser les couleurs d'un fichier .pal (en attente)
spr-pixels = Pixels…
spr-pixels-title = Dessiner sur cette image : crayon, gomme, remplissage et pipette avec les couleurs de la palette (en attente)
spr-replace = Remplacer…
spr-replace-title = Remplacer cette image par un PNG (en attente, enregistré avec « Enregistrer… »)
spr-remove-title = Retirer cette image ; l'animation voisine suit (en attente)
spr-replace-pick = PNG pour l'image { $frame } de { $name }
spr-png-filter = Image PNG
spr-replace-status = Image { $frame } de { $name } en attente (les couleurs sont ramenées à la palette du sprite) ; l'aperçu montre le résultat. Enregistrez avec « Enregistrer… ».
spr-replace-failed = Impossible de remplacer l'image
spr-add-pick = PNG à ajouter à { $name }
spr-add-status = Image ajoutée à { $name } (en attente) ; enregistrez avec « Enregistrer… ».
spr-add-failed = Impossible d'ajouter l'image
spr-remove-heading = Retirer l'image { $n } ?
spr-remove-body = Les images suivantes reculent d'une place. Les calques de l'animation voisine de ce sprite qui utilisent l'image deviennent vides, et les suivants suivent.
spr-remove-status = Image { $frame } de { $name } retirée (en attente) ; enregistrez avec « Enregistrer… ».
spr-remove-failed = Impossible de retirer l'image
pix-error = Pixels
pix-canvas-aria = Image { $frame }, { $w } sur { $h } pixels. Les flèches déplacent le curseur, la barre d'espace dessine.
pix-size = { $w } x { $h } px
pix-readout = { $w } x { $h } px — x { $x }, y { $y } : couleur { $c } (#{ $hex })
pix-readout-t = { $w } x { $h } px — x { $x }, y { $y } : couleur { $c } (#{ $hex }), transparent
pix-pencil = Crayon
pix-eraser = Gomme
pix-fill = Remplir
pix-picker = Pipette
pix-zoom = Zoom
pix-grid = Grille
pix-revert = Rétablir
pix-palette = Palette
pix-palette-heading = Palette (cliquez sur une couleur ; 0 est transparente)
pix-swatch-zero =  (transparent)
pix-title = Pixels de l'image { $frame } — { $name }
pix-nochange = Aucun pixel modifié.
pix-status = Image { $frame } de { $name } modifiée (en attente) ; enregistrez avec « Enregistrer… ».
pix-failed = Impossible de mettre les pixels en attente
gat-alt = carte, { $x } sur { $y } cellules
gat-x-from = x de
gat-y-from = y de
gat-to = à
gat-type-aria = Type de cellule
gat-type-nochange = type : inchangé
gat-height-ph = inchangée
gat-height = hauteur
gat-set = Définir les cellules
gat-set-title = Définir le type et/ou la hauteur des cellules choisies (en attente, enregistré avec « Enregistrer… »)
gat-drag = Faites glisser sur la carte pour choisir des cellules.
gat-readout = x { $xa }–{ $xb }, y { $ya }–{ $yb } : { $cells }
gat-cells = { $n ->
    [one] { $n } cellule
   *[other] { $n } cellules
 }
gat-select-all = Tout sélectionner
gat-need = Choisissez d'abord un type de cellule et/ou une hauteur.
gat-status = { $cells } de { $name } modifiées (en attente) ; enregistrez avec « Enregistrer… ».
gat-failed = Impossible de modifier les cellules

webgl-missing = WebGL 2 n’est pas disponible dans cette fenêtre.

pvw-n-empty-marker = Entrée marqueur vide
pvw-n-sound-fail = Ce son ne peut pas être lu ici : { $error }
pvw-n-luac-fail = Lua compilé (bytecode) qui ne peut pas encore être affiché : { $error }
pvw-n-palette = Palette : 256 couleurs
pvw-n-palette-label = 256 couleurs (l’indice 0 est transparent)
pvw-n-gat = GAT { $version } – { $w } x { $h } cellules, hauteurs de { $lo } à { $hi }
pvw-n-gat-type = type { $k } ({ $name }) : { $cells }
pvw-n-gnd = GND { $version } – { $w } x { $h } cellules, { $tiles }, { $lightmaps }, { $textures }
pvw-n-rsw = RSW { $version } – sol { $ground }, { $models }, { $lights }, { $sounds }, { $effects }
pvw-n-instances = { $instances } de { $models }
pvw-n-models-missing = { $n } introuvables : { $names }
pvw-n-textures = { $n }
pvw-n-textures-missing = { $n ->
    [one] { $n } texture manquante
   *[other] { $n } textures manquantes
 }
pvw-n-rsm = RSM { $version } – { $nodes }, { $triangles }, { $textures }
pvw-n-rsm-loop = L’animation boucle toutes les { $secs } s
pvw-n-node = nœud { $name } ({ $v }, { $f })
pvw-n-node-moves = nœud { $name } ({ $v }, { $f }), se déplace
pvw-n-str = STR – { $fps } i/s, { $frames }, { $layers }, { $textures }
pvw-n-str-step = Seule une image sur { $step } est affichée
pvw-n-str-missing = textures manquantes ({ $n }) :
pvw-n-str-simple = Aperçu simplifié : modes de fusion et animation par clés comme dans le client, sans éclairage, sur fond sombre.
pvw-n-spr = SPR { $version } – { $pal }, { $rgba }
pvw-n-warning = avertissement : { $text }
pvw-n-spr-limit = affichage des { $shown } premières images sur { $total }
pvw-n-act = ACT { $version } – { $actions }, { $frames }, { $layers }
pvw-n-act-nospr = Le .spr correspondant n’est pas dans cette archive, il n’y a donc rien à dessiner.
pvw-n-act-none = L’animation n’a aucune action
pvw-n-act-empty = L’action { $action } ne dessine rien
gat-tn-0 = praticable
gat-tn-1 = non praticable
gat-tn-2 = non praticable, non ciblable
gat-tn-3 = praticable (eau)
gat-tn-4 = inconnu
gat-tn-5 = non praticable, ciblable
gat-tn-6 = praticable
gat-tn-weird = inhabituel (bit de poids fort activé)

un-tiles = { $n ->
    [one] { $n } tuile
   *[other] { $n } tuiles
 }
un-lightmaps = { $n ->
    [one] { $n } carte de lumière
   *[other] { $n } cartes de lumière
 }
un-textures = { $n ->
    [one] { $n } texture
   *[other] { $n } textures
 }
un-models = { $n ->
    [one] { $n } modèle
   *[other] { $n } modèles
 }
un-lights = { $n ->
    [one] { $n } lumière
   *[other] { $n } lumières
 }
un-sounds = { $n ->
    [one] { $n } son
   *[other] { $n } sons
 }
un-effects = { $n ->
    [one] { $n } effet
   *[other] { $n } effets
 }
un-nodes = { $n ->
    [one] { $n } nœud
   *[other] { $n } nœuds
 }
un-triangles = { $n ->
    [one] { $n } triangle
   *[other] { $n } triangles
 }
un-vertices = { $n ->
    [one] { $n } sommet
   *[other] { $n } sommets
 }
un-faces = { $n ->
    [one] { $n } face
   *[other] { $n } faces
 }
un-palimg = { $n ->
    [one] { $n } image à palette
   *[other] { $n } images à palette
 }
un-rgbaimg = { $n ->
    [one] { $n } image en couleurs vraies
   *[other] { $n } images en couleurs vraies
 }
un-actions = { $n ->
    [one] { $n } action
   *[other] { $n } actions
 }
un-layers = { $n ->
    [one] { $n } calque
   *[other] { $n } calques
 }
un-modelinst = { $n ->
    [one] { $n } instance
   *[other] { $n } instances
 }
un-diffmodels = { $n ->
    [one] { $n } modèle différent
   *[other] { $n } modèles différents
 }

err-target-open = La cible ne doit pas être l’archive ouverte
err-alpha = L’image a un canal alpha, que les BMP 8 bits ne peuvent pas conserver
err-saved-png = L’image est enregistrée en .png
err-not-png = Ce n’est pas une image PNG
err-no-palette = Le sprite n’a pas de palette
err-interval-positive = L’intervalle doit être un nombre positif de millisecondes
err-merge-two = Choisissez au moins deux archives
err-merge-target = La cible ne doit pas être l’une des archives d’entrée
err-empty-marker = Cette entrée est un marqueur vide
err-enter-path = Saisissez le chemin dans l’archive, par ex. data\texture\a.bmp
err-name-chars = Le chemin contient des caractères que ce mode de noms ne peut pas écrire
err-marked-removal = L’entrée est marquée pour suppression
err-no-spr = Le .spr correspondant n’est pas dans cette archive
err-no-changes = Il n’y a aucune modification à enregistrer
err-entry-encrypted = L’entrée est chiffrée (le déchiffrement n’est pas encore pris en charge)
err-map-large = La carte est trop grande
err-search-len = La recherche nécessite de 1 à 256 octets
err-no-byte = Aucun octet n’a été modifié
err-no-draft = Il n’y a pas de brouillon pour cette archive
err-already-bits = L’image a déjà { $bits } bits par pixel
err-not-file = { $path } n’est pas un fichier
err-file-4gib = { $path } dépasse 4 Gio
err-text-chars = Le texte contient des caractères que { $encoding } ne peut pas représenter
err-preview-large = { $mb } Mo est trop volumineux pour l’aperçu
err-map-part = « { $part } » n’est pas une partie de carte qui peut être omise
err-number = { $what } doit être un nombre raisonnable
err-act-keep-action = Une animation garde au moins une action
err-act-no-action = Cette action n’existe pas
err-act-interval = L’intervalle doit être compris entre 1 et 60000 ms
err-act-no-frame = Cette image n’existe pas
err-act-keep-frame = Une action garde au moins une image
err-act-no-frame-n = L’image { $n } n’existe pas
err-act-image-number = Le numéro d’image est -1 (vide) ou plus
err-act-image-type = Le type d’image est 0 (palette) ou 1 (couleurs vraies)
err-act-one-scale = Cette version d’ACT a une seule échelle pour les deux directions
err-act-too-many-layers = Trop de calques
err-act-no-layer-n = Le calque { $n } n’existe pas
err-act-no-layer = Ce calque n’existe pas
err-act-no-anchors = Cette version d’ACT n’a pas d’ancres
err-act-too-many-anchors = Trop d’ancres
err-act-no-anchor-n = L’ancre { $n } n’existe pas
err-act-no-frames = L’action n’a aucune image
err-gat-need = Choisissez un type et/ou une hauteur à appliquer
err-gat-height = La hauteur doit être un nombre raisonnable
err-gat-type = Le type de cellule va de 0 à 6
err-pal-range = Les numéros de couleur vont de 0 à 255
err-pal-order = La première couleur est après la dernière
err-pal-numbers = La teinte, la saturation et la luminosité doivent être des nombres
err-pal-factors = La saturation et la luminosité sont des facteurs d’au moins 0
err-spr-size = L’image fait { $w } x { $h } pixels, les données contiennent { $n } valeurs
err-no-archive = Aucune archive n’est ouverte
err-job-running = Une autre tâche est déjà en cours

menu-new = Nouveau
menu-new-grf = Archive GRF...
menu-new-gpf = Correctif GPF...
menu-new-rgz = Correctif RGZ...
menu-new-thor = Correctif Thor...
menu-new-window = Nouvelle fenêtre
menu-recent = Ouvrir récent
menu-recent-none = Aucune archive récente
menu-recent-clear = Effacer le menu
menu-close-archive = Fermer l’archive
menu-save = Enregistrer
menu-save-as = Enregistrer sous...
menu-save-advanced = Enregistrement avancé
menu-save-repack = Réempaqueter (recompresser)...
menu-save-compact = Compacter (copier les données)...
menu-save-encrypt = Chiffrer...
menu-save-decrypt = Enregistrer déchiffré...
menu-archive-settings = Réglages de l’archive...
menu-changes = Vérifier les modifications...
menu-tools = Outils
menu-hash = Empreintes...
menu-compare = Comparer avec une archive...
menu-data-folder = Ouvrir le dossier de données
menu-about = À propos de RustyGRF...
open-elsewhere = Cette archive est ouverte dans une autre fenêtre.
err-open-elsewhere = L’archive est ouverte dans une autre fenêtre
untitled = Sans titre
new-failed = Impossible de créer une nouvelle archive
close-title = Fermer l’archive ?
close-body = { $n } modifications non enregistrées sont gardées comme brouillon et proposées à la prochaine ouverture.
close-anyway = Fermer
save-here = Enregistrer
save-as-button = Enregistrer sous…
save-nothing = Rien à enregistrer : aucune modification.
save-here-done = { $name } enregistré : { $files }, { $size }, vérifié.
sa-title = Enregistrer sous
sa-title-repack = Réempaqueter (recompresser)
sa-title-compact = Compacter
sa-title-encrypt = Chiffrer
sa-title-decrypt = Enregistrer déchiffré
as-title = Réglages de l’archive
sa-store = Fichiers
sa-store-copy = Copier tels quels (rapide, compacte l’archive)
sa-store-recompress = Recompresser chaque fichier
sa-format-keep = Conserver ({ $format })
sa-encryption = Chiffrement
sa-enc-keep = Conserver le mot de passe
sa-enc-none = Retirer le chiffrement
sa-enc-plain = Aucun
sa-enc-password = Chiffrer avec un mot de passe
sa-password = Mot de passe
sa-password-again = Confirmer le mot de passe
sa-password-mismatch = Les deux mots de passe diffèrent.
err-password-empty = Saisissez un mot de passe
sa-hint = « Enregistrer » écrit l’archive à côté d’elle, la vérifie puis la remplace. Le chiffrement ne fonctionne qu’avec GRF 0x200 ; GRF Editor et les clients avec sa DLL le lisent. Thor et RGZ sont écrits selon le nom de fichier choisi.
sa-filter = Fichiers { $type }
as-path = Fichier
as-unsaved = pas encore enregistré
as-type = Type
as-content = Contenu
as-encryption = Chiffrement
as-encrypted = Mot de passe GRF Editor
as-plain = aucun
as-names = Noms affichés en
err-save-same = Choisissez un autre fichier : « Enregistrer » écrit dans l’archive ouverte
err-save-unverified = L’archive écrite n’a pas passé la vérification ; l’original est inchangé
err-save-replace = Impossible de remplacer { $path } ({ $error }). L’archive enregistrée est { $tmp }.
err-encrypt-format = Le chiffrement n’est possible qu’avec GRF 0x200
err-patch-empty = Un correctif doit contenir au moins un fichier
hash-title = Empreintes
hash-pick = Sélectionnez d’abord un fichier.
hash-size = Taille
cmp-pick = Comparer avec quelle archive ?
cmp-running = Comparaison …
cmp-failed = Comparaison impossible
cmp-title = Différences
cmp-same = Même contenu
cmp-summary = Comparé à { $other } : { $here } seulement ici, { $changed } modifiés, { $there } seulement là-bas, { $same } identiques.
cmp-only-here = Seulement dans cette archive
cmp-changed = Modifiés
cmp-only-there = Seulement dans l’autre archive
cmp-patch-hint = Un correctif contient ce qui est nouveau ou modifié ici ; un correctif Thor supprime aussi ce qui n’est que dans l’autre archive.
cmp-make-patch = Enregistrer comme correctif…
cmp-patch-pick = Enregistrer le correctif sous
cmp-patch-running = Écriture du correctif …
cmp-patch-done = Correctif écrit
cmp-patch-result = { $added } nouveaux, { $changed } modifiés, { $removed } supprimés ; { $size }, écrit dans { $dst }.
about-title = À propos de RustyGRF
about-version = Version { $version }
about-tagline = Un outil pour les archives, sprites, animations, palettes et cartes de Ragnarok Online, sur macOS, Windows et Linux.
about-made-by = Réalisé par
about-thanks = Remerciements
about-tokeiburu = GRF Editor et ACT Editor, l’inspiration ; formats repris avec son autorisation
about-lua = Lua 5.0 et 5.1 (MIT), pour vérifier et compiler les fichiers Lua
about-tauri = la fenêtre de l’application
about-license = Sous licence MIT ou Apache-2.0. Sans lien avec Gravity.
save-as-done = Enregistré sous { $dst } ; cette fenêtre l’affiche maintenant.
save-unverified = L’archive enregistrée n’a pas passé la vérification : { $dst }
menu-search-contents = Rechercher dans les fichiers...
sc-title = Rechercher dans les fichiers
sc-query = Texte
sc-query-ph = ce qu’il faut trouver, par ex. prontera
sc-files = Fichiers
sc-files-ph = *.lua;*.txt (vide : tous)
sc-regex = Expression régulière
sc-case = Respecter la casse
sc-word = Mots entiers
sc-in-list = Seulement les fichiers de la liste
sc-search = Rechercher
sc-running = Recherche …
sc-result = { $lines } lignes dans { $files } (sur { $searched } parcourus).
sc-more = D’autres lignes correspondent que celles listées.
sc-none = Rien trouvé dans { $searched }.
sc-show = Afficher dans la liste
err-search-empty = Saisissez le texte à trouver
err-bad-pattern = L’expression régulière n’est pas valide : { $error }
menu-undo = Annuler
menu-redo = Rétablir
menu-history = Historique des modifications...
menu-nav-back = Précédent
menu-nav-forward = Suivant
menu-text-encoding = Encodage du texte...
hist-add = Ajout de { $name }
hist-replace = Remplacement de { $name }
hist-edit = Modification de { $name }
hist-remove = Suppression de { $name }
hist-discard = Modification de { $name } abandonnée
hist-mixed = Modification de { $name }
hist-add-n = Ajout de { $n } fichiers
hist-replace-n = Remplacement de { $n } fichiers
hist-edit-n = Modification de { $n } fichiers
hist-remove-n = Suppression de { $n } fichiers
hist-discard-n = { $n } modifications abandonnées
hist-mixed-n = Modification de { $n } fichiers
history-undone-n = { $n } étapes annulées
history-redone-n = { $n } étapes rétablies
hist-title = Historique des modifications
hist-redo-head = Peut être rétabli
hist-now = ▸ Maintenant
hist-undo-head = Peut être annulé (le plus récent d’abord)
hist-empty = Rien à annuler.
hist-hint = Un clic revient (ou avance) jusqu’à cette étape. Jusqu’à 200 étapes par archive ; l’historique s’arrête à l’enregistrement, les modifications survivent à un plantage sous forme de brouillon. Un clic droit sur Annuler ou Rétablir affiche la même liste.
te-title = Encodage du texte
te-intro = Un texte lu dans le mauvais encodage (par exemple un nom coréen affiché en Latin-1) redevient lisible : il est réécrit dans l’encodage d’affichage puis lu dans le bon.
te-source = Texte tel qu’affiché
te-result = Résultat
te-from = Affiché en
te-to = Lire en
te-swap = Inverser
te-bytes = Octets : { $hex }
te-lossy = Certains caractères n’existent pas dans l’un des deux encodages et ont été remplacés.
as-magic = Signature (magic)
as-magic-reset = Habituel
err-bad-magic = La signature n’est pas valide : 16 caractères ASCII au plus ({ $error })
menu-flat-maps = Cartes plates...
pvw-flat-map = Modifier la carte…
pvw-flat-map-title = Sol plat, sans ombres, lumière, objets ni eau, dans cette archive (Annuler revient en arrière)
flat-flatten = Sol plat (ni collines ni murs ; ni objets ni eau non plus)
flat-remove_shadow = Retirer les ombres
flat-remove_light = Retirer la lumière colorée
flat-remove_color = Retirer les couleurs des tuiles
flat-gat_shadows = Ombres d'après la grille de marche (cases bloquées sombres ; remplace lumière et ombres)
flat-reset_light = Lumière du soleil neutre
flat-remove_models = Retirer les modèles
flat-remove_objects = Retirer tous les objets (modèles, lumières, sons, effets)
flat-remove_water = Retirer l'eau
flat-notes = Les règles suivent le créateur de cartes plates de GRF Editor. Les versions des fichiers restent inchangées ; l'eau passe sous le sol. La grille de marche (où l'on peut aller) ne change pas.
flat-none = Rien n'est coché.
flat-failed = Impossible de modifier la carte
flat1-title = Modifier la carte
flat1-intro = Les fichiers de monde, de sol et d'altitude de cette carte sont modifiés dans l'archive ouverte. Enregistrez pour garder la modification ; Annuler revient en arrière. Les parties désactivées dans la vue sont déjà cochées.
flat1-go = Modifier
flat1-status = Carte { $map } modifiée (pas encore enregistrée)
flats-title = Cartes plates
flats-intro = Écrit une nouvelle petite archive avec seulement les fichiers de carte modifiés. Placée avant les autres dans DATA.INI, le client utilise ces cartes ; l'archive ouverte ne change pas.
flats-selected = Carte sélectionnée
flats-in-folder = Cartes du dossier sélectionné
flats-all = Toutes les cartes
flats-pending = Les modifications en attente des fichiers de carte ({ $n } modifications en tout) sont prises en compte.
flats-save-as = Enregistrer les cartes plates sous
flats-running = Modification des cartes
flats-summary = Cartes modifiées : { $changed } sur { $maps }.
flats-problems = Laissées de côté ({ $n }) :
flats-nothing = Rien n'a changé, rien n'a été écrit
flats-written = { $files } écrits dans { $dst } ({ $size }).
flats-how = Pour l'utiliser, ajoutez-la dans DATA.INI avant les autres archives (par exemple en 0=).
flats-status = { $changed } carte(s) plate(s) écrite(s) dans { $dst }
flats-done = Cartes plates écrites
flats-cancelled = Cartes plates annulées
em-change = Modifier les fichiers de la carte
em-strip = Retirer aussi du fichier de monde les parties laissées de côté plus haut (pour que le client ne les cherche pas)
em-changed = { $files } écrits modifiés.
err-flat-not-map = Ce n'est pas un fichier de carte (.rsw, .gnd ou .gat)
err-flat-no-gnd = Le fichier de sol { $name } n'est pas dans cette archive
err-flat-failed = La carte ne peut pas être modifiée : { $error }
err-flat-nothing = Ces options ne changent rien à cette carte
ctx-open = Ouvrir
ctx-opened = Copie ouverte : { $path }
ctx-reveal = Afficher dans { $app }
ctx-fm = gestionnaire de fichiers
ctx-extract = Extraire…
ctx-extract-last = Extraire vers « { $folder } »
ctx-extract-folder = Extraire le dossier…
ctx-export = Exporter vers une archive…
ctx-export-folder = Exporter le dossier vers une archive…
ctx-map-extract = Extraire la carte…
ctx-edit = Modifier le texte…
ctx-hex = Vue hexadécimale…
ctx-reduce = Réduire à 8 bits…
ctx-replace = Remplacer par un fichier…
ctx-add-here = Ajouter un fichier ici…
ctx-rename = Renommer…
ctx-rename-folder = Renommer le dossier…
ctx-delete = Supprimer
ctx-delete-folder = Supprimer le dossier
ctx-usages = Chercher le nom dans les fichiers…
ctx-copy-name = Copier le nom
ctx-copy-path = Copier le chemin
ctx-props = Propriétés…
ctx-copied = Copié : { $text }
ren-title = Renommer le fichier
ren-title-folder = Renommer le dossier
ren-intro = Le nouveau chemin de ce fichier dans l'archive. Un autre dossier l'y déplace.
ren-intro-folder = Le nouveau chemin de ce dossier ; chaque fichier qu'il contient le suit.
ren-hint = Cela s'enregistre comme la suppression de l'ancien chemin et l'ajout du contenu sous le nouveau ; Annuler revient en arrière. Les gros dossiers (plus de 512 Mo) ne peuvent pas être renommés ainsi.
ren-go = Renommer
ren-status = { $from } renommé en { $to } (pas encore enregistré)
ren-failed = Impossible de renommer
err-rename-nothing = Il n'y a rien à renommer
err-rename-large = Le renommage garde les fichiers en mémoire ; plus de { $mb } Mo, c'est trop. Extrayez plutôt le dossier et ajoutez-le de nouveau.
err-rename-mismatch = { $name } ne se trouve pas sous le chemin renommé
err-rename-exists = { $name } existe déjà
err-export-exists = { $path } existe déjà
exp-title = Exporter vers une archive
exp-intro = { $n } fichier(s) forment une nouvelle archive ; l'archive ouverte ne change pas.
exp-method = Compression
exp-m-zlib = zlib (fonctionne dans tous les clients)
exp-m-lzma = LZMA (nécessite la cps.dll compatible LZMA)
exp-m-zopfli = Zopfli (compatible zlib, plus petit, lent)
exp-note = Le contenu est pris tel qu'il est enregistré dans l'archive ; les modifications en attente n'en font pas partie.
exp-choose = Choisir le fichier…
exp-save-as = Enregistrer la nouvelle archive sous
exp-running = Exportation
exp-done = Archive écrite
exp-done-body = { $files } écrits dans { $dst } ({ $size }, { $verified }).
exp-status = { $n } fichier(s) exporté(s) vers { $dst }
exp-failed = Impossible d'exporter
prop-title = Propriétés
prop-path = Chemin
prop-type = Type
prop-size = Taille
prop-stored = Taille stockée
prop-codec = Compression
prop-offset = Position dans le fichier
prop-flags = Indicateurs
prop-files = Fichiers
prop-total = Taille totale
