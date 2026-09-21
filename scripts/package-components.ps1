[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('core','rvc')][string]$Kind,
    [Parameter(Mandatory)][string]$Version,
    [string]$SourceRoot = 'D:\Projects\Audio\MicNoiseReducer',
    [string]$SigningKeyPath = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$free = (Get-PSDrive D).Free
if ($free -lt 12GB) { throw "At least 12 GB free on D: is required; available $([math]::Round($free/1GB,1)) GB." }
$work = Join-Path $root ".tmp\component-$Kind"
$stage = Join-Path $work 'stage'
$out = Join-Path $work 'out'
Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage,$out | Out-Null

if ($Kind -eq 'core') {
    New-Item -ItemType Directory -Force (Join-Path $stage 'vendor'),(Join-Path $stage 'bin') | Out-Null
    Copy-Item (Join-Path $SourceRoot 'vendor\nvidia-afx-3.0.0') (Join-Path $stage 'vendor') -Recurse
    Copy-Item (Join-Path $SourceRoot 'vendor\tag-2.0.0.1903-demo') (Join-Path $stage 'vendor') -Recurse
    Copy-Item (Join-Path $SourceRoot 'bin\mic_tag_host.exe') (Join-Path $stage 'bin')
} else {
    $source = Join-Path $SourceRoot 'vendor\vcclient-2.1.4-alpha'
    $destination = Join-Path $stage 'vendor\vcclient-2.1.4-alpha'
    New-Item -ItemType Directory -Force (Split-Path -Parent $destination) | Out-Null
    & robocopy $source $destination /E /XD model_dir /NFL /NDL /NJH /NJS /NP
    if ($LASTEXITCODE -ge 8) { throw "robocopy failed with $LASTEXITCODE" }
}

$archive = Join-Path $work "$Kind-runtime.tar.zst"
& tar.exe -caf $archive -C $stage .
if ($LASTEXITCODE -ne 0) { throw 'Component archive failed.' }
$archiveHash = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant()
$chunkSize = 1800MB
$input = [IO.File]::OpenRead($archive)
try {
    $parts = @()
    $index = 0
    $buffer = New-Object byte[] (4MB)
    while ($input.Position -lt $input.Length) {
        $name = "$Kind-runtime-$Version.part{0:d3}" -f $index
        $path = Join-Path $out $name
        $part = [IO.File]::Create($path)
        try {
            $remaining = [math]::Min([long]$chunkSize, [long]($input.Length - $input.Position))
            while ($remaining -gt 0) {
                $read = $input.Read($buffer, 0, [int][math]::Min([long]$buffer.Length, $remaining))
                if ($read -eq 0) { break }
                $part.Write($buffer, 0, $read)
                $remaining -= $read
            }
        } finally { $part.Dispose() }
        $file = Get-Item $path
        $parts += [ordered]@{
            url = "https://github.com/Arkanoidvfx/MicNoiseReducer/releases/download/runtime-$Kind-v$Version/$name"
            size = $file.Length
            sha256 = (Get-FileHash $path -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        $index++
    }
} finally { $input.Dispose() }

$payload = [ordered]@{ version=$Version; archive_sha256=$archiveHash; parts=$parts } | ConvertTo-Json -Compress -Depth 5
$payloadPath = Join-Path $work 'payload.json'
[IO.File]::WriteAllText($payloadPath, $payload, [Text.UTF8Encoding]::new($false))
if (-not $SigningKeyPath) {
    if (-not $env:COMPONENT_SIGNING_KEY) { throw 'COMPONENT_SIGNING_KEY or SigningKeyPath is required.' }
    $SigningKeyPath = Join-Path $work 'signing-key.pem'
    [IO.File]::WriteAllText($SigningKeyPath, $env:COMPONENT_SIGNING_KEY, [Text.UTF8Encoding]::new($false))
}
$signature = Join-Path $work 'signature.bin'
& openssl pkeyutl -sign -rawin -inkey $SigningKeyPath -in $payloadPath -out $signature
if ($LASTEXITCODE -ne 0) { throw 'Manifest signing failed.' }
$envelope = [ordered]@{
    payload = $payload
    signature = [Convert]::ToBase64String([IO.File]::ReadAllBytes($signature))
} | ConvertTo-Json -Depth 4
[IO.File]::WriteAllText((Join-Path $out 'components.json'), $envelope, [Text.UTF8Encoding]::new($false))
Write-Host "Component assets:" $out
