# Spike round 1: attach mechanics for chromiumoxide vs headless_chrome
# Orchestrated with actl-owned session lifecycle (launch -> attach probes -> alive check -> graceful close)
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
    Add-Content -Path "$ev/round-attach.log" -Value $line -Encoding utf8
}

Log "=== 1. actl browser-launch (ephemeral, headless) ==="
$launchRaw = (& $actl browser-launch --browser-executable edge --headless 2>$null) | Out-String
$launchRaw | Out-File "$ev/launch.json" -Encoding utf8
$launch = $launchRaw | ConvertFrom-Json
if (-not $launch.ok) { throw "launch failed: $launchRaw" }
$sid = $launch.data.session_id
Log "session_id=$sid"

$sessionFile = Join-Path $env:LOCALAPPDATA "actl/browser/sessions/$sid.json"
$session = Get-Content $sessionFile -Raw | ConvertFrom-Json
$wsUrl = "ws://127.0.0.1:{0}{1}" -f $session.devtools_port, $session.browser_ws_path
Log "ws_url=$wsUrl"

Log "=== 2. chromiumoxide-probe attach ==="
$ts1 = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$env:PROBE_URL = "https://example.com/?r=$ts1"
$coOut = (& $co attach $wsUrl 2>$null) | Out-String
$coOut | Out-File "$ev/co-attach.txt" -Encoding utf8
Log ($coOut -replace "`r`n", " | ")

Log "=== 3. headless-chrome-probe attach ==="
$ts2 = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$env:PROBE_URL = "https://example.com/?r=$ts2"
$hcOut = (& $hc attach $wsUrl 2>$null) | Out-String
$hcOut | Out-File "$ev/hc-attach.txt" -Encoding utf8
Log ($hcOut -replace "`r`n", " | ")

Log "=== 4. actl session still alive after both probe drops? ==="
$verifyRaw = (& $actl browser-verify --session $sid --alive 2>$null) | Out-String
$verifyRaw | Out-File "$ev/verify-alive.txt" -Encoding utf8
$verify = $verifyRaw | ConvertFrom-Json
if ($verify.ok) { Log "ALIVE after probe drops (ok)" } else { Log "DEAD after probe drops: $verifyRaw" }

Log "=== 5. actl graceful close ==="
$closeRaw = (& $actl browser-close --session $sid 2>$null) | Out-String
$closeRaw | Out-File "$ev/close.json" -Encoding utf8
$close = $closeRaw | ConvertFrom-Json
if ($close.ok) { Log "closed ok" } else { Log "close failed: $closeRaw" }

Log "=== 6. final browser-list (should be 0) ==="
$listRaw = (& $actl browser-list 2>$null) | Out-String
Log $listRaw.Trim()

Log "=== round-attach done ==="
