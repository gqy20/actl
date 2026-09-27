$ErrorActionPreference='Stop'
$root=(Resolve-Path "$PSScriptRoot/../../..").Path
$testRoot=Join-Path $root ('target/browser-stop-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory "$testRoot/actl" -Force | Out-Null
[IO.File]::WriteAllText("$testRoot/actl/stop-requested",'test-stop')
$oldLocal=$env:LOCALAPPDATA; $oldDir=$env:ACTL_TEST_DIR
try {
    $env:LOCALAPPDATA=$testRoot
    foreach($browser in @('edge','chrome')){
        $env:ACTL_TEST_DIR=Join-Path $testRoot $browser
        & "$root/fixtures/browser-probe.ps1" -Browser $browser -Scenario content-view -Actl (Get-Process -Id $PID).Path 2>$null
        if($LASTEXITCODE -ne 77){throw 'Browser probe ignored stop'}
        $file=Get-ChildItem $env:ACTL_TEST_DIR -Recurse -Filter result.json | Select-Object -First 1
        $report=Get-Content $file.FullName -Raw | ConvertFrom-Json
        if($report.calls.Count -ne 0 -or $report.passed){throw 'Browser probe launched despite stop'}
    }
    if(-not (Test-Path "$testRoot/actl/stop-requested")){throw 'Browser probe cleared stop'}
    foreach($app in @('cloudmusic','zotero','meeting','vscode')){
        $hostExe=(Get-Process -Id $PID).Path
        & "$root/scenarios/app-observe.ps1" -App $app -Program $hostExe -Actl $hostExe -OutputRoot "$testRoot/observe" -CloseOwned
        if($LASTEXITCODE -ne 1){throw 'App observation ignored stop'}
        $report=Get-Content "$testRoot/observe/$app/report.json" -Raw | ConvertFrom-Json
        if($report.calls.Count -ne 0 -or -not $report.failure){throw 'App observation launched despite stop'}
    }
}finally{$env:LOCALAPPDATA=$oldLocal;$env:ACTL_TEST_DIR=$oldDir}
Write-Output 'Browser stop guard passed'
. "$root/fixtures/system-fixture.ps1"
. "$root/fixtures/browser-probe-lib.ps1"
$elements=@(
    [pscustomobject]@{depth=0;role='Window';name='test'},
    [pscustomobject]@{depth=1;role='Tab';name=''},
    [pscustomobject]@{depth=2;role='TabItem';name='owned-page'},
    [pscustomobject]@{depth=1;role='Pane';name='translation'},
    [pscustomobject]@{depth=2;role='Tab';name='English'},
    [pscustomobject]@{depth=3;role='TabItem';name='English'},
    [pscustomobject]@{depth=3;role='TabItem';name='Chinese'}
)
Assert-BrowserOwnedTab $elements 'owned-page'
$extra=@($elements[0..2])+@([pscustomobject]@{depth=2;role='TabItem';name='user-page'})+@($elements[3..6])
$rejected=$false
try{Assert-BrowserOwnedTab $extra 'owned-page'}catch{$rejected=$true}
Assert-Fixture $rejected 'Extra browser tab was accepted'
