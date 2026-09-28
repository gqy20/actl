# Passive acceptance: no launches, focus changes, input, clipboard or window cleanup.
param(
    [string]$Actl="$PSScriptRoot/../target/validation/release/actl.exe",
    [long[]]$Hwnd=@(),
    [ValidateRange(1,1000)][int]$Samples=100,
    [string]$OutputDirectory="$PSScriptRoot/../target/phase2-readonly-$([guid]::NewGuid().ToString('N'))"
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/../fixtures/system-fixture.ps1"
. "$PSScriptRoot/phase2-readonly-lib.ps1"
$binary=(Resolve-Path -LiteralPath $Actl).Path
$directory=[IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $directory) { throw 'Output directory must be new; previous evidence is immutable' }
$null=New-Item -ItemType Directory -Path $directory
$context=@{root=$directory;binary=$binary;calls=[Collections.Generic.List[object]]::new();uncertain=$false}
$report=[ordered]@{version=1;started_at=[DateTimeOffset]::Now.ToString('o');binary=$binary;
    binary_sha256=(Get-FileHash -LiteralPath $binary).Hash;os=[Environment]::OSVersion.VersionString;
    machine=$env:COMPUTERNAME;processor_count=[Environment]::ProcessorCount;sample_count=$Samples;
    calls=$context.calls;checks=[Collections.Generic.List[object]]::new();cleanup='no_desktop_resources_created';
    limitations=@('Process wall time ends at exit, excluding evidence serialization; OS file cache is not flushed',
        'Sequential snapshots are not atomic','Byte reduction does not measure tokenizer tokens',
        'This report does not cover physical input, human handoff, other machines or complete Phase 2')}
function Save-Phase2Report { $report | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath "$directory/report.json" -Encoding utf8 }
function Check-Phase2Stop {
    if (-not $env:LOCALAPPDATA) { throw 'LOCALAPPDATA unavailable' }
    if (Test-Path -LiteralPath "$env:LOCALAPPDATA/actl/stop-requested") { throw 'ABORTED: existing emergency stop retained' }
}
function Read-Phase2([string[]]$Arguments) {
    Check-Phase2Stop
    Invoke-FixtureCli $context $Arguments
}
function Get-Phase2Identity([long]$Handle) {
    $items=@((Read-Phase2 @('list-windows','--native','--hwnd',[string]$Handle)).windows)
    if ($items.Count -ne 1 -or $items[0].hwnd -ne $Handle) { throw 'Exact window identity unavailable' }
    $window=$items[0]
    if (($window.PSObject.Properties['minimized'] -and $window.minimized) -or ($window.PSObject.Properties['visible'] -and -not $window.visible)) { throw 'Target is minimized or hidden; will not restore it' }
    $process=Get-Process -Id $window.pid -ErrorAction Stop
    return @{hwnd=$Handle;pid=$window.pid;started=$process.StartTime.ToUniversalTime().Ticks;class=$window.class}
}
try {
    Check-Phase2Stop
    $times=[Collections.Generic.List[double]]::new()
    for ($i=0;$i -lt $Samples;$i++) {
        $null=Read-Phase2 @('status')
        $times.Add($context.calls[$context.calls.Count-1].process_elapsed_ms)
    }
    $p50=Get-Phase2Percentile $times.ToArray() 0.5
    $p95=Get-Phase2Percentile $times.ToArray() 0.95
    $report.checks.Add(@{case='new_process_status';status=$(if ($Samples -lt 100) {'blocked'} elseif ($p50 -lt 50 -and $p95 -lt 80) {'passed'} else {'failed'});p50_ms=$p50;p95_ms=$p95;required_samples=100})
    foreach ($callArgs in @(@('list-windows','--native'),@('list-windows'),@('display-status'),@('wait','--idle','1','--timeout','1000'))) {
        try { $null=Read-Phase2 $callArgs; $report.checks.Add(@{case=($callArgs -join ' ');status='passed'}) }
        catch { $report.checks.Add(@{case=($callArgs -join ' ');status='failed';reason=$_.Exception.Message}) }
        Save-Phase2Report
    }
    foreach ($handle in $Hwnd) {
        try {
            $before=Get-Phase2Identity $handle
            $full=Read-Phase2 @('snapshot','--app',"hwnd:$handle")
            $fullCall=$context.calls[$context.calls.Count-1]
            $skeleton=Read-Phase2 @('snapshot','--app',"hwnd:$handle",'--skeleton')
            $after=Read-Phase2 @('snapshot','--app',"hwnd:$handle")
            $identity=Get-Phase2Identity $handle
            foreach ($key in @('hwnd','pid','started','class')) { if ($before[$key] -ne $identity[$key]) { throw 'Window instance changed during sampling' } }
            $projection=Get-Phase2Projection $full $skeleton $after
            $report.checks.Add(@{case='snapshot_projection';hwnd=$handle;identity=$identity;status=$projection.status;measurement=$projection;
                snapshot_wall_ms=$fullCall.process_elapsed_ms;envelope_bytes=(Get-Item -LiteralPath $fullCall.stdout).Length;
                below_500ms=($fullCall.process_elapsed_ms -lt 500);below_2kb=((Get-Item -LiteralPath $fullCall.stdout).Length -lt 2048)})
        } catch { $report.checks.Add(@{case='snapshot_projection';hwnd=$handle;status='failed';reason=$_.Exception.Message}) }
        Save-Phase2Report
    }
    if (-not $Hwnd.Count) { $report.checks.Add(@{case='snapshot_projection';status='skipped';reason='No observed target HWND supplied'}) }
} catch { $report.checks.Add(@{case='run';status='blocked';reason=$_.Exception.Message}) }
finally { $report.finished_at=[DateTimeOffset]::Now.ToString('o'); Save-Phase2Report }
Write-Output "Report: $directory/report.json"
if (@($report.checks | Where-Object { $_.status -in @('failed','blocked') }).Count) { exit 1 }
exit 0
