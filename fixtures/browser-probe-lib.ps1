function Assert-BrowserOwnedTab($Elements,[string]$Marker) {
    $matches=@(for($i=0;$i -lt $Elements.Count;$i++){
        if($Elements[$i].role -eq 'TabItem' -and $Elements[$i].name.Contains($Marker)){$i}
    })
    Assert-Fixture ($matches.Count -eq 1) 'Owned browser tab is not unique'
    $index=$matches[0];$parent=-1
    for($i=$index-1;$i -ge 0;$i--){
        if($Elements[$i].depth -lt $Elements[$index].depth){$parent=$i;break}
    }
    Assert-Fixture ($parent -ge 0 -and $Elements[$parent].role -eq 'Tab') 'Browser tab strip not identified'
    $tabs=@(for($i=$parent+1;$i -lt $Elements.Count;$i++){
        if($Elements[$i].depth -le $Elements[$parent].depth){break}
        if($Elements[$i].role -eq 'TabItem' -and $Elements[$i].depth -eq $Elements[$parent].depth+1){$Elements[$i]}
    })
    Assert-Fixture ($tabs.Count -eq 1) 'Browser tabs changed; window retained'
}
