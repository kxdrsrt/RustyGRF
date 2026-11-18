#!/bin/bash
# Laesst GRF Editors GrfCL (-lub) in der Parallels-VM alle .lub einer GRF dekompilieren (Referenz).
# usage: run-lub-in-vm.sh <GRF relativ zu "RO/1 CLIENT-SIDE", mit Backslashes> <Ausgabeordner-Name>
#   z. B.: run-lub-in-vm.sh '0 OTHER CLIENTS\client-kro\data.grf' kro
# Schreibt nach "0 GRFs/reference/grfcl-lub/<name>/" (Referenzdaten, nicht committen). Liest die GRF nur.
set -euo pipefail
VM="${RGRF_VM:-Tiny 11 Pro}"
R='\\Mac\Home\Downloads\RO\1 CLIENT-SIDE'
G="$R\\0 GRFs\\RustyGRF"
prlctl exec "$VM" 'C:\Windows\SysWOW64\WindowsPowerShell\v1.0\powershell.exe' -NoProfile -ExecutionPolicy Bypass \
  -File "$G\\tools\\golden\\Run-GrfClLub.ps1" \
  -Grf "$R\\$1" -OutDir "$R\\0 GRFs\\reference\\grfcl-lub\\$2"
