# Dekompiliert mit GRF Editors GrfCL (-lub) alle passenden .lub einer GRF in einen Ordner.
# Nur Lesen der GRF; die Ausgabe landet in -OutDir (nie in client-dev/full). Referenz fuer den Vergleich
# mit `rgrf lua decompile`. Start vom Mac aus: tools/golden/run-lub-in-vm.sh
param(
	[Parameter(Mandatory)][string]$Grf,
	[Parameter(Mandatory)][string]$OutDir,
	[string]$Pattern = 'data\*.lub',
	[string]$GrfClDir = '\\Mac\Home\Downloads\RO\1 CLIENT-SIDE\1 CLIENT TOOLS\5 GRF Editor\GrfCL'
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$log = Join-Path $OutDir '_grfcl.log'
$err = Join-Path $OutDir '_grfcl.err.log'
# GrfCL braucht eine Konsole; unter SYSTEM (prlctl exec) gibt es keine, daher Start-Process mit Umleitung
$args = @('-open', ('"{0}"' -f $Grf), '-lub', ('"{0}"' -f $Pattern), ('"{0}"' -f $OutDir))
$p = Start-Process -FilePath (Join-Path $GrfClDir 'GrfCL.exe') -ArgumentList $args -NoNewWindow -Wait -PassThru `
	-RedirectStandardOutput $log -RedirectStandardError $err
"exit code $($p.ExitCode)"
Get-Content $log -Tail 5
Get-Content $err -Tail 5
"files: " + (Get-ChildItem $OutDir -Recurse -File -Filter *.lua).Count
