# Spike round 2: persistent profile (bilibili) — extension world inheritance + attach on real profile
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$ErrorActionPreference = 'Stop'

$actl = "E:/explore/computer_cli-spike/target/debug/actl.exe"
$co   = "E:/explore/computer_cli-spike/prototypes/browser-lib-probe/chromiumoxide-probe/target/debug/chromiumoxide-probe.exe"
$hc   = "E:/explore/computer_cli-spike/prototypes/browser-lib-probe/headless-chrome-probe/target/debug/headless-chrome-probe.exe"
$ev   = "E:/explore/computer_cli-spike/target/browser-lib-probe-20261013"
New-Item -ItemType Directory -Force -Path $ev | Out-Null

function Log($msg) {
    $line = "[{0}] {1}" -f (Get-Date -Format "HH:mm:ss"), $msg
    Write-Host $line
    Add-Content -Path "$ev/round-persistent.log" -Value $line -Encoding utf8
}

Log "=== 1. actl browser-launch --profile bilibili (headed) ==="
$launchRaw = (& $actl browser-launch --browser-executable edge --profile bilibili 2>$null) | Out-String
$launchRaw | Out-File "$ev/p-launch.json" -Encoding utf8
$launch = $launchRaw | ConvertFrom-Json
if (-not $launch.ok) { throw "launch failed: $launchRaw" }
$sid = $launch.data.session_id
Log "session_id=$sid reused=$($launch.data.reused)"

$sessionFile = Join-Path $env:LOCALAPPDATA "actl/browser/sessions/$sid.json"
$session = Get-Content $sessionFile -Raw | ConvertFrom-Json
$port = $session.devtools_port
$wsUrl = "ws://127.0.0.1:{0}{1}" -f $port, $session.browser_ws_path
Log "ws_url=$wsUrl"

Log "=== 2. /json/list target enumeration (extension world check) ==="
$targets = Invoke-RestMethod -Uri "http://127.0.0.1:$port/json/list" -TimeoutSec 10
$targets | ConvertTo-Json -Depth 4 | Out-File "$ev/p-targets.json" -Encoding utf8
$extTargets = @($targets | Where-Object { $_.url -like "chrome-extension://*" })
$swTargets  = @($targets | Where-Object { $_.type -eq "service_worker" })
Log "total_targets=$(@($targets).Count) extension_targets=$($extTargets.Count) service_workers=$($swTargets.Count)"
foreach ($t in $extTargets) { Log "  ext: $($t.url)" }

Log "=== 3. chromiumoxide-probe attach on persistent profile ==="
$ts1 = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$env:PROBE_URL = "https://example.com/?r=$ts1"
$coOut = (& $co attach $wsUrl 2>$null) | Out-String
$coOut | Out-File "$ev/p-co-attach.txt" -Encoding utf8
Log ($coOut -replace "`r`n", " | ")

Log "=== 4. headless-chrome-probe attach on persistent profile ==="
$ts2 = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$env:PROBE_URL = "https://example.com/?r=$ts2"
$hcOut = (& $hc attach $wsUrl 2>$null) | Out-String
$hcOut | Out-File "$ev/p-hc-attach.txt" -Encoding utf8
Log ($hcOut -replace "`r`n", " | ")

Log "=== 5. session alive check ==="
$verifyRaw = (& $actl browser-verify --session $sid --alive 2>$null) | Out-String
$verify = $verifyRaw | ConvertFrom-Json
if ($verify.ok) { Log "ALIVE (ok)" } else { Log "DEAD: $verifyRaw" }

Log "=== 6. actl graceful close ==="
$closeRaw = (& $actl browser-close --session $sid 2>$null) | Out-String
$close = $closeRaw | ConvertFrom-Json
if ($close.ok) { Log "closed ok" } else { Log "close failed: $closeRaw" }

Log "=== round-persistent done ==="
