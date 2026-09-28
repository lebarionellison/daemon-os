$path = "worker.js"
$bytes = [IO.File]::ReadAllBytes($path)
$old = [Text.Encoding]::UTF8.GetBytes('const list = await env.TELEMETRY.list({ prefix: "public:" });')
$new = [Text.Encoding]::UTF8.GetBytes('const publicValue = await env.TELEMETRY.get("public:demo");' + "`r`n" + '      const list = { keys: publicValue ? [{ name: "public:demo" }] : [] };')

$matches = 0
$positions = @()

for ($i = 0; $i -le $bytes.Length - $old.Length; $i++) {
    $match = $true
    for ($j = 0; $j -lt $old.Length; $j++) {
        if ($bytes[$i + $j] -ne $old[$j]) {
            $match = $false
            break
        }
    }
    if ($match) {
        $matches++
        $positions += $i
        $i += $old.Length - 1
    }
}

Write-Host "Byte matches: $matches"

if ($matches -ne 4) {
    throw "Expected exactly 4 matches. worker.js was NOT modified."
}

$result = New-Object System.Collections.Generic.List[byte]
$last = 0

foreach ($position in $positions) {
    if ($position -gt $last) {
        $result.AddRange($bytes[$last..($position - 1)])
    }

    $result.AddRange($new)
    $last = $position + $old.Length
}

if ($last -lt $bytes.Length) {
    $result.AddRange($bytes[$last..($bytes.Length - 1)])
}

[IO.File]::WriteAllBytes($path, $result.ToArray())

Write-Host "Replacement complete."
