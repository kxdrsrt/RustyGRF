# Referenz fuer Thor, RGZ und GPF: GRF Editors GrfCL erzeugt die Dateien aus einem Ordner und liest
# die von rgrf erzeugten. Schreibt nur in -OutDir. Start vom Mac aus: tools/golden/run-patch-in-vm.sh
param(
	[Parameter(Mandatory)][string]$Src,
	[Parameter(Mandatory)][string]$OutDir,
	[string]$GrfClDir = '\\Mac\Home\Downloads\RO\1 CLIENT-SIDE\1 CLIENT TOOLS\5 GRF Editor\GrfCL'
)
$ErrorActionPreference = 'Continue'
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$exe = Join-Path $GrfClDir 'GrfCL.exe'
function Run([string]$Name, [string[]]$Arguments) {
	$log = Join-Path $OutDir "_$Name.log"
	$err = Join-Path $OutDir "_$Name.err.log"
	$p = Start-Process -FilePath $exe -ArgumentList $Arguments -NoNewWindow -Wait -PassThru `
		-RedirectStandardOutput $log -RedirectStandardError $err
	"== $Name : exit code $($p.ExitCode)"
	Get-Content $log -Tail 8
	Get-Content $err -Tail 8
}
foreach ($ext in 'thor', 'rgz', 'gpf', 'grf') {
	Remove-Item -ErrorAction SilentlyContinue (Join-Path $OutDir "ref.$ext")
	Run "make-$ext" @('-new', '-add', 'data', ('"{0}"' -f (Join-Path $Src 'data')), '-save', ('"{0}"' -f (Join-Path $OutDir "ref.$ext")))
}
# ein einzelnes Programm: Thor-Modus 0x21 (Quelle: <Src>\..\src-exe\ragexe.exe)
$exeSrc = Join-Path (Split-Path $Src -Parent) 'src-exe\ragexe.exe'
Remove-Item -ErrorAction SilentlyContinue (Join-Path $OutDir 'ref-exe.thor')
Run 'make-exe' @('-new', '-add', 'data', ('"{0}"' -f $exeSrc), '-save', ('"{0}"' -f (Join-Path $OutDir 'ref-exe.thor')))
# GrfCL liest unsere Dateien
foreach ($f in 'ours.thor', 'ours.rgz') {
	$dest = Join-Path $OutDir ("extracted-" + $f.Replace('.', '-'))
	if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
	if ($f -like '*.rgz') {
		Run "read-$f" @('-extractRgz', ('"{0}"' -f (Join-Path $OutDir $f)), ('"{0}"' -f $dest))
	} else {
		Run "read-$f" @('-open', ('"{0}"' -f (Join-Path $OutDir $f)), '-extractGrf', ('"{0}"' -f $dest))
	}
}
# Befehle ohne Fortschrittsanzeige (die Konsole fehlt unter SYSTEM): Baum und Analyse
Run 'tree-ours.thor' @('-open', ('"{0}"' -f (Join-Path $OutDir 'ours.thor')), '-tree')
Run 'tree-ref.thor' @('-open', ('"{0}"' -f (Join-Path $OutDir 'ref.thor')), '-tree')
Run 'info-ours.thor' @('-open', ('"{0}"' -f (Join-Path $OutDir 'ours.thor')), '-info')
Run 'rgz-analysis-ours' @('-rgzAnalysis', ('"{0}"' -f (Join-Path $OutDir 'ours.rgz')))
Run 'rgz-analysis-ref' @('-rgzAnalysis', ('"{0}"' -f (Join-Path $OutDir 'ref.rgz')))
Get-ChildItem $OutDir | Select-Object Name, Length
