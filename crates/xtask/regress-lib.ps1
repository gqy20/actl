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
    try {
        $process = [Diagnostics.Process]::Start($info)
        $outTask = $process.StandardOutput.BaseStream.CopyToAsync($stdout)
        $errTask = $process.StandardError.BaseStream.CopyToAsync($stderr)
        $process.WaitForExit()
        $null = $outTask.GetAwaiter().GetResult()
        $null = $errTask.GetAwaiter().GetResult()
        return $process.ExitCode
    } finally {
        if ($process) { $process.Dispose() }
        $stdout.Dispose()
        $stderr.Dispose()
    }
}
