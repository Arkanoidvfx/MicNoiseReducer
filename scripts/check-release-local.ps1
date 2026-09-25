$ErrorActionPreference = 'Stop'
$project = Split-Path -Parent $PSScriptRoot
$tempRoot = Join-Path $project ('.tmp\release-local-check-' + [guid]::NewGuid().ToString('N'))
$output = Join-Path $tempRoot 'Releases\v1.2.3'
$names = @('MicNoize-1.2.3-win-x64-stable-v2-full.nupkg', 'MicNoize-Upgrade-1.2.3.zip', 'Setup.exe', 'checksums.sha256')
try {
    New-Item -ItemType Directory -Force (Join-Path $tempRoot 'scripts'),(Join-Path $tempRoot 'release'),$output | Out-Null
    Copy-Item (Join-Path $PSScriptRoot 'release-local.ps1') (Join-Path $tempRoot 'scripts\release-local.ps1')
    Set-Content (Join-Path $tempRoot 'release\version.txt') '1.2.3'
    foreach ($name in $names) { Set-Content (Join-Path $output $name) $name }
    @{version='1.2.3';tree='tree';commit='commit';assets=@($names | ForEach-Object {
        @{name=$_;sha256=(Get-FileHash (Join-Path $output $_)).Hash.ToLowerInvariant()}
    })} | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $output '.release-source.json')
    Set-Content (Join-Path $output 'Setup.exe') 'tampered'

    function git {
        $global:LASTEXITCODE = 0
        $command = $args -join ' '
        if ($command -like '*rev-parse*HEAD*tree*' -or $command -like '*rev-parse*release/main*tree*') { return 'tree' }
        if ($command -like '*rev-parse*release/main*') { return 'commit' }
        if ($command -like '*ls-remote release refs/heads/main*') { return 'commit refs/heads/main' }
    }
    function gh { throw 'GitHub must not be called for a changed asset.' }
    try {
        & (Join-Path $tempRoot 'scripts\release-local.ps1') -Version 1.2.3 -Publish
        throw 'Changed asset was accepted.'
    } catch {
        if ($_.Exception.Message -notlike 'Prepared release checksum mismatch: Setup.exe*') { throw }
    }
    Write-Output 'Changed asset rejected before GitHub publication.'

    $previousLocalAppData = $env:LOCALAPPDATA
    try {
        $env:LOCALAPPDATA = $tempRoot
        $install = Join-Path $tempRoot 'MicNoize'
        $packages = Join-Path $install 'packages'
        New-Item -ItemType Directory -Force (Join-Path $install 'current'),$packages | Out-Null
        Set-Content (Join-Path $install 'current\sq.version') '<package><metadata><version>0.2.8</version></metadata></package>'
        Set-Content (Join-Path $packages 'MicNoize-0.2.9-win-x64-stable-v2-full.nupkg') 'candidate'
        Set-Content (Join-Path $packages 'MicNoize-0.2.8-win-x64-stable-v2-full.nupkg') 'wrong previous package'
        try {
            & (Join-Path $PSScriptRoot 'repair-028-update.ps1')
            throw 'Invalid previous package was accepted.'
        } catch {
            if ($_.Exception.Message -notlike 'Existing 0.2.8 package has an unexpected SHA-256*') { throw }
        }
        Write-Output 'Invalid rollback package rejected before update.'
    } finally { $env:LOCALAPPDATA = $previousLocalAppData }
} finally {
    $safeRoot = [IO.Path]::GetFullPath((Join-Path $project '.tmp')) + [IO.Path]::DirectorySeparatorChar
    if (-not [IO.Path]::GetFullPath($tempRoot).StartsWith($safeRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe test cleanup path.' }
    if (Test-Path -LiteralPath $tempRoot) { Remove-Item -LiteralPath $tempRoot -Recurse -Force }
}
