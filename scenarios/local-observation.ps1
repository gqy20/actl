# Read-only sampling of an existing window. No launch, activation, input or cleanup.
param(
    [string]$Actl="$PSScriptRoot/../target/validation/release/actl.exe",
    [Parameter(Mandatory)][long]$Hwnd,
    [Parameter(Mandatory)][string]$Within,
    [ValidateRange(0,40)][int]$Depth=40,
    [ValidateRange(1,5000)][int]$Limit=5000,
    [string]$OutputDirectory="$PSScriptRoot/../target/local-observation-$([guid]::NewGuid().ToString('N'))"
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/../fixtures/system-fixture.ps1"
. "$PSScriptRoot/local-observation-lib.ps1"
$binary=(Resolve-Path -LiteralPath $Actl).Path
$directory=[IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $directory) { throw 'Output directory must be new; prior evidence is immutable' }
$null=New-Item -ItemType Directory -Path $directory
$context=@{root=$directory;binary=$binary;calls=[Collections.Generic.List[object]]::new();uncertain=$false}
$report=[ordered]@{version=1;started_at=[DateTimeOffset]::Now.ToString('o');binary=$binary;
    binary_sha256=(Get-FileHash -LiteralPath $binary).Hash;os=[Environment]::OSVersion.VersionString;
    machine=$env:COMPUTERNAME;hwnd=$Hwnd;within=$Within;depth=$Depth;limit=$Limit;status='blocked';
    calls=$context.calls;replays=[Collections.Generic.List[object]]::new();cleanup='no_desktop_resources_created';
    limitations=@('Sequential observations are not atomic','Bytes are not tokenizer tokens',
        'Container resolution may still scan the whole window','No input, virtualization or external-machine acceptance')}
function Read-LocalObservation([string[]]$Arguments) {
    try { Invoke-FixtureCli $context $Arguments }
    finally { if ($context.calls.Count) { $context.calls[$context.calls.Count-1].arguments=$Arguments } }
}
function Get-LocalWindowIdentity {
    $items=@((Read-LocalObservation @('list-windows','--native','--hwnd',[string]$Hwnd)).windows)
    if ($items.Count -ne 1 -or $items[0].hwnd -ne $Hwnd) { throw 'Exact window identity unavailable' }
    $window=$items[0]
    if (($window.PSObject.Properties['minimized'] -and $window.minimized) -or
        ($window.PSObject.Properties['visible'] -and -not $window.visible)) { throw 'Window hidden/minimized; not restoring it' }
    $process=Get-Process -Id $window.pid -ErrorAction Stop
    return [ordered]@{hwnd=$Hwnd;pid=$window.pid;started=$process.StartTime.ToUniversalTime().Ticks;
        class=$window.class;process_path=$process.Path;process_version=$process.FileVersion}
}
try {
    $state=Read-LocalObservation @('execution-status')
    $report.execution_before=$state
    $beforeIdentity=Get-LocalWindowIdentity
    $report.identity=$beforeIdentity
    $app="hwnd:$Hwnd"
    $full=Read-LocalObservation @('snapshot','--app',$app)
    $fullCall=$context.calls[$context.calls.Count-1]
    $localArgs=@('snapshot','--app',$app,'--within',$Within,'--depth',[string]$Depth,'--limit',[string]$Limit)
    $local=Read-LocalObservation $localArgs
    $localCall=$context.calls[$context.calls.Count-1]
    $snapshot=$context.last_snapshot_id
    foreach ($element in @($local.elements | Where-Object {$_.PSObject.Properties['ref']} | Select-Object -First 3)) {
        $value=Read-LocalObservation @('get',$element.ref,'--app',$app,'--snapshot',$snapshot,'--property','name')
        if ($value.properties.name -cne $element.name) { throw 'Snapshot-bound ref read differs from local observation' }
        $report.replays.Add(@{ref=$element.ref;snapshot_id=$snapshot;status='passed'})
    }
    $localAfter=Read-LocalObservation $localArgs
    $fullAfter=Read-LocalObservation @('snapshot','--app',$app)
    $afterIdentity=Get-LocalWindowIdentity
    if (($beforeIdentity | ConvertTo-Json -Compress) -cne ($afterIdentity | ConvertTo-Json -Compress)) { throw 'Window process instance changed' }
    $afterState=Read-LocalObservation @('execution-status')
    $report.execution_after=$afterState
    if ($state.generation -cne $afterState.generation) { throw 'Execution generation changed during sampling; retain evidence' }
    $measurement=Get-LocalObservationMeasurement $full $local $localAfter $fullAfter (Get-Item $fullCall.stdout).Length (Get-Item $localCall.stdout).Length
    $report.measurement=$measurement
    $report.status=$measurement.status
    $report.full_wall_ms=$fullCall.process_elapsed_ms
    $report.local_wall_ms=$localCall.process_elapsed_ms
} catch {
    $report.reason=$_.Exception.Message
    if ($context.calls.Count) {
        $last=$context.calls[$context.calls.Count-1]
        try {
            $failed=Get-Content -LiteralPath $last.stdout -Raw | ConvertFrom-Json
            if ($failed.PSObject.Properties['error']) { $report.error=$failed.error }
        } catch { $report.evidence_error=$_.Exception.Message }
    }
}
finally {
    $report.finished_at=[DateTimeOffset]::Now.ToString('o')
    $report | ConvertTo-Json -Depth 40 | Set-Content -LiteralPath "$directory/report.json" -Encoding utf8
}
Write-Output "Report: $directory/report.json"
if ($report.status -ne 'measured') { exit 1 }
