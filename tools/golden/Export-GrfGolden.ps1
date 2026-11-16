<#
  Erzeugt ein Golden File (TSV) aus einer GRF mit GRF.dll von GRF Editor (Referenz fuer RustyGRF).
  NUR in der Windows-VM ausfuehren, 32-Bit-PowerShell:
    C:\Windows\SysWOW64\WindowsPowerShell\v1.0\powershell.exe -File Export-GrfGolden.ps1 -Grf <datei> -Out <tsv>
  Arbeitsverzeichnis darf kein UNC-Pfad sein. Liest nur, schreibt nur die Ausgabedatei.
  Status: auf dem Mac NICHT getestet (GRF.dll laeuft nur unter Windows).

  Spalten (Tab-getrennt, UTF-8, Kopfzeile, sortiert nach Pfad ordinal):
    path_display  Name wie GRF Editor ihn anzeigt (CP1252-Mojibake bei koreanischen Namen)
    path_utf8_hex UTF-8-Bytes des angezeigten Namens (Hex), nur zur Eindeutigkeit
    zsize         komprimierte Groesse
    zsize_aligned ausgerichtete komprimierte Groesse
    size          unkomprimierte Groesse
    flags         Dateiflag (unteres Byte, wie im Archiv):EntryType-Name laut GRF Editor (z. B. 1:File, LzmaCompressed)
    offset        FileExactOffset (absolut in der Datei)
    crc32, md5    ueber die entpackten Daten (leer bei Ordnermarker oder Fehler)
    error         Text, falls Entpacken fehlschlug (z. B. worm_tail_damage.wav)
#>
param(
	[Parameter(Mandatory)][string]$Grf,
	[Parameter(Mandatory)][string]$Out,
	[string]$GrfClDir = 'Z:\Downloads\RO\1 CLIENT-SIDE\1 CLIENT TOOLS\5 GRF Editor\GrfCL',
	[switch]$NoHash
)
$ErrorActionPreference = 'Stop'
[Environment]::CurrentDirectory = $env:TEMP
Set-Location $env:TEMP

$d = Join-Path $env:TEMP 'grfapi'
New-Item -ItemType Directory -Force $d | Out-Null
Copy-Item (Join-Path $GrfClDir '*.dll') $d -Force
foreach ($a in 'netstandard', 'Utilities', 'ErrorManager', 'GRF') { [Reflection.Assembly]::LoadFrom("$d\$a.dll") | Out-Null }

if (-not ('RgrfCrc32' -as [type])) {
	Add-Type -TypeDefinition @'
public static class RgrfCrc32 {
	static readonly uint[] T = Make();
	static uint[] Make() { var t = new uint[256]; for (uint i = 0; i < 256; i++) { uint c = i; for (int k = 0; k < 8; k++) c = (c & 1) != 0 ? 0xEDB88320u ^ (c >> 1) : c >> 1; t[i] = c; } return t; }
	public static uint Compute(byte[] b) { uint c = 0xFFFFFFFFu; foreach (var x in b) c = T[(c ^ x) & 0xFF] ^ (c >> 8); return ~c; }
}
'@
}

# Wie lzmaverify.ps1: sonst scheitert GetDecompressedData() bei LZMA-Eintraegen (erstes Byte 0x00)
[GRF.Core.Compression]::CompressionAlgorithm = [GRF.Core.Compression]::LzmaCompression
$g = New-Object GRF.Core.GrfHolder
$g.Open($Grf)
$md5 = [Security.Cryptography.MD5]::Create()
$utf8 = New-Object Text.UTF8Encoding($false)
$rows = New-Object Collections.Generic.List[object]
foreach ($e in $g.FileTable.Entries) {
	$crc = ''; $hash = ''; $err = ''
	if (-not $NoHash -and $e.SizeDecompressed -gt 0) {
		try {
			$data = $e.GetDecompressedData()
			$crc = '{0:x8}' -f [RgrfCrc32]::Compute($data)
			$hash = ([BitConverter]::ToString($md5.ComputeHash($data)) -replace '-', '').ToLower()
		} catch { $err = $_.Exception.Message -replace "[\t\r\n]+", ' ' }
	}
	$name = $e.RelativePath
	$hex = ([BitConverter]::ToString($utf8.GetBytes($name)) -replace '-', '').ToLower()
	$rows.Add([pscustomobject]@{
		p = $name; hex = $hex; z = $e.SizeCompressed; za = $e.SizeCompressedAlignment; s = $e.SizeDecompressed
		f = ('{0}:{1}' -f ([int]$e.Flags -band 0xFF), $e.Flags); o = $e.FileExactOffset; crc = $crc; md5 = $hash; err = $err })
}
$arr = $rows.ToArray()
[Array]::Sort($arr, [Comparison[object]]{ param($a, $b) [string]::CompareOrdinal($a.p, $b.p) })
$rows = $arr
$sb = New-Object Text.StringBuilder
[void]$sb.AppendLine("path_display`tpath_utf8_hex`tzsize`tzsize_aligned`tsize`tflags`toffset`tcrc32`tmd5`terror")
foreach ($r in $rows) { [void]$sb.AppendLine(($r.p, $r.hex, $r.z, $r.za, $r.s, $r.f, $r.o, $r.crc, $r.md5, $r.err) -join "`t") }
[IO.File]::WriteAllText($Out, $sb.ToString(), $utf8)
"entries=$($rows.Count) errors=$(@($rows | Where-Object { $_.err }).Count) -> $Out"
$g.Close()
