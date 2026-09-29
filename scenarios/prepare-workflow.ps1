param(
 [Parameter(Mandatory)][ValidateSet('vscode-edit','word-edit','excel-table','zotero-search')][string]$Scenario,
 [string]$Actl="$PSScriptRoot/../target/validation/release/actl.exe",
 [string]$Program,
 [switch]$PrepareOnly
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/office-package.ps1"
$binary=(Resolve-Path -LiteralPath $Actl).Path
$id=$Scenario+'-'+[guid]::NewGuid().ToString('N')
$root=Join-Path "$PSScriptRoot/../target" $id
$null=New-Item -ItemType Directory -Path $root
$root=(Resolve-Path -LiteralPath $root).Path
$values=@{app=$id}
switch($Scenario){
 'vscode-edit' {
  $values.program="$env:LOCALAPPDATA/Programs/Microsoft VS Code/Code.exe"
  $values.profile=Join-Path $root 'profile'
  $values.file=Join-Path $root "$id.txt"
  $values.editor='uia:'+(@{match=@{role='Edit';name_contains="$id.txt"}}|ConvertTo-Json -Compress)
  $values.second=Join-Path $root "$id-second.txt"
  $values.second_editor='uia:'+(@{match=@{role='Edit';name_contains="$id-second.txt"}}|ConvertTo-Json -Compress)
  [IO.File]::WriteAllText($values.second,"Second owned file`n")
  [IO.File]::WriteAllText($values.file,"Original 中文 baseline 123`nSecond line`n")
 }
 'word-edit' {
  $values.program="$env:ProgramFiles/Microsoft Office/Root/Office16/WINWORD.EXE"
  $values.file=Join-Path $root "$id.docx"
  New-OfficeSeed $values.file word
 }
 'excel-table' {
  $values.program="$env:ProgramFiles/Microsoft Office/Root/Office16/EXCEL.EXE"
  $values.file=Join-Path $root "$id.xlsx"
  New-OfficeSeed $values.file excel
 }
 'zotero-search' {
  $values.program='C:/Program Files/Zotero/zotero.exe'
  $values.app='Zotero'
  $values.profile=Join-Path $root 'profile';$values.data=Join-Path $root 'data'
  $values.export=Join-Path $root 'export.json'
  $null=New-Item -ItemType Directory -Path $values.profile,$values.data
  @'
user_pref("extensions.zotero.httpServer.enabled", false);
user_pref("extensions.zotero.sync.autoSync", false);
user_pref("extensions.zotero.firstRun2", false);
user_pref("extensions.zotero.firstRunGuidance", false);
'@|Set-Content "$($values.profile)/user.js" -Encoding utf8
 }
}
if($Program){$values.program=$Program}
if(-not (Test-Path -LiteralPath $values.program)){throw "Application not installed: $($values.program)"}
$values.program=(Resolve-Path -LiteralPath $values.program).Path
$template=(Resolve-Path "$PSScriptRoot/../skills/actl/workflows/$Scenario.yml").Path
$inputFile=Join-Path $root 'inputs.json'
$values|ConvertTo-Json|Set-Content -LiteralPath $inputFile -Encoding utf8
& $binary flow-validate --file $template --inputs $inputFile > "$root/validate.json"
if($LASTEXITCODE -ne 0){throw "Preflight failed: $root/validate.json"}
if(-not $PrepareOnly){
 & $binary flow-run --file $template --inputs $inputFile --id $id --paused > "$root/start.json"
 if($LASTEXITCODE -ne 0){throw "Registration failed: $root/start.json"}
}
$result=@{id=$id;scenario=$Scenario;template=$template;inputs=$inputFile;binary=$binary;sha256=(Get-FileHash $binary).Hash;registered=(-not $PrepareOnly);cleanup='resources_prepared';coverage='workflow_migrated; live_acceptance_pending'}
$result|ConvertTo-Json|Set-Content "$root/context.json" -Encoding utf8
$result|ConvertTo-Json -Compress
