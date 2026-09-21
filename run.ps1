$ErrorActionPreference = 'Stop'
$exe = Join-Path $PSScriptRoot 'bin\MicNoize.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw 'Run .\build.ps1 first.' }
& nvidia-smi --query-gpu=name,memory.total,memory.used,memory.free,utilization.gpu --format=csv,noheader,nounits
if ($LASTEXITCODE -ne 0) { throw 'NVIDIA driver is unavailable.' }
& nvidia-smi --query-compute-apps=pid,process_name,used_memory --format=csv,noheader,nounits
$free = & nvidia-smi --id=0 --query-gpu=memory.free --format=csv,noheader,nounits
if ($LASTEXITCODE -ne 0 -or [int]$free -lt 4096) { throw 'Less than 4 GiB of free VRAM. Free GPU memory before starting.' }
Start-Process -FilePath $exe -WorkingDirectory $PSScriptRoot
