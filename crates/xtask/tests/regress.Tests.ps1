$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot/../../..").Path
$golden = Get-Content "$root/fixtures/golden-task-2.sh" -Raw
if ($golden -match 'ACTL=\./target/debug') { throw 'golden-task-2 ignores the selected validation binary' }
if ($golden -match 'close_all_by_title') { throw 'golden-task-2 closes windows it does not own' }
Write-Output 'Fixture entry contract passed'
foreach ($name in @('edge-deep-tree','edge-content-view')) {
    $entry = Get-Content "$root/fixtures/$name.sh" -Raw
    if ($entry -match 'close-window "\$WIN"|rm -f|ACTL:-target/release') {
        throw "$name still uses title cleanup, deletes evidence or bypasses validation binary"
    }
}
foreach ($name in @('golden-task-1','win32-table-extract','virtualized-list')) {
    $entry = Get-Content "$root/fixtures/$name.sh" -Raw
    if ($entry -match 'close_all_by_title|rm -rf') { throw "$name still uses broad cleanup" }
}
. "$root/crates/xtask/regress-lib.ps1"
foreach ($sample in @(
    @{state='ready';input='none'},
    @{state='ready';input='keyboard';unattended=$true},
    @{state='manual';input='none';unattended=$true},
    @{state='ready';input='none';unattended='true'}
)) {
    if (Test-UnattendedCase ([pscustomobject]$sample)) { throw 'Unaudited or interactive fixture was allowed unattended' }
}
if (-not (Test-UnattendedCase ([pscustomobject]@{state='ready';input='none';unattended=$true}))) { throw 'Audited read-only fixture was rejected' }
$all = @(Get-RegressionCases $root 'all' '')
if ($all.Count -lt 9) { throw 'Missing regression inventory' }
if (@(Get-RegressionCases $root 'flow' '').Count -ne 1) { throw 'Suite selection failed' }
if ((Get-RegressionCases $root 'all' 'coord-click').state -ne 'blocked') { throw 'Unsafe fixture must remain blocked' }
$rejected = $false
try { Get-RegressionCases $root 'all' 'missing-test' } catch { $rejected = $true }
if (-not $rejected) { throw 'Unknown test was silently skipped' }
foreach ($pair in @(@(0,'passed'),@(1,'failed'),@(77,'blocked'))) {
    if ((Get-CaseStatus $pair[0]) -ne $pair[1]) { throw 'Incorrect result classification' }
}
$temp = Join-Path $root "target/regress-tests-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $temp | Out-Null
$probe = Join-Path $temp 'probe.ps1'
[IO.File]::WriteAllText($probe,@'
param([string]$Value)
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
[Console]::Out.Write($Value)
[Console]::Error.Write('diagnostic-' + ('e' * 100000))
exit 77
'@)
$hostExe = (Get-Process -Id $PID).Path
$payload = '中文 path with spaces "quotes" $literal'
$code = Invoke-RegressionProcess $hostExe @('-NoProfile','-File',$probe,'-Value',$payload) $root "$temp/probe" @{}
if ($code -ne 77) { throw 'Child exit code lost' }
if ([IO.File]::ReadAllText("$temp/probe.stdout.log") -ne $payload) { throw 'Arguments or stdout corrupted' }
if (-not [IO.File]::ReadAllText("$temp/probe.stderr.log").StartsWith('diagnostic-')) { throw 'stderr not preserved separately' }
# Reproduce the old cleanup defect without opening or closing a GUI window.
$previousDir = $env:ACTL_TEST_DIR
$previousLocal = $env:LOCALAPPDATA
try {
    $env:ACTL_TEST_DIR = "$temp/blocked"
    $env:LOCALAPPDATA = "$temp/isolated"
    function Get-Process { param($Name, $ErrorAction) return [pscustomobject]@{ProcessName='Notepad'} }
    & "$root/fixtures/golden-task-2.ps1" -Actl $hostExe 2>$null
    if ($LASTEXITCODE -ne 77) { throw 'Existing Notepad must block before any CLI call' }
    $result = Get-Content "$temp/blocked/result.json" -Raw | ConvertFrom-Json
    if ($result.calls.Count -ne 0 -or $result.passed) { throw 'Existing user session was touched or reported passed' }
    New-Item -ItemType Directory -Force "$env:LOCALAPPDATA/actl" | Out-Null
    [IO.File]::WriteAllText("$env:LOCALAPPDATA/actl/stop-requested",'test-stop')
    & "$root/fixtures/golden-task-2.ps1" -Actl $hostExe 2>$null
    if ($LASTEXITCODE -ne 77 -or -not (Test-Path "$env:LOCALAPPDATA/actl/stop-requested")) { throw 'Stop flag was ignored or cleared' }
    & "$root/fixtures/input-target.ps1" -Actl $hostExe
    if ($LASTEXITCODE -ne 77) { throw 'Input target fixture ignored stop' }
    $result = Get-Content "$temp/blocked/result.json" -Raw | ConvertFrom-Json
    if ($result.calls.Count -ne 0 -or $result.passed) { throw 'Input target fixture operated despite stop' }
} finally {
    Remove-Item Function:Get-Process
    $env:ACTL_TEST_DIR = $previousDir
    $env:LOCALAPPDATA = $previousLocal
}
Write-Output 'Regression selection, stream capture, exit classification and existing-session protection passed'
& "$PSScriptRoot/system-fixture.Tests.ps1"
& "$PSScriptRoot/fixture-stream.Tests.ps1"
& "$PSScriptRoot/browser-probe.Tests.ps1"
& "$PSScriptRoot/office-package.Tests.ps1"

& "$PSScriptRoot/input-target.Tests.ps1"
& "$PSScriptRoot/phase2-readonly.Tests.ps1"
