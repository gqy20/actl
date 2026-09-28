$ErrorActionPreference='Stop'
$root=(Resolve-Path "$PSScriptRoot/../../..").Path
. "$root/scenarios/uia-compare-lib.ps1"
$temp=Join-Path $root ('target/uia-compare-tests-'+[guid]::NewGuid().ToString('N'))
$null=New-Item -ItemType Directory -Path $temp
$context=@{root=$temp;calls=[Collections.Generic.List[object]]::new();timeout_ms=10000}
$hostExe=(Get-Process -Id $PID).Path
$data=Invoke-UiaReadProcess $context $hostExe @('-NoProfile','-Command','[Console]::Error.Write((''x'' * 100000)); [Console]::Out.Write(''{"ok":true,"data":""}'')')
if (-not $data.ok -or $data.data -cne '' -or (Get-Item $context.calls[0].stderr).Length -lt 100000) { throw 'Lost empty value or stderr evidence' }
foreach ($case in @(@('Write-Output ''not json''','invalid_json'),@('exit 9','failed'),@('Start-Sleep -Seconds 10','timeout'))) {
    $context.timeout_ms=if ($case[1] -eq 'timeout') { 200 } else { 10000 }
    $failed=$false
    try { $null=Invoke-UiaReadProcess $context $hostExe @('-NoProfile','-Command',$case[0]) } catch { $failed=$true }
    if (-not $failed -or $context.calls[-1].status -ne $case[1]) { throw "Missing failure classification: $($case[1])" }
    if (-not (Test-Path $context.calls[-1].stdout) -or -not (Test-Path $context.calls[-1].stderr)) { throw 'Failure evidence missing' }
}
$priorLocal=$env:LOCALAPPDATA
try {
    $env:LOCALAPPDATA=Join-Path $temp 'local'
    $null=New-Item -ItemType Directory -Path "$env:LOCALAPPDATA/actl"
    [IO.File]::WriteAllText("$env:LOCALAPPDATA/actl/stop-requested",'test-stop')
    & $hostExe -NoProfile -File "$root/scenarios/uia-compare.ps1" -Hwnd 1 -Actl missing.exe -Winapp missing.exe -OutputDirectory "$temp/blocked"
    if ($LASTEXITCODE -ne 77) { throw 'Stop must block before resolving or starting tools' }
    $report=Get-Content "$temp/blocked/report.json" -Raw | ConvertFrom-Json
    if ($report.calls.Count -ne 0 -or $report.collection_status -ne 'blocked') { throw 'Blocked run claimed observation' }
    if ((Get-Content "$env:LOCALAPPDATA/actl/stop-requested" -Raw) -ne 'test-stop') { throw 'Stop changed' }
} finally { $env:LOCALAPPDATA=$priorLocal }
Write-Output 'UIA comparison capture, timeout, malformed output and stop guard checks passed'
