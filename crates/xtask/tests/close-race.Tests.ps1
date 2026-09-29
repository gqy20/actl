$ErrorActionPreference='Stop'
. "$PSScriptRoot/../../../fixtures/system-fixture.ps1"
$previous=$env:LOCALAPPDATA
$env:LOCALAPPDATA=Join-Path $PSScriptRoot '../../../target/close-race-no-state'
try {
 $script:reads=0;$script:closes=0
 $w=[pscustomobject]@{hwnd=42;pid=7;class='Test';title='owned'}
 function Get-Process {param($Id,$ErrorAction) [pscustomobject]@{StartTime=[datetime]::new(123,[DateTimeKind]::Utc)}}
 function Invoke-FixtureCli($Context,$Arguments) {
  if($Arguments[0] -eq 'close-window'){$script:closes++;return @{}}
  if($Arguments[0] -eq 'list-windows'){$script:reads++;return @{windows=@(if($script:reads -lt 4){$w})}}
  throw 'Unexpected action'
 }
 $c=@{case='test';owned=@{hwnd=42;pid=7;class='Test';marker='owned';started=123};uncertain=$false;cleanup=''}
 Close-FixtureWindow $c
 Assert-Fixture ($c.cleanup -eq 'closed_verified' -and $script:closes -eq 1 -and $script:reads -eq 4) 'Asynchronous close must be observed without replay'
}finally{$env:LOCALAPPDATA=$previous}
Write-Output 'Asynchronous native close is observed without repeating the close'
