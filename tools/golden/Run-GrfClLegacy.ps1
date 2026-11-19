# Referenz fuer GRF 0x102/0x103: GrfCL (GRF Editor) liest die von rgrf geschriebenen Archive und
# entpackt sie. Schreibt nur in -OutDir. Start vom Mac aus: tools/golden/run-legacy-in-vm.sh
param(
	[Parameter(Mandatory)][string]$OutDir,
	[Parameter(Mandatory)][string]$Grfs,
	[string]$GrfClDir = 'Z:\Downloads\RO\1 CLIENT-SIDE\1 CLIENT TOOLS\5 GRF Editor\GrfCL'
)
$ErrorActionPreference = 'Continue'
$exe = Join-Path $GrfClDir 'GrfCL.exe'
foreach ($g in $Grfs.Split('|')) {
	$name = [IO.Path]::GetFileNameWithoutExtension($g)
	$dest = Join-Path $OutDir "extracted-$name"
	if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
	$p = Start-Process -FilePath $exe -NoNewWindow -Wait -PassThru -RedirectStandardOutput (Join-Path $OutDir "_$name.log") `
		-RedirectStandardError (Join-Path $OutDir "_$name.err.log") `
		-ArgumentList @('-open', ('"{0}"' -f $g), '-extractGrf', ('"{0}"' -f $dest))
	"== $name : exit code $($p.ExitCode)"
	Get-Content (Join-Path $OutDir "_$name.log") -Tail 6
	Get-Content (Join-Path $OutDir "_$name.err.log") -Tail 6
}
