# Disposable Calculator only; no physical injection, no desktop-wide minimization.
# Run when no actl-signal is running. Uses a fresh LOCALAPPDATA directory.
param([string]$Actl = "$PSScriptRoot/../target/release/actl.exe")
$ErrorActionPreference = 'Stop'
$Actl = (Resolve-Path -LiteralPath $Actl).Path
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class DisplayFixture {
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c,string t);
 public static IntPtr Signal() { return FindWindow("actl_signal_window",null); }
 [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int cmd);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h,uint m,UIntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
}
'@
if ([DisplayFixture]::Signal() -ne [IntPtr]::Zero) { throw 'Close the existing display before this isolated fixture.' }
$previousLocal = $env:LOCALAPPDATA
$testDir = Join-Path $PSScriptRoot "../target/readiness-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $testDir -Force | Out-Null
$env:LOCALAPPDATA = (Resolve-Path -LiteralPath $testDir).Path
$calculator = $null
function Invoke-Actl([string[]]$Arguments) {
    $value = (& $Actl @Arguments | ConvertFrom-Json)
    if (-not $value.ok) { throw ($value | ConvertTo-Json -Depth 12) }
    return $value
}
try {
    $state = Invoke-Actl @('display-start')
    if (-not $state.data.ready) { throw 'Display not ready.' }
    $baseline = @((Invoke-Actl @('list-windows')).data.windows | ForEach-Object hwnd)
    $launch = Invoke-Actl @('launch','calc.exe')
    $deadline = [DateTime]::UtcNow.AddSeconds(12)
    do {
        Start-Sleep -Milliseconds 250
        $candidate = @((Invoke-Actl @('list-windows')).data.windows | Where-Object { $_.hwnd -and $_.hwnd -notin $baseline -and $_.title -match 'Calculator|计算器' }) | Select-Object -First 1
    } while (-not $candidate -and [DateTime]::UtcNow -lt $deadline)
    if (-not $candidate) { throw 'No disposable Calculator window created.' }
    $calculator = [long]$candidate.hwnd
    $signal = [DisplayFixture]::Signal()
    [DisplayFixture]::ShowWindow($signal,0) | Out-Null
    Start-Sleep -Milliseconds 250
    $blocked = (& $Actl minimize-window --hwnd $calculator | ConvertFrom-Json)
    if ($blocked.error.code -ne 'NOT_ACTIONABLE' -or [DisplayFixture]::IsIconic([IntPtr]$calculator)) { throw 'Hidden display did not block minimization.' }
    [DisplayFixture]::ShowWindow($signal,4) | Out-Null
    $null = Invoke-Actl @('display-start')
    $prepared = Invoke-Actl @('desktop-prepare','--hwnd',"$calculator")
    if (@($prepared.data.results | Where-Object status -EQ 'minimized').Count -ne 1) { throw 'Expected one minimized Calculator.' }
    $restored = Invoke-Actl @('desktop-restore','--session',$prepared.data.session)
    if (@($restored.data.results | Where-Object status -EQ 'restored').Count -ne 1) { throw 'Expected one restored Calculator.' }
    $prepared2 = Invoke-Actl @('desktop-prepare','--hwnd',"$calculator")
    $null = Invoke-Actl @('restore-window','--hwnd',"$calculator")
    $skipped = Invoke-Actl @('desktop-restore','--session',$prepared2.data.session)
    if (@($skipped.data.results | Where-Object status -EQ 'skipped').Count -ne 1) { throw 'Changed window should be skipped.' }
    $stop = Join-Path $env:LOCALAPPDATA 'actl/stop-requested'
    [IO.File]::WriteAllText($stop,'fixture-stop')
    $null = Invoke-Actl @('display-start')
    if (-not (Test-Path -LiteralPath $stop)) { throw 'Startup cleared stop.' }
    $aborted = (& $Actl minimize-window --hwnd $calculator | ConvertFrom-Json)
    if ($aborted.error.code -ne 'ABORTED') { throw 'Sticky stop failed.' }
    Remove-Item -LiteralPath $stop
    Write-Output 'PASS: startup, acknowledged launch, hidden gate, scoped minimize/restore, changed-window skip, sticky stop.'
} finally {
    $stop = Join-Path $env:LOCALAPPDATA 'actl/stop-requested'
    if (Test-Path -LiteralPath $stop) { Remove-Item -LiteralPath $stop }
    $signal = [DisplayFixture]::Signal()
    if ($signal -ne [IntPtr]::Zero) { [DisplayFixture]::ShowWindow($signal,4) | Out-Null }
    if ($calculator) {
        $closed = (& $Actl close-window --hwnd $calculator | ConvertFrom-Json)
        if (-not $closed.ok) { [DisplayFixture]::PostMessage([IntPtr]$calculator,16,[UIntPtr]::Zero,[IntPtr]::Zero) | Out-Null }
    }
    if ($signal -ne [IntPtr]::Zero) { [DisplayFixture]::PostMessage($signal,16,[UIntPtr]::Zero,[IntPtr]::Zero) | Out-Null }
    $env:LOCALAPPDATA = $previousLocal
}
