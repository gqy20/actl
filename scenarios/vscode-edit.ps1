param([string]$Actl=$env:ACTL,[string]$Program="$env:LOCALAPPDATA/Programs/Microsoft VS Code/Code.exe",[switch]$Smoke)
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
function Type-Input([string]$Name,[string]$Text){
 $deadline=[DateTime]::UtcNow.AddSeconds(5)
 do{
  $f=Find-FixtureTarget $c $Name 'Edit'
  if(@($f.elements).Count -eq 1){break}
 }while([DateTime]::UtcNow -lt $deadline)
 Assert-Fixture (@($f.elements).Count -eq 1) "Input not unique: $Name"
 $c.input_ref=$f.elements[0].target;
 $null=Invoke-FixtureCli $c @('type',$f.elements[0].target,$Text,'--paste','--app',$c.app)
}
function Editor-Command([string]$Query,[string]$Prefix){
 $null=Invoke-FixtureCli $c @('press','f1','--app',$c.app)
 # VS Code exposes the active command result as UIA focus. Set the palette value
 # directly instead of weakening the exact keyboard-focus assertion for its Edit.
 $input=Find-FixtureTarget $c '键入要运行的命令的名称。' 'Edit'
 $value='>'+$Query
 $null=Invoke-FixtureCli $c @('set-value',$input.elements[0].target,$value,'--app',$c.app)
 $read=Invoke-FixtureCli $c @('get',$input.elements[0].target,'--property','value','--app',$c.app)
 Assert-Fixture ($read.properties.value -ceq $value) 'Command palette value did not match'
 $f=Find-FixtureTarget $c $Prefix 'ListItem'
 Assert-Fixture (@($f.elements).Count -eq 1) "Command not unique: $Prefix"
 $null=Invoke-FixtureCli $c @('click',$f.elements[0].target,'--app',$c.app)
}
function Assert-Saved([string]$Label,[string]$Expected){
 $null=Invoke-FixtureCli $c @('press','ctrl+s','--target',$editor,'--app',$c.app)
 $deadline=[DateTime]::UtcNow.AddSeconds(5)
 do{$actual=Read-Normalized $file;if($actual -eq $Expected){break};Start-Sleep -Milliseconds 100}while([DateTime]::UtcNow -lt $deadline)
 Assert-Fixture ($actual -eq $Expected) "Full file mismatch: $Label"
 $report.checks.Add(@{name=$Label;passed=$true;sha256=(Get-FileHash $file).Hash})
}
function Open-OwnedEditor {
 $baseline=@((Invoke-FixtureCli $c @('list-windows','--native')).windows|ForEach-Object hwnd)
 $null=Invoke-FixtureCli $c @('launch',$Program,'--new-window','--disable-extensions','--skip-welcome','--skip-release-notes','--force-renderer-accessibility',"--user-data-dir=$profile",$file)
 $null=Invoke-FixtureCli $c @('wait','--window',$marker,'--timeout','20000')
 $windows=@(Get-FixtureCandidates (Invoke-FixtureCli $c @('list-windows','--native')).windows $baseline $marker)
 Assert-Fixture ($windows.Count -eq 1) 'Editor ownership is ambiguous'
 $process=Get-CimInstance Win32_Process -Filter "ProcessId=$($windows[0].pid)"
 Assert-Fixture ($process.ExecutablePath -eq $Program -and $process.CommandLine.Contains("--user-data-dir=$profile")) 'Editor profile mismatch'
 Save-FixtureOwner $c $windows[0] $marker
    $null=Invoke-FixtureCli $c @('focus-window','--hwnd',[string]$c.owned.hwnd)
 for($attempt=0;$attempt -lt 3;$attempt++){
  try { $found=Find-FixtureTarget $c "$marker.txt" 'Edit'; break }
  catch {
   $last=Get-Content -LiteralPath $c.calls[$c.calls.Count-1].stdout -Raw|ConvertFrom-Json
   if($attempt -eq 2 -or -not $last.PSObject.Properties['error'] -or $last.error.code -ne 'STALE_REF'){throw}
   # Read-only startup recovery; each failed envelope remains in the report.
   Start-Sleep -Milliseconds 500
  }
 }
 Assert-Fixture (@($found.elements).Count -eq 1) 'Editor is not unique'
 # Re-resolve the exact editor on every call, including after UI reconstruction.
 return 'uia:'+(@{match=@{role='Edit';name=$found.elements[0].name}}|ConvertTo-Json -Compress)
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
 if($Smoke){
  $null=Invoke-FixtureCli $c @('type',$editor,'Consent probe ','--paste','--app',$c.app)
  Assert-Saved 'consent_input_saved' ('Consent probe '+$seed)
 }
 if(-not $Smoke){
 $current=$seed
 foreach($payload in @("Round 1 中文 789`n","Round 2 spaces & symbols [] {}`n","Round 3 多行`nUnicode Ω 12345`n")){
  $null=Invoke-FixtureCli $c @('type',$editor,$payload,'--paste','--app',$c.app)
  # Each previous iteration leaves the caret at the start of the document.
  $inserted=$payload+$current
  Assert-Saved 'insert' $inserted
  $null=Invoke-FixtureCli $c @('press','ctrl+z','--target',$editor,'--app',$c.app)
  Assert-Saved 'undo' $current
  $null=Invoke-FixtureCli $c @('press','ctrl+y','--target',$editor,'--app',$c.app)
  Assert-Saved 'redo' $inserted
  $current=$inserted
  $null=Invoke-FixtureCli $c @('press','ctrl+home','--target',$editor,'--app',$c.app)
 }
 $replacement="Replacement 中文 final`nNumber 123456789`n"
 $null=Invoke-FixtureCli $c @('press','ctrl+a','--target',$editor,'--app',$c.app)
 $null=Invoke-FixtureCli $c @('type',$editor,$replacement,'--paste','--app',$c.app)
 Assert-Saved 'replace_all' $replacement
 Close-FixtureWindow $c
 $c.owned=$null
 $editor=Open-OwnedEditor
 $read=Invoke-FixtureCli $c @('get',$editor,'--property','text','--app',$c.app)
 Assert-Fixture ($read.properties.text.Replace("`r`n","`n") -eq $replacement) 'Reopened editor text differs from saved file'
 $report.checks.Add(@{name='reopen_text';passed=$true})
 # Go-to-line and find must return focus to the editor, not the Chat Input.
 Editor-Command '转到行' '转到行/列...,'
 Type-Input '转到行。' '2'
 $null=Invoke-FixtureCli $c @('press','enter','--target',$c.input_ref,'--app',$c.app)
 $null=Invoke-FixtureCli $c @('press','home','--target',$editor,'--app',$c.app)
 $null=Invoke-FixtureCli $c @('type',$editor,'GOTO-','--paste','--app',$c.app)
 $replacement=$replacement.Replace('Number','GOTO-Number')
 Assert-Saved 'goto_line_insert' $replacement
 $null=Invoke-FixtureCli $c @('press','ctrl+h','--app',$c.app)
 foreach($fieldValue in @(@('查找','123456789'),@('替换','987654321'))){
  $field=Find-FixtureTarget $c $fieldValue[0] 'Edit'
  Assert-Fixture (@($field.elements).Count -eq 1) 'Find/replace input not unique'
  $null=Invoke-FixtureCli $c @('set-value',$field.elements[0].target,$fieldValue[1],'--app',$c.app)
 }
 $null=Invoke-FixtureCli $c @('click','name:全部替换 (Ctrl+Alt+Enter)','--app',$c.app)
 $null=Invoke-FixtureCli $c @('click','name:关闭 (Escape)','--app',$c.app)
 $replacement=$replacement.Replace('123456789','987654321')
 Assert-Saved 'find_replace_all' $replacement
 $second=Join-Path $root 'second-owned.txt'
 [IO.File]::WriteAllText($second,"Second owned file`n")
 Editor-Command '转到文件' '转到文件...,'
 Type-Input '按名称搜索文件' $second
 $null=Invoke-FixtureCli $c @('press','enter','--target',$c.input_ref,'--app',$c.app)
 $null=Invoke-FixtureCli $c @('wait','--window','second-owned.txt','--timeout','10000')
 $secondEditor=Find-FixtureTarget $c 'second-owned.txt' 'Edit'
 Assert-Fixture (@($secondEditor.elements).Count -eq 1) 'Second editor is ambiguous'
 $read=Invoke-FixtureCli $c @('get',$secondEditor.elements[0].target,'--property','text','--app',$c.app)
 Assert-Fixture ($read.properties.text.Replace("`r`n","`n") -eq "Second owned file`n") 'Quick-open selected wrong content'
 $report.checks.Add(@{name='quick_open_second_file';passed=$true})
 Editor-Command '转到文件' '转到文件...,'
 Type-Input '按名称搜索文件' $file
 $null=Invoke-FixtureCli $c @('press','enter','--target',$c.input_ref,'--app',$c.app)
 $null=Invoke-FixtureCli $c @('wait','--window',$marker,'--timeout','10000')
 $firstEditor=Find-FixtureTarget $c "$marker.txt" 'Edit'
 Assert-Fixture (@($firstEditor.elements).Count -eq 1) 'First editor is ambiguous'
 $read=Invoke-FixtureCli $c @('get',$firstEditor.elements[0].target,'--property','text','--app',$c.app)
 Assert-Fixture ($read.properties.text.Replace("`r`n","`n") -eq $replacement) 'Returning to first file lost content'
 $report.checks.Add(@{name='quick_open_return';passed=$true})
 }
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
