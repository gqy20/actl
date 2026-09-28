Set-StrictMode -Version Latest

function Get-Phase2Percentile([double[]]$Values, [double]$Percentile) {
    if (-not $Values.Count -or $Percentile -le 0 -or $Percentile -gt 1) { throw 'Invalid percentile sample' }
    $sorted = @($Values | Sort-Object)
    return $sorted[[int][Math]::Ceiling($Percentile * $sorted.Count) - 1]
}

function Get-Phase2Projection($Before, $Skeleton, $After) {
    foreach ($sample in @($Before,$Skeleton,$After)) {
        if ($null -eq $sample.PSObject.Properties['elements'] -or
            $null -eq $sample.PSObject.Properties['truncated'] -or $sample.truncated) {
            return @{status='blocked';reason='Missing or truncated observation cannot establish reduction'}
        }
    }
    # Compare semantic payloads, excluding observation IDs and command timings.
    $first = ConvertTo-Json -InputObject @($Before.elements) -Depth 30 -Compress
    $last = ConvertTo-Json -InputObject @($After.elements) -Depth 30 -Compress
    if ($first -cne $last) { return @{status='blocked';reason='UI changed between sequential samples'} }
    $compact = ConvertTo-Json -InputObject @($Skeleton.elements) -Depth 30 -Compress
    $fullBytes = [Text.Encoding]::UTF8.GetByteCount($first)
    $compactBytes = [Text.Encoding]::UTF8.GetByteCount($compact)
    return @{status='measured';full_bytes=$fullBytes;skeleton_bytes=$compactBytes;
        representative=(@($Before.elements).Count -gt 1 -and @($Skeleton.elements).Count -gt 0);
        reduction=1.0-$compactBytes/[double]$fullBytes;unit='UTF-8 bytes of elements; not tokenizer tokens'}
}
