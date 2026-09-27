param([string]$Actl=$env:ACTL,[string]$Program="$env:LOCALAPPDATA/Programs/Microsoft VS Code/Code.exe")
$ErrorActionPreference='Stop'
. "$PSScriptRoot/../fixtures/system-fixture.ps1"
if(-not $Actl){$Actl="$PSScriptRoot/../target/validation/release/actl.exe"}
$base=if($env:ACTL_TEST_DIR){$env:ACTL_TEST_DIR}else{"$PSScriptRoot/../target"}
$root=Join-Path $base ('vscode-edit-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $root -Force|Out-Null
$root=(Resolve-Path $root).Path
$c=@{root=$root;binary=(Resolve-Path $Actl).Path;calls=[Collections.Generic.List[object]]::new();owned=$null;app=$null;uncertain=$false;cleanup='not_started';case='vscode-edit'}
$report=@{passed=$false;reason='';checks=[Collections.Generic.List[object]]::new();cleanup='';application=$null}
$code=1
function Read-Normalized([string]$Path){[IO.File]::ReadAllText($Path).Replace("`r`n","`n")}
function Assert-Saved([string]$Label,[string]$Expected){
 $null=Invoke-FixtureCli $c @('press','ctrl+s','--app',$c.app)
 $deadline=[DateTime]::UtcNow.AddSeconds(5)
 do{$actual=Read-Normalized $file;if($actual -eq $Expected){break};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt $deadline)
 Assert-Fixture ($actual -eq $Expected) "Full file mismatch: $Label"
 $report.checks.Add(@{name=$Label;passed=$true;sha256=(Get-FileHash $file).Hash})
}
function Open-OwnedEditor {
 $baseline=@((Invoke-FixtureCli $c @('list-windows')).windows|ForEach-Object hwnd)
 $null=Invoke-FixtureCli $c @('launch',$Program,'--new-window','--disable-extensions','--skip-welcome','--skip-release-notes','--force-renderer-accessibility',"--user-data-dir=$profile",$file)
 $null=Invoke-FixtureCli $c @('wait','--window',$marker,'--timeout','20000')
 $windows=@(Get-FixtureCandidates (Invoke-FixtureCli $c @('list-windows')).windows $baseline $marker)
 Assert-Fixture ($windows.Count -eq 1) 'Editor ownership is ambiguous'
 $process=Get-CimInstance Win32_Process -Filter "ProcessId=$($windows[0].pid)"
 Assert-Fixture ($process.ExecutablePath -eq $Program -and $process.CommandLine.Contains("--user-data-dir=$profile")) 'Editor profile mismatch'
 Save-FixtureOwner $c $windows[0] $marker
 $found=Invoke-FixtureCli $c @('find','--name',"$marker.txt",'--role','Edit','--app',$c.app)
 Assert-Fixture (@($found.elements).Count -eq 1) 'Editor is not unique'
 return $found.elements[0].ref
}
try {
 if(Test-Path (Join-Path $env:LOCALAPPDATA 'actl/stop-requested')){$code=77;throw 'Emergency stop active'}
 if(-not (Test-Path -LiteralPath $Program)){$code=77;throw 'VS Code not installed'}
 $Program=(Resolve-Path -LiteralPath $Program).Path
 $marker='actl-edit-'+[guid]::NewGuid().ToString('N');$file=Join-Path $root "$marker.txt";$profile=Join-Path $root 'profile'
 $seed="Original 中文 baseline 123`nSecond line`n"
 [IO.File]::WriteAllText($file,$seed)
 $report.application=@{version=(Get-Item $Program).VersionInfo.FileVersion;file=$file;profile=$profile;program=$Program}
 $editor=Open-OwnedEditor
 $current=$seed
 foreach($payload in @("Round 1 中文 789`n","Round 2 spaces & symbols [] {}`n","Round 3 多行`nUnicode Ω 12345`n")){
  $null=Invoke-FixtureCli $c @('type',$editor,$payload,'--paste','--app',$c.app)
  # Each previous iteration leaves the caret at the start of the document.
  $inserted=$payload+$current
  Assert-Saved 'insert' $inserted
  $null=Invoke-FixtureCli $c @('press','ctrl+z','--app',$c.app)
  Assert-Saved 'undo' $current
  $null=Invoke-FixtureCli $c @('press','ctrl+y','--app',$c.app)
  Assert-Saved 'redo' $inserted
  $current=$inserted
  $null=Invoke-FixtureCli $c @('press','ctrl+home','--app',$c.app)
 }
 $replacement="Replacement 中文 final`nNumber 123456789`n"
 $null=Invoke-FixtureCli $c @('press','ctrl+a','--app',$c.app)
 $null=Invoke-FixtureCli $c @('type',$editor,$replacement,'--paste','--app',$c.app)
 Assert-Saved 'replace_all' $replacement
 Close-FixtureWindow $c
 $c.owned=$null
 $editor=Open-OwnedEditor
 $read=Invoke-FixtureCli $c @('get',$editor,'--property','text','--app',$c.app)
 Assert-Fixture ($read.properties.text.Replace("`r`n","`n") -eq $replacement) 'Reopened editor text differs from saved file'
 $report.checks.Add(@{name='reopen_text';passed=$true})
 $code=0
}catch{$report.reason=$_.Exception.Message}
finally {
 try{Close-FixtureWindow $c}catch{$code=1;$report.reason+="; cleanup: $($_.Exception.Message)"}
 $report.passed=$code -eq 0;$report.cleanup=$c.cleanup;$report.calls=$c.calls.ToArray();$report.exit_code=$code
 $report|ConvertTo-Json -Depth 12|Set-Content "$root/result.json" -Encoding utf8
 Write-Host "Report: $root/result.json"
 Write-Host ($report|Select-Object passed,reason,cleanup|ConvertTo-Json -Compress)
}
exit $code
