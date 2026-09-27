$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot/../../..").Path
. "$root/fixtures/system-fixture.ps1"
$window = [pscustomobject]@{hwnd=42;pid=7;class='TestWindow';title='owned-marker'}
$owner = @{hwnd=42;pid=7;class='TestWindow';marker='owned-marker';started=123}
Assert-Fixture (Test-FixtureIdentity $owner $window 123) 'Expected matching identity'
Assert-Fixture (-not (Test-FixtureIdentity $owner $window 124)) 'Reused PID was accepted'
$changed = [pscustomobject]@{hwnd=42;pid=7;class='TestWindow';title='user document'}
Assert-Fixture (-not (Test-FixtureIdentity $owner $changed 123)) 'Changed window was accepted'
Assert-Fixture (@(Get-FixtureCandidates @($window) @(42) 'owned-marker' 7).Count -eq 0) 'Existing HWND accepted as owned'
Assert-Fixture (@(Get-FixtureCandidates @($window) @() 'owned-marker' 8).Count -eq 0) 'Unrelated process accepted'
Assert-Fixture (@(Get-FixtureCandidates @($window,$window) @() 'owned-marker' 7).Count -eq 2) 'Ambiguous candidates silently narrowed'

$testRoot = Join-Path $root "target/fixture-lifecycle-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory "$testRoot/actl" -Force | Out-Null
$previousLocal = $env:LOCALAPPDATA
$previousDir = $env:ACTL_TEST_DIR
$hostExe = (Get-Process -Id $PID).Path
try {
    $env:LOCALAPPDATA = $testRoot
    [IO.File]::WriteAllText("$testRoot/actl/stop-requested",'test-stop')
    foreach ($case in @('golden-task-1','win32-table-extract','virtualized-list')) {
        $env:ACTL_TEST_DIR = Join-Path $testRoot $case
        & "$root/fixtures/$case.ps1" -Actl $hostExe 2>$null
        Assert-Fixture ($LASTEXITCODE -eq 77) "$case did not block on stop"
        $reportPath = Get-ChildItem $env:ACTL_TEST_DIR -Filter result.json -Recurse | Select-Object -First 1
        $report = Get-Content $reportPath.FullName -Raw | ConvertFrom-Json
        Assert-Fixture ($report.calls.Count -eq 0 -and -not $report.passed) "$case ran a command despite stop"
    }
    Assert-Fixture (Test-Path "$testRoot/actl/stop-requested") 'Stop flag removed'
    # Change only our isolated test path, not the real desktop stop state.
    $env:LOCALAPPDATA = "$testRoot/no-stop"
    function Get-Process { param($Name,$Id,$ErrorAction) return [pscustomobject]@{StartTime=[datetime]::new(123,[DateTimeKind]::Utc);ProcessName='mock'} }
    foreach ($case in @('golden-task-1','win32-table-extract')) {
        $env:ACTL_TEST_DIR = Join-Path $testRoot "$case-existing"
        & "$root/fixtures/$case.ps1" -Actl $hostExe 2>$null
        Assert-Fixture ($LASTEXITCODE -eq 77) "$case reused an existing process"
        $reportPath = Get-ChildItem $env:ACTL_TEST_DIR -Filter result.json -Recurse | Select-Object -First 1
        $report = Get-Content $reportPath.FullName -Raw | ConvertFrom-Json
        Assert-Fixture ($report.calls.Count -eq 0) 'Existing application triggered CLI cleanup'
    }
    $script:issued = [Collections.Generic.List[string]]::new()
    $script:visible = $changed
    function Invoke-FixtureCli($Context,[string[]]$Arguments) {
        $script:issued.Add($Arguments[0])
        if ($Arguments[0] -eq 'list-windows') { return [pscustomobject]@{windows=@($script:visible)} }
        throw 'Unexpected mutation during identity test'
    }
    $context = @{owned=$owner;uncertain=$false;cleanup='';case='golden-task-1'}
    $rejected = $false
    try { Close-FixtureWindow $context } catch { $rejected = $true }
    Assert-Fixture ($rejected -and 'close-window' -notin $script:issued) 'Changed window was closed'
    $script:issued.Clear()
    $context.uncertain = $true
    try { Close-FixtureWindow $context } catch {}
    Assert-Fixture ($script:issued.Count -eq 0) 'Uncertain result caused follow-up commands'
    $script:visible = $window
    $script:tabs = @([pscustomobject]@{role='TabItem';name='owned-marker'},[pscustomobject]@{role='TabItem';name='user folder'})
    function Invoke-FixtureCli($Context,[string[]]$Arguments) {
        $script:issued.Add($Arguments[0])
        switch ($Arguments[0]) {
            'list-windows' { return [pscustomobject]@{windows=@($script:visible | Where-Object { $null -ne $_ })} }
            'snapshot' { return [pscustomobject]@{elements=$script:tabs} }
            'close-window' {
                Assert-Fixture ($Arguments[1] -eq '--hwnd' -and $Arguments[2] -eq '42') 'Cleanup targeted wrong window'
                $script:visible = $null
                return @{}
            }
            default { throw 'Unexpected cleanup command' }
        }
    }
    $context = @{owned=$owner;uncertain=$false;cleanup='';case='virtualized-list';app='hwnd:42'}
    $rejected = $false
    try { Close-FixtureWindow $context } catch { $rejected = $true }
    Assert-Fixture ($rejected -and 'close-window' -notin $script:issued) 'Explorer user tab was closed'
    $script:issued.Clear()
    $script:tabs = @([pscustomobject]@{role='TabItem';name='owned-marker'})
    Close-FixtureWindow $context
    Assert-Fixture ($context.cleanup -eq 'closed_verified' -and @($script:issued | Where-Object { $_ -eq 'close-window' }).Count -eq 1) 'Owned window cleanup was not verified'
    $rejected = $false
    try { Close-FixtureWindow $context } catch { $rejected = $true }
    Assert-Fixture $rejected 'Missing UIA window with a live owner process was reported closed'
} finally {
    $env:LOCALAPPDATA = $previousLocal
    $env:ACTL_TEST_DIR = $previousDir
}
Write-Output 'System fixture ownership, process reuse, stop and uncertain-result checks passed'
