param(
    # Defaults are relative to the repo root (three levels above this
    # script); the bitcoind/ord binaries are the verified ones already
    # cached under target/ by the nk-verify fetch examples.
    [string]$Probe = "$PSScriptRoot\..\..\..\target\debug\examples\console_less_probe.exe",
    [string]$Bitcoind = "$PSScriptRoot\..\..\..\target\nodekeeper-bitcoin-core-31.1\extracted\bitcoin-31.1\bin\bitcoind.exe",
    [string]$Ord = "$PSScriptRoot\..\..\..\target\nodekeeper-ord-0.29.0\extracted\ord-0.29.0\ord.exe",
    [string]$Result = "$env:TEMP\console-less-probe-result.txt"
)

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class Win {
    delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    public static List<string> Visible() {
        var r = new List<string>();
        EnumWindows((h, l) => {
            if (!IsWindowVisible(h)) return true;
            int pid; GetWindowThreadProcessId(h, out pid);
            var c = new StringBuilder(256); GetClassName(h, c, 256);
            var t = new StringBuilder(256); GetWindowText(h, t, 256);
            r.Add(pid + "|" + c + "|" + t);
            return true;
        }, IntPtr.Zero);
        return r;
    }
}
'@

function Describe-Procs($names) {
    Get-CimInstance Win32_Process | Where-Object { $names -contains $_.Name } | ForEach-Object {
        $kids = Get-CimInstance Win32_Process -Filter "ParentProcessId=$($_.ProcessId)" |
            Where-Object { $_.Name -in 'conhost.exe', 'OpenConsole.exe' } |
            ForEach-Object { "$($_.Name)#$($_.ProcessId)" }
        $p = Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue
        $parent = Get-CimInstance Win32_Process -Filter "ProcessId=$($_.ParentProcessId)" -ErrorAction SilentlyContinue
        [pscustomobject]@{
            Name = $_.Name; Pid = $_.ProcessId
            Parent = "$($parent.Name)#$($_.ParentProcessId)"
            MainWindowHandle = $p.MainWindowHandle
            ConsoleHostChildren = ($kids -join ',')
        }
    }
}

# Never touch a node the user may be running: refuse to start if any exists.
$pre = @(Get-Process bitcoind, ord -ErrorAction SilentlyContinue)
if ($pre.Count -gt 0) {
    "ABORT: bitcoind/ord already running (pids $($pre.Id -join ',')); not touching them."
    return
}
Remove-Item $Result -ErrorAction SilentlyContinue

$before = [Win]::Visible()
$probe = Start-Process -FilePath $Probe -ArgumentList "`"$Bitcoind`"", "`"$Ord`"", "`"$Result`"" -PassThru
"probe launched, pid $($probe.Id)"

# Wait for the sample point: executor child running alongside bitcoind + ord.
$deadline = (Get-Date).AddSeconds(150)
while ((Get-Date) -lt $deadline) {
    if ((Test-Path $Result) -and (Select-String -Path $Result -Pattern 'executor child \(ping\) started' -Quiet)) { break }
    if ($probe.HasExited) { break }
    Start-Sleep -Milliseconds 300
}
Start-Sleep -Milliseconds 1500
"--- sample (bitcoind + ord + ping alive) ---"
Describe-Procs @('bitcoind.exe', 'ord.exe', 'ping.exe', 'console_less_probe.exe') | Format-Table -AutoSize | Out-String -Width 200
$after = [Win]::Visible()
"--- NEW visible top-level windows since before the probe ---"
$new = $after | Where-Object { $before -notcontains $_ }
if ($new) { $new } else { '(none)' }

# Wait for the probe to finish.
$deadline = (Get-Date).AddSeconds(120)
while ((Get-Date) -lt $deadline -and -not $probe.HasExited) { Start-Sleep -Milliseconds 500 }
"--- probe result log ---"
Get-Content $Result
"--- survivors after probe exit ---"
$surv = Describe-Procs @('bitcoind.exe', 'ord.exe', 'ping.exe')
if ($surv) { $surv | Format-Table -AutoSize | Out-String -Width 200 } else { '(none)' }
Get-Process bitcoind, ord -ErrorAction SilentlyContinue | Stop-Process -Force
