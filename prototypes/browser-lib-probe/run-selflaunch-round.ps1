# Spike round 3: selflaunch — launch invariants (stdio pipe behavior, graceful close, process exit)
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$ErrorActionPreference = 'Stop'

$edge = "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe"
$co   = "E:/explore/computer_cli-spike/prototypes/browser-lib-probe/chromiumoxide-probe/target/debug/chromiumoxide-probe.exe"
$hc   = "E:/explore/computer_cli-spike/prototypes/browser-lib-probe/headless-chrome-probe/target/debug/headless-chrome-probe.exe"
$ev   = "E:/explore/computer_cli-spike/target/browser-lib-probe-20261013"
New-Item -ItemType Directory -Force -Path $ev | Out-Null

function Log($msg) {
    $line = "[{0}] {1}" -f (Get-Date -Format "HH:mm:ss"), $msg
    Write-Host $line
    Add-Content -Path "$ev/round-selflaunch.log" -Value $line -Encoding utf8
}

function Count-Msedge {
    @(Get-Process -Name msedge -ErrorAction SilentlyContinue).Count
}

# Run a probe with a hard timeout: if the pipeline never completes because the spawned
# browser tree inherited our stdout pipe, the job times out = stdio leak (docs/72 S12).
function Run-Probe {
    param($exe, $argList, $timeoutSec)
    $job = Start-Job -ScriptBlock {
        param($exe, $argList)
        (& $exe @argList 2>$null) | Out-String
    } -ArgumentList $exe, $argList
    if (Wait-Job $job -Timeout $timeoutSec) {
        $out = Receive-Job $job
        Remove-Job $job -Force
        return @{ ok = $true; output = $out }
    } else {
        Stop-Job $job
        Remove-Job $job -Force
        return @{ ok = $false; output = "TIMEOUT after ${timeoutSec}s (stdout pipe held by child tree = stdio inheritance leak)" }
    }
}

if (-not (Test-Path $edge)) { throw "edge not found at $edge" }

$ts = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$env:PROBE_URL = "https://example.com/?r=$ts"

Log "=== 1. chromiumoxide selflaunch (fresh abs profile dir) ==="
$before = Count-Msedge
Log "msedge_before=$before"
$coProfile = "$ev/co-self-profile-$ts"
$r1 = Run-Probe $co @("selflaunch", $edge, $coProfile) 90
$r1.output | Out-File "$ev/co-selflaunch.txt" -Encoding utf8
Log ($r1.output -replace "`r`n", " | ")
if (-not $r1.ok) { Log "STDIO_LEAK_SUSPECT chromiumoxide" }
Start-Sleep -Seconds 2
Log "msedge_after_co=$(Count-Msedge) (delta from before = self-launch leftovers)"

Log "=== 2. headless_chrome selflaunch (fresh abs profile dir) ==="
$ts2 = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$env:PROBE_URL = "https://example.com/?r=$ts2"
$hcProfile = "$ev/hc-self-profile-$ts2"
$r2 = Run-Probe $hc @("selflaunch", $edge, $hcProfile) 90
$r2.output | Out-File "$ev/hc-selflaunch.txt" -Encoding utf8
Log ($r2.output -replace "`r`n", " | ")
if (-not $r2.ok) { Log "STDIO_LEAK_SUSPECT headless_chrome" }
Start-Sleep -Seconds 2
$after = Count-Msedge
Log "msedge_after_hc=$after (baseline was $before)"

Log "=== 3. pid exit verification (hc prints pid) ==="
$hcOut = Get-Content "$ev/hc-selflaunch.txt" -Raw
if ($hcOut -match "pid=Some\((\d+)\)") {
    $browserPid = [int]$Matches[1]
    Start-Sleep -Seconds 1
    $proc = Get-Process -Id $browserPid -ErrorAction SilentlyContinue
    if ($proc) { Log "hc browser pid=$browserPid STILL RUNNING (close failed)" } else { Log "hc browser pid=$browserPid exited (drop->Browser.Close works)" }
} else {
    Log "hc pid not parsed from output"
}

Log "=== round-selflaunch done ==="
