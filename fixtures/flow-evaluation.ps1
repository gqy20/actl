# Read-only workflow evaluation. Isolated records; no injected input or user stop-flag changes.
param([string]$Actl = $(if ($env:ACTL) { $env:ACTL } else { "$PSScriptRoot/../target/validation/release/actl.exe" }))
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$Actl = (Resolve-Path -LiteralPath $Actl).Path
$evalRoot = if ($env:ACTL_TEST_DIR) { Join-Path $env:ACTL_TEST_DIR 'flow-evaluation' } else { Join-Path $PSScriptRoot "../target/flow-evaluation-$([guid]::NewGuid().ToString('N'))" }
New-Item -ItemType Directory -Path $evalRoot -Force | Out-Null
$evalRoot = (Resolve-Path -LiteralPath $evalRoot).Path
$previousLocal = $env:LOCALAPPDATA
$previousTask = $env:ACTL_TASK_ID
$results = [Collections.Generic.List[object]]::new()
$owned = [Collections.Generic.List[Diagnostics.Process]]::new()
function Invoke-Flow([string[]]$Arguments) {
    $text = & $Actl @Arguments
    if (-not $text) { throw "No envelope: $Arguments" }
    return ($text | ConvertFrom-Json)
}
function Start-Flow([string[]]$Arguments) {
    $info = [Diagnostics.ProcessStartInfo]::new($Actl)
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    foreach ($arg in $Arguments) { $info.ArgumentList.Add($arg) }
    $process = [Diagnostics.Process]::Start($info)
    $owned.Add($process)
    return $process
}
function Wait-Running([string]$Id) {
    $file = Join-Path $evalRoot "actl/flows/$Id/state.json"
    $timer = [Diagnostics.Stopwatch]::StartNew()
    while ($timer.Elapsed.TotalSeconds -lt 10) {
        if (Test-Path -LiteralPath $file) {
            $value = Get-Content -LiteralPath $file -Raw | ConvertFrom-Json
            if ($value.steps[0].status -eq 'running') { return }
        }
        Start-Sleep -Milliseconds 20
    }
    throw "Runner did not enter step: $Id"
}
function Finish-Flow([Diagnostics.Process]$Process) {
    if (-not $Process.WaitForExit(10000)) { throw 'Owned CLI did not finish.' }
    $out = $Process.StandardOutput.ReadToEnd()
    return ($out | ConvertFrom-Json)
}
function Check([bool]$Condition,[string]$Message) { if (-not $Condition) { throw $Message } }
try {
    $env:LOCALAPPDATA = $evalRoot
    Remove-Item Env:ACTL_TASK_ID -ErrorAction SilentlyContinue
    $file = Join-Path $evalRoot 'inspect.yml'
    Copy-Item -LiteralPath "$PSScriptRoot/flows/inspect.yml" -Destination $file
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $valid = Invoke-Flow @('flow-validate','--file',$file)
    Check ($valid.ok -and $valid.data.valid) 'Validation failed.'
    $run = Invoke-Flow @('flow-run','--file',$file,'--id','checkpoint')
    Check ($run.data.run.status -eq 'paused') 'Checkpoint not paused.'
    [IO.File]::WriteAllText($file,'invalid source after start')
    $done = Invoke-Flow @('flow-resume','--id','checkpoint')
    Check ($done.data.run.status -eq 'completed') 'Saved plan did not resume.'
    Check ($done.data.run.steps[0].attempts -eq 1) 'Completed step repeated.'
    $results.Add(@{case='checkpoint_and_source_isolation';passed=$true;elapsed_ms=$watch.ElapsedMilliseconds})

    $waitFile = Join-Path $evalRoot 'wait.yml'
    [IO.File]::WriteAllText($waitFile,@'
version: 1
name: Evaluation wait
steps:
  - id: wait
    title: Wait for missing window
    run: {command: wait, args: [--window, ACTL-EVALUATION-NONEXISTENT-694201, --timeout, '2000']}
  - id: next
    title: Next step
    run: {command: status}
'@)
    $process = Start-Flow @('flow-run','--file',$waitFile,'--id','pause')
    Wait-Running 'pause'
    $busy = Invoke-Flow @('flow-resume','--id','pause')
    Check ($busy.error.code -eq 'NOT_ACTIONABLE') 'Second executor was not rejected.'
    $watch.Restart()
    $requested = Invoke-Flow @('flow-pause','--id','pause')
    $requestMs = $watch.ElapsedMilliseconds
    $paused = Finish-Flow $process
    Check ($paused.data.run.status -eq 'paused') 'Pause not acknowledged.'
    Check ($paused.data.run.steps[1].attempts -eq 0) 'Next step ran after pause.'
    $results.Add(@{case='live_boundary_pause_and_exclusion';passed=$true;request_ms=$requestMs;acknowledgement_ms=$watch.ElapsedMilliseconds;request_state=$requested.data.run.status})

    $crash = Start-Flow @('flow-run','--file',$waitFile,'--id','crash')
    Wait-Running 'crash'
    # Kill only the CLI process created above; never kill GUI applications.
    $crash.Kill()
    $crash.WaitForExit()
    $interrupted = Invoke-Flow @('flow-status','--id','crash')
    Check ($interrupted.data.interrupted -and -not $interrupted.data.runner_active) 'Crash not distinguished.'
    $recovered = Invoke-Flow @('flow-resume','--id','crash')
    Check ($recovered.data.run.status -eq 'paused') 'Read-only crash recovery failed.'
    Check ($recovered.data.run.steps[0].attempts -eq 2) 'Unexpected read-only retry count.'
    $results.Add(@{case='actual_cli_process_death_and_resume';passed=$true;recovered_state=$recovered.data.run.status})

    $stop = Join-Path $evalRoot 'actl/stop-requested'
    [IO.File]::WriteAllText($stop,'evaluation-stop')
    $blocked = Invoke-Flow @('flow-resume','--id','pause')
    Check ($blocked.error.code -eq 'ABORTED' -and (Test-Path -LiteralPath $stop)) 'Stop was bypassed.'
    $results.Add(@{case='sticky_stop';passed=$true})

    $invalid = Join-Path $evalRoot 'semantic-invalid.yml'
    [IO.File]::WriteAllText($invalid,"version: 1`nname: Invalid condition`nsteps:`n  - id: wait`n    title: Wait`n    run: {command: wait}`n")
    $accepted = Invoke-Flow @('flow-validate','--file',$invalid)
    Check (-not $accepted.ok -and $accepted.error.code -eq 'PROTOCOL') 'Missing wait condition passed preflight.'
    $results.Add(@{case='semantic_preflight_probe';passed=$true;validation_accepted=$accepted.ok})
    $report = @{timestamp=[DateTimeOffset]::Now.ToString('o');os=[Environment]::OSVersion.VersionString;binary=$Actl;results=$results}
    $reportFile = Join-Path $evalRoot 'report.json'
    $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $reportFile -Encoding utf8
    $report | ConvertTo-Json -Depth 10
    Write-Output "Report: $reportFile"
} finally {
    foreach ($process in $owned) { if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }; $process.Dispose() }
    $env:LOCALAPPDATA = $previousLocal
    $env:ACTL_TASK_ID = $previousTask
}
