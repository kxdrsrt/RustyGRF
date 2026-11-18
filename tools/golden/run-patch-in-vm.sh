#!/bin/bash
# Laesst GRF Editors GrfCL in der Parallels-VM Thor/RGZ/GPF aus einem Ordner erzeugen (Referenz) und
# unsere Dateien lesen. usage: run-patch-in-vm.sh   (Ordner "0 GRFs/reference/patchformats/src", Ausgabe
# nach ".../out", siehe docs/02-format-notes.md)
set -euo pipefail
VM="${RGRF_VM:-Tiny 11 Pro}"
R='\\Mac\Home\Downloads\RO\1 CLIENT-SIDE'
G="$R\\0 GRFs\\RustyGRF"
P="$R\\0 GRFs\\reference\\patchformats"
prlctl exec "$VM" 'C:\Windows\SysWOW64\WindowsPowerShell\v1.0\powershell.exe' -NoProfile -ExecutionPolicy Bypass \
  -File "$G\\tools\\golden\\Run-GrfClPatch.ps1" -Src "$P\\src" -OutDir "$P\\out"
