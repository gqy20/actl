$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/../../../fixtures/system-fixture.ps1"
$directory = Join-Path $PSScriptRoot "../../../target/stream-test-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory $directory | Out-Null
$script = Join-Path $directory 'child.ps1'
@'
$info = [Diagnostics.ProcessStartInfo]::new((Get-Process -Id $PID).Path)
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
foreach ($arg in @('-NoProfile','-Command','Start-Sleep -Seconds 6')) { $info.ArgumentList.Add($arg) }
$child = [Diagnostics.Process]::Start($info)
[Console]::Error.WriteLine('diagnostic')
[Console]::WriteLine('{"ok":true,"data":{"test":42}}')
'@ | Set-Content $script -Encoding utf8
$context = @{root=$directory;binary=(Get-Process -Id $PID).Path;calls=[Collections.Generic.List[object]]::new();uncertain=$false}
$timer = [Diagnostics.Stopwatch]::StartNew()
$result = Invoke-FixtureCli $context @('-NoProfile','-File',$script)
Assert-Fixture ($timer.ElapsedMilliseconds -lt 5000) 'CLI capture waited for descendant pipe EOF'
Assert-Fixture (@($result).Count -eq 1 -and $result.test -eq 42) 'Envelope polluted or lost'
Assert-Fixture ((Get-Content $context.calls[0].stderr -Raw).Contains('diagnostic')) 'Diagnostic lost'
Write-Output 'Descendant pipe capture passed'
. "$PSScriptRoot/../regress-lib.ps1"
$timer.Restart()
$exitCode=Invoke-RegressionProcess $context.binary @('-NoProfile','-File',$script) (Resolve-Path $directory).Path (Join-Path $directory 'runner') @{}
Assert-Fixture ($timer.ElapsedMilliseconds -lt 5000 -and $exitCode -eq 0) 'Runner waited for descendant pipe EOF'
$captureResult=Get-Content (Join-Path $directory 'runner.capture.json') -Raw | ConvertFrom-Json
Assert-Fixture (-not $captureResult.streams_drained -and $captureResult.exit_code -eq 0) 'Runner hid incomplete stream capture'
$scenario = Get-Content "$PSScriptRoot/../../../fixtures/system-scenarios.ps1" -Raw
Assert-Fixture ($scenario.Contains('shell:AppsFolder')) 'Calculator fixture bypasses packaged app activation'
$rows = [Collections.Generic.List[object]]::new()
$rows.Add(@('操作系统名称','Microsoft Windows 11'))
$rows.Add(@('BIOS 版本','test'))
1..30 | ForEach-Object { $rows.Add(@("key$_","value$_")) }
$rows.Add(@('已检测到虚拟机监控程序。将不显示 Hyper-V 所需的功能。'))
Assert-FixtureSummaryTable ([pscustomobject]@{source='tree-flatten';rows=$rows.Count;table=$rows.ToArray()})
$rows.Add(@('unexpected incomplete row'))
$rejected = $false
try { Assert-FixtureSummaryTable ([pscustomobject]@{source='tree-flatten';rows=$rows.Count;table=$rows.ToArray()}) } catch { $rejected = $true }
Assert-Fixture $rejected 'Unknown incomplete table row was accepted'
