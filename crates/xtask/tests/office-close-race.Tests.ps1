param([string]$Package="$PSScriptRoot/../../../scenarios/office-package.ps1")
$ErrorActionPreference='Stop'
. "$PSScriptRoot/../../../fixtures/system-fixture.ps1"
. $Package
$temp=Join-Path $PSScriptRoot '../../../target/office-close-race'
New-Item -ItemType Directory -Force $temp|Out-Null
'{"ok":false,"error":{"code":"NOT_FOUND"}}'|Set-Content "$temp/missing.json"
function Close-FixtureWindow($Context){throw 'Owned window did not close'}
function Get-Process {param($Id,$ErrorAction) [pscustomobject]@{StartTime=[datetime]::new(123,[DateTimeKind]::Utc)}}
$script:reads=0
function Invoke-FixtureCli($Context,$Arguments) {
 if($Arguments[0] -eq 'snapshot'){$Context.calls.Add(@{stdout="$temp/missing.json"});throw 'Window disappeared during snapshot'}
 if($Arguments[0] -eq 'list-windows'){
  $script:reads++
  return @{windows=@(if($script:reads -eq 1){[pscustomobject]@{hwnd=42;pid=7;class='XLMAIN';title='Excel'}})}
 }
 throw 'Unexpected write during close observation'
}
$previous=$env:LOCALAPPDATA;$env:LOCALAPPDATA=$temp
try {
 $c=@{owned=@{hwnd=42;pid=7;started=123};app='hwnd:42';cleanup='';uncertain=$false;calls=[Collections.Generic.List[object]]::new()}
 Close-OfficeWindow $c
 Assert-Fixture ($c.cleanup -eq 'closed_verified' -and $script:reads -eq 2) 'Window disappearance must be confirmed by a fresh native read'
}finally{$env:LOCALAPPDATA=$previous}
Write-Output 'Office close race verified without replay'
