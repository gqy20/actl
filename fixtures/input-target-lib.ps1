# Notepad restores old tabs even when no process was running. Close only our saved tab.
function Get-InputTabCloseTarget($Elements,[string]$Marker) {
 $tabs=@(for($i=0;$i -lt $Elements.Count;$i++){
  if($Elements[$i].role -eq 'TabItem' -and $Elements[$i].name.Contains($Marker)){$i}
 })
 Assert-Fixture ($tabs.Count -eq 1) 'Owned Notepad tab not unique'
 $tab=$Elements[$tabs[0]]
 Assert-Fixture ($tab.name.Contains('未修改')) 'Task tab has unsaved or unrecognized state; retained'
 $buttons=@(for($i=$tabs[0]+1;$i -lt $Elements.Count;$i++){
  if($Elements[$i].depth -le $tab.depth){break}
  if($Elements[$i].role -eq 'Button' -and $Elements[$i].PSObject.Properties['automation_id'] -and $Elements[$i].automation_id -eq 'CloseButton'){$Elements[$i]}
 })
 Assert-Fixture ($buttons.Count -eq 1) 'Owned tab close button not proven; retained'
 return $buttons[0].ref
}
function Close-InputFixtureTab($Context) {
 $Context.cleanup='retained_for_review'
 $windows=@((Invoke-FixtureCli $Context @('list-windows','--native','--hwnd',[string]$Context.owned.hwnd)).windows)
 $process=Get-Process -Id $Context.owned.pid -ErrorAction Stop
 Assert-Fixture ($windows.Count -eq 1 -and (Test-FixtureIdentity $Context.owned $windows[0] $process.StartTime.ToUniversalTime().Ticks)) 'Window identity changed'
 $s=Invoke-FixtureCli $Context @('snapshot','--app',$Context.app)
 $target=Get-InputTabCloseTarget $s.elements $Context.owned.marker
 $null=Invoke-FixtureCli $Context @('click',$target,'--snapshot',$Context.last_snapshot_id,'--app',$Context.app)
 $remaining=@((Invoke-FixtureCli $Context @('list-windows','--native','--hwnd',[string]$Context.owned.hwnd)).windows)
 if($remaining.Count){
  $s=Invoke-FixtureCli $Context @('snapshot','--app',$Context.app)
  Assert-Fixture (@($s.elements|Where-Object {$_.role -eq 'TabItem' -and $_.name.Contains($Context.owned.marker)}).Count -eq 0) 'Owned tab still present'
 }
 $Context.cleanup='owned_tab_closed'
}
