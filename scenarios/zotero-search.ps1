param([string]$Actl=$env:ACTL,[string]$Program='C:/Program Files/Zotero/zotero.exe')
$ErrorActionPreference='Stop'
. "$PSScriptRoot/../fixtures/system-fixture.ps1"
if(-not $Actl){$Actl="$PSScriptRoot/../target/validation/release/actl.exe"}
$base=if($env:ACTL_TEST_DIR){$env:ACTL_TEST_DIR}else{"$PSScriptRoot/../target"}
$root=Join-Path $base ('zotero-search-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory "$root/profile","$root/data" -Force|Out-Null
$root=(Resolve-Path $root).Path
$c=@{root=$root;binary=(Resolve-Path $Actl).Path;calls=[Collections.Generic.List[object]]::new();owned=$null;app=$null;uncertain=$false;cleanup='not_started';case='zotero-search'}
$report=@{passed=$false;reason='';checks=[Collections.Generic.List[object]]::new();application=$null}
$code=1
function Check-Rows([string]$Label,[string[]]$Expected){
 $deadline=[DateTime]::UtcNow.AddSeconds(10)
 do{
  $s=Invoke-FixtureCli $c @('snapshot','--app',$c.app)
  $rows=@($s.elements|Where-Object {$_.role -eq 'TreeItem' -and $_.PSObject.Properties['automation_id'] -and $_.automation_id.StartsWith('item-tree-main-default-row-')}|ForEach-Object name|Sort-Object)
  $tree=@($s.elements|Where-Object {$_.PSObject.Properties['automation_id'] -and $_.automation_id -eq 'item-tree-main-default'})
  if($tree.Count -eq 1 -and ($rows -join '|') -eq (($Expected|Sort-Object) -join '|')){break}
 }while([DateTime]::UtcNow -lt $deadline)
 Assert-Fixture (-not $s.truncated -and $tree.Count -eq 1 -and ($rows -join '|') -eq (($Expected|Sort-Object) -join '|')) "Wrong result rows: $Label"
 $report.checks.Add(@{name=$Label;rows=$rows;passed=$true})
}
try{
 if(Test-Path "$env:LOCALAPPDATA/actl/stop-requested"){$code=77;throw 'Emergency stop active'}
 if(-not (Test-Path -LiteralPath $Program)){$code=77;throw 'Zotero not installed'}
 $Program=(Resolve-Path $Program).Path
 $profile=Join-Path $root 'profile';$data=Join-Path $root 'data'
 @'
user_pref("extensions.zotero.httpServer.enabled", false);
user_pref("extensions.zotero.sync.autoSync", false);
user_pref("extensions.zotero.firstRun2", false);
user_pref("extensions.zotero.firstRunGuidance", false);
'@|Set-Content "$profile/user.js"
 $report.application=@{version=(Get-Item $Program).VersionInfo.FileVersion;program=$Program;profile=$profile;data=$data}
 $baseline=@((Invoke-FixtureCli $c @('list-windows','--native')).windows|ForEach-Object hwnd)
 $null=Invoke-FixtureCli $c @('launch',$Program,'-no-remote','-profile',$profile,'-datadir',$data)
 $deadline=[DateTime]::UtcNow.AddSeconds(30)
 do{
  $windows=@((Invoke-FixtureCli $c @('list-windows','--native')).windows|Where-Object {
   if($_.hwnd -in $baseline -or $_.class -ne 'MozillaWindowClass' -or $_.visible -eq $false){return $false}
   $p=Get-CimInstance Win32_Process -Filter "ProcessId=$($_.pid)"
   $p -and $p.ExecutablePath -eq $Program -and $p.CommandLine.Contains($profile) -and $p.CommandLine.Contains($data)
  })
  if($windows.Count -eq 0){Start-Sleep -Milliseconds 300}
 }while($windows.Count -eq 0 -and [DateTime]::UtcNow -lt $deadline)
 Assert-Fixture ($windows.Count -eq 1) 'Owned Zotero window not unique'
 Save-FixtureOwner $c $windows[0] 'Zotero'
 Assert-Fixture (Test-Path "$data/zotero.sqlite") 'Isolated database not created'
 $ris="TY  - JOUR`r`nTI  - ACTL Alpha 中文测试`r`nAU  - Test, Alice`r`nPY  - 2026`r`nER  - `r`n`r`nTY  - JOUR`r`nTI  - ACTL Beta unrelated`r`nAU  - Test, Bob`r`nPY  - 2025`r`nER  - `r`n"
 $ris|Set-Content "$root/seed.ris" -Encoding utf8
 $null=Invoke-FixtureCli $c @('click','id:fileMenu','--app',$c.app)
 $null=Invoke-FixtureCli $c @('set-clipboard',$ris)
 $null=Invoke-FixtureCli $c @('click','name:从剪贴板导入','--app',$c.app)
 $both=@('ACTL Alpha 中文测试. 期刊文章. Test','ACTL Beta unrelated. 期刊文章. Test')
 Check-Rows 'imported_two_records' $both
 foreach($query in @('Alpha','中文测试','ACTL_missing_987654')){
  $f=Invoke-FixtureCli $c @('find','--name','快速搜索','--role','Edit','--app',$c.app)
  Assert-Fixture (@($f.elements).Count -eq 1) 'Search field is not unique'
  $null=Invoke-FixtureCli $c @('type',$f.elements[0].ref,$query,'--paste','--verify-value',$query,'--snapshot',$c.last_snapshot_id,'--app',$c.app)
  $expected=if($query -eq 'ACTL_missing_987654'){@()}else{@($both[0])}
  Check-Rows "query:$query" $expected
  # Tree changes can renumber refs; resolve and check identity after list changes.
  $deadline=[DateTime]::UtcNow.AddSeconds(10)
  $readMatches=$false
  do{
   $f=Invoke-FixtureCli $c @('find','--name','快速搜索','--role','Edit','--app',$c.app)
   Assert-Fixture (@($f.elements).Count -eq 1) 'Search field changed after snapshot'
   $identity=Invoke-FixtureCli $c @('get',$f.elements[0].ref,'--snapshot',$c.last_snapshot_id,'--app',$c.app)
   $read=Invoke-FixtureCli $c @('get',$f.elements[0].ref,'--property','text','--snapshot',$c.last_snapshot_id,'--app',$c.app)
   $readMatches=$identity.properties.name -eq '快速搜索' -and $identity.properties.role -eq 'Edit' -and $read.properties.PSObject.Properties['text'] -and $read.properties.text -eq $query
  }while(-not $readMatches -and [DateTime]::UtcNow -lt $deadline)
  Assert-Fixture $readMatches 'Search text readback mismatch'
  $report.checks.Add(@{name='search_text';query=$query;passed=$true})
  $null=Invoke-FixtureCli $c @('press','ctrl+a','--target',$f.elements[0].ref,'--snapshot',$c.last_snapshot_id,'--app',$c.app)
  $null=Invoke-FixtureCli $c @('press','backspace','--target',$f.elements[0].ref,'--snapshot',$c.last_snapshot_id,'--app',$c.app)
  Check-Rows 'restored_two_records' $both
 }
 # Edit an owned synthetic item's publication, then verify the persisted export.
 $null=Invoke-FixtureCli $c @('click','id:item-tree-main-default-row-0','--app',$c.app)
 $fields=Invoke-FixtureCli $c @('find','--name','出版物','--role','Edit','--app',$c.app)
 $publication=@($fields.elements|Where-Object name -EQ '出版物')
 Assert-Fixture ($publication.Count -eq 1) 'Publication field is ambiguous'
 $null=Invoke-FixtureCli $c @('type',$publication[0].ref,'ACTL Journal 2026','--paste','--verify-value','ACTL Journal 2026','--snapshot',$c.last_snapshot_id,'--app',$c.app)
 $null=Invoke-FixtureCli $c @('press','tab','--target',$publication[0].ref,'--snapshot',$c.last_snapshot_id,'--app',$c.app)
 $null=Invoke-FixtureCli $c @('right-click','id:item-tree-main-default-row-0','--app',$c.app)
 $null=Invoke-FixtureCli $c @('click','name:导出条目','--app',$c.app)
 $null=Invoke-FixtureCli $c @('wait','--window','导出…','--timeout','10000')
 $dialogs=@((Invoke-FixtureCli $c @('list-windows','--native')).windows|Where-Object {$_.pid -eq $c.owned.pid -and $_.class -eq 'MozillaDialogClass' -and $_.title -eq '导出…'})
 Assert-Fixture ($dialogs.Count -eq 1) 'Export format dialog not unique'
 $formatApp="hwnd:$($dialogs[0].hwnd)"
 $null=Invoke-FixtureCli $c @('click','id:format-menu','--app',$formatApp)
 $null=Invoke-FixtureCli $c @('click','name:CSL JSON','--app',$formatApp)
 $beforeSave=@((Invoke-FixtureCli $c @('list-windows','--native')).windows|ForEach-Object hwnd)
 $null=Invoke-FixtureCli $c @('click','name:OK','--app',$formatApp)
 $deadline=[DateTime]::UtcNow.AddSeconds(15)
 do{
  $save=@((Invoke-FixtureCli $c @('list-windows','--native')).windows|Where-Object {$_.hwnd -notin $beforeSave -and $_.class -eq '#32770' -and $_.title -eq '导出' -and $_.PSObject.Properties['owner_title'] -and $_.owner_title.Contains('Zotero')})
 }while($save.Count -eq 0 -and [DateTime]::UtcNow -lt $deadline)
 Assert-Fixture ($save.Count -eq 1) 'Export save dialog not unique'
 $saveApp="hwnd:$($save[0].hwnd)"
 # Common dialogs reuse numeric IDs for file rows and the toolbar; filter by role.
 $field=Invoke-FixtureCli $c @('find','--name','文件名:','--role','Edit','--app',$saveApp)
 Assert-Fixture (@($field.elements).Count -eq 1) 'Filename field not unique'
 $export=Join-Path $root 'export.json'
 $null=Invoke-FixtureCli $c @('set-value',$field.elements[0].ref,$export,'--snapshot',$c.last_snapshot_id,'--app',$saveApp)
 $null=Invoke-FixtureCli $c @('click','name:保存(S)','--app',$saveApp)
 $deadline=[DateTime]::UtcNow.AddSeconds(10)
 do{
  $items=@()
  if(Test-Path -LiteralPath $export){try{$items=@(Get-Content -LiteralPath $export -Raw|ConvertFrom-Json)}catch{}}
  if($items.Count -eq 1){break}
  Start-Sleep -Milliseconds 100
 }while([DateTime]::UtcNow -lt $deadline)
 Assert-Fixture ($items.Count -eq 1 -and $items[0].title -eq 'ACTL Alpha 中文测试' -and $items[0].'container-title' -eq 'ACTL Journal 2026' -and $items[0].author[0].family -eq 'Test') 'Exported metadata differs from owned record'
 $report.checks.Add(@{name='publication_edit_export';passed=$true;file=$export;sha256=(Get-FileHash $export).Hash})
 $code=0
}catch{$report.reason=$_.Exception.Message}
finally{
 try{Close-FixtureWindow $c}catch{$code=1;$report.reason+="; cleanup: $($_.Exception.Message)"}
 $report.passed=$code -eq 0;$report.exit_code=$code;$report.cleanup=$c.cleanup;$report.calls=$c.calls.ToArray()
 $report|ConvertTo-Json -Depth 12|Set-Content "$root/result.json" -Encoding utf8
 Write-Host "Report: $root/result.json"
 Write-Host ($report|Select-Object passed,reason,cleanup|ConvertTo-Json -Compress)
}
exit $code
