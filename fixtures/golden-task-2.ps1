# Isolated document; existing Notepad sessions are never reused or closed.
param([string]$Actl = $env:ACTL)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
Set-StrictMode -Version Latest
if (-not $Actl) { $Actl = "$PSScriptRoot/../target/validation/release/actl.exe" }
$Actl = (Resolve-Path -LiteralPath $Actl).Path
$testRoot = if ($env:ACTL_TEST_DIR) { $env:ACTL_TEST_DIR } else { "$PSScriptRoot/../target/notepad-$([guid]::NewGuid().ToString('N'))" }
New-Item -ItemType Directory -Force -Path $testRoot | Out-Null
$testRoot = (Resolve-Path -LiteralPath $testRoot).Path
$marker = 'actl-notepad-' + [guid]::NewGuid().ToString('N')
$source = Join-Path $testRoot "$marker.txt"
$destination = Join-Path $testRoot "$marker-saved.txt"
$owned = $null
$sequence = 0
$passed = $false
$exitCode = 1
$calls = [Collections.Generic.List[object]]::new()
function Invoke-TestActl([string[]]$Arguments) {
    $script:sequence++
    $prefix = Join-Path $testRoot ('step-{0:d3}' -f $script:sequence)
    $info = [Diagnostics.ProcessStartInfo]::new($Actl)
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.StandardOutputEncoding = [Text.Encoding]::UTF8
    $info.StandardErrorEncoding = [Text.Encoding]::UTF8
    foreach ($arg in $Arguments) { $info.ArgumentList.Add($arg) }
    $process = [Diagnostics.Process]::Start($info)
    $outTask = $process.StandardOutput.ReadToEndAsync()
    $errTask = $process.StandardError.ReadToEndAsync()
    $timer = [Diagnostics.Stopwatch]::StartNew()
    try {
        if (-not $process.WaitForExit(30000)) {
            # Only this CLI invocation, never its tree or any GUI application.
            $process.Kill()
            $process.WaitForExit()
            throw 'CLI deadline exceeded; effect unknown, no automatic retry'
        }
        $value = $outTask.GetAwaiter().GetResult() | ConvertFrom-Json
        if ($process.ExitCode -ne 0 -or -not $value.ok) { throw "Command failed: $($Arguments[0])" }
        return $value.data
    } finally {
        [IO.File]::WriteAllText("$prefix.stdout.json", $outTask.GetAwaiter().GetResult())
        [IO.File]::WriteAllText("$prefix.stderr.log", $errTask.GetAwaiter().GetResult())
        $calls.Add(@{command=$Arguments[0];exit_code=$process.ExitCode;elapsed_ms=$timer.ElapsedMilliseconds;stdout="$prefix.stdout.json";stderr="$prefix.stderr.log"})
        $process.Dispose()
    }
}
try {
    if (Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'actl/stop-requested')) {
        $exitCode = 77
        throw 'Environment blocked: emergency stop remains set; no actions started'
    }
    if (@(Get-Process -Name Notepad -ErrorAction SilentlyContinue).Count) {
        $exitCode = 77
        throw 'Environment blocked: existing Notepad process; no user window changed'
    }
    $baseline = @((Invoke-TestActl @('list-windows')).windows | ForEach-Object hwnd)
    $notepad = Join-Path $env:WINDIR 'System32/notepad.exe'
    if (-not (Test-Path -LiteralPath $notepad)) {
        $package = Get-AppxPackage Microsoft.WindowsNotepad | Select-Object -First 1
        if (-not $package) { $exitCode = 77; throw 'Notepad is not installed' }
        $notepad = Join-Path $package.InstallLocation 'Notepad/Notepad.exe'
    }
    if (-not (Test-Path -LiteralPath $notepad)) { $exitCode = 77; throw 'Cannot resolve Notepad executable' }
    [IO.File]::WriteAllText($source, '')
    $null = Invoke-TestActl @('launch',$notepad,$source)
    $null = Invoke-TestActl @('wait','--window',$marker,'--timeout','12000')
    $candidates = @((Invoke-TestActl @('list-windows')).windows | Where-Object { $_.title.Contains($marker) -and $_.hwnd -notin $baseline })
    if ($candidates.Count -ne 1) { throw 'Cannot establish unique test window ownership' }
    $owned = $candidates[0]
    $app = "hwnd:$($owned.hwnd)"
    $text = "golden task 2`n第二行中文"
    $null = Invoke-TestActl @('type','role:Document',$text,'--app',$app,'--paste')
    $null = Invoke-TestActl @('verify','--value','role:Document','第二行中文','--contains','--app',$app)
    $beforeSave = @((Invoke-TestActl @('list-windows')).windows | ForEach-Object hwnd)
    $null = Invoke-TestActl @('press','ctrl+shift+s','--app',$app)
    $null = Invoke-TestActl @('wait','--window','另存为','--timeout','8000')
    $dialogs = @((Invoke-TestActl @('list-windows')).windows | Where-Object { $_.hwnd -notin $beforeSave -and $_.pid -eq $owned.pid -and $_.title -eq '另存为' -and $_.PSObject.Properties['owner_title'] -and $_.owner_title -and $_.owner_title.Contains($marker) })
    if ($dialogs.Count -ne 1) { throw 'Save dialog ownership is ambiguous; leaving it for review' }
    $dialogApp = "hwnd:$($dialogs[0].hwnd)"
    $null = Invoke-TestActl @('set-value','id:1001',$destination,'--app',$dialogApp)
    $null = Invoke-TestActl @('press','enter','--app',$dialogApp)
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        if (Test-Path -LiteralPath $destination) {
            $actual = [IO.File]::ReadAllText($destination).Replace("`r`n","`n")
            if ($actual -eq $text) { $passed = $true; break }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if (-not $passed) { throw 'Saved file does not contain the expected complete text' }
    $exitCode = 0
} catch {
    Write-Error $_ -ErrorAction Continue
} finally {
    # Uncertain edits stay visible. Never close a whole multi-tab window.
    if ($passed -and $owned) {
        try {
            $current = @((Invoke-TestActl @('list-windows')).windows | Where-Object { $_.hwnd -eq $owned.hwnd -and $_.pid -eq $owned.pid -and $_.title.Contains($marker) })
            if ($current.Count -ne 1) { throw 'Window changed before cleanup; review required' }
            $null = Invoke-TestActl @('press','ctrl+w','--app',"hwnd:$($owned.hwnd)")
        } catch { $exitCode = 1; Write-Error $_ -ErrorAction Continue }
    }
    @{case='golden-task-2';passed=($exitCode -eq 0);exit_code=$exitCode;binary=$Actl;source=$source;artifact=$destination;calls=@($calls.ToArray());cleanup='Only identified saved tab is closed; files and failed windows retained'} | ConvertTo-Json -Depth 8 | Set-Content "$testRoot/result.json" -Encoding utf8
}
exit $exitCode
