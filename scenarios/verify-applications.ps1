param([string]$Actl="$PSScriptRoot/../target/release/actl.exe")
$ErrorActionPreference='Stop'
$previousTask=$env:ACTL_TASK_ID
$previousConfig=$env:ACTL_TASK_CONFIG
$previousDir=$env:ACTL_TEST_DIR
$env:ACTL_TASK_ID='verification-'+[guid]::NewGuid().ToString('N')
$env:ACTL_TASK_CONFIG=(Resolve-Path "$PSScriptRoot/verification-task.yml").Path
$env:ACTL_TEST_DIR=Join-Path (Resolve-Path "$PSScriptRoot/../target").Path $env:ACTL_TASK_ID
New-Item -ItemType Directory $env:ACTL_TEST_DIR|Out-Null
$binary=(Resolve-Path $Actl).Path
$runs=[Collections.Generic.List[object]]::new()
try {
    @{task=$env:ACTL_TASK_ID;config=$env:ACTL_TASK_CONFIG;binary=$binary;sha256=(Get-FileHash $binary).Hash;started=[DateTime]::Now.ToString('o')}|
        ConvertTo-Json|Set-Content "$env:ACTL_TEST_DIR/environment.json" -Encoding utf8
    foreach($scenario in @('vscode-edit','excel-table','word-edit','zotero-search')) {
        & pwsh -NoProfile -File "$PSScriptRoot/$scenario.ps1" -Actl $binary *> "$env:ACTL_TEST_DIR/$scenario.log"
        $runs.Add(@{scenario=$scenario;exit_code=$LASTEXITCODE})
        # An interrupted desktop handoff ends this batch; never repeatedly prompt.
        $reports=@(Get-ChildItem $env:ACTL_TEST_DIR -Directory -Filter "$scenario-*"|ForEach-Object {
            $path=Join-Path $_.FullName result.json
            if(Test-Path $path){Get-Content $path -Raw|ConvertFrom-Json}
        })
        if($reports.Count -ne 1){break}
        $report=$reports[0]
        $blocked=$false
        foreach($call in $report.calls){
            if(Test-Path $call.stdout){
                $result=Get-Content $call.stdout -Raw|ConvertFrom-Json
                if(-not $result.ok -and ($result.error.code -eq 'ABORTED' -or $result.error.evidence.stage -eq 'handoff')){$blocked=$true}
            }
        }
        if($blocked -or $report.cleanup -like 'retained*'){break}
    }
}finally {
    $runs.ToArray()|ConvertTo-Json|Set-Content "$env:ACTL_TEST_DIR/summary.json" -Encoding utf8
    Write-Output "Evidence: $env:ACTL_TEST_DIR"
    $env:ACTL_TASK_ID=$previousTask
    $env:ACTL_TASK_CONFIG=$previousConfig
    $env:ACTL_TEST_DIR=$previousDir
}
if(@($runs|Where-Object exit_code -NE 0).Count){exit 1}
