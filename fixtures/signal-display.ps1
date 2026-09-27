# Live display regression; pass the HWND of a disposable Calculator window.
# Explicitly injects one physical click. Never targets or closes unrelated windows.
param(
    [Parameter(Mandatory=$true)][long]$CalculatorHwnd,
    [string]$Actl = "$PSScriptRoot/../target/release/actl.exe"
)
$ErrorActionPreference = 'Stop'
$Actl = (Resolve-Path -LiteralPath $Actl).Path
$signalDir = Join-Path $env:LOCALAPPDATA 'actl'
$statePath = Join-Path $signalDir 'state.json'
$stopPath = Join-Path $signalDir 'stop-requested'
$outputDir = Join-Path $PSScriptRoot '../target/display-regression'
New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
$outputDir = (Resolve-Path -LiteralPath $outputDir).Path
if (Test-Path -LiteralPath $stopPath) { throw 'Stop already requested; leave user state intact.' }
if (-not (Test-Path -LiteralPath (Join-Path $signalDir 'display-live'))) {
    throw 'Start the matching actl-signal build before this fixture.'
}
$anchor = "hwnd:$CalculatorHwnd"
$ownsStop = $false
$validated = $false
try {
    $snapshot = (& $Actl snapshot --app $anchor | ConvertFrom-Json)
    if (-not $snapshot.ok -or -not ($snapshot.data.elements | Where-Object automation_id -EQ 'num7Button')) {
        throw 'The supplied HWND is not a supported Calculator window.'
    }
    $validated = $true
    $click = (& $Actl click id:num7Button --app $anchor --physical | ConvertFrom-Json)
    if (-not $click.ok) { throw ($click | ConvertTo-Json -Depth 10) }
    $state = Get-Content -Raw -LiteralPath $statePath | ConvertFrom-Json
    if (-not $state.action.effects.pointer -or $state.action.phase -ne 'delivered' -or $state.action.target.point.Count -ne 2) {
        throw 'Physical click feedback missing.'
    }
    $uia = (& $Actl click id:num2Button --app $anchor | ConvertFrom-Json)
    if (-not $uia.ok) { throw 'UIA click failed.' }
    $state = Get-Content -Raw -LiteralPath $statePath | ConvertFrom-Json
    if ($state.action.effects.pointer -or $state.action.target.bounds.Count -ne 4) { throw 'UIA feedback invented a mouse effect.' }
    $out = Join-Path $outputDir 'cancelled.json'
    $child = Start-Process -FilePath $Actl -ArgumentList "click id:num9Button --app $anchor --physical" -WindowStyle Hidden -PassThru -RedirectStandardOutput $out
    $deadline = [DateTime]::UtcNow.AddSeconds(8)
    while ([DateTime]::UtcNow -lt $deadline -and -not $child.HasExited) {
        try { $state = Get-Content -Raw -LiteralPath $statePath | ConvertFrom-Json } catch { continue }
        if ($state.pid -eq $child.Id -and $state.action.phase -eq 'preparing') {
            [IO.File]::WriteAllText($stopPath,[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds().ToString())
            $ownsStop = $true
            break
        }
        Start-Sleep -Milliseconds 10
    }
    if (-not $child.WaitForExit(8000)) { throw 'Cancelled action did not return.' }
    $cancelled = Get-Content -Raw -LiteralPath $out | ConvertFrom-Json
    if (-not $ownsStop -or $cancelled.error.code -ne 'ABORTED') { throw 'Preparation cancellation failed.' }
    $state = Get-Content -Raw -LiteralPath $statePath | ConvertFrom-Json
    if ($state.action.phase -ne 'cancelled') { throw 'Cancelled action was not recorded.' }
    Remove-Item -LiteralPath $stopPath
    $ownsStop = $false
    $windows = (& $Actl list-windows | ConvertFrom-Json)
    if ($windows.data.windows | Where-Object class -Like 'actl_signal_*') { throw 'Display window leaked into target enumeration.' }
    Write-Output 'PASS: pointer effects, UIA bounds, cancellable preview, window exclusion'
} finally {
    if ($ownsStop) { Remove-Item -LiteralPath $stopPath -ErrorAction SilentlyContinue }
    # Caller explicitly supplies a disposable Calculator window; always close gracefully.
    if ($validated) { & $Actl close-window --hwnd "$CalculatorHwnd" | Out-Null }
}
