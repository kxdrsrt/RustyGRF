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
    .title = Review and save the pending changes as a new archive (Cmd/Ctrl+S)

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
    [one] Save the { $n } pending change as a new archive
   *[other] Save the { $n } pending changes as a new archive
 }
save-none = Save the pending changes as a new archive
save-aria-n = { $n ->
    [one] Save { $n } pending change as a new archive
   *[other] Save { $n } pending changes as a new archive
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
menu-build = Build...
menu-merge = Merge...
menu-extract = Extract...
menu-verify = Verify
menu-changes-save = Save Changes as New Archive...
menu-reduce-images = Reduce Images...
menu-add = Add File...
menu-replace = Replace...
menu-remove = Remove
menu-changes-review = Review Changes...
menu-layout-tree = As Tree
menu-layout-folders = As Folders
menu-layout-list = As List
menu-layout-details = As Details
menu-layout-gallery = As Gallery
menu-preview-right = Preview on the Right
menu-preview-bottom = Preview at the Bottom
menu-preview-off = Hide Preview
