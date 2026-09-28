$ErrorActionPreference='Stop'
. "$PSScriptRoot/../../../scenarios/phase2-readonly-lib.ps1"
if ((Get-Phase2Percentile @(9,1,3,7,5) 0.5) -ne 5) { throw 'Median calculation failed' }
if ((Get-Phase2Percentile (1..100) 0.95) -ne 95) { throw 'P95 calculation failed' }
$full=[pscustomobject]@{elements=@(@{name='root';depth=0},@{name='child';depth=1});truncated=$false}
$small=[pscustomobject]@{elements=@(@{name='child';depth=1});truncated=$false}
if ((Get-Phase2Projection $full $small $full).status -ne 'measured') { throw 'Stable projection rejected' }
if ((Get-Phase2Projection $full $small $small).status -ne 'blocked') { throw 'Changed UI used as reduction evidence' }
$cut=[pscustomobject]@{elements=@();truncated=$true}
if ((Get-Phase2Projection $cut $small $full).status -ne 'blocked') { throw 'Truncated UI used as reduction evidence' }
Write-Output 'Phase 2 percentile and projection evidence tests passed'
