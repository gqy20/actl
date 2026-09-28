$ErrorActionPreference='Stop'
. "$PSScriptRoot/../../../scenarios/local-observation-lib.ps1"
$scope=[pscustomobject]@{version=1;selector='uia:test';identity=@{runtime_id=@(1,2);pid=3};depth=3;limit=100}
$full=[pscustomobject]@{elements=@(@{name='root';depth=0},@{name='child';depth=1});truncated=$false}
$local=[pscustomobject]@{elements=@(@{name='child';depth=0});truncated=$false;scope=$scope;ref_scope='container'}
$result=Get-LocalObservationMeasurement $full $local $local $full 400 500
if ($result.status -ne 'measured' -or $result.envelope_reduction -ge 0 -or $result.elements_reduction -le 0) {
    throw 'Metadata overhead must remain visible independently of element reduction'
}
foreach ($cut in @(
    [pscustomobject]@{elements=@();truncated=$true},
    [pscustomobject]@{elements=@()},
    [pscustomobject]@{elements=@();truncated='false'}
)) {
    if ((Get-LocalObservationMeasurement $cut $local $local $full 400 300).status -ne 'blocked') { throw 'Incomplete observation accepted' }
}
if ((Get-LocalObservationMeasurement $full $local $local $local 400 300).status -ne 'blocked') { throw 'Window changes accepted' }
$changed=$local | ConvertTo-Json -Depth 20 | ConvertFrom-Json
$changed.scope.identity.runtime_id=@(1,9)
if ((Get-LocalObservationMeasurement $full $local $changed $full 400 300).status -ne 'blocked') { throw 'Rebuilt container accepted' }
if ((Get-LocalObservationMeasurement $full $full $full $full 400 300).status -ne 'blocked') { throw 'Missing scope accepted' }
$empty=[pscustomobject]@{elements=@();truncated=$false;scope=$scope;ref_scope='container'}
if ((Get-LocalObservationMeasurement $full $empty $empty $full 400 300).status -ne 'blocked') { throw 'Empty local tree accepted' }
$root=[pscustomobject]@{elements=@(@{name='root';depth=0});truncated=$false}
if ((Get-LocalObservationMeasurement $root $local $local $root 400 300).status -ne 'blocked') { throw 'Root-only target accepted' }
Write-Output 'Local observation stability, completeness and byte accounting checks passed'
