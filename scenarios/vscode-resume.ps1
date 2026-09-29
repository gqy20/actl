param([string]$Actl="$PSScriptRoot/../target/validation/release/actl.exe",[switch]$PrepareOnly,[switch]$Checkpoint)
$ErrorActionPreference='Stop'
# All desktop actions belong to the durable flow. This script only prepares
# isolated input and registers the plan; it never resumes by rerunning itself.
$binary=(Resolve-Path -LiteralPath $Actl).Path
$program="$env:LOCALAPPDATA/Programs/Microsoft VS Code/Code.exe"
if(-not (Test-Path -LiteralPath $program)){throw 'VS Code not installed'}
$id='vscode-resume-'+[guid]::NewGuid().ToString('N')
$root=Join-Path "$PSScriptRoot/../target" $id
$null=New-Item -ItemType Directory -Path $root
$root=(Resolve-Path -LiteralPath $root).Path
$file=Join-Path $root "$id.txt"
$profile=Join-Path $root 'profile'
[IO.File]::WriteAllText($file,'')
$target='uia:'+(@{match=@{role='Edit';name_contains="$id.txt"}}|ConvertTo-Json -Compress)
$expected='ACTL resumable input verified.'
# Desktop steps live in the shared workflow; only isolated test resources are prepared here.
$planFile=(Resolve-Path "$PSScriptRoot/../skills/actl/workflows/vscode-resume.yml").Path
if($Checkpoint){
 $template=[IO.File]::ReadAllText($planFile).Replace('  - id: input',"  - id: input`n    pause_before: true")
 $planFile=Join-Path $root 'checkpoint.yml'
 [IO.File]::WriteAllText($planFile,$template)
}
$inputFile=Join-Path $root 'inputs.json'
@{program=$program;profile=$profile;file=$file;app=$id;target=$target;text=$expected}|ConvertTo-Json|Set-Content -LiteralPath $inputFile -Encoding utf8
@{id=$id;root=$root;file=$file;profile=$profile;binary=$binary;binary_sha256=(Get-FileHash $binary).Hash;expected=$expected;cleanup='retained_for_review'}|ConvertTo-Json|Set-Content "$root/context.json" -Encoding utf8
& $binary flow-validate --file $planFile --inputs $inputFile | Set-Content "$root/validate.json" -Encoding utf8
if($LASTEXITCODE -ne 0){throw "Plan validation failed: $root/validate.json"}
if(-not $PrepareOnly){
 & $binary flow-run --file $planFile --inputs $inputFile --id $id --paused | Set-Content "$root/start.json" -Encoding utf8
 if($LASTEXITCODE -ne 0){throw "Plan registration failed: $root/start.json"}
}
Write-Output (@{id=$id;root=$root;registered=(-not $PrepareOnly);resume="actl flow-resume --id $id"}|ConvertTo-Json -Compress)
# The editor is deliberately retained: this task verifies in-memory text, not a
# disk save. Never claim Ctrl+S or window disappearance proves document persistence.
