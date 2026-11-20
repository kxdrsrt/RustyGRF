# Builds the jRO test vectors (crates/rgrf-core/tests/fixtures/protected-grf-jro.bin) on Windows
# with the .NET SDK: copies GRF Editor's decoder sources next to Program.cs, runs it and writes
# the bundle to -Out. Every vector is checked with GRF Editor's own ProtectedGrfDecoder.Decode.
param(
    [Parameter(Mandatory)] [string]$GrfEditorSrc,
    [Parameter(Mandatory)] [string]$Out,
    [string]$Work = "$env:TEMP\rgrf-jro-vectors"
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force "$Work\ZlibNet" | Out-Null
Copy-Item "$PSScriptRoot\*.cs", "$PSScriptRoot\*.csproj" $Work -Force
Copy-Item "$GrfEditorSrc\GRF\ZlibNet\*.cs" "$Work\ZlibNet" -Force
foreach ($f in 'ProtectedGrfDecoder', 'ProtectedGrfCiphers', 'ProtectedJroDecoder', 'ProtectedCnDecoder', 'JroArchiveProfiles') {
    $t = Get-Content "$GrfEditorSrc\GRF\Core\GrfCompression\$f.cs" -Raw
    # the generator calls a few of the decoder's helpers to build entries
    $t = $t -replace 'private static', 'internal static'
    Set-Content "$Work\$f.cs" $t -Encoding UTF8
}
Set-Location $Work
$ErrorActionPreference = 'Continue'
dotnet run -c Release -- $Out 2>&1 | ForEach-Object { "$_" }
exit $LASTEXITCODE
