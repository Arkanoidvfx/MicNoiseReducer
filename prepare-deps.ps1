$ErrorActionPreference = 'Stop'
$folder = Join-Path $PSScriptRoot 'vendor\rubberband-4.0.0'
if (Test-Path -LiteralPath (Join-Path $folder 'rubberband\RubberBandLiveShifter.h')) { return }
$temporary = Join-Path $PSScriptRoot '.tmp'
New-Item -ItemType Directory -Force $temporary, (Join-Path $PSScriptRoot 'vendor') | Out-Null
$archive = Join-Path $temporary 'rubberband-4.0.0.tar.gz'
& curl.exe -fL --retry 2 'https://codeload.github.com/breakfastquay/rubberband/tar.gz/refs/tags/v4.0.0' -o $archive
if ($LASTEXITCODE -ne 0) { throw 'Rubber Band download failed.' }
$expected = '24300F48A8014B7C863B573A9647E61B1B19B37875E2CDD92005E64C6424D266'
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expected) { throw 'Rubber Band checksum mismatch.' }
& tar -xzf $archive -C (Join-Path $PSScriptRoot 'vendor')
if ($LASTEXITCODE -ne 0) { throw 'Rubber Band extraction failed.' }
