#!/bin/bash
# Laesst GrfCL die von rgrf geschriebenen 0x102/0x103-Archive in der Parallels-VM entpacken.
# usage: run-legacy-in-vm.sh <Ordner mit .grf, relativ zu ~>   (Ausgabe in <Ordner>/gc, Vergleich auf dem Mac)
set -euo pipefail
VM="${RGRF_VM:-Tiny 11 Pro}"
DIR="${1:?Ordner relativ zu ~}"
WIN="Z:\\${DIR//\//\\}"
G='Z:\Downloads\RO\1 CLIENT-SIDE\0 GRFs\RustyGRF'
files=()
for f in "$HOME/$DIR"/*.grf; do files+=("$WIN\\$(basename "$f")"); done
prlctl exec "$VM" --current-user powershell -NoProfile -ExecutionPolicy Bypass -File "$G\\tools\\golden\\Run-GrfClLegacy.ps1" \
  -OutDir "$WIN" -Grfs "$(IFS="|"; echo "${files[*]}")"
