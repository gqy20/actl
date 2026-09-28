Set-StrictMode -Version Latest

function Get-RegressionCases([string]$Root, [string]$Suite, [string]$Case) {
    $items = @(Get-Content "$Root/crates/xtask/regress-cases.json" -Raw | ConvertFrom-Json)
    $ids = @{}
    foreach ($item in $items) {
        if ($ids.ContainsKey($item.id)) { throw "Duplicate case: $($item.id)" }
        $ids[$item.id] = $true
        if ($item.state -notin @('ready','blocked','manual')) { throw 'Invalid case state' }
        if (-not (Test-Path -LiteralPath (Join-Path $Root $item.script) -PathType Leaf)) { throw "Missing fixture: $($item.script)" }
    }
    if ($Case) {
        $items = @($items | Where-Object id -EQ $Case)
        if (-not $items.Count) { throw "Unknown case: $Case" }
    } elseif ($Suite -ne 'all') {
        $items = @($items | Where-Object suite -EQ $Suite)
        if (-not $items.Count) { throw "Unknown or empty suite: $Suite" }
    }
    return $items
}

function Get-CaseStatus([int]$Code) {
    if ($Code -eq 0) { return 'passed' }
    if ($Code -eq 77) { return 'blocked' }
    return 'failed'
}

# Explicit opt-in: absence of keyboard injection does not imply absence of consent.
function Test-UnattendedCase($Case) {
    return ($Case.state -eq 'ready' -and $Case.input -eq 'none' -and
        $null -ne $Case.PSObject.Properties['unattended'] -and $Case.unattended -is [bool] -and $Case.unattended)
}

# Stream both channels to separate files. No shell string expansion, no pipe-buffer deadlock.
function Invoke-RegressionProcess([string]$Program, [string[]]$Arguments, [string]$Directory, [string]$LogPrefix, [hashtable]$Environment) {
    $info = [Diagnostics.ProcessStartInfo]::new($Program)
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.WorkingDirectory = $Directory
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    foreach ($arg in $Arguments) { $info.ArgumentList.Add($arg) }
    foreach ($key in $Environment.Keys) { $info.Environment[$key] = $Environment[$key] }
    $stdout = [IO.File]::Create("$LogPrefix.stdout.log")
    $stderr = [IO.File]::Create("$LogPrefix.stderr.log")
    $process = $null
    $capture = [Threading.CancellationTokenSource]::new()
    try {
        $process = [Diagnostics.Process]::Start($info)
        $outTask = $process.StandardOutput.BaseStream.CopyToAsync($stdout,$capture.Token)
        $errTask = $process.StandardError.BaseStream.CopyToAsync($stderr,$capture.Token)
        $process.WaitForExit()
        $capture.CancelAfter(1000)
        foreach ($task in @($outTask,$errTask)) {
            try { $null = $task.GetAwaiter().GetResult() } catch {
                if (-not $task.IsCanceled) { throw }
            }
        }
        @{streams_drained=(-not ($outTask.IsCanceled -or $errTask.IsCanceled));exit_code=$process.ExitCode} |
            ConvertTo-Json | Set-Content "$LogPrefix.capture.json" -Encoding utf8
        return $process.ExitCode
    } finally {
        $capture.Dispose()
        if ($process) { $process.Dispose() }
        $stdout.Dispose()
        $stderr.Dispose()
    }
}
