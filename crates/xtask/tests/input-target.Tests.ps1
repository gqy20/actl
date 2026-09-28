$ErrorActionPreference='Stop'
. "$PSScriptRoot/../../../fixtures/system-fixture.ps1"
. "$PSScriptRoot/../../../fixtures/input-target-lib.ps1"
$elements=@(
 [pscustomobject]@{role='TabItem';name='user document. 已修改。';depth=5},
 [pscustomobject]@{role='Button';automation_id='CloseButton';ref='@e2';depth=6},
 [pscustomobject]@{role='TabItem';name='owned-marker.txt. 未修改。';depth=5},
 [pscustomobject]@{role='Button';automation_id='CloseButton';ref='@e4';depth=6}
)
Assert-Fixture ((Get-InputTabCloseTarget $elements 'owned-marker') -eq '@e4') 'Picked restored user tab close button'
$elements[2].name='owned-marker.txt. 已修改。'
$rejected=$false
try{Get-InputTabCloseTarget $elements 'owned-marker'|Out-Null}catch{$rejected=$true}
Assert-Fixture $rejected 'Unsaved tab was accepted for cleanup'
Write-Output 'Owned Notepad tab cleanup preserves restored user tabs'
