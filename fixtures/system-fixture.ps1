# Shared system-fixture lifecycle. Dot-source only; does not launch applications.
Set-StrictMode -Version Latest

function Get-FixtureTaskId($Context, [string]$CallerTask) {
    if ($CallerTask) { return $CallerTask }
    if (-not $Context.ContainsKey('task_id')) { $Context.task_id = 'fixture-' + [guid]::NewGuid().ToString('N') }
    return $Context.task_id
}

function Invoke-FixtureCli($Context, [string[]]$Arguments) {
    $prefix = Join-Path $Context.root ('step-{0:d3}' -f ($Context.calls.Count + 1))
    $info = [Diagnostics.ProcessStartInfo]::new($Context.binary)
    $info.UseShellExecute = $false
    $info.Environment['ACTL_TASK_ID'] = Get-FixtureTaskId $Context $env:ACTL_TASK_ID
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.StandardOutputEncoding = [Text.Encoding]::UTF8
    $info.StandardErrorEncoding = [Text.Encoding]::UTF8
    foreach ($arg in $Arguments) { $info.ArgumentList.Add($arg) }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $process = [Diagnostics.Process]::Start($info)
    $outBuffer = [IO.MemoryStream]::new()
    $errBuffer = [IO.MemoryStream]::new()
    $capture = [Threading.CancellationTokenSource]::new()
    $stdout = $process.StandardOutput.BaseStream.CopyToAsync($outBuffer,$capture.Token)
    $stderr = $process.StandardError.BaseStream.CopyToAsync($errBuffer,$capture.Token)
    $record = @{command=$Arguments[0];status='failed';exit_code=$null;elapsed_ms=0;streams_drained=$false;stdout="$prefix.stdout.json";stderr="$prefix.stderr.log"}
    try {
        if (-not $process.WaitForExit(30000)) {
            $Context.uncertain = $true
            $process.Kill() # Only the owned CLI, not descendants or GUI applications.
            $process.WaitForExit()
            throw 'CLI timeout; effect unknown, no automatic replay or cleanup'
        }
        # A GUI descendant can retain a pipe after the CLI exits. Require a complete
        # JSON envelope, but do not wait for that GUI to close before reading it.
        $capture.CancelAfter(1000)
        foreach ($task in @($stdout,$stderr)) {
            try { $null = $task.GetAwaiter().GetResult() } catch {
                if (-not $task.IsCanceled) { throw }
            }
        }
        $record.streams_drained = -not ($stdout.IsCanceled -or $stderr.IsCanceled)
        $envelope = [Text.Encoding]::UTF8.GetString($outBuffer.ToArray()) | ConvertFrom-Json
        if ($process.ExitCode -ne 0 -or -not $envelope.ok) {
            if ($envelope.PSObject.Properties['error'] -and $envelope.error.PSObject.Properties['evidence'] -and
                $envelope.error.evidence -and $envelope.error.evidence.PSObject.Properties['stage'] -and
                $envelope.error.evidence.stage -eq 'handoff') { $Context.uncertain = $true }
            throw "CLI $($Arguments[0]) failed; see $prefix.stdout.json"
        }
        $record.status = 'passed'
        if ($envelope.PSObject.Properties['snapshot_id']) { $Context.last_snapshot_id = $envelope.snapshot_id }
        return $envelope.data
    } finally {
        $capture.Cancel()
        foreach ($task in @($stdout,$stderr)) { try { $null = $task.GetAwaiter().GetResult() } catch {} }
        [IO.File]::WriteAllBytes($record.stdout,$outBuffer.ToArray())
        [IO.File]::WriteAllBytes($record.stderr,$errBuffer.ToArray())
        $record.exit_code = $process.ExitCode
        $record.elapsed_ms = $timer.ElapsedMilliseconds
        $Context.calls.Add($record)
        $process.Dispose()
        $capture.Dispose()
        $outBuffer.Dispose()
        $errBuffer.Dispose()
    }
}

function Assert-Fixture([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function Get-FixtureCandidates($Windows, $Baseline, [string]$Title, [int]$ExpectedPid = 0) {
    return @($Windows | Where-Object {
        $_.PSObject.Properties['hwnd'] -and $_.hwnd -and $_.hwnd -notin $Baseline -and
        $_.title.Contains($Title) -and ($ExpectedPid -eq 0 -or $_.pid -eq $ExpectedPid)
    })
}

function Save-FixtureOwner($Context, $Window, [string]$Marker) {
    $process = Get-Process -Id $Window.pid -ErrorAction Stop
    $Context.owned = @{hwnd=$Window.hwnd;pid=$Window.pid;class=$Window.class;marker=$Marker;started=$process.StartTime.ToUniversalTime().Ticks}
    $Context.app = "hwnd:$($Window.hwnd)"
}

function Test-FixtureIdentity($Owner, $Window, [long]$Started) {
    return ($Window.hwnd -eq $Owner.hwnd -and $Window.pid -eq $Owner.pid -and
        $Window.class -eq $Owner.class -and $Window.title.Contains($Owner.marker) -and $Started -eq $Owner.started)
}

function Close-FixtureWindow($Context) {
    if (-not $Context.owned) { $Context.cleanup = 'no_owned_window'; return }
    if ($Context.uncertain) { $Context.cleanup = 'retained_uncertain'; throw 'Cleanup deferred after uncertain CLI result' }
    if (Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'actl/stop-requested')) {
        $Context.cleanup = 'retained_stop'; throw 'Cleanup deferred while emergency stop is active'
    }
    $windows = @((Invoke-FixtureCli $Context @('list-windows','--native','--hwnd',[string]$Context.owned.hwnd)).windows)
    $same = @($windows | Where-Object { $_.PSObject.Properties['hwnd'] -and $_.hwnd -eq $Context.owned.hwnd })
    if (-not $same.Count) {
        $ownerProcess = Get-Process -Id $Context.owned.pid -ErrorAction SilentlyContinue
        if ($ownerProcess -and $ownerProcess.StartTime.ToUniversalTime().Ticks -eq $Context.owned.started) {
            $Context.cleanup = 'retained_not_observed'
            throw 'Window absent from native inventory but owner process is still alive; closure is unproven'
        }
        $Context.cleanup = 'already_closed'; return
    }
    $process = Get-Process -Id $Context.owned.pid -ErrorAction Stop
    if ($same.Count -ne 1 -or -not (Test-FixtureIdentity $Context.owned $same[0] $process.StartTime.ToUniversalTime().Ticks)) {
        $Context.cleanup = 'retained_identity_changed'; throw 'Window identity changed; no close issued'
    }
    # Explorer can contain user-added tabs. Close a window only when the UIA tree proves a single owned tab.
    if ($Context.case -eq 'virtualized-list') {
        $Context.cleanup = 'retained_for_review'
        $snapshot = Invoke-FixtureCli $Context @('snapshot','--app',$Context.app)
        $tabs = @($snapshot.elements | Where-Object role -EQ 'TabItem')
        Assert-Fixture ($tabs.Count -eq 1 -and $tabs[0].name.Contains($Context.owned.marker)) 'Explorer tab ownership is unproven; window retained'
    }
    $null = Invoke-FixtureCli $Context @('close-window','--hwnd',[string]$Context.owned.hwnd)
    $Context.cleanup = 'retained_close_pending'
    $remaining = @((Invoke-FixtureCli $Context @('list-windows','--native','--hwnd',[string]$Context.owned.hwnd)).windows | Where-Object { $_.PSObject.Properties['hwnd'] -and $_.hwnd -eq $Context.owned.hwnd })
    Assert-Fixture ($remaining.Count -eq 0) 'Owned window did not close'
    $Context.cleanup = 'closed_verified'
}

function Invoke-SystemFixture([string]$Case, [string]$Actl) {
    $ErrorActionPreference = 'Stop'
    [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
    if (-not $Actl) { $Actl = "$PSScriptRoot/../target/validation/release/actl.exe" }
    $binary = (Resolve-Path -LiteralPath $Actl).Path
    $root = if ($env:ACTL_TEST_DIR) { Join-Path $env:ACTL_TEST_DIR ('run-' + [guid]::NewGuid().ToString('N')) } else { "$PSScriptRoot/../target/$Case-$([guid]::NewGuid().ToString('N'))" }
    New-Item -ItemType Directory -Path $root -Force | Out-Null
    $root = (Resolve-Path -LiteralPath $root).Path
    $context = @{case=$Case;root=$root;binary=$binary;calls=[Collections.Generic.List[object]]::new();owned=$null;app=$null;uncertain=$false;cleanup='not_started';artifacts=@();rounds=@();application=$null}
    $code = 1
    $reason = ''
    try {
        if (Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'actl/stop-requested')) {
            $code = 77; throw 'Environment blocked: emergency stop remains set'
        }
        $names = switch ($Case) { 'golden-task-1' { @('CalculatorApp','Calculator') } 'win32-table-extract' { @('msinfo32') } default { @() } }
        foreach ($name in $names) {
            if (@(Get-Process -Name $name -ErrorAction SilentlyContinue).Count) { $code=77; throw "Environment blocked: existing $name process" }
        }
        & "${PSScriptRoot}/system-scenarios.ps1" -Context $context
        $code = 0
    } catch {
        $reason = $_.Exception.Message
        Write-Error $_ -ErrorAction Continue
    } finally {
        try { Close-FixtureWindow $context } catch { $code=1; $reason += "; $($_.Exception.Message)"; Write-Error $_ -ErrorAction Continue }
        $report = @{case=$Case;passed=($code -eq 0);exit_code=$code;reason=$reason;binary=$binary;os=[Environment]::OSVersion.VersionString;application=$context.application;owned=$context.owned;cleanup=$context.cleanup;artifacts=$context.artifacts;rounds=$context.rounds;calls=$context.calls.ToArray()}
        $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath "$root/result.json" -Encoding utf8
        Write-Host "Fixture report: $root/result.json"
    }
    return $code
}

function Assert-FixtureSummaryTable($table) {
        Assert-Fixture ($table.source -in @('tree-flatten','grid-pattern')) 'Unexpected extraction source'
        Assert-Fixture ($table.rows -ge 30 -and @($table.table).Count -eq $table.rows) 'Incomplete summary rows'
        foreach ($row in $table.table) {
            $notice = $table.source -eq 'tree-flatten' -and $row.Count -eq 1 -and
                $row[0] -eq '已检测到虚拟机监控程序。将不显示 Hyper-V 所需的功能。'
            Assert-Fixture ($row -is [array] -and ($row.Count -eq 2 -or $notice)) 'Expected two-column rows or known Hyper-V notice'
        }
        $os = @($table.table | Where-Object { $_.Count -eq 2 -and $_[0] -eq '操作系统名称' -and $_[1] -like '*Microsoft Windows*' })
        $bios = @($table.table | Where-Object { $_[0] -like '*BIOS*' })
        Assert-Fixture ($os.Count -eq 1 -and $bios.Count -gt 0) 'OS/BIOS summary evidence missing'
}
