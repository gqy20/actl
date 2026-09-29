# Resource preparation and plan composition only. All desktop calls run in one flow.
param([string]$Actl="$PSScriptRoot/../target/validation/release/actl.exe",[switch]$PrepareOnly)
$ErrorActionPreference='Stop'
$binary=(Resolve-Path -LiteralPath $Actl).Path
$id='applications-'+[guid]::NewGuid().ToString('N')
$root=Join-Path "$PSScriptRoot/../target" $id
$null=New-Item -ItemType Directory -Path $root
$root=(Resolve-Path -LiteralPath $root).Path
$steps=[Collections.Generic.List[object]]::new()
$resources=[Collections.Generic.List[object]]::new()
foreach($scenario in @('vscode-edit','excel-table','word-edit','zotero-search')){
 $prepared=& "$PSScriptRoot/prepare-workflow.ps1" -Scenario $scenario -Actl $binary -PrepareOnly|ConvertFrom-Json
 $resources.Add($prepared)
 $raw=& $binary flow-validate --file $prepared.template --inputs $prepared.inputs --emit-plan
 if($LASTEXITCODE -ne 0){throw "Cannot prepare $scenario"}
 $result=$raw|ConvertFrom-Json -AsHashtable
 foreach($step in $result.data.plan.steps){
  $step.id="$scenario-$($step.id)"
  foreach($phase in @('run','before','verify','resume_check')){
   $call=$step[$phase]
   if($call -and $call.bind){foreach($key in @($call.bind.Keys)){$call.bind[$key]="$scenario-$($call.bind[$key])"}}
  }
  $steps.Add($step)
 }
}
$plan=Join-Path $root 'plan.yml'
@{version=1;name='Application verification';handoff=@{approval='once_per_run'};steps=$steps.ToArray()}|ConvertTo-Json -Depth 30|Set-Content $plan -Encoding utf8
& $binary flow-validate --file $plan > "$root/validate.json"
if($LASTEXITCODE -ne 0){throw 'Combined workflow preflight failed'}
if(-not $PrepareOnly){
 & $binary flow-run --file $plan --id $id --paused > "$root/start.json"
 if($LASTEXITCODE -ne 0){throw 'Combined workflow registration failed'}
}
$context=@{id=$id;plan=$plan;registered=(-not $PrepareOnly);steps=$steps.Count;resources=$resources.ToArray();binary=$binary;sha256=(Get-FileHash $binary).Hash;live_acceptance='pending'}
$context|ConvertTo-Json -Depth 10|Set-Content "$root/context.json" -Encoding utf8
$context|ConvertTo-Json -Depth 10 -Compress
