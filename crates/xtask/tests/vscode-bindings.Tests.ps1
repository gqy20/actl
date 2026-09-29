$ErrorActionPreference='Stop'
$lines=@(Get-Content "$PSScriptRoot/../../../scenarios/vscode-edit.ps1") + @(Get-Content "$PSScriptRoot/../../../fixtures/browser-probe.ps1")
foreach($line in $lines){
 if($line -match 'Invoke-FixtureCli' -and $line -match '\.ref' -and $line -notmatch '--snapshot'){
  throw "Application scenario replays an unbound ref: $line"
 }
}
Write-Output 'VS Code and browser scenario refs carry snapshot bindings'
$word=Get-Content "$PSScriptRoot/../../../scenarios/word-edit.ps1"
foreach($line in $word){
 if($line -match "Invoke-FixtureCli.*'press'" -and $line -notmatch '--target'){throw "Word keyboard action lacks document focus assertion: $line"}
 if($line -match "Invoke-FixtureCli.*@\('paste'"){throw 'Word paste must resolve the document target'}
}
$excel=Get-Content "$PSScriptRoot/../../../scenarios/excel-table.ps1"
foreach($line in $excel){
 if($line -match "Invoke-FixtureCli.*'press'" -and $line -notmatch '--target'){throw "Excel keyboard action lacks focus assertion: $line"}
 if($line -match "Invoke-FixtureCli.*@\('paste'"){throw 'Excel paste must assert the selected cell focus'}
}
