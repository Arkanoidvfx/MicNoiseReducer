param([int]$InputIndex = 4, [ValidateRange(10,80)][int]$BufferMs = 40, [ValidateRange(0,86400)][int]$Seconds = 0, [string]$StopFile = '', [ValidateSet(-1,0,1)][int]$CudaGraphs = -1)
$ErrorActionPreference = 'Stop'
& nvidia-smi --query-gpu=name,memory.total,memory.used,memory.free,utilization.gpu --format=csv,noheader,nounits
& nvidia-smi --query-compute-apps=pid,process_name,used_memory --format=csv,noheader,nounits
$free = & nvidia-smi --id=0 --query-gpu=memory.free --format=csv,noheader,nounits
if ($LASTEXITCODE -ne 0 -or [int]$free -lt 4096) { throw 'Need at least 4096 MiB free VRAM.' }
$stopArgument = if ($StopFile) { $StopFile } else { '-' }
$arguments = @($InputIndex, $Seconds, $BufferMs, $stopArgument, $CudaGraphs)
& (Join-Path $PSScriptRoot 'bin\mic_tag.exe') @arguments
if ($LASTEXITCODE -ne 0) { throw "TAG prototype stopped with error or gaps: $LASTEXITCODE" }
