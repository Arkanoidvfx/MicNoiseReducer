# Windows lifecycle regression: no window, NVIDIA processing, audio recording or host restart.
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
$ui=Join-Path $root 'bin\MicNoize.exe'
if(Get-Process MicNoize -ErrorAction SilentlyContinue | Where-Object Path -eq $ui){throw 'Close the UI before the tray lifecycle check'}
Add-Type @'
using System;using System.Runtime.InteropServices;
public static class MicNoizeTrayCheck {
 [DllImport("user32.dll",CharSet=CharSet.Unicode)]public static extern IntPtr FindWindow(string cls,string title);
 [DllImport("user32.dll")]public static extern uint GetWindowThreadProcessId(IntPtr w,out uint pid);
 [DllImport("user32.dll")]public static extern bool PostMessage(IntPtr w,uint msg,IntPtr wp,IntPtr lp);
}
'@
$p=Start-Process $ui -ArgumentList '--start-tray --ui-benchmark' -WindowStyle Hidden -PassThru
try {
    $clock=[Diagnostics.Stopwatch]::StartNew();$window=[IntPtr]::Zero
    do {
        if($p.HasExited){throw 'UI exited before the test request'}
        $candidate=[MicNoizeTrayCheck]::FindWindow('MicNoize.Shell','Mic Noize background');[uint32]$owner=0
        if($candidate -ne [IntPtr]::Zero){[void][MicNoizeTrayCheck]::GetWindowThreadProcessId($candidate,[ref]$owner)}
        if($owner -eq $p.Id){$window=$candidate;break}
        Start-Sleep -Milliseconds 100
    } while($clock.Elapsed.TotalSeconds -lt 5)
    if($window -eq [IntPtr]::Zero){throw 'Owned tray shell did not appear'}
    Start-Sleep -Seconds 1
    $clock.Restart()
    if(![MicNoizeTrayCheck]::PostMessage($window,0x11,[IntPtr]::Zero,[IntPtr]::Zero)){throw 'Graceful exit request failed'}
    if(!$p.WaitForExit(8000)){throw 'Tray-only exit exceeded 8 seconds; process left intact for diagnosis'}
    if($p.ExitCode){throw "UI failed: $($p.ExitCode)"}
    "PASS: tray-only UI exited normally in $([math]::Round($clock.Elapsed.TotalMilliseconds)) ms"
} finally {$p.Dispose()}
