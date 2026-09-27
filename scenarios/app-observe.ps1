param(
 [Parameter(Mandatory=$true)][ValidateSet('cloudmusic','zotero','meeting','vscode')][string]$App,
 [Parameter(Mandatory=$true)][string]$Program,
 [string]$Actl="$PSScriptRoot/../target/validation/release/actl.exe",
 [string]$OutputRoot="$PSScriptRoot/../target/app-observe-$([guid]::NewGuid().ToString('N'))",
 [switch]$CloseOwned
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/../fixtures/system-fixture.ps1"
$program=(Resolve-Path -LiteralPath $Program).Path
$root=Join-Path ([IO.Path]::GetFullPath($OutputRoot)) $App
New-Item -ItemType Directory $root -Force | Out-Null
$context=@{root=$root;binary=(Resolve-Path -LiteralPath $Actl).Path;calls=[Collections.Generic.List[object]]::new();owned=$null;app=$null;uncertain=$false;cleanup='not_started';case=$App}
$report=@{app=$App;program=$program;version=(Get-Item $program).VersionInfo.FileVersion;failure='';existing=$false;window=$null;cleanup='retained';owned=$null}
$minimized=$false
try {
 Assert-Fixture (-not (Test-Path (Join-Path $env:LOCALAPPDATA 'actl/stop-requested'))) 'Stop active'
 $windows=@((Invoke-FixtureCli $context @('list-windows')).windows)
 $baseline=@($windows | ForEach-Object hwnd)
 function Candidates($items){ @($items | Where-Object { $p=Get-Process -Id $_.pid -ErrorAction SilentlyContinue; $p -and $p.Path -eq $program -and $_.PSObject.Properties['hwnd'] -and $_.hwnd }) }
 $candidates=@(Candidates $windows)
 if($candidates.Count -eq 0){
  $args=@('launch',$program)
  if($App -eq 'vscode'){
   $marker='actl-probe-'+[guid]::NewGuid().ToString('N')
   $file=Join-Path $root "$marker.txt"
   [IO.File]::WriteAllText($file,"ACTL isolated editor probe`r`nChinese: 中文读取测试`r`nNumber: 123456`r`n")
   $args+=@('--new-window','--disable-extensions','--skip-welcome','--skip-release-notes','--force-renderer-accessibility',"--user-data-dir=$(Join-Path $root 'profile')",$file)
   $report.file=$file
  }
  $launch=Invoke-FixtureCli $context $args
  $deadline=[DateTime]::UtcNow.AddSeconds(20)
  do {
   Start-Sleep -Milliseconds 400
   $candidates=@(Candidates (Invoke-FixtureCli $context @('list-windows')).windows | Where-Object hwnd -NotIn $baseline)
  }while($candidates.Count -eq 0 -and [DateTime]::UtcNow -lt $deadline)
 }else{$report.existing=$true}
 Assert-Fixture ($candidates.Count -eq 1) "Expected one app window, found $($candidates.Count)"
 $window=$candidates[0];$context.app="hwnd:$($window.hwnd)";$report.window=$window
 if(-not $report.existing){Save-FixtureOwner $context $window $window.title;$report.owned=$context.owned}
 $minimized=$window.PSObject.Properties['minimized'] -and $window.minimized
 if($minimized){$null=Invoke-FixtureCli $context @('restore-window','--hwnd',[string]$window.hwnd)}
 $snapshot=Invoke-FixtureCli $context @('snapshot','--app',$context.app)
 $report.elements=@($snapshot.elements).Count
 $report.truncated=$snapshot.truncated
 $report.roles=@($snapshot.elements | Group-Object role | ForEach-Object { @{role=$_.Name;count=$_.Count} })
 $report.controls=@($snapshot.elements | Where-Object { $_.PSObject.Properties['ref'] } | Select-Object -First 60)
}catch{$report.failure=$_.Exception.Message}
finally {
 if($CloseOwned -and $context.owned){
  try{Close-FixtureWindow $context}catch{$report.failure+="; cleanup: $($_.Exception.Message)"}
  $report.cleanup=$context.cleanup
 }
 if($minimized -and $report.cleanup -notin @('closed_verified','already_closed') -and -not $context.uncertain -and -not (Test-Path (Join-Path $env:LOCALAPPDATA 'actl/stop-requested'))){try{$null=Invoke-FixtureCli $context @('minimize-window','--hwnd',[string]$report.window.hwnd);$report.cleanup='restored_minimized'}catch{$report.cleanup='restore_failed';$report.failure+="; restore: $($_.Exception.Message)"}}
 $report.calls=$context.calls.ToArray()
 $report | ConvertTo-Json -Depth 12 | Set-Content "$root/report.json"
 Write-Host "$App report: $root/report.json"
 Write-Host ($report | Select-Object app,version,existing,elements,truncated,failure,cleanup | ConvertTo-Json -Compress)
}
if($report.failure){exit 1}
