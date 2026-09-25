param([switch]$SupervisorOnly,[switch]$StartupOnly)
# Explicit disruptive Windows acceptance; never run by ordinary CTest.
# Requires a matching installed pair and a closed UI. Does not record audio.
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$hostExe=Join-Path $root 'bin\mic_tag_host.exe'
$probe=Join-Path $root 'bin\mic_tag_probe.exe'
$ui=Join-Path $root 'bin\MicNoize.exe'
if(Get-Process MicNoize -ErrorAction SilentlyContinue | Where-Object Path -eq $ui){throw 'Close the installed UI before recovery acceptance'}
function Hosts {
    @(Get-CimInstance Win32_Process -Filter "Name='mic_tag_host.exe'" | Where-Object ExecutablePath -eq $hostExe)
}
function Role([string]$mode) {
    $items=@(Hosts | Where-Object { $_.CommandLine -match ('--'+$mode+'(?:=|\s|$)') })
    if($items.Count -gt 1){throw "Duplicate $mode processes"}
    if($items.Count){return $items[0]}
}
function Command([string]$mode) {
    $p=Start-Process $hostExe -ArgumentList $mode -WindowStyle Hidden -Wait -PassThru
    if($p.ExitCode){throw "Host command $mode failed: $($p.ExitCode)"}
}
function Ready {
    $output=& $probe --host-status 2>&1
    if($LASTEXITCODE){return $false}
    if($output -notmatch 'endpoint=(\S+)' -or $Matches[1] -ne $script:endpoint){throw "Endpoint changed: $output"}
    return $true
}
function Wait-Ready {
    for($i=0;$i -lt 30;$i++){if(Ready){return};Start-Sleep -Milliseconds 500}
    throw 'Host not ready'
}
function Crash($identity) {
    if(!$identity){throw 'Missing target process'}
    $p=Get-Process -Id $identity.ProcessId
    try {
        $null=$p.Handle
        if($p.Path -ne $hostExe -or [math]::Abs(($p.StartTime.ToUniversalTime()-$identity.CreationDate.ToUniversalTime()).TotalMilliseconds) -gt 1 -or $p.SessionId -ne (Get-Process -Id $PID).SessionId){throw 'Process identity changed; crash cancelled'}
        $p.Kill();if(!$p.WaitForExit(5000)){throw 'Verified process did not exit'}
    } finally {$p.Dispose()}
}
if($SupervisorOnly -or $StartupOnly) {
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class TagHangCheck {
 [DllImport("ntdll.dll")] public static extern int NtSuspendProcess(IntPtr process);
 [DllImport("ntdll.dll")] public static extern int NtResumeProcess(IntPtr process);
}
'@
$suspended=[Collections.Generic.List[Diagnostics.Process]]::new()
function Suspend-Owned($identity) {
    if(!$identity){throw 'Missing target process'}
    $p=Get-Process -Id $identity.ProcessId
    $null=$p.Handle
    if($p.Path -ne $hostExe -or [math]::Abs(($p.StartTime.ToUniversalTime()-$identity.CreationDate.ToUniversalTime()).TotalMilliseconds) -gt 1 -or $p.SessionId -ne (Get-Process -Id $PID).SessionId){$p.Dispose();throw 'Process identity changed; suspension cancelled'}
    if([TagHangCheck]::NtSuspendProcess($p.Handle) -ne 0){$p.Dispose();throw 'Cannot suspend verified host'}
    $suspended.Add($p)
}
$initial=& $probe --host-status
if($LASTEXITCODE -or $initial -notmatch 'endpoint=(\S+)'){throw 'Initial host not ready'}
$script:endpoint=$Matches[1]
try {
    if($StartupOnly) {
        foreach($action in @('stop','adopt')) {
            Command '--stop';Start-Sleep -Seconds 2
            $gate=[Threading.Mutex]::new($true,'Local\MicNoize.HeadphoneLink.Lock.v2')
            try {
                $launcher=Start-Process $hostExe '--task-start' -WindowStyle Hidden -PassThru
                $worker=$null;$until=[DateTime]::UtcNow.AddSeconds(20)
                do {
                    $owner=(Get-ItemProperty 'HKCU:\Software\MicNoize\TAG\Recovery' -Name Worker -ErrorAction SilentlyContinue).Worker
                    if($owner -and $owner.Length -eq 32) {
                        $workerPid=[BitConverter]::ToUInt32($owner,16)
                        if(Get-Process -Id $workerPid -ErrorAction SilentlyContinue) {
                            $worker=[pscustomobject]@{ProcessId=$workerPid;CreationDate=[DateTime]::FromFileTimeUtc([BitConverter]::ToInt64($owner,20))}
                            break
                        }
                    }
                    Start-Sleep -Milliseconds 5
                }while([DateTime]::UtcNow -lt $until)
                Suspend-Owned $worker
                if(!$launcher.WaitForExit(15000) -or $launcher.ExitCode){throw 'Startup launcher did not complete'}
                $parent=Role 'scheduled'
                $status=Start-Process $probe '--host-status' -WindowStyle Hidden -PassThru -Wait -RedirectStandardOutput "$root\results\tag-startup-status.log" -RedirectStandardError "$root\results\tag-startup-status-error.log"
                if($status.ExitCode -eq 0){throw 'Startup suspension occurred after endpoint publication; check is inconclusive'}
                if($action -eq 'stop') {
                    Command '--stop'
                    if(Hosts){throw 'Stop left an initializing process alive'}
                    'PASS: Stop identified and stopped the initializing worker before endpoint publication'
                } else {
                    Crash $parent
                    $clock=[Diagnostics.Stopwatch]::StartNew();$adopted=$false
                    do {
                        Start-Sleep -Milliseconds 200
                        $next=Role 'scheduled';$same=Role 'worker'
                        if(!$same -or $same.ProcessId -ne $worker.ProcessId){throw 'Initializing worker was replaced before adoption'}
                        if($next -and $next.ProcessId -ne $parent.ProcessId){$adopted=$true;break}
                    }while($clock.Elapsed.TotalSeconds -lt 155)
                    if(!$adopted){throw 'Initializing worker was not adopted after supervisor loss'}
                    $p=$suspended[$suspended.Count-1]
                    if([TagHangCheck]::NtResumeProcess($p.Handle) -ne 0){throw 'Cannot resume initializing worker'}
                    "PASS: supervisor recovered around initializing worker in $([math]::Round($clock.Elapsed.TotalSeconds,1)) seconds; one original worker retained"
                }
            } finally {$gate.ReleaseMutex();$gate.Dispose()}
            if($action -eq 'adopt') {
                $deadline=[DateTime]::UtcNow.AddSeconds(95)
                do {Start-Sleep -Seconds 1;$state=& $probe --device-state;if($LASTEXITCODE -eq 0 -and $state -match '^state=3 '){break}}while([DateTime]::UtcNow -lt $deadline)
                Wait-Ready
                'PASS: ready after resuming the initializing worker; original endpoint retained'
            }
        }
        return
    }
    foreach($both in @($false,$true)) {
        Command '--stop';Start-Sleep -Seconds 2;Command '--task-start';Wait-Ready
        $parent=Role 'scheduled';$worker=Role 'worker'
        Suspend-Owned $parent
        if($both){Suspend-Owned $worker}
        $clock=[Diagnostics.Stopwatch]::StartNew();$restored=$false
        do {
            Start-Sleep -Seconds 1
            $next=Role 'scheduled';$nextWorker=Role 'worker'
            if(!$both -and (!(Ready) -or $nextWorker.ProcessId -ne $worker.ProcessId)){throw 'Supervisor hang recovery interrupted the surviving worker'}
            if($next -and $next.ProcessId -ne $parent.ProcessId -and $nextWorker -and (!$both -or $nextWorker.ProcessId -ne $worker.ProcessId) -and (Ready)){$restored=$true;break}
        }while($clock.Elapsed.TotalSeconds -lt 300)
        if(!$restored){throw "No recovery after suspension (both=$both)"}
        if($clock.Elapsed.TotalSeconds -lt 55){throw 'Hang recovery bypassed its bounded backoff'}
        $state=& $probe --device-state
        if($LASTEXITCODE -or $state -notmatch '^state=3 '){throw "Recovered device not Ready: $state"}
        "PASS: suspended supervisor (both=$both) recovered in $([math]::Round($clock.Elapsed.TotalSeconds,1)) seconds; exact endpoint retained"
    }
    & $probe --no-listeners
    if($LASTEXITCODE){throw 'Unexpected capture client during closed-UI hang check'}
    'PASS: readiness and recovery did not require a capture listener'
} finally {
    foreach($p in $suspended){if(!$p.HasExited){[void][TagHangCheck]::NtResumeProcess($p.Handle)};$p.Dispose()}
    if(Hosts){Command '--stop';Start-Sleep -Seconds 2}
    Command '--task-start';Wait-Ready
}

return
}
$initial=& $probe --host-status
if($LASTEXITCODE -or $initial -notmatch 'endpoint=(\S+)'){throw 'Initial host not ready'}
$script:endpoint=$Matches[1]
try {
    # Start a fresh, shared three-retry budget.
    Command '--stop';Start-Sleep -Seconds 2;Command '--task-start';Wait-Ready
    foreach($failure in @('scheduled','worker','scheduled')) {
        $before=Role $failure;$worker=Role 'worker';Crash $before
        $clock=[Diagnostics.Stopwatch]::StartNew();$restored=$false
        do {
            Start-Sleep -Seconds 1
            if($failure -eq 'scheduled') {
                if(!(Ready) -or (Role 'worker').ProcessId -ne $worker.ProcessId){throw 'Parent crash interrupted the surviving worker'}
            }
            $next=Role $failure
            if($next -and $next.ProcessId -ne $before.ProcessId -and (Ready)){$restored=$true;break}
        } while($clock.Elapsed.TotalSeconds -lt 85)
        if(!$restored){throw "No recovery after $failure crash"}
        if($clock.Elapsed.TotalSeconds -lt 55){throw 'Recovery bypassed its minute backoff'}
        "PASS: $failure crash recovered in $([math]::Round($clock.Elapsed.TotalSeconds,1)) seconds; exact endpoint retained"
    }
    $worker=Role 'worker';Crash (Role 'scheduled')
    for($i=0;$i -lt 70;$i++) {
        Start-Sleep -Seconds 1
        if((Role 'scheduled') -or !(Ready) -or (Role 'worker').ProcessId -ne $worker.ProcessId){throw 'Three-retry budget reset, or surviving worker lost'}
    }
    'PASS: fourth crash did not restart the parent; worker stayed ready for 70 seconds'
    $warning=& $probe --task-warning
    if($LASTEXITCODE -or !$warning){throw 'Exhausted background recovery has no user-visible warning'}
    Crash (Role 'worker')
    for($i=0;$i -lt 130;$i++){Start-Sleep -Seconds 1;if((Role 'scheduled') -or (Role 'worker')){throw 'Loss of both processes reset the durable three-retry budget'}}
    'PASS: exhausted retry budget survived loss of both processes; no restart for 130 seconds; warning available to UI'
    Command '--stop';Start-Sleep -Seconds 2;Command '--task-start';Wait-Ready
    Crash (Role 'scheduled');Start-Sleep -Seconds 2;Command '--stop'
    for($i=0;$i -lt 70;$i++) {
        Start-Sleep -Seconds 1
        if(Hosts){throw 'Stop did not cancel recovery'}
    }
    'PASS: Stop cancelled pending parent recovery; no process respawned for 70 seconds'
    foreach($failure in @('both','parent-during-worker-backoff')) {
        Command '--task-start';Wait-Ready
        $worker=Role 'worker';$parent=Role 'scheduled'
        if($failure -eq 'both') {Crash $parent;Crash $worker}
        else {Crash $worker;Start-Sleep -Seconds 3;Crash $parent}
        $clock=[Diagnostics.Stopwatch]::StartNew();$restored=$false
        do {
            Start-Sleep -Seconds 1
            if((Role 'scheduled') -and (Role 'worker') -and (Ready)){$restored=$true;break}
        } while($clock.Elapsed.TotalSeconds -lt 155)
        if(!$restored){throw "No external recovery after $failure"}
        if($clock.Elapsed.TotalSeconds -lt 55){throw 'External recovery bypassed its minute backoff'}
        "PASS: $failure recovered in $([math]::Round($clock.Elapsed.TotalSeconds,1)) seconds without UI or surviving worker; exact endpoint retained"
        Command '--stop';Start-Sleep -Seconds 2
    }
    Command '--task-start';Wait-Ready
    $staleWorker=(Role 'worker').CommandLine -replace '^.*(--worker=\S+).*$', '$1'
    Crash (Role 'scheduled');Crash (Role 'worker')
    Start-Sleep -Seconds 2;Command '--stop'
    Command $staleWorker;Command '--scheduled';Command '--recover-host'
    for($i=0;$i -lt 70;$i++){Start-Sleep -Seconds 1;if(Hosts){throw 'Late command or scheduled recovery undid Stop'}}
    'PASS: Stop cancelled total-loss recovery; old worker generation and delayed supervisor commands did not restart audio'
} finally {
    if(Hosts){Command '--stop';Start-Sleep -Seconds 2}
    Command '--task-start';Wait-Ready
}
'PASS: final host ready with original endpoint and fresh recovery budget'
