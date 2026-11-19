# Listet die IL-Anweisungen aller Methoden einer .NET-Assembly (zum Lesen kleiner Bibliotheken, hier
# GRF Editors Encryption.dll fuer die Schnittstellen-Kompatibilitaet). Nur Lesen, schreibt nach -Out.
# Start vom Mac aus: tools/golden/dump-il-in-vm.sh
param(
	[Parameter(Mandatory)][string]$Dll,
	[Parameter(Mandatory)][string]$Out
)
$ErrorActionPreference = 'Stop'
$bytes = [IO.File]::ReadAllBytes($Dll)
$asm = [Reflection.Assembly]::Load($bytes)
$one = @{}
$two = @{}
[Reflection.Emit.OpCodes].GetFields('Public,Static') | ForEach-Object {
	$op = $_.GetValue($null)
	if ($op.Size -eq 1) { $one[[int]$op.Value] = $op } else { $two[[int]($op.Value -band 0xFF)] = $op }
}
$sb = New-Object Text.StringBuilder
function Resolve([Reflection.Module]$m, [int]$token, [Reflection.Emit.OperandType]$t) {
	try {
		switch ($t) {
			'InlineMethod' { $x = $m.ResolveMethod($token); return "$($x.DeclaringType.FullName)::$($x.Name)" }
			'InlineField' { $x = $m.ResolveField($token); return "$($x.DeclaringType.FullName)::$($x.Name)" }
			'InlineType' { return $m.ResolveType($token).FullName }
			'InlineTok' { return $m.ResolveMember($token).Name }
			'InlineString' { return '"' + $m.ResolveString($token) + '"' }
			default { return ('0x{0:x8}' -f $token) }
		}
	} catch { return ('0x{0:x8}' -f $token) }
}
foreach ($type in $asm.GetTypes()) {
	[void]$sb.AppendLine("TYPE $($type.FullName)")
	foreach ($f in $type.GetFields('Public,NonPublic,Static,Instance')) {
		[void]$sb.AppendLine("  FIELD $($f.FieldType.Name) $($f.Name)")
	}
	foreach ($m in $type.GetMethods('Public,NonPublic,Static,Instance,DeclaredOnly')) {
		$params = ($m.GetParameters() | ForEach-Object { "$($_.ParameterType.Name) $($_.Name)" }) -join ', '
		[void]$sb.AppendLine("  METHOD $($m.ReturnType.Name) $($m.Name)($params)")
		$body = $m.GetMethodBody()
		if (-not $body) { continue }
		foreach ($l in $body.LocalVariables) { [void]$sb.AppendLine("    LOCAL $($l.LocalIndex): $($l.LocalType.Name)") }
		$il = $body.GetILAsByteArray()
		$i = 0
		while ($i -lt $il.Length) {
			$start = $i
			$b = $il[$i]; $i++
			if ($b -eq 0xFE) { $op = $two[[int]$il[$i]]; $i++ } else { $op = $one[[int]$b] }
			$arg = ''
			switch ($op.OperandType) {
				'InlineNone' { }
				'ShortInlineBrTarget' { $d = $(if ($il[$i] -gt 127) { $il[$i] - 256 } else { $il[$i] }); $i++; $arg = ('IL_{0:x4}' -f ($i + $d)) }
				'ShortInlineI' { $arg = [string]$(if ($il[$i] -gt 127) { $il[$i] - 256 } else { $il[$i] }); $i++ }
				'ShortInlineVar' { $arg = [string]$il[$i]; $i++ }
				'InlineVar' { $arg = [string][BitConverter]::ToInt16($il, $i); $i += 2 }
				'InlineBrTarget' { $d = [BitConverter]::ToInt32($il, $i); $i += 4; $arg = ('IL_{0:x4}' -f ($i + $d)) }
				'InlineI' { $arg = [string][BitConverter]::ToInt32($il, $i); $i += 4 }
				'InlineI8' { $arg = [string][BitConverter]::ToInt64($il, $i); $i += 8 }
				'ShortInlineR' { $arg = [string][BitConverter]::ToSingle($il, $i); $i += 4 }
				'InlineR' { $arg = [string][BitConverter]::ToDouble($il, $i); $i += 8 }
				'InlineSwitch' {
					$n = [BitConverter]::ToInt32($il, $i); $i += 4
					$base = $i + 4 * $n
					$targets = @()
					for ($k = 0; $k -lt $n; $k++) { $targets += ('IL_{0:x4}' -f ($base + [BitConverter]::ToInt32($il, $i))); $i += 4 }
					$arg = $targets -join ' '
				}
				default { $tok = [BitConverter]::ToInt32($il, $i); $i += 4; $arg = Resolve $m.Module $tok $op.OperandType }
			}
			[void]$sb.AppendLine(('    IL_{0:x4}: {1} {2}' -f $start, $op.Name, $arg))
		}
	}
}
[IO.File]::WriteAllText($Out, $sb.ToString())
"geschrieben: $Out ($($sb.Length) Zeichen)"
