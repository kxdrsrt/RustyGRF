#!/bin/bash
# Startet Export-GrfGolden.ps1 in der Parallels-VM (Windows, GRF Editor als Referenz) vom Mac aus.
# usage: run-in-vm.sh <GRF relativ zu "RO/1 CLIENT-SIDE", mit Backslashes> <Ausgabename ohne Endung>
#   z. B.: run-in-vm.sh 'client-dev\full\cdata.grf' cdata
# Liest nur die GRF, schreibt tests/golden/<name>.tsv. Danach: gzip -9 -n tests/golden/<name>.tsv
# Hinweis: prlctl exec laeuft als SYSTEM; dort ist Z: nicht gemappt, daher der UNC-Pfad \\Mac\Home.
# Gelegentlich meldet prlctl "PrlJob_GetRetCode: Invalid argument" (voruebergehend, Aufruf wiederholen).
set -euo pipefail
VM="${RGRF_VM:-Tiny 11 Pro}"
R='\\Mac\Home\Downloads\RO\1 CLIENT-SIDE'
G="$R\\0 GRFs\\RustyGRF"
prlctl exec "$VM" 'C:\Windows\SysWOW64\WindowsPowerShell\v1.0\powershell.exe' -NoProfile -ExecutionPolicy Bypass \
  -File "$G\\tools\\golden\\Export-GrfGolden.ps1" \
  -Grf "$R\\$1" -Out "$G\\tests\\golden\\$2.tsv" \
  -GrfClDir "$R\\1 CLIENT TOOLS\\5 GRF Editor\\GrfCL"
