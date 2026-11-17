param([string]$triple = 'aarch64-pc-windows-msvc')
$ErrorActionPreference = 'Continue'
$mac = '\\Mac\Home'
$repoRel = 'Downloads\RO\1 CLIENT-SIDE\0 GRFs\RustyGRF'
$repoSrc = Join-Path $mac $repoRel
$repoDst = Join-Path 'C:\Users\k' $repoRel
$binSrc = Join-Path $mac "build\win\$triple\release"
$binDst = "C:\Users\k\build\win\$triple\release"
$log = Join-Path $mac "build\win\test-log-$triple.txt"

"=== $(Get-Date -Format s) mirror $triple" | Out-File $log -Encoding ascii
robocopy $repoSrc $repoDst /MIR /XD .git .cargo target /NFL /NDL /NJH /NJS /NP | Out-Null
New-Item -ItemType Directory -Force $binDst | Out-Null
Copy-Item (Join-Path $binSrc 'rgrf.exe'), (Join-Path $binSrc 'rgrf-mcp.exe') $binDst -Force

$env:RGRF_TESTDATA = Join-Path $mac 'Downloads\RO\1 CLIENT-SIDE'
$env:RGRF_REQUIRE_TESTDATA = '1'
$env:RGRF_GOLDEN_FULL = '1'
$env:RUST_BACKTRACE = '0'

$tests = Get-ChildItem (Join-Path $binSrc 'build') -Recurse -Filter '*.exe' |
    Where-Object { $_.DirectoryName -like '*\out' -and $_.Name -match '^(rgrf_core|rgrf_mcp|cli|golden|server|stdio|rgrf)-[0-9a-f]{16}\.exe$' } |
    Where-Object { $_.Name -notmatch '^rgrf-' }
foreach ($t in $tests) {
    $crate = if ($t.FullName -match 'build\\(rgrf-[a-z]+)\\') { $Matches[1] } else { 'rgrf-core' }
    "=== $($t.Name) ($crate)" | Out-File $log -Append -Encoding ascii
    Push-Location (Join-Path $repoDst "crates\$crate")
    $sw = [Diagnostics.Stopwatch]::StartNew()
    & $t.FullName 2>&1 | Where-Object { $_ -match '^test result|FAILED|panicked|SKIPPED|failed|left:|right:' } | Out-File $log -Append -Encoding ascii
    "exit=$LASTEXITCODE seconds=$([int]$sw.Elapsed.TotalSeconds)" | Out-File $log -Append -Encoding ascii
    Pop-Location
}
"=== done $(Get-Date -Format s)" | Out-File $log -Append -Encoding ascii
