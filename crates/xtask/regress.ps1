[CmdletBinding()]
param([switch]$List, [switch]$Unattended, [ValidateSet('all','flow','system','scenario')][string]$Suite = 'all', [string]$Case)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
. "$PSScriptRoot/regress-lib.ps1"
$root = (Resolve-Path "$PSScriptRoot/../..").Path
$cases = @(Get-RegressionCases $root $Suite $Case)
if ($List) {
    $cases | Select-Object id,suite,state,input,reason | Format-Table -Wrap | Out-String | Write-Host
    exit 0
}
$actl = (Resolve-Path "$root/target/validation/release/actl.exe").Path
$runId = 'regress-' + [guid]::NewGuid().ToString('N')
$runDir = Join-Path $root "target/$runId"
New-Item -ItemType Directory -Path $runDir | Out-Null
$commit = (& git -C $root rev-parse HEAD | Out-String).Trim()
$dirty = [bool](& git -C $root status --porcelain | Out-String).Trim()
$version = (& $actl --version | Out-String).Trim()
$report = [ordered]@{version=1;id=$runId;unattended=[bool]$Unattended;started_at=[DateTimeOffset]::Now.ToString('o');commit=$commit;dirty=$dirty;binary=$actl;binary_sha256=(Get-FileHash $actl -Algorithm SHA256).Hash;binary_version=$version;os=[Environment]::OSVersion.VersionString;cases=@()}
$reportFile = Join-Path $runDir 'report.json'
$report.selection = @{suite=$Suite;case=$Case;complete_scope='selected_cases_only'}
function Save-Report { $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $reportFile -Encoding utf8 }
Save-Report
foreach ($item in $cases) {
    $record = [ordered]@{id=$item.id;suite=$item.suite;status='running';reason=$item.reason;exit_code=$null;elapsed_ms=0;stdout=$null;stderr=$null}
    $report.cases += $record
    if ($Unattended -and -not (Test-UnattendedCase $item)) {
        $record.status = 'skipped'
        $record.reason = 'Not approved for unattended execution; requires separate acceptance. ' + $item.reason
        Save-Report
        continue
    }
    if ($item.state -ne 'ready') {
        $record.status = if ($item.state -eq 'manual') { 'skipped' } else { 'blocked' }
        Save-Report
        continue
    }
    $caseDir = Join-Path $runDir $item.id
    New-Item -ItemType Directory -Path $caseDir | Out-Null
    $prefix = Join-Path $caseDir 'fixture'
    $record.stdout = "$prefix.stdout.log"
    $record.stderr = "$prefix.stderr.log"
    $record.capture = "$prefix.capture.json"
    Save-Report
    Write-Host "Running $($item.id) (input: $($item.input))"
    $timer = [Diagnostics.Stopwatch]::StartNew()
    try {
        $exitCode = Invoke-RegressionProcess (Get-Process -Id $PID).Path @('-NoProfile','-File',(Join-Path $root $item.script),'-Actl',$actl) $root $prefix @{ACTL=$actl;ACTL_TEST_DIR=$caseDir;ACTL_TASK_ID="$runId-$($item.id)"}
        $record.exit_code = $exitCode
        $record.status = Get-CaseStatus $exitCode
    } catch {
        $record.status = 'failed'
        $record.reason = $_.Exception.Message
    } finally {
        $record.elapsed_ms = $timer.ElapsedMilliseconds
        Save-Report
    }
    Write-Host "$($item.id): $($record.status)"
    # A failed fixture can leave an uncertain desktop. Do not start another one.
    if ($record.status -eq 'failed') { break }
}
foreach ($item in $cases) {
    if ($item.id -notin @($report.cases | ForEach-Object { $_.id })) {
        $report.cases += @{id=$item.id;suite=$item.suite;status='skipped';reason='Earlier fixture failed; desktop state requires review';exit_code=$null;elapsed_ms=0}
    }
}
$report.finished_at = [DateTimeOffset]::Now.ToString('o')
$report.complete = -not @($report.cases | Where-Object { $_.status -ne 'passed' }).Count
Save-Report
Write-Host "Report: $reportFile"
if (@($report.cases | Where-Object { $_.status -in @('failed','blocked') }).Count -or -not @($report.cases | Where-Object { $_.status -eq 'passed' }).Count) { exit 1 }
exit 0
