# System-only fixture: identity, exact focus refusal and post-input readback.
param([string]$Actl=$env:ACTL)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/system-fixture.ps1"
. "$PSScriptRoot/input-target-lib.ps1"
if(-not $Actl){$Actl="$PSScriptRoot/../target/validation/release/actl.exe"}
$base=if($env:ACTL_TEST_DIR){$env:ACTL_TEST_DIR}else{"$PSScriptRoot/../target/input-target-$([guid]::NewGuid().ToString('N'))"}
New-Item -ItemType Directory -Force $base|Out-Null
$root=(Resolve-Path $base).Path
$marker='actl-input-'+[guid]::NewGuid().ToString('N')
$file=Join-Path $root "$marker.txt"
$c=@{root=$root;binary=(Resolve-Path $Actl).Path;calls=[Collections.Generic.List[object]]::new();owned=$null;app=$null;uncertain=$false;cleanup='not_started';case='input-target'}
$checks=[Collections.Generic.List[object]]::new();$code=1;$reason='';$saved=$false
function Expect-Refusal([string[]]$Arguments,[string]$Expected){
 $before=$c.calls.Count
 try{$null=Invoke-FixtureCli $c $Arguments;throw 'Expected refusal did not occur'}catch{
  if($c.calls.Count -ne $before+1){throw}
  $last=$c.calls[$c.calls.Count-1]
  $e=Get-Content -LiteralPath $last.stdout -Raw|ConvertFrom-Json
  Assert-Fixture (-not $e.ok -and $e.error.code -eq $Expected) "Wrong refusal; expected $Expected"
  $last.status='expected_refusal'
  $checks.Add(@{name=$Expected;passed=$true;evidence=$last.stdout})
 }
}
try{
 if(Test-Path "$env:LOCALAPPDATA/actl/stop-requested"){$code=77;throw 'Emergency stop active'}
 if(@(Get-Process Notepad -ErrorAction SilentlyContinue).Count){$code=77;throw 'Existing Notepad session; untouched'}
 $program=Join-Path $env:WINDIR 'System32/notepad.exe'
 if(-not(Test-Path $program)){
  $package=Get-AppxPackage Microsoft.WindowsNotepad|Select-Object -First 1
  if(-not $package){$code=77;throw 'Notepad unavailable'}
  $program=Join-Path $package.InstallLocation 'Notepad/Notepad.exe'
 }
 [IO.File]::WriteAllText($file,'')
 $null=Invoke-FixtureCli $c @('launch',$program,$file)
 $null=Invoke-FixtureCli $c @('wait','--window',$marker,'--timeout','12000')
 $windows=@((Invoke-FixtureCli $c @('list-windows')).windows|Where-Object {$_.title.Contains($marker)})
 Assert-Fixture ($windows.Count -eq 1) 'Owned window not unique'
 Save-FixtureOwner $c $windows[0] $marker
 $null=Invoke-FixtureCli $c @('focus-window','--hwnd',[string]$c.owned.hwnd)
 $s=Invoke-FixtureCli $c @('snapshot','--app',$c.app,'--skeleton')
 $doc=@($s.elements|Where-Object role -EQ 'Document')
 Assert-Fixture ($doc.Count -eq 1) 'Document not unique'
 $snapshot=$c.last_snapshot_id;$ref=$doc[0].ref
 $null=Invoke-FixtureCli $c @('get',$ref,'--snapshot',$snapshot,'--app',$c.app)
 $null=Invoke-FixtureCli $c @('type',$ref,'ACTL input 42','--paste','--verify-value','ACTL input 42','--snapshot',$snapshot,'--app',$c.app)
 $checks.Add(@{name='bound_type_and_readback';passed=$true})
 $null=Invoke-FixtureCli $c @('press','ctrl+s','--target','role:Document','--app',$c.app)
 Assert-Fixture ([IO.File]::ReadAllText($file) -eq 'ACTL input 42') 'Saved content differs'
 $saved=$true
 $s=Invoke-FixtureCli $c @('snapshot','--app',$c.app)
 $button=@($s.elements|Where-Object role -EQ 'Button'|Select-Object -First 1)
 Assert-Fixture ($button.Count -eq 1) 'Need a second focus target'
 Expect-Refusal @('press','enter','--target',$button[0].ref,'--snapshot',$c.last_snapshot_id,'--app',$c.app) 'NOT_ACTIONABLE'
 # Deliberately mismatched expected value after a zero-length write; never repeat text.
 Expect-Refusal @('type','role:Document','','--verify-value','incorrect','--app',$c.app) 'ASSERTION_FAILED'
 $s=Invoke-FixtureCli $c @('snapshot','--app',$c.app)
 $doc=@($s.elements|Where-Object role -EQ 'Document');$snapshot=$c.last_snapshot_id
 $recordPath=Join-Path $env:LOCALAPPDATA "actl/snapshots/$snapshot.json"
 $record=Get-Content -LiteralPath $recordPath -Raw|ConvertFrom-Json
 $index=[int]$doc[0].ref.Substring(2)-1
 # Mutate only this fixture's record to simulate ordinal reuse by another instance.
 $record.elements[$index].runtime_id=@(-2147483647,123456789)
 $record|ConvertTo-Json -Depth 12|Set-Content -LiteralPath $recordPath -Encoding utf8
 Expect-Refusal @('get',$doc[0].ref,'--snapshot',$snapshot,'--app',$c.app) 'STALE_REF'
 $null=Invoke-FixtureCli $c @('verify','--value','role:Document','ACTL input 42','--app',$c.app)
 $checks.Add(@{name='refusals_preserved_content';passed=$true})
 $code=0
}catch{$reason=$_.Exception.Message}finally{
 if($saved -and $c.owned){
  try{
   Close-InputFixtureTab $c
  }catch{$code=1;$reason+='; cleanup: '+$_.Exception.Message}
 }
 @{case='input-target';passed=($code -eq 0);exit_code=$code;reason=$reason;checks=@($checks.ToArray());calls=@($c.calls.ToArray());cleanup=$c.cleanup;file=$file;owned=$c.owned}|ConvertTo-Json -Depth 12|Set-Content "$root/result.json" -Encoding utf8
 Write-Output "Report: $root/result.json"
}
exit $code
