param([Parameter(Mandatory)][long]$Hwnd,[string]$Actl=$env:ACTL)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/../fixtures/system-fixture.ps1"
if(-not $Actl){$Actl="$PSScriptRoot/../target/validation/release/actl.exe"}
$base=if($env:ACTL_TEST_DIR){$env:ACTL_TEST_DIR}else{"$PSScriptRoot/../target"}
$root=Join-Path $base ('cloudmusic-playback-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $root -Force|Out-Null
$root=(Resolve-Path $root).Path
$c=@{root=$root;binary=(Resolve-Path $Actl).Path;calls=[Collections.Generic.List[object]]::new();uncertain=$false;app="hwnd:$Hwnd"}
$r=@{passed=$false;reason='';samples=[Collections.Generic.List[object]]::new();restored=$false;os=[Environment]::OSVersion.VersionString;version=$null;baseline=$null}
$code=1
function State {
 $v=Invoke-FixtureCli $c @('get','id:btn_pc_minibar_play','--app',$c.app)
 Assert-Fixture ($v.properties.name -in @('play','pause') -and $v.properties.role -eq 'MenuItem') 'Unexpected playback control'
 return $v.properties.name
}
try{
 if(Test-Path "$env:LOCALAPPDATA/actl/stop-requested"){$code=77;throw 'Emergency stop active'}
 $w=@((Invoke-FixtureCli $c @('list-windows')).windows|Where-Object hwnd -EQ $Hwnd)
 Assert-Fixture ($w.Count -eq 1 -and $w[0].class -eq 'OrpheusBrowserHost') 'CloudMusic window not found'
 $p=Get-Process -Id $w[0].pid
 Assert-Fixture ($p.ProcessName -eq 'cloudmusic') 'Unexpected process'
 $r.version=(Get-Item $p.Path).VersionInfo.FileVersion
 $null=Invoke-FixtureCli $c @('focus-window','--hwnd',"$Hwnd")
 $r.baseline=State
 foreach($method in @('uia','keyboard','keyboard','uia')){
  foreach($phase in @('toggle','restore')){
   $before=State
   $expected=if($before -eq 'play'){'pause'}else{'play'}
   $timer=[Diagnostics.Stopwatch]::StartNew()
   if($method -eq 'uia'){$null=Invoke-FixtureCli $c @('click','id:btn_pc_minibar_play','--app',$c.app)}
   else{$null=Invoke-FixtureCli $c @('press','ctrl+p','--app',$c.app)}
   $actionMs=$timer.ElapsedMilliseconds
   $deadline=[DateTime]::UtcNow.AddSeconds(5)
   do{$actual=State}while($actual -ne $expected -and [DateTime]::UtcNow -lt $deadline)
   Assert-Fixture ($actual -eq $expected) 'Playback state did not toggle'
   $r.samples.Add(@{method=$method;phase=$phase;before=$before;after=$actual;action_ms=$actionMs;verified_ms=$timer.ElapsedMilliseconds})
  }
  Assert-Fixture ((State) -eq $r.baseline) 'Playback state not restored'
 }
 $r.restored=$true;$code=0
}catch{$r.reason=$_.Exception.Message}
finally{
 # On failure do not toggle blindly; report the observed state for review.
 $r.passed=$code -eq 0;$r.exit_code=$code;$r.calls=$c.calls.ToArray();$r.uncertain=$c.uncertain
 $r|ConvertTo-Json -Depth 12|Set-Content "$root/result.json" -Encoding utf8
 Write-Host "Report: $root/result.json"
}
exit $code
