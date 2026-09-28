# Read-only provider regression. Never launches, focuses, closes or writes to an app.
param(
    [string]$Actl = "$PSScriptRoot/../target/validation/release/actl.exe",
    [Parameter(Mandatory)][long]$Hwnd,
    [Parameter(Mandatory)][string]$AutomationId
)
$ErrorActionPreference = 'Stop'
$binary = (Resolve-Path $Actl).Path
$dir = Join-Path $PSScriptRoot ('../target/uia-composite-observe-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $dir | Out-Null
$dir = (Resolve-Path $dir).Path
$app = "hwnd:$Hwnd"
$checks = [Collections.Generic.List[object]]::new()
$script:callNumber = 0
function Invoke-Observe([string[]]$CallArgs) {
    $script:callNumber++
    $stem = Join-Path $dir ('{0:D2}' -f $script:callNumber)
    $CallArgs | ConvertTo-Json | Set-Content "$stem.args.json" -Encoding utf8
    $raw = & $binary @CallArgs 2> "$stem.stderr"
    $exitCode = $LASTEXITCODE
    $raw | Set-Content "$stem.json" -Encoding utf8
    $result = $raw | ConvertFrom-Json
    if ($null -eq $result.ok) { throw "Invalid envelope; exit=$exitCode" }
    return $result
}
function Assert-Result($Result, [string]$Expected, [string]$Label) {
    $actual = if ($Result.ok) {'ok'} else {$Result.error.code}
    $checks.Add(@{label=$Label;expected=$Expected;actual=$actual;passed=($actual -eq $Expected)})
    if ($actual -ne $Expected) { throw "$Label expected $Expected, got $actual" }
}
$failure = $null
try {
    $native = Invoke-Observe @('list-windows','--native','--hwnd',[string]$Hwnd)
    Assert-Result $native 'ok' 'native window observed'
    $snapshot = Invoke-Observe @('snapshot','--app',$app)
    Assert-Result $snapshot 'ok' 'snapshot observed'
    if ($snapshot.data.truncated) { throw 'Snapshot is incomplete' }
    $elements = @($snapshot.data.elements)
    $matches = @($elements | Where-Object { $_.automation_id -eq $AutomationId })
    if ($matches.Count -ne 1) { throw 'Choose an AutomationId unique in this snapshot' }
    $node = $matches[0]
    $root = $elements[0]
    if (-not $root.name) { throw 'Root has no name for a scoped query' }
    $filter = @{role=$node.role;id=$AutomationId}
    $selector = 'uia:' + (@{match=$filter;within=@{role=$root.role;name=$root.name}} | ConvertTo-Json -Depth 4 -Compress)
    $found = Invoke-Observe @('find','--target',$selector,'--app',$app)
    Assert-Result $found 'ok' 'scoped snapshot element is locatable'
    if ($found.data.elements[0].automation_id -ne $AutomationId) { throw 'Wrong element returned' }
    $read = Invoke-Observe @('get',$selector,'--property','automation_id','--app',$app)
    Assert-Result $read 'ok' 'read scoped target'
    if ($read.data.properties.automation_id -ne $AutomationId) { throw 'Read returned the wrong AutomationId' }
    $direct = 'uia:' + (@{match=$filter;within=@{role=$root.role;name=$root.name};scope='children'} | ConvertTo-Json -Depth 4 -Compress)
    $expected = if ($node.depth -eq 1) {'ok'} else {'NOT_FOUND'}
    Assert-Result (Invoke-Observe @('find','--target',$direct,'--app',$app)) $expected 'direct-child scope agrees with snapshot depth'
    $wrong = 'uia:' + (@{match=@{id=$AutomationId;name=('ACTL-missing-' + [guid]::NewGuid().ToString('N'))}} | ConvertTo-Json -Compress)
    Assert-Result (Invoke-Observe @('find','--target',$wrong,'--app',$app)) 'NOT_FOUND' 'all conditions required'
} catch {
    $failure = $_.Exception.Message
} finally {
    @{binary=$binary;sha256=(Get-FileHash $binary).Hash;os=[Environment]::OSVersion.VersionString;
      time=(Get-Date).ToString('o');app=$app;automation_id=$AutomationId;checks=@($checks.ToArray());
      passed=($null -eq $failure);failure=$failure;cleanup='read_only_existing_window_untouched'} |
      ConvertTo-Json -Depth 8 | Set-Content (Join-Path $dir 'report.json') -Encoding utf8
    Write-Output "Report: $dir/report.json"
}
if ($failure) { Write-Error $failure; exit 1 }
