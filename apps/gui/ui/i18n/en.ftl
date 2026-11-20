# RustyGRF – English (source language). Fluent syntax, see i18n.js.
# Product and format names (GRF, GPF, Thor, RGZ, zlib, LZMA, zopfli, SPR, ACT, PAL) are not translated.

## Toolbar
tb-main = Main
    .aria-label = Main
tb-app = App
    .aria-label = App
tb-archive = Archive
    .aria-label = Archive
tb-edit = Edit
    .aria-label = Edit
tb-edit-mark = Edit
tb-view = View and filter
    .aria-label = View and filter

btn-settings = Settings
    .title = Settings (Cmd/Ctrl+,)
    .aria-label = Settings
btn-open = Open…
    .title = Open a GRF archive (Cmd/Ctrl+O)
btn-build = Build…
    .title = Pack a folder into a new GRF
btn-merge = Merge…
    .title = Merge several GRFs into a new one
btn-extract = Extract…
    .title = Extract the selected entries, or all shown
btn-verify = Verify
    .title = Check the archive for errors
btn-add = Add file…
    .title = Add a file from disk under a path in the archive
btn-replace = Replace…
    .title = Replace the selected entry with a file from disk
btn-remove = Remove
    .title = Leave the selected entry out of the saved archive
btn-undo = Undo
    .title = Undo the last staged change (Cmd/Ctrl+Z)
btn-redo = Redo
    .title = Redo (Shift+Cmd/Ctrl+Z)
btn-save = Save…
    .title = Review and save the pending changes (Cmd/Ctrl+S saves at once)

search = Search
    .placeholder = Search: a name, or a pattern like *.spr
    .aria-label = Search

pv-group = Preview position
    .aria-label = Preview position
pv-right = Right
    .title = Preview beside the list
    .aria-label = Preview on the right
pv-bottom = Bottom
    .title = Preview below the list
    .aria-label = Preview at the bottom
pv-off = Off
    .title = Hide the preview (Cmd/Ctrl+Shift+P)
    .aria-label = Hide the preview

## View bar
view-group = List layout
    .aria-label = List layout
view-tree = Tree
    .title = Folder tree with files (Cmd/Ctrl+1)
view-folders = Folders
    .title = Folders on the left, the files of the chosen folder on the right (Cmd/Ctrl+2)
view-list = List
    .title = Flat list: name, folder, size (Cmd/Ctrl+3)
view-details = Details
    .title = Flat table with sortable columns (Cmd/Ctrl+4)
view-gallery = Gallery
    .title = Thumbnail gallery of pictures and sprites (Cmd/Ctrl+5)

sort = Sort
sort-archive = Stored order
sort-name = Name
sort-path = Path
sort-type = Type
sort-size = Size
sort-stored = Stored size
sort-offset = Position in file
sort-dir = Sort direction
    .title = Ascending or descending
    .aria-label = Sort direction

kind-filter-title = Show only some kinds of file
kind-label-all = Type: all
kind-label-pictures = Type: pictures
kind-label-one = Type: { $kind }
kind-label-n = Type: { $n } kinds
kind-all = All
kind-only-images = Only images
kind-image = Images
kind-sprite = Sprites
kind-animation = Animations
kind-palette = Palettes
kind-map = Maps
kind-model = Models
kind-audio = Audio
kind-text = Text and Lua
kind-effect = Effects
kind-file = Other

size = Size
size-any = any
size-over-1k = over 1 KB
size-over-100k = over 100 KB
size-over-1m = over 1 MB
size-over-10m = over 10 MB
size-under-1k = under 1 KB
size-under-100k = under 100 KB
size-under-1m = under 1 MB

subfolders = Subfolders
    .title = Also list the files of the subfolders
expand-all = Expand all
    .title = Open every folder
collapse-all = Collapse all
    .title = Close every folder
reset-filters = Reset
    .title = Clear search, type and size filters

## List and preview
empty-hint = Open a GRF archive, or drop it on this window.
open-archive = Open archive…
folders-pane = Folders
    .aria-label = Folders
all-files = All files
    .title = Show the files of every folder
col-name = Name
col-type = Type
col-size = Size
col-stored = Stored
entries-list = Entries
    .aria-label = Entries
preview-pane = Preview
    .aria-label = Preview
split-folders = Resize the folder list
    .aria-label = Resize the folder list
split-preview = Resize the preview (arrow keys)
    .aria-label = Resize the preview (arrow keys)
    .title = Drag to resize, or focus and use the arrow keys

## Status bar
status-ready = Ready
count-entries = { $n ->
    [one] { $n } entry
   *[other] { $n } entries
 }
count-filtered = { $n } of { $total }
count-note-folders = (search covers all folders)
count-note-pictures = (pictures and sprites)
names = Names
    .title = How file names are shown and searched. Extracting always writes Latin-1 names, so the result matches `rgrf build`.
names-korean = Korean

## Pending changes
save-n = { $n ->
    [one] Review and save the { $n } pending change
   *[other] Review and save the { $n } pending changes
 }
save-none = Save a copy with other settings
save-aria-n = { $n ->
    [one] Save { $n } pending change
   *[other] Save { $n } pending changes
 }
save-aria-none = Save changes

## Common buttons
dlg-ok = OK
dlg-cancel = Cancel
dlg-close = Close
dlg-done = Done
dlg-quick = Quick
dlg-full = Full
dlg-edit-animation = Edit animation
dlg-edit-pixels = Edit pixels
dlg-edit-text = Edit text

## Verify
verify-title = Verify
verify-body = A quick check reads only the file table. A full check also decodes every entry; that reads the whole archive.
verify-running = Verifying …
verify-status = Verified: { $errors } errors, { $warnings } warnings
verify-summary-full = { $entries } entries, all decoded: { $errors } errors, { $warnings } warnings, { $infos } notes.
verify-summary-quick = { $entries } entries, table only: { $errors } errors, { $warnings } warnings, { $infos } notes.
verify-problems = Problems found
verify-ok = Archive checked
verify-failed = Verification failed

## Settings
set-title = Settings
    .aria-label = Settings
set-appearance = Appearance
set-theme = Theme
set-theme-system = Follow the system
set-theme-light = Light
set-theme-dark = Dark
set-accent = Accent colour
set-accent-orange = orange
set-accent-blue = blue
set-accent-green = green
set-accent-purple = purple
set-textsize = Text size
set-small = Small
set-normal = Normal
set-large = Large
set-density = List density
set-compact = Compact
set-comfortable = Comfortable
set-fileicons = File icons
set-plainicons = Plain icons (no colours by file type)
set-animations = Animations
set-animations-hint = Turns off the arrow and other transitions.
set-motion-off = Off
set-language = Language
set-language-system = Follow the system
set-language-beta = { $name } (Beta)
set-language-note = Translations marked Beta are machine drafts that no native speaker has checked yet. Texts not translated yet appear in English.
set-opening = When opening an archive
set-names = File names
set-names-cp949 = Korean (CP949)
set-names-cp1252 = Latin-1 (as GRF Editor shows them)
set-names-utf8 = UTF-8
set-layout = List layout
set-as-last = As last used
set-next-start = Takes effect the next time the app starts.
set-preview = Preview
set-startup = Start-up
set-reopen = Open the last archive again
set-changes = Changes
set-warn-discard = Ask before pending changes are discarded
set-building = Building archives
set-compression = Compression
set-format = Format
set-building-note = These are only the starting choices of the Build… dialog; you can change them there for each archive.
set-stored-note = Settings are stored in this app only. Shortcut: { $key }+, opens this window.
set-reset = Reset to defaults

method-zlib = zlib (every client)
    .hint = The standard. Every client can read it and it builds fast. Choose this unless you have a reason not to.
method-zopfli = zopfli (smaller, slow)
    .hint = Writes the same zlib format, so every client can still read it, but squeezes the files a little further. Building takes much longer; reading is just as fast.
method-lzma = LZMA (needs client support)
    .hint = Smaller than zlib, but only a client with LZMA support can read it (for example one with a cps.dll). Try the archive in your client before you share it.
format-0x200 = 0x200 (every client)
    .hint = The standard GRF format. Every client reads it. File positions are stored in 32 bits, so the archive cannot grow beyond 4 GB.
format-0x300 = 0x300 (over 4 GB)
    .hint = Newer format with 64-bit file positions, so the archive can be larger than 4 GB. Only newer clients read it (measured: a 2025 client does, 2020 clients do not). Use it only when 4 GB is not enough.
format-0x103 = 0x103 (very old clients)
    .hint = Very old format from before 0x200: the file table is not compressed and names and data are scrambled with the old DES variant. For clients too old to read 0x200. Same 4 GB limit.
format-0x102 = 0x102 (very old clients)
    .hint = Same as 0x103 (they differ only in the version number). Use it if an old client asks for exactly this version.

## Native menu bar
menu-file = File
menu-edit = Edit
menu-view = View
menu-window = Window
menu-help = Help
menu-settings = Settings...
menu-open = Open...
menu-build = Build from Folder...
menu-merge = Merge...
menu-extract = Extract...
menu-verify = Verify Archive
menu-reduce-images = Reduce Images...
menu-add = Add File...
menu-replace = Replace...
menu-remove = Remove
menu-layout-tree = As Tree
menu-layout-folders = As Folders
menu-layout-details = As Details
menu-layout-gallery = As Gallery
menu-preview-right = Preview on the Right
menu-preview-bottom = Preview at the Bottom
menu-preview-off = Hide Preview

## Dialogs
dlg-choose-target = Choose target…
dlg-open-it = Open it
dlg-add = Add
job-starting = Starting …
job-cancelling = Cancelling …
job-progress = { $done } of { $total }
nothing-written = Nothing was written.
verified-yes = verified
verified-no = not verified
build-pick = Folder to pack (the folder that contains data\ …)
build-title = Build archive
build-pack = Pack { $src }
build-share = Share identical files
build-share-hint = Stores files with identical content only once, which saves space. Not possible in 0x102 and 0x103.
build-verify-note = The finished archive is verified against the files on disk.
build-save-as = Save archive as
build-running = Building archive
build-status = Building { $dst } …
build-done-status = Built { $dst }: { $files } files
build-result = Files: { $files } in { $blocks } blocks ({ $from } → { $to }), { $verified }.
build-shared = Identical files sharing a block: { $n }.
build-skipped = Items skipped: { $n }
build-done-title = Archive built
build-cancelled = Build cancelled
build-failed = Build failed
build-could-not = Could not build
merge-pick = Archives to merge (at least two)
merge-need-two = Choose at least two archives.
merge-title = Merge archives
merge-higher = Higher priority
merge-lower = Lower priority
merge-intro = Order = priority: for a path in several archives, the top one wins. The inputs are not changed.
merge-replace-only = Replace only: the last archive is the base, the others add no new paths
merge-save-as = Save merged archive as
merge-running = Merging archives
merge-status = Merging into { $dst } …
merge-done-status = Merged into { $dst }: { $files } files
merge-line = Files: { $files }, used: { $used }, replaced: { $replaced }
merge-line-ignored = , new paths ignored: { $n }
merge-result = Files: { $files }, { $size }, { $verified }.
merge-damaged = Damaged blocks copied unchanged: { $n }.
merge-done-title = Archives merged
merge-cancelled = Merge cancelled
merge-failed = Merge failed
merge-could-not = Could not merge
ext-title = Extract
ext-folder = Folder ({ $n } files)
ext-entry = Selected entry
ext-all = All { $n } in the list
ext-filtered = The list is filtered; "all" means the entries that match.
ext-whole = The list shows the whole archive.
ext-dest = Extract into folder
ext-empty-folder = This folder holds no files.
ext-running = Extracting …
ext-done-status = Extracted { $files } file(s) to { $dest }
ext-done-title = Extracted
ext-done-body = { $files } file(s) written to { $dest }.
ext-damaged = { $n } entries were damaged; their recoverable part was written.
ext-stopped = Extraction stopped
ext-could-not = Could not extract
ext-rename = Rename new files
replace-pick = File that replaces { $name }
replace-status = Replacement staged for { $name }; save it with Changes…
replace-failed = Could not stage the replacement
add-pick = File to add
add-title = Add file
add-prompt = Path inside the archive (an existing path is replaced):
add-status = Staged { $path }; save it with Changes…
add-failed = Could not stage the file
remove-title = Remove from the saved archive
remove-body = Nothing is deleted now: the entries are left out when you save the changes as a new archive.
remove-status = Entries marked for removal: { $n }
remove-failed = Could not stage the removal
changes-title = Changes
changes-none = No pending changes. Use Replace…, Add file… or Remove; the open archive is never modified, the result is saved as a new archive.
changes-heading = { $n ->
    [one] { $n } pending change
   *[other] { $n } pending changes
 }
changes-more = … and { $n } more
changes-save = Save as new archive…
changes-discard = Discard all
changes-discarded = Changes discarded
save-pick = Save changed archive as
save-running = Saving archive
save-status = Saving { $dst } …
save-done-status = Saved { $dst }
save-result = Files: { $files }, { $size }, { $verified }: added { $added }, replaced { $replaced }, removed { $removed }.
save-note = The original archive was not changed. The pending changes are kept until you discard them.
save-done-title = Archive saved
save-cancelled = Save cancelled
save-failed = Save failed
save-could-not = Could not save
history-undone = Change undone
history-redone = Change redone
history-failed = Could not change the history

hex-title = Bytes of { $name }
hex-intro = Type two hex digits to change a byte. The size of the file stays the same; changes are staged like any other and can be undone.
hex-bytes-aria = Bytes of the file
hex-goto-ph = offset, e.g. 0x400
hex-goto-aria = Go to offset
hex-prev = ◀ Previous
hex-next = Next ▶
hex-byte-aria = Byte at offset { $at }
hex-count = { $n ->
    [one] { $n } byte changed
   *[other] { $n } bytes changed
 }
hex-where = Offset { $from } – { $to } of { $size } ({ $human })
hex-apply = Apply changes
hex-cannot = Cannot show the bytes
hex-status = Bytes changed in { $name } (staged): { $n }
hex-failed = Could not change the bytes
hex-find-mode = Search type
hex-mode-hex = Hex bytes
hex-mode-text = Text
hex-find-ph = search, e.g. 4D 5A
hex-find-btn = Find next
hex-found = Found at offset { $at }
hex-not-found = Not found
hex-bad-pattern = Enter hex digits in pairs, e.g. 4D 5A
reduce-title = Reduce images
reduce-level-45 = 45 dB: best, smallest savings
reduce-level-40 = 40 dB: hard to tell apart (recommended)
reduce-level-38 = 38 dB: smaller, slight loss in gradients
reduce-level-35 = 35 dB: smallest, gradients may show bands
reduce-intro = Writes a copy of this archive in which every true-colour BMP is stored with 256 colours, where the picture keeps the quality below and the stored file gets smaller. The open archive is not changed. The client loads 8-bit BMPs (many of its own are).
psnr-hint = PSNR measures how far the colours moved: from 40 dB on the difference is hard to see, below 35 dB smooth gradients show bands.
reduce-notes = Magenta (transparent) stays exact, and no other colour becomes transparent. Pictures with at most 256 colours lose nothing.
reduce-quality = Lowest quality
reduce-pending = The { $n } pending changes are written into the copy too; those entries are not reduced.
reduce-save-as = Save the reduced archive as
reduce-running = Reducing images
reduce-status = Reducing images into { $dst } …
reduce-why-already_8_bit = already 8-bit
reduce-why-below_min_psnr = below the quality
reduce-why-not_smaller = not smaller stored
reduce-why-noisy_transparency = noisy around magenta
reduce-why-alpha = with alpha
reduce-why-unreadable = unreadable
reduce-summary = { $reduced } of { $matched } BMPs reduced ({ $lossless } lossless{ $worst }), { $saved } smaller.
reduce-worst = , worst { $db } dB
reduce-kept = Kept as they are: { $list }.
reduce-nothing = Nothing to reduce
reduce-files = Files: { $files }, { $size }, { $verified }.
reduce-done-status = Reduced into { $dst }: { $saved } smaller
reduce-done-title = Images reduced
reduce-cancelled = Reducing cancelled
reduce-failed = Reducing failed
reduce-could-not = Could not reduce
reduce1-btn = 8-bit…
reduce1-btn-title = Store this picture with 256 colours (smaller; magenta stays transparent). Shows both to compare first (staged)
reduce1-cannot = Cannot reduce this picture
reduce1-title = Reduce to 8 bits?
reduce1-lossless = lossless: the picture has no more than 256 colours
reduce1-psnr = PSNR { $db } dB
reduce1-info = { $before } → { $after } unpacked, { $colors } colours, { $quality }. Magenta (transparent) stays exact.
reduce1-original = Original
reduce1-8bit = 8-bit
reduce1-go = Reduce
reduce1-status = { $name } reduced to 8 bits (staged)

pvw-select = Select an entry to preview it.
pvw-loading = Loading …
pvw-meta = { $size } unpacked, { $stored } stored ({ $codec }), offset { $offset }
pvw-staged-removed = staged: removed, shown as it will be saved
pvw-staged-added = staged: added, shown as it will be saved
pvw-staged-changed = staged: changed, shown as it will be saved
pvw-hex-title = Show every byte of this file and change single bytes (staged)
pvw-files = { $n ->
    [one] { $n } file
   *[other] { $n } files
 }
pvw-frames = { $n ->
    [one] { $n } frame
   *[other] { $n } frames
 }
pvw-folder-meta = Folder — { $files }, { $size } unpacked
pvw-open = Open
pvw-folder-note = Extract… and Remove act on this folder when it is selected.
pvw-action = Action
pvw-action-n = Action { $n }
pvw-anim-meta = { $frames }, { $ms } ms each
pvw-anim-edit = Edit frames…
pvw-anim-edit-title = Edit layers, anchors and frames of this animation (staged)
pvw-anim-interval = Set interval…
pvw-anim-interval-title = Change the frame time of this action (staged, saved with Changes…)
pvw-warning = warning: { $note }
pvw-model-hint = { $n } triangles — drag to turn, wheel to zoom, double-click to reset
pvw-lighting = Lighting
pvw-lightmaps = Lightmaps
pvw-models = Models
pvw-water = Water
pvw-wireframe = Wireframe
pvw-animate = Animate
pvw-all-title = Show or hide lighting, lightmaps, models and water together
pvw-hide-all = Hide all
pvw-show-all = Show all
pvw-reset-view = Reset view
pvw-reset-view-title = Back to the start view (also: double-click the picture)
pvw-save-picture = Save picture…
pvw-save-picture-title = Save the view as it is now (with what is switched off) as a PNG, larger than on screen
pvw-extract-map = Extract map…
pvw-extract-map-title = Write this map with the files it needs into a folder; parts switched off here can be left out
pvw-map-hint = Drag to turn, right- or Shift-drag or the arrow keys to move, wheel to zoom, double-click to reset.
pvw-effect = { $frames } shown at { $fps } fps
pvw-video-open = Open with the system's player
pvw-video-open-title = Write the file to a temporary folder and open it with the program your system has for it
pvw-video-opened = Opened { $path }. If nothing started, your system has no player for this format (VLC and RAD Video Tools play Bink).
pvw-video-info = Bink video, { $w } × { $h }, { $frames } at { $fps } fps ({ $secs } s), { $size }.
pvw-video-note = This window cannot play Bink, a format of its own. It stays in the archive unchanged.
pvw-hex-head = { $n } bytes, first 512 shown
sp-cannot = Cannot save the picture
sp-not-ready = The 3D view is not ready.
sp-scale-1 = As on screen
sp-scale-2 = 2 × (recommended)
sp-scale-4 = 4 ×
sp-title = Save picture
sp-intro = The view as it is now: same angle, and only what is switched on. A larger picture is drawn again at that size, not enlarged.
sp-size = Size
sp-choose-file = Choose file…
sp-save-as = Save picture as
sp-filter = PNG pictures
sp-status = Picture saved: { $path } ({ $w } × { $h }, { $size })
sp-failed = Could not save the picture
em-part-models = Models and their textures
em-part-water = Water pictures
em-part-sounds = Sounds
em-part-minimap = Minimap
em-part-effects = Effect script and its textures
em-lit-note = Lighting and lightmaps are part of the world and ground files, which are always written; switching them off only changes the view (use Save picture… for that).
em-title = Extract map
em-intro = The world, ground and altitude files and the ground textures are always written. Parts switched off in the view start unticked here; the files are copied unchanged, so in the client the map still has everything it lists.
em-choose-folder = Choose folder…
em-dest = Folder for the map (the files go below it as data\...)
em-running = Extracting the map …
em-exists-title = Files exist already
em-exists-body = { $error }. Keep both, with a number added to the new file names?
em-keep-both = Keep both
em-nothing = Nothing extracted
em-status = Map { $map }: { $files } written to { $dest }
em-body = { $files } of { $map } written to { $dest }.
em-skipped = { $files } left out ({ $parts }).
em-missing = { $files } the map names are not in this archive (they may be in another GRF of the client):
em-damaged = { $files } are damaged and were written as far as they could be read.
em-done-gaps = Map extracted, with gaps
em-done = Map extracted
em-failed = Could not extract the map

open-status = { $name }: { $format }, { $files }, { $size }

act-title = Edit animation: { $name }
act-frame-label = Frame { $n } / { $total }
act-play = Play
act-stop = Stop
act-play-title = Play the action at its frame time, with the sounds of its frames
act-ms = ms/frame
act-copy-frame = Copy frame
act-copy-frame-title = Insert a copy after this frame
act-inbetween = In-betweens…
act-inbetween-title = Add frames that blend smoothly from one frame to the next
act-frame-earlier = Frame ←
act-frame-earlier-title = Move the frame one place earlier
act-frame-later = Frame →
act-frame-later-title = Move the frame one place later
act-copy-action = Copy action
act-copy-action-title = Insert a copy of this action after it
act-action-up = Action ↑
act-action-up-title = Move the action one place earlier
act-action-down = Action ↓
act-action-down-title = Move the action one place later
act-remove-action = Remove action
act-remove-frame = Remove frame
act-stage-role = frame editor
act-stage-aria = Frame { $n }. Click a layer to select it and drag it to move it; the arrow keys move the selected layer by 1 pixel, with Shift by 10.
act-stage-sel = Layer { $n } is selected.
act-anchor-title = Anchor { $n }: drag to move it
act-no-picture = No picture: { $error }
act-stage-note = The lines mark the origin; red dots are anchors. Layers are drawn in order, the last one on top.
act-th-type = Type
act-th-image = Image
act-th-mirror = Mirror
act-th-scale = Scale
act-th-scale-xy = Scale x / y
act-th-rot = Rot
act-th-colour = Colour
act-type-palette = palette ({ $n })
act-type-rgba = true colour ({ $n })
act-layer-select = Select this layer (drag it on the picture or use the arrow keys there)
act-layer-earlier = Draw earlier (below)
act-layer-later = Draw later (above)
act-layer-remove = Remove the layer
act-add-layer = + Layer
act-anchors-heading = Anchors (where other sprites attach, e.g. headgear)
act-add-anchor = + Anchor
act-no-anchors = ACT { $version } has no anchors.
act-sound = Sound
act-sound-none = none
act-sound-title = Sound file played with this frame, e.g. effect\hit.wav (empty = none)
act-sound-play-title = Play this sound (data\wav\ in the archive)
act-sound-play-aria = Play the sound of this frame
act-interval-title = Interval of action { $n }
act-interval-prompt = Time per frame in milliseconds (the file stores multiples of 25 ms):
act-interval-set = Set
act-interval-status = Interval of action { $n } staged; save with Changes….
act-interval-failed = Could not set the interval
act-remove-title = Remove action { $n }?
act-remove-body = Later actions move down by one. The client picks actions by number (direction, state), so other files that use them may change meaning.
ip-title = Add in-between frames
ip-intro = Adds frames that move position, scale, rotation, colour and anchors in steps from one frame to the next. The picture and mirroring change halfway, and sounds stay with the original frames.
ip-count = In-between frames per pair
ip-whole = Whole action (all { $frames } frames), not only frame { $frame } and the next
ip-smooth = Smooth: slow at the start and the end of each step
ip-loop = Loop: the last frame also blends back into the first
ip-keep = Keep the speed: shorten the frame time so the action lasts as long as before (whole action only)
ip-last = This is the last frame: without "Loop" or "Whole action" there is no next frame to blend with.
ip-go = Add frames
ref-title = Add a reference animation
ref-intro = Another animation of this archive drawn with the one you edit, e.g. a body and a head to fit headgear on. Its first anchor meets the first anchor of the edited frame, as the client attaches parts to the body. Same action and frame number; the reference is only shown, never changed.
ref-animation = Animation (.act)
ref-behind-check = Behind the edited animation
ref-preset-body-m = Body, novice (male)
ref-preset-body-f = Body, novice (female)
ref-preset-head-m = Head 1 (male)
ref-preset-head-f = Head 1 (female)
ref-label = References
ref-add-btn = Add reference…
ref-behind = Behind
ref-front = In front
ref-toggle-title = Draw the reference behind or in front of the edited animation
ref-remove-aria = Remove the reference { $path }
te-title = Edit { $name }
te-partial = Parts of this file could not be decompiled and stay as a listing; they cannot be compiled again. Edit only what the text shows as Lua.
te-compile = Compile and stage
te-stage = Stage
te-compiling = Compiling …
te-staging = Staging …
te-status = { $name } staged. Save with Changes….
te-status-compiled = { $name } staged (compiled again). Save with Changes….

enc-title = Encrypted archive
enc-body = This archive is encrypted (GRF Editor's scheme). Enter its password to read the files.
enc-wrong = This password does not belong to the archive.
enc-skip = Open without password
enc-needs = The file table of this archive is encrypted; a password is needed to open it.
open-another-title = Open another archive?
open-another-body = { $n ->
    [one] { $n } pending change of the open archive is not saved in an archive yet. It is kept as a draft and offered again when you open this archive next time.
   *[other] { $n } pending changes of the open archive are not saved in an archive yet. They are kept as a draft and offered again when you open this archive next time.
 }
open-running = Opening { $path } …
open-failed-status = Could not open the archive
open-failed = Could not open
draft-title = Restore unsaved changes?
draft-body = { $n ->
    [one] { $n } change from { $when } was not saved in an archive. It was kept as a draft (for example because the app closed).
   *[other] { $n } changes from { $when } were not saved in an archive. They were kept as a draft (for example because the app closed).
 }
draft-archive-changed = The archive itself has changed since then. Check the changes before you save.
draft-missing = { $n ->
    [one] { $n } replacement file no longer exists on disk and will be left out.
   *[other] { $n } replacement files no longer exist on disk and will be left out.
 }
draft-more = { $n ->
    [one] { $n } older draft of this archive follows, one each time you open it.
   *[other] { $n } older drafts of this archive follow, one each time you open it.
 }
draft-later-note = Later keeps this draft for another time; the changes you make now are saved as a draft of their own.
draft-restore = Restore
draft-discard = Discard
draft-later = Later
draft-restored = { $n ->
    [one] { $n } change restored
   *[other] { $n } changes restored
 }
draft-restored-skipped = { $restored }, { $skipped } left out
draft-discarded = Draft discarded
draft-kept = The draft is kept for another time
draft-failed = Could not restore the changes
txt-encoding = Encoding:
txt-shortened = (shortened)
txt-edit = Edit…
txt-edit-title = Edit the text; compiled Lua is compiled again (staged, saved with Changes…)
pal-adjust = Adjust…
pal-adjust-title = Shift hue, scale saturation and brightness of a range of colours (staged)
pal-click-hint = Click a colour to change it.
pal-click-title = Click a colour to change it
pal-colour = Colour { $n }
pal-zero = Index 0 is the transparent colour in the client; its colour is rarely seen.
pal-number = Colour number { $n } of 255:
pal-changed = Colour { $n } of { $name } changed (staged); save with Changes….
pal-read-failed = Could not read the palette
pal-adjust-heading = Adjust colours of { $name }
pal-range-note = Colour 0 is the transparent one, so the range starts at 1.
pal-from = From colour
pal-to = to
pal-hue = Hue shift (°)
pal-sat = Saturation ×
pal-bri = Brightness ×
dlg-apply = Apply
pal-adjusted = Colours of { $name } adjusted (staged); save with Changes….
pal-failed = Could not change the palette
pal-pick = Palette for { $name }
pal-filter = Palette
pal-set-status = Palette of { $name } changed (staged); save with Changes….
pal-set-failed = Could not set the palette
spr-add = Add image…
spr-add-title = Append a PNG as a new image (staged)
spr-setpal = Set palette…
spr-setpal-title = Use the colours of a .pal file (staged)
spr-pixels = Pixels…
spr-pixels-title = Draw on this image: pencil, eraser, fill and colour picker on the palette colours (staged)
spr-replace = Replace…
spr-replace-title = Replace this image with a PNG (staged, saved with Changes…)
spr-remove-title = Remove this image; the animation next to it follows (staged)
spr-replace-pick = PNG for image { $frame } of { $name }
spr-png-filter = PNG image
spr-replace-status = Image { $frame } of { $name } staged (colours are mapped to the sprite's palette); the preview shows the result. Save with Changes….
spr-replace-failed = Could not replace the image
spr-add-pick = PNG to add to { $name }
spr-add-status = Image added to { $name } (staged); save with Changes….
spr-add-failed = Could not add the image
spr-remove-heading = Remove image { $n }?
spr-remove-body = Later images move down by one. Layers of the animation next to this sprite that use the image become empty, and later ones follow.
spr-remove-status = Image { $frame } of { $name } removed (staged); save with Changes….
spr-remove-failed = Could not remove the image
pix-error = Pixels
pix-canvas-aria = Image { $frame }, { $w } by { $h } pixels. Arrow keys move the cursor, Space draws.
pix-size = { $w } x { $h } px
pix-readout = { $w } x { $h } px — x { $x }, y { $y }: colour { $c } (#{ $hex })
pix-readout-t = { $w } x { $h } px — x { $x }, y { $y }: colour { $c } (#{ $hex }), transparent
pix-pencil = Pencil
pix-eraser = Eraser
pix-fill = Fill
pix-picker = Picker
pix-zoom = Zoom
pix-grid = Grid
pix-revert = Revert
pix-palette = Palette
pix-palette-heading = Palette (click a colour; 0 is transparent)
pix-swatch-zero =  (transparent)
pix-title = Pixels of image { $frame } — { $name }
pix-nochange = No pixel changed.
pix-status = Image { $frame } of { $name } edited (staged); save with Changes….
pix-failed = Could not stage the pixels
gat-alt = map, { $x } by { $y } cells
gat-x-from = x from
gat-y-from = y from
gat-to = to
gat-type-aria = Cell type
gat-type-nochange = type: no change
gat-height-ph = no change
gat-height = height
gat-set = Set cells
gat-set-title = Set the type and/or height of the chosen cells (staged, saved with Changes…)
gat-drag = Drag on the map to choose cells.
gat-readout = x { $xa }–{ $xb }, y { $ya }–{ $yb }: { $cells }
gat-cells = { $n ->
    [one] { $n } cell
   *[other] { $n } cells
 }
gat-select-all = Select all
gat-need = Choose a cell type and/or a height first.
gat-status = { $cells } of { $name } changed (staged); save with Changes….
gat-failed = Could not change the cells

webgl-missing = WebGL 2 is not available in this window.

pvw-n-empty-marker = Empty marker entry
pvw-n-sound-fail = This sound cannot be played here: { $error }
pvw-n-luac-fail = Compiled Lua (bytecode) that cannot be shown yet: { $error }
pvw-n-palette = Palette: 256 colours
pvw-n-palette-label = 256 colours (index 0 is transparent)
pvw-n-gat = GAT { $version } - { $w } x { $h } cells, heights { $lo } to { $hi }
pvw-n-gat-type = type { $k } ({ $name }): { $cells }
pvw-n-gnd = GND { $version } - { $w } x { $h } cells, { $tiles }, { $lightmaps }, { $textures }
pvw-n-rsw = RSW { $version } - ground { $ground }, { $models }, { $lights }, { $sounds }, { $effects }
pvw-n-instances = { $instances } of { $models }
pvw-n-models-missing = { $n } not found: { $names }
pvw-n-textures = { $n }
pvw-n-textures-missing = { $n ->
    [one] { $n } texture missing
   *[other] { $n } textures missing
 }
pvw-n-rsm = RSM { $version } - { $nodes }, { $triangles }, { $textures }
pvw-n-rsm-loop = The animation loops every { $secs } s
pvw-n-node = node { $name } ({ $v }, { $f })
pvw-n-node-moves = node { $name } ({ $v }, { $f }), moves
pvw-n-str = STR - { $fps } fps, { $frames }, { $layers }, { $textures }
pvw-n-str-step = Only every { $step }th frame is shown
pvw-n-str-missing = missing textures ({ $n }):
pvw-n-str-simple = Simplified preview: blend modes and key animation as in the client, no lighting, shown on a dark background.
pvw-n-spr = SPR { $version } - { $pal }, { $rgba }
pvw-n-warning = warning: { $text }
pvw-n-spr-limit = showing the first { $shown } of { $total } images
pvw-n-act = ACT { $version } - { $actions }, { $frames }, { $layers }
pvw-n-act-nospr = The matching .spr is not in this archive, so there is nothing to draw.
pvw-n-act-none = The animation has no actions
pvw-n-act-empty = Action { $action } draws nothing
gat-tn-0 = walkable
gat-tn-1 = not walkable
gat-tn-2 = not walkable, not snipable
gat-tn-3 = walkable (water)
gat-tn-4 = unknown
gat-tn-5 = not walkable, snipable
gat-tn-6 = walkable
gat-tn-weird = weird (high bit set)

un-tiles = { $n ->
    [one] { $n } tile
   *[other] { $n } tiles
 }
un-lightmaps = { $n ->
    [one] { $n } lightmap
   *[other] { $n } lightmaps
 }
un-textures = { $n ->
    [one] { $n } texture
   *[other] { $n } textures
 }
un-models = { $n ->
    [one] { $n } model
   *[other] { $n } models
 }
un-lights = { $n ->
    [one] { $n } light
   *[other] { $n } lights
 }
un-sounds = { $n ->
    [one] { $n } sound
   *[other] { $n } sounds
 }
un-effects = { $n ->
    [one] { $n } effect
   *[other] { $n } effects
 }
un-nodes = { $n ->
    [one] { $n } node
   *[other] { $n } nodes
 }
un-triangles = { $n ->
    [one] { $n } triangle
   *[other] { $n } triangles
 }
un-vertices = { $n ->
    [one] { $n } vertex
   *[other] { $n } vertices
 }
un-faces = { $n ->
    [one] { $n } face
   *[other] { $n } faces
 }
un-palimg = { $n ->
    [one] { $n } palette image
   *[other] { $n } palette images
 }
un-rgbaimg = { $n ->
    [one] { $n } true-colour image
   *[other] { $n } true-colour images
 }
un-actions = { $n ->
    [one] { $n } action
   *[other] { $n } actions
 }
un-layers = { $n ->
    [one] { $n } layer
   *[other] { $n } layers
 }
un-modelinst = { $n ->
    [one] { $n } model instance
   *[other] { $n } model instances
 }
un-diffmodels = { $n ->
    [one] { $n } different model
   *[other] { $n } different models
 }

err-target-open = The target must not be the open archive
err-alpha = The picture has an alpha channel, which 8-bit BMPs cannot keep
err-saved-png = The picture is saved as .png
err-not-png = Not a PNG picture
err-no-palette = The sprite has no palette
err-interval-positive = The interval must be a positive number of milliseconds
err-merge-two = Choose at least two archives
err-merge-target = The target must not be one of the input archives
err-empty-marker = This entry is an empty marker
err-enter-path = Enter the path inside the archive, e.g. data\texture\a.bmp
err-name-chars = The path has characters that this name mode cannot write
err-marked-removal = The entry is marked for removal
err-no-spr = The matching .spr is not in this archive
err-no-changes = There are no changes to save
err-entry-encrypted = The entry is encrypted (decryption is not supported yet)
err-map-large = The map is too large
err-search-len = The search needs 1 to 256 bytes
err-no-byte = No byte was changed
err-no-draft = There is no draft for this archive
err-already-bits = The picture already has { $bits } bits per pixel
err-not-file = { $path } is not a file
err-file-4gib = { $path } is larger than 4 GiB
err-text-chars = The text has characters that { $encoding } cannot hold
err-preview-large = { $mb } MB is too large to preview
err-map-part = '{ $part }' is not a part of a map that can be left out
err-number = { $what } must be a reasonable number
err-act-keep-action = An animation keeps at least one action
err-act-no-action = No such action
err-act-interval = The interval must be between 1 and 60000 ms
err-act-no-frame = No such frame
err-act-keep-frame = An action keeps at least one frame
err-act-no-frame-n = No frame { $n }
err-act-image-number = The image number is -1 (empty) or more
err-act-image-type = The image type is 0 (palette) or 1 (true colour)
err-act-one-scale = This ACT version has one scale for both directions
err-act-too-many-layers = Too many layers
err-act-no-layer-n = No layer { $n }
err-act-no-layer = No such layer
err-act-no-anchors = This ACT version has no anchors
err-act-too-many-anchors = Too many anchors
err-act-no-anchor-n = No anchor { $n }
err-act-no-frames = The action has no frames
err-gat-need = Choose a type and/or a height to set
err-gat-height = The height must be a reasonable number
err-gat-type = The cell type is 0 to 6
err-pal-range = The colour numbers are 0 to 255
err-pal-order = The first colour is above the last
err-pal-numbers = Hue, saturation and brightness must be numbers
err-pal-factors = Saturation and brightness are factors of at least 0
err-spr-size = The image is { $w } x { $h } pixels, the data has { $n } values
err-no-archive = No archive is open
err-job-running = Another job is already running

menu-new = New
menu-new-grf = GRF Archive...
menu-new-gpf = GPF Patch...
menu-new-rgz = RGZ Patch...
menu-new-thor = Thor Patch...
menu-new-window = New Window
menu-recent = Open Recent
menu-recent-none = No Recent Archives
menu-recent-clear = Clear Menu
menu-close-archive = Close Archive
menu-save = Save
menu-save-as = Save As...
menu-save-advanced = Save Advanced
menu-save-repack = Repack (Recompress)...
menu-save-compact = Compact (Copy Streams)...
menu-save-encrypt = Encrypt...
menu-save-decrypt = Save Decrypted...
menu-archive-settings = Archive Settings...
menu-changes = Review Changes...
menu-tools = Tools
menu-hash = Hashes...
menu-compare = Compare with Archive...
menu-data-folder = Open App Data Folder
menu-about = About RustyGRF...
open-elsewhere = This archive is open in another window.
err-open-elsewhere = The archive is open in another window
untitled = Untitled
new-failed = Could not make a new archive
close-title = Close the archive?
close-body = { $n } unsaved changes are kept as a draft and offered when you open the archive again.
close-anyway = Close
save-here = Save
save-as-button = Save As…
save-nothing = Nothing to save: there are no changes.
save-here-done = { $name } saved: { $files }, { $size }, checked.
sa-title = Save As
sa-title-repack = Repack (compress again)
sa-title-compact = Compact
sa-title-encrypt = Encrypt
sa-title-decrypt = Save decrypted
as-title = Archive settings
sa-store = Files
sa-store-copy = Copy as stored (fast, compacts the archive)
sa-store-recompress = Compress every file again
sa-format-keep = Keep ({ $format })
sa-encryption = Encryption
sa-enc-keep = Keep the password
sa-enc-none = Remove the encryption
sa-enc-plain = None
sa-enc-password = Encrypt with a password
sa-password = Password
sa-password-again = Password again
sa-password-mismatch = The two passwords differ.
err-password-empty = Enter a password
sa-hint = “Save” writes the archive next to itself, checks it and then replaces it. Encryption works with GRF 0x200 only; GRF Editor and clients with its DLL read it. Thor and RGZ are written from the file name you choose.
sa-filter = { $type } files
as-path = File
as-unsaved = not saved yet
as-type = Type
as-content = Content
as-encryption = Encryption
as-encrypted = GRF Editor password
as-plain = none
as-names = Names shown as
err-save-same = Choose another file: “Save” writes into the open archive
err-save-unverified = The written archive did not match the check; the original is unchanged
err-save-replace = { $path } could not be replaced ({ $error }). The saved archive is { $tmp }.
err-encrypt-format = Encryption is possible for GRF 0x200 only
err-patch-empty = A patch needs at least one file
hash-title = Hashes
hash-pick = Select a file first.
hash-size = Size
cmp-pick = Compare with which archive?
cmp-running = Comparing …
cmp-failed = Could not compare
cmp-title = Differences
cmp-same = Same content
cmp-summary = Compared with { $other }: { $here } only here, { $changed } changed, { $there } only there, { $same } the same.
cmp-only-here = Only in this archive
cmp-changed = Changed
cmp-only-there = Only in the other archive
cmp-patch-hint = A patch holds what is new or changed here; a Thor patch also deletes what is only in the other archive.
cmp-make-patch = Save as Patch…
cmp-patch-pick = Save the patch as
cmp-patch-running = Writing the patch …
cmp-patch-done = Patch written
cmp-patch-result = { $added } new, { $changed } changed, { $removed } removed; { $size }, written to { $dst }.
about-title = About RustyGRF
about-version = Version { $version }
about-tagline = One tool for Ragnarok Online archives, sprites, animations, palettes and maps, on macOS, Windows and Linux.
about-made-by = Made by
about-thanks = Thanks
about-tokeiburu = GRF Editor and ACT Editor, the inspiration; formats ported with his permission
about-lua = Lua 5.0 and 5.1 (MIT), for checking and compiling Lua files
about-tauri = the window around the app
about-license = Licensed under MIT or Apache-2.0. Not affiliated with Gravity.
save-as-done = Saved as { $dst }; this window shows it now.
save-unverified = The saved archive did not pass the check: { $dst }
