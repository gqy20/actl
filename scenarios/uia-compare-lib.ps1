# Read-only diagnostics process capture. Dot-source; never starts target applications.
function Invoke-UiaReadProcess($Context, [string]$Program, [string[]]$Arguments, [bool]$Json = $true) {
    $stem = Join-Path $Context.root ('call-{0:D2}' -f ($Context.calls.Count + 1))
    $record = @{program=$Program;arguments=$Arguments;started_at=(Get-Date -Format o);status='failed';exit_code=$null;elapsed_ms=0;stdout="$stem.stdout";stderr="$stem.stderr"}
    $info = [Diagnostics.ProcessStartInfo]::new($Program)
    $info.UseShellExecute=$false; $info.CreateNoWindow=$true
    $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
    foreach ($argument in $Arguments) { $info.ArgumentList.Add($argument) }
    $process=$null; $out=[IO.MemoryStream]::new(); $err=[IO.MemoryStream]::new()
    $capture=[Threading.CancellationTokenSource]::new()
    $tasks=@(); $timer=[Diagnostics.Stopwatch]::StartNew()
    try {
        $process=[Diagnostics.Process]::Start($info)
        $tasks=@($process.StandardOutput.BaseStream.CopyToAsync($out,$capture.Token), $process.StandardError.BaseStream.CopyToAsync($err,$capture.Token))
        if (-not $process.WaitForExit($Context.timeout_ms)) {
            $record.status='timeout'
            $process.Kill() # Only this owned read-only CLI process, never GUI targets or descendants.
            $null=$process.WaitForExit(2000)
            throw 'TIMEOUT: diagnostic CLI exceeded its bound'
        }
        $record.exit_code=$process.ExitCode
        $capture.CancelAfter(1000)
        foreach ($task in $tasks) { $null=$task.GetAwaiter().GetResult() }
        if ($record.exit_code -ne 0) { throw "INTERNAL: diagnostic CLI exit $($record.exit_code)" }
        $raw=[Text.Encoding]::UTF8.GetString($out.ToArray())
        if ($Json) {
            try { $data=ConvertFrom-Json -InputObject $raw -ErrorAction Stop } catch { $record.status='invalid_json'; throw 'PROTOCOL: diagnostic output is not complete JSON' }
            if ($null -eq $data) { throw 'PROTOCOL: null observation' }
            if ($data.PSObject.Properties['ok'] -and -not $data.ok) { throw 'INTERNAL: error envelope; see raw evidence' }
        } else { $data=$raw.Trim() }
        $record.status='collected'
        return $data
    } catch {
        $record.error=$_.Exception.Message
        throw
    } finally {
        $capture.Cancel()
        foreach ($task in $tasks) { try { $null=$task.GetAwaiter().GetResult() } catch {} }
        [IO.File]::WriteAllBytes($record.stdout,$out.ToArray())
        [IO.File]::WriteAllBytes($record.stderr,$err.ToArray())
        $record.elapsed_ms=$timer.ElapsedMilliseconds
        $Context.calls.Add($record)
        if ($process) { $process.Dispose() }
        $capture.Dispose(); $out.Dispose(); $err.Dispose()
    }
}

function Get-UiaObservationKeys($Elements, [string]$Kind) {
    foreach ($node in $Elements) {
        if ($Kind -eq 'actl') { $key=@($node.role,$node.automation_id,$node.name,$node.depth) }
        else { $key=@($node.type,$node.automationId,$node.name) }
        ConvertTo-Json -InputObject $key -Compress
        if ($Kind -ne 'actl' -and $node.PSObject.Properties['children']) { Get-UiaObservationKeys $node.children $Kind }
    }
}
