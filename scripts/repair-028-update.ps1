# Run only after Mic Noize 0.2.8 has downloaded 0.2.9 and reports that the
# previous full package is missing. The application performs the update itself.
$ErrorActionPreference = 'Stop'
$install = Join-Path $env:LOCALAPPDATA 'MicNoize'
$versionFile = Join-Path $install 'current\sq.version'
if (-not (Test-Path -LiteralPath $versionFile) -or ([xml](Get-Content -LiteralPath $versionFile -Raw)).package.metadata.version -ne '0.2.8') {
    throw 'This repair applies only to an installed Mic Noize 0.2.8.'
}
$packages = Join-Path $install 'packages'
if (-not (Test-Path -LiteralPath (Join-Path $packages 'MicNoize-0.2.9-win-x64-stable-v2-full.nupkg'))) {
    throw 'First download 0.2.9 in Mic Noize and wait for the missing previous package error.'
}
$name = 'MicNoize-0.2.8-win-x64-stable-v2-full.nupkg'
$expected = '065f9e7c0fc83005840ffb76f7b1f02a8894456afa3cdaca3bdfe190c93d5bae'
$destination = Join-Path $packages $name
if (Test-Path -LiteralPath $destination) {
    if ((Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ne $expected) { throw 'Existing 0.2.8 package has an unexpected SHA-256.' }
} else {
    $partial = Join-Path $packages ($name + '.' + [guid]::NewGuid().ToString('N') + '.partial')
    try {
        Invoke-WebRequest -Uri "https://github.com/Arkanoidvfx/MicNoize/releases/download/v0.2.8/$name" -OutFile $partial
        if ((Get-FileHash -LiteralPath $partial -Algorithm SHA256).Hash -ne $expected) { throw 'Downloaded 0.2.8 package has an unexpected SHA-256.' }
        Move-Item -LiteralPath $partial -Destination $destination
    } finally {
        if (Test-Path -LiteralPath $partial) { Remove-Item -LiteralPath $partial -Force }
    }
}
Write-Output 'The official 0.2.8 rollback package is restored. Click Update now in Mic Noize again.'
