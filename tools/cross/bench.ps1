param(
    [string]$Triple = 'aarch64-pc-windows-msvc',
    [string[]]$Jobs = @('info', 'extract-small', 'build', 'merge', 'extract-data')
)
# Fair comparison RustyGRF vs GRF Editor's GrfCL in the same VM: local disk, same inputs, wall time
# and peak working set of the tool process. Results: \\Mac\Home\build\bench\results.jsonl
$ErrorActionPreference = 'Continue'
$Jobs = @($Jobs | ForEach-Object { $_ -split ',' })
$mac = '\\Mac\Home'
$ref = Join-Path $mac 'Downloads\RO\1 CLIENT-SIDE'
$rgrf = "C:\Users\k\build\win\$Triple\release\rgrf.exe"
$grfcl = 'C:\Users\k\build\grfcl\GrfCL.exe'
$b = 'C:\bench'
$resultsDir = Join-Path $mac 'build\bench'
New-Item -ItemType Directory -Force $b, $resultsDir | Out-Null
$results = Join-Path $resultsDir 'results2.jsonl'
New-Item -ItemType Directory -Force (Split-Path $rgrf) | Out-Null
Copy-Item "$mac\build\win\$Triple\release\rgrf.exe" $rgrf -Force
$timeoutSec = 2400

function Prepare($rel, $name) {
    $dst = Join-Path $b $name
    if (-not (Test-Path $dst)) { Copy-Item (Join-Path $ref $rel) $dst }
    $dst
}

function Run($label, $tool, $exe, $argString, $outPath) {
    if ($outPath -and (Test-Path $outPath)) { Remove-Item $outPath -Recurse -Force }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $exe -ArgumentList $argString -PassThru -WindowStyle Minimized
    $peak = 0L
    $timedOut = $false
    while (-not $p.WaitForExit(20)) {
        try { $p.Refresh(); $peak = [Math]::Max($peak, $p.PeakWorkingSet64) } catch {}
        if ($sw.Elapsed.TotalSeconds -gt $timeoutSec) { $p.Kill(); $timedOut = $true; break }
    }
    $sw.Stop()
    $code = if ($timedOut) { -1 } else { $p.ExitCode }
    $size = 0L; $files = 0
    if ($outPath -and (Test-Path $outPath)) {
        if ((Get-Item $outPath).PSIsContainer) {
            $m = Get-ChildItem $outPath -Recurse -File | Measure-Object Length -Sum
            $size = [int64]$m.Sum; $files = $m.Count
        } else { $size = (Get-Item $outPath).Length }
    }
    $row = [ordered]@{ label = $label; tool = $tool; seconds = [Math]::Round($sw.Elapsed.TotalSeconds, 2);
        peak_mb = [Math]::Round($peak / 1MB); exit = $code; timed_out = $timedOut; out_bytes = $size; out_files = $files;
        args = $argString; at = (Get-Date -Format s) }
    ($row | ConvertTo-Json -Compress) | Out-File $results -Append -Encoding ascii
    "{0,-34} {1,-14} {2,8}s {3,6} MB exit={4}" -f $label, $tool, $row.seconds, $row.peak_mb, $code | Write-Output
}

$cdata = Prepare '0 GRFs\reference\cdata-2024-original.grf' 'cdata.grf'
$rdata = Prepare '0 OTHER CLIENTS\client-kro\rdata.grf' 'rdata.grf'

function Pair($label, $a, $b, $reps) {
    # $a, $b: @(tool, exe, args, out); order alternates so neither tool always runs first or on a warm cache
    for ($i = 1; $i -le $reps; $i++) {
        $order = if ($i % 2 -eq 1) { @($a, $b) } else { @($b, $a) }
        foreach ($x in $order) { Run $label $x[0] $x[1] $x[2] $x[3] }
    }
}

if ($Jobs -contains 'extract-small') {
    foreach ($g in $cdata, $rdata) {
        $n = [IO.Path]::GetFileNameWithoutExtension($g)
        $o1 = "$b\x-rgrf-$n"; $o2 = "$b\x-grfcl-$n"
        Pair "extract $n" @('rgrf', $rgrf, "extract `"$g`" `"$o1`" --quiet", $o1) @('GrfCL', $grfcl, "-breakOnExceptions false false -open `"$g`" -extractGrf `"$o2`"", $o2) 3
    }
}

if ($Jobs -contains 'build') {
    foreach ($n in 'cdata', 'rdata') {
        $src = "$b\src-$n"
        if (-not (Test-Path $src)) { continue }
        foreach ($m in 'zlib', 'lzma') {
            $reps = if ($n -eq 'rdata' -and $m -eq 'lzma') { 1 } else { 3 }
            $o1 = "$b\b-rgrf-$n-$m.grf"; $o1n = "$b\b-rgrf-nv-$n-$m.grf"; $o2 = "$b\b-grfcl-$n-$m.grf"; $o6 = "$b\b-rgrf6-$n.grf"
            Pair "build $n $m" @('rgrf', $rgrf, "build `"$src`" -o `"$o1`" --method $m --quiet", $o1) @('GrfCL', $grfcl, "-breakOnExceptions false false -compression 9 -compressionMethod $m -makeGrf `"$o2`" `"$src\data`"", $o2) $reps
            Run "build $n $m" 'rgrf-noverify' $rgrf "build `"$src`" -o `"$o1n`" --method $m --no-verify --quiet" $o1n
            if ($m -eq 'lzma') { Run "build $n lzma" 'rgrf-level6-noverify' $rgrf "build `"$src`" -o `"$o6`" --method lzma --level 6 --no-verify --quiet" $o6 }
        }
    }
}

if ($Jobs -contains 'merge') {
    $data = Prepare '0 OTHER CLIENTS\client-kro\data.grf' 'data.grf'
    $o1 = "$b\m-rgrf.grf"; $o1n = "$b\m-rgrf-nv.grf"; $o2 = "$b\m-grfcl.grf"
    Pair 'merge rdata over data' @('rgrf', $rgrf, "merge `"$rdata`" `"$data`" -o `"$o1`" --format 0x200 --quiet", $o1) @('GrfCL', $grfcl, "-breakOnExceptions false false -merge `"$data`" `"$rdata`" `"$o2`"", $o2) 3
    Run 'merge rdata over data' 'rgrf-noverify' $rgrf "merge `"$rdata`" `"$data`" -o `"$o1n`" --format 0x200 --no-verify --quiet" $o1n
}

if ($Jobs -contains 'extract-data') {
    # one run each, GrfCL first this time (the first measurement had rgrf first)
    $data = Prepare '0 OTHER CLIENTS\client-kro\data.grf' 'data.grf'
    $o1 = "$b\x-rgrf-data"; $o2 = "$b\x-grfcl-data"
    Run 'extract data.grf (GrfCL first)' 'GrfCL' $grfcl "-breakOnExceptions false false -open `"$data`" -extractGrf `"$o2`"" $o2
    Run 'extract data.grf (GrfCL first)' 'rgrf' $rgrf "extract `"$data`" `"$o1`" --quiet" $o1
}
"done $(Get-Date -Format s)" | Write-Output
