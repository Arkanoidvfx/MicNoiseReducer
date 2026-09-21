param(
    [ValidateSet(1, 2)][int]$Version = 1,
    [ValidateRange(0.0, 1.0)][double]$Intensity = 1.0
)
$ErrorActionPreference = 'Stop'
$sdk = Join-Path $PSScriptRoot 'vendor\nvidia-afx-3.0.0'
$exe = Join-Path $PSScriptRoot 'build\nvidia-samples\Release\effects_demo.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw 'Run .\build.ps1 -Samples first.' }
$modelName = if ($Version -eq 1) { 'denoiser_48k.trtpkg' } else { 'denoiser_v2_48k.trtpkg' }
$model = Join-Path $sdk "features\nvafxdenoiser\models\ampere\$modelName"
if (-not (Test-Path -LiteralPath $model)) { throw "Model missing: $model" }
& nvidia-smi --query-gpu=name,memory.total,memory.used,memory.free,utilization.gpu --format=csv,noheader,nounits
if ($LASTEXITCODE -ne 0) { throw 'nvidia-smi failed.' }
& nvidia-smi --query-compute-apps=pid,process_name,used_memory --format=csv,noheader,nounits
if ($LASTEXITCODE -ne 0) { throw 'Cannot check GPU processes.' }
$freeMiB = & nvidia-smi --id=0 --query-gpu=memory.free --format=csv,noheader,nounits
if ($LASTEXITCODE -ne 0 -or [int]$freeMiB -lt 4096) { throw 'Need at least 4096 MiB free VRAM for this test.' }
$run = Join-Path $PSScriptRoot ('results\' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff') + "-v$Version")
New-Item -ItemType Directory -Path $run | Out-Null
$inputWav = Join-Path $sdk 'samples\apps\effects_demo\input_files\denoiser\48k\Typing_48k.wav'
$outputWav = Join-Path $run 'typing-denoised.wav'
$cfg = Join-Path $run 'effect.cfg'
$intensityText = $Intensity.ToString([Globalization.CultureInfo]::InvariantCulture)
$configuration = @"
effect denoiser
effect_version $Version
input_sample_rate 48000
output_sample_rate 48000
model $model
input_wav $inputWav
output_wav $outputWav
real_time 0
intensity_ratio $intensityText
enable_vad 0
"@
[IO.File]::WriteAllText($cfg, $configuration, [Text.UTF8Encoding]::new($false))
$previousPath, $previousTemp, $previousTmp = $env:PATH, $env:TEMP, $env:TMP
try {
    $env:PATH = "$sdk\bin;$sdk\features\nvafxdenoiser\bin;$sdk\bin\external\cuda\bin;$sdk\bin\external\nvtrt\bin;$sdk\bin\external\openssl\bin;$previousPath"
    $env:TEMP = Join-Path $PSScriptRoot '.tmp'
    New-Item -ItemType Directory -Force -Path $env:TEMP | Out-Null
    $env:TMP = $env:TEMP
    & $exe -c $cfg | Tee-Object -FilePath (Join-Path $run 'sdk.log')
    if ($LASTEXITCODE -ne 0) { throw "effects_demo failed: $LASTEXITCODE" }
    if (-not (Test-Path -LiteralPath $outputWav) -or (Get-Item -LiteralPath $outputWav).Length -le 44) {
        throw 'SDK did not produce a WAV file.'
    }
    if (Select-String -LiteralPath (Join-Path $run 'sdk.log') -Pattern '\[ERROR\]' -Quiet) { throw 'SDK reported an error; see sdk.log.' }
    Write-Host "Input:  $inputWav"
    Write-Host "Output: $outputWav"
} finally {
    $env:PATH, $env:TEMP, $env:TMP = $previousPath, $previousTemp, $previousTmp
}
