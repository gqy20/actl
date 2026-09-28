Set-StrictMode -Version Latest

function Get-LocalObservationMeasurement($Before, $Local, $LocalAfter, $After, [long]$FullEnvelopeBytes, [long]$LocalEnvelopeBytes) {
    foreach ($sample in @($Before,$Local,$LocalAfter,$After)) {
        if ($null -eq $sample -or -not $sample.PSObject.Properties['elements'] -or
            -not $sample.PSObject.Properties['truncated'] -or $sample.truncated -isnot [bool] -or $sample.truncated) {
            return @{status='blocked';reason='Missing or truncated observation cannot establish reduction'}
        }
    }
    foreach ($sample in @($Local,$LocalAfter)) {
        if (-not $sample.PSObject.Properties['scope'] -or -not $sample.PSObject.Properties['ref_scope'] -or $sample.ref_scope -ne 'container') {
            return @{status='blocked';reason='Local scope metadata unavailable'}
        }
    }
    $fullJson=ConvertTo-Json -InputObject @($Before.elements) -Depth 40 -Compress
    $lastJson=ConvertTo-Json -InputObject @($After.elements) -Depth 40 -Compress
    $localJson=ConvertTo-Json -InputObject @($Local.elements) -Depth 40 -Compress
    $repeatJson=ConvertTo-Json -InputObject @($LocalAfter.elements) -Depth 40 -Compress
    $scope=ConvertTo-Json -InputObject $Local.scope -Depth 15 -Compress
    $repeatScope=ConvertTo-Json -InputObject $LocalAfter.scope -Depth 15 -Compress
    if ($fullJson -cne $lastJson -or $localJson -cne $repeatJson -or $scope -cne $repeatScope) {
        return @{status='blocked';reason='Window content or local container changed between samples'}
    }
    if (@($Before.elements).Count -le 1 -or @($Local.elements).Count -eq 0 -or $FullEnvelopeBytes -le 0 -or $LocalEnvelopeBytes -le 0) {
        return @{status='blocked';reason='Empty or root-only sample is not representative'}
    }
    $fullBytes=[Text.Encoding]::UTF8.GetByteCount($fullJson)
    $localBytes=[Text.Encoding]::UTF8.GetByteCount($localJson)
    return @{status='measured';full_nodes=@($Before.elements).Count;local_nodes=@($Local.elements).Count;
        full_elements_bytes=$fullBytes;local_elements_bytes=$localBytes;elements_reduction=1.0-$localBytes/[double]$fullBytes;
        full_envelope_bytes=$FullEnvelopeBytes;local_envelope_bytes=$LocalEnvelopeBytes;envelope_reduction=1.0-$LocalEnvelopeBytes/[double]$FullEnvelopeBytes;
        unit='UTF-8 bytes, not tokenizer tokens';atomic=$false}
}
