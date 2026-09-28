# Existing signed-in drive tab. Search only; never uploads, downloads or removes files.
param([string]$Actl=$env:ACTL,[Parameter(Mandatory)][long]$Hwnd,[string]$Query='qzxvkjwmpbhfytzz')
$ErrorActionPreference='Stop'
. "$PSScriptRoot/../fixtures/system-fixture.ps1"
if(-not $Actl){$Actl="$PSScriptRoot/../target/validation/release/actl.exe"}
$base=if($env:ACTL_TEST_DIR){$env:ACTL_TEST_DIR}else{"$PSScriptRoot/../target"}
$root=Join-Path $base ('quark-search-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $root|Out-Null
$root=(Resolve-Path $root).Path
$c=@{root=$root;binary=(Resolve-Path $Actl).Path;calls=[Collections.Generic.List[object]]::new();app="hwnd:$Hwnd";uncertain=$false}
$code=1;$reason='';$checks=[Collections.Generic.List[object]]::new()
function Find-SearchField {
 $f=Invoke-FixtureCli $c @('find','--role','Edit','--name','搜网盘','--app',$c.app)
 Assert-Fixture (@($f.elements).Count -eq 1) 'Drive search field is ambiguous'
 return @{target=$f.elements[0].ref;snapshot=$c.last_snapshot_id}
}
try{
 if(Test-Path "$env:LOCALAPPDATA/actl/stop-requested"){$code=77;throw 'Emergency stop active'}
 Assert-Fixture (-not [string]::IsNullOrWhiteSpace($Query)) 'Empty-result test requires a nonempty query'
 $initial=Invoke-FixtureCli $c @('get','id:RootWebArea','--property','value','--app',$c.app)
 Assert-Fixture ($initial.properties.value -like 'uccd://cloud.quark/clouddrive/*') 'Target is not the drive page'
 $null=Invoke-FixtureCli $c @('press','ctrl+f','--app',$c.app)
 $field=Find-SearchField
 $null=Invoke-FixtureCli $c @('press','ctrl+a','--target',$field.target,'--snapshot',$field.snapshot,'--app',$c.app)
 $null=Invoke-FixtureCli $c @('type',$field.target,$Query,'--paste','--verify-value',$Query,'--snapshot',$field.snapshot,'--app',$c.app)
 $checks.Add(@{name='bound_search_input_readback';passed=$true})
 # Suggestions may change the tree: locate again before submitting.
 $field=Find-SearchField
 $null=Invoke-FixtureCli $c @('press','enter','--target',$field.target,'--snapshot',$field.snapshot,'--app',$c.app)
 $deadline=[DateTime]::UtcNow.AddSeconds(15);$matched=$false
 do{
  $route=Invoke-FixtureCli $c @('get','id:RootWebArea','--property','value','--app',$c.app)
  $text=Invoke-FixtureCli $c @('get','id:RootWebArea','--property','text','--app',$c.app)
  $keyword=[regex]::Match($route.properties.value,'[?&]keyword=([^&]*)')
  $matched=$keyword.Success -and [Uri]::UnescapeDataString($keyword.Groups[1].Value) -ceq $Query -and $text.properties.text.Contains('相关搜索结果 0 项') -and $text.properties.text.Contains('暂无搜索内容')
  if($matched){break}
 }while([DateTime]::UtcNow -lt $deadline)
 Assert-Fixture $matched 'Exact query route and empty result were not both confirmed; no resubmit'
 $checks.Add(@{name='route_and_empty_results';passed=$true})
 $null=Invoke-FixtureCli $c @('screenshot','--app',$c.app,'--out',"$root/result.png")
 $code=0
}catch{$reason=$_.Exception.Message}finally{
 @{passed=($code -eq 0);exit_code=$code;reason=$reason;query=$Query;hwnd=$Hwnd;checks=@($checks.ToArray());calls=@($c.calls.ToArray());cleanup='existing_window_retained';binary=$c.binary;sha256=(Get-FileHash $c.binary).Hash}|ConvertTo-Json -Depth 10|Set-Content "$root/result.json" -Encoding utf8
 Write-Output "Report: $root/result.json"
}
exit $code
