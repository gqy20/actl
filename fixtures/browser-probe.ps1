param(
    [ValidateSet('edge','chrome')][string]$Browser='edge',
    [ValidateSet('deep-tree','content-view')][string]$Scenario='deep-tree',
    [string]$Actl=$env:ACTL
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/system-fixture.ps1"
. "$PSScriptRoot/browser-probe-lib.ps1"
if (-not $Actl) { $Actl="$PSScriptRoot/../target/validation/release/actl.exe" }
$base=if($env:ACTL_TEST_DIR){$env:ACTL_TEST_DIR}else{"$PSScriptRoot/../target"}
$root=Join-Path $base ("browser-$Browser-"+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $root -Force | Out-Null
$root=(Resolve-Path $root).Path
$context=@{root=$root;binary=(Resolve-Path $Actl).Path;calls=[Collections.Generic.List[object]]::new();owned=$null;app=$null;uncertain=$false;cleanup='not_started';case='browser-probe'}
$code=1; $reason=''; $measurements=@{}; $application=$null
try {
    if(Test-Path (Join-Path $env:LOCALAPPDATA 'actl/stop-requested')){$code=77;throw 'Environment blocked: emergency stop active'}
    $exe=if($Browser -eq 'edge'){'msedge.exe'}else{'chrome.exe'}
    $program=$null
    foreach($key in @('HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths','HKLM:\Software\Microsoft\Windows\CurrentVersion\App Paths')){
        if(Test-Path "$key/$exe"){$program=(Get-Item "$key/$exe").GetValue('');break}
    }
    if(-not $program -or -not (Test-Path -LiteralPath $program)){$code=77;throw "$Browser is not installed in App Paths"}
    $profile=Join-Path $root 'profile'
    $marker="actl-$Browser-"+[guid]::NewGuid().ToString('N')
    $html=Join-Path $root 'probe.html'
    @"
<!doctype html><html lang="en"><meta charset="utf-8"><title>$marker</title>
<body><main><h1>Owned Browser Probe</h1><p>The quick brown fox. 敏捷的棕色狐狸。</p>
<section><div><article><button id="primary">Submit form</button></article></div></section>
<button id="start" onclick="this.disabled=true;let i=0;const t=setInterval(()=>{const li=document.createElement('li');li.textContent='lazy-item-'+(++i);document.querySelector('ol').append(li);if(i===40)clearInterval(t)},150)">Start stream</button>
<button>dup-target</button><button>dup-target</button><ol></ol></main></body></html>
"@ | Set-Content -LiteralPath $html -Encoding utf8
    $application=@{browser=$Browser;program=$program;version=(Get-Item $program).VersionInfo.FileVersion;profile=$profile;page=$html}
    $baseline=@((Invoke-FixtureCli $context @('list-windows')).windows | ForEach-Object hwnd)
    $private=if($Browser -eq 'edge'){'--inprivate'}else{'--incognito'}
    $launched=Invoke-FixtureCli $context @('launch',$program,'--new-window','--no-first-run','--no-default-browser-check','--disable-extensions',$private,'--force-renderer-accessibility',"--user-data-dir=$profile",([uri]$html).AbsoluteUri)
    $null=Invoke-FixtureCli $context @('wait','--window',$marker,'--timeout','15000')
    $candidates=@(Get-FixtureCandidates (Invoke-FixtureCli $context @('list-windows')).windows $baseline $marker $launched.pid)
    Assert-Fixture ($candidates.Count -eq 1) 'Browser window ownership is not unique'
    $owner=Get-CimInstance Win32_Process -Filter "ProcessId=$($candidates[0].pid)"
    Assert-Fixture ($owner.ExecutablePath -eq $program -and $owner.CommandLine.Contains("--user-data-dir=$profile")) 'Browser profile ownership mismatch'
    Save-FixtureOwner $context $candidates[0] $marker
    $null=Invoke-FixtureCli $context @('wait','--element','name:Submit form','--app',$context.app,'--timeout','12000')
    if($Scenario -eq 'content-view'){
        $document=Invoke-FixtureCli $context @('find','--name',$marker,'--role','Document','--app',$context.app)
        Assert-Fixture (@($document.elements).Count -eq 1) 'Test document is not unique'
        $null=Invoke-FixtureCli $context @('click','name:Start stream','--app',$context.app)
        $settle=Invoke-FixtureCli $context @('wait','--element',$document.elements[0].ref,'--stable','--app',$context.app,'--timeout','20000')
        $measurements.settle=$settle
        Assert-Fixture ($settle.waited_ms -ge 1000 -and $settle.waited.changes -ge 1) 'No observed content changes during settle'
        $unexpected=$false
        try {$null=Invoke-FixtureCli $context @('wait','--element','name:dup-target','--app',$context.app,'--timeout','5000');$unexpected=$true}catch{}
        Assert-Fixture (-not $unexpected) 'Ambiguous target unexpectedly succeeded'
        $errorResult=Get-Content $context.calls[$context.calls.Count-1].stdout -Raw | ConvertFrom-Json
        Assert-Fixture ($errorResult.error.code -eq 'AMBIGUOUS' -and $errorResult.duration_ms -lt 3000) 'Expected immediate AMBIGUOUS'
        $context.calls[$context.calls.Count-1].status='expected_error'
    }
    $document=Invoke-FixtureCli $context @('find','--name',$marker,'--role','Document','--app',$context.app)
    Assert-Fixture (@($document.elements).Count -eq 1) 'Test document is not unique'
    $text=Invoke-FixtureCli $context @('get',$document.elements[0].ref,'--property','text','--app',$context.app)
    Assert-Fixture ($text.properties.text.Contains('quick brown fox') -and $text.properties.text.Contains('敏捷的棕色狐狸')) 'Page text mismatch'
    if($Scenario -eq 'content-view'){Assert-Fixture ($text.properties.text.Contains('lazy-item-40')) 'Dynamic content incomplete'}
    foreach($view in @('full','skeleton','content','both')){
        $args=@('snapshot','--app',$context.app)
        if($view -in @('skeleton','both')){$args+='--skeleton'}
        if($view -in @('content','both')){$args+=@('--view','content')}
        $snapshot=Invoke-FixtureCli $context $args
        Assert-Fixture (-not $snapshot.truncated) 'Snapshot truncated'
        if($view -in @('content','both')){Assert-Fixture ($snapshot.view -eq 'content') 'Content view marker missing'}
        $measurements[$view]=@{elements=@($snapshot.elements).Count;bytes=(Get-Item $context.calls[$context.calls.Count-1].stdout).Length}
    }
    Assert-Fixture ($measurements.skeleton.elements -lt $measurements.full.elements) 'Skeleton did not reduce tree'
    Assert-Fixture ($measurements.content.bytes -lt $measurements.skeleton.bytes) 'Content projection did not remove browser chrome'
    $code=0
}catch{$reason=$_.Exception.Message;Write-Error $_ -ErrorAction Continue}
finally {
    try {
        if($context.owned -and -not $context.uncertain -and -not (Test-Path (Join-Path $env:LOCALAPPDATA 'actl/stop-requested'))){
            $snapshot=Invoke-FixtureCli $context @('snapshot','--app',$context.app)
            Assert-BrowserOwnedTab $snapshot.elements $context.owned.marker
        }
        Close-FixtureWindow $context
    }catch{$code=1;$reason+="; cleanup: $($_.Exception.Message)"; $context.cleanup='retained_for_review'}
    @{case="$Browser-$Scenario";exit_code=$code;passed=($code -eq 0);reason=$reason;application=$application;measurements=$measurements;owned=$context.owned;cleanup=$context.cleanup;calls=$context.calls.ToArray()} | ConvertTo-Json -Depth 14 | Set-Content "$root/result.json" -Encoding utf8
    Write-Host "Fixture report: $root/result.json"
}
exit $code
