#!/bin/bash
# Listet die IL einer kleinen .NET-DLL in der Windows-VM (keine Installation noetig, nutzt .NET Reflection).
# usage: dump-il-in-vm.sh <dll relativ zu ~> <ausgabe relativ zu ~>
set -euo pipefail
VM="${RGRF_VM:-Tiny 11 Pro}"
w() { echo "Z:\\${1//\//\\}"; }
G='Z:\Downloads\RO\1 CLIENT-SIDE\0 GRFs\RustyGRF'
prlctl exec "$VM" --current-user powershell -NoProfile -ExecutionPolicy Bypass -File "$G\\tools\\golden\\Dump-Il.ps1" -Dll "$(w "$1")" -Out "$(w "$2")"
