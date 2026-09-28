# Sequential A/B/A observation. Does not launch, focus, write to or close target apps.
param(
    [Parameter(Mandatory)][long]$Hwnd,
    [Parameter(Mandatory)][string]$Winapp,
    [string]$Actl="$PSScriptRoot/../target/validation/release/actl.exe",
    [ValidateRange(1,12)][int]$Depth=5,
    [ValidateRange(100,30000)][int]$TimeoutMs=10000,
    [string]$OutputDirectory="$PSScriptRoot/../target/uia-compare-$([guid]::NewGuid().ToString('N'))"
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/uia-compare-lib.ps1"
$root=[IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $root) { throw 'PROTOCOL: output directory must be new' }
$null=New-Item -ItemType Directory -Path $root
$context=@{root=$root;calls=[Collections.Generic.List[object]]::new();timeout_ms=$TimeoutMs}
$report=@{version=1;collection_status='failed';calls=$context.calls;os=[Environment]::OSVersion.VersionString;cleanup='no_target_resources_created';comparison='not_evaluated';views=@{actl='raw/full';winapp='control';winapp_depth=$Depth};limitations=@('Sequential observations are not atomic','Matching role/id/name is only a candidate correspondence','Raw/control hierarchy and depth differ; counts do not imply defects','Failed property/tree reads are errors; neither client provides an atomic observation')}
$exitCode=1
function Assert-StopClear {
    if (-not $env:LOCALAPPDATA) { throw 'INTERNAL: LOCALAPPDATA unavailable' }
    $stop=Join-Path $env:LOCALAPPDATA 'actl/stop-requested'
    if (Test-Path -LiteralPath $stop -ErrorAction Stop) { throw 'ABORTED: emergency stop remains active' }
}
function Read-Actl([string[]]$CallArgs) {
    Assert-StopClear
    Invoke-UiaReadProcess $context $script:actlPath $CallArgs
}
function Get-Identity {
    $native=Read-Actl @('list-windows','--native','--hwnd',[string]$Hwnd)
    $windows=@($native.data.windows)
    if ($windows.Count -ne 1 -or $windows[0].hwnd -ne $Hwnd) { throw 'STALE_REF: exact HWND is no longer observable' }
    $window=$windows[0]
    if (($window.PSObject.Properties['minimized'] -and $window.minimized) -or ($window.PSObject.Properties['visible'] -and -not $window.visible)) { throw 'NOT_ACTIONABLE: target is minimized or hidden; observation will not restore it' }
    $process=Get-Process -Id $window.pid -ErrorAction Stop
    return @{hwnd=$Hwnd;pid=$window.pid;process_started=$process.StartTime.ToUniversalTime().ToString('o');class=$window.class;window=$window;process_path=$process.Path}
}
try {
    Assert-StopClear
    if (-not $IsWindows) { throw 'PROTOCOL: Windows is required' }
    $script:actlPath=(Resolve-Path -LiteralPath $Actl).Path
    $winappPath=(Resolve-Path -LiteralPath $Winapp).Path
    if ([IO.Path]::GetFileName($actlPath) -ne 'actl.exe' -or [IO.Path]::GetFileName($winappPath) -ne 'winapp.exe') { throw 'PROTOCOL: supply direct actl.exe and winapp.exe paths; wrappers are not supported' }
    $report.tools=@(
        @{path=$actlPath;sha256=(Get-FileHash -LiteralPath $actlPath).Hash;version=(Invoke-UiaReadProcess $context $actlPath @('--version') $false)},
        @{path=$winappPath;sha256=(Get-FileHash -LiteralPath $winappPath).Hash;version=(Invoke-UiaReadProcess $context $winappPath @('--version') $false)}
    )
    $before=Get-Identity; $report.before=$before
    $first=Read-Actl @('snapshot','--app',"hwnd:$Hwnd")
    Assert-StopClear
    $middle=Invoke-UiaReadProcess $context $winappPath @('ui','inspect','-w',[string]$Hwnd,'--depth',[string]$Depth,'--json')
    $second=Read-Actl @('snapshot','--app',"hwnd:$Hwnd")
    $after=Get-Identity; $report.after=$after
    foreach ($field in @('hwnd','pid','process_started','class')) {
        if ($before[$field] -ne $after[$field]) { throw 'STALE_REF: target identity changed during observation' }
    }
    if (-not $first.data.PSObject.Properties['elements'] -or -not $second.data.PSObject.Properties['elements']) { throw 'PROTOCOL: missing actl elements' }
    if (-not $middle.PSObject.Properties['windows'] -or @($middle.windows).Count -ne 1 -or $middle.windows[0].hwnd -ne $Hwnd) { throw 'PROTOCOL: winapp returned a different window or schema' }
    $a=@(Get-UiaObservationKeys $first.data.elements 'actl')
    $b=@(Get-UiaObservationKeys $middle.windows[0].elements 'winapp')
    $c=@(Get-UiaObservationKeys $second.data.elements 'actl')
    $stable=($a.Count -eq $c.Count -and ($a -join "`n") -ceq ($c -join "`n"))
    $report.observed_structure_unchanged=$stable
    $report.comparison='manual_review_required'
    $report.counts=@{actl_before=$a.Count;winapp=$b.Count;actl_after=$c.Count}
    $report.geometry=$middle.windows[0] | Select-Object windowDpi,scale,dpiAwareness,coordinateSpace
    $report.actl_truncated=@($first.data.truncated,$second.data.truncated)
    $report.collection_status='collected'
    $exitCode=0
} catch {
    $report.error=$_.Exception.Message
    if ($report.error -like 'ABORTED:*') { $report.collection_status='blocked'; $exitCode=77 }
} finally {
    $report | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath (Join-Path $root 'report.json') -Encoding utf8
    [Console]::Error.WriteLine("UIA comparison evidence: $root")
}
exit $exitCode
