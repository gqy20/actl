param([string]$Actl=$env:ACTL,[string]$Program="$env:ProgramFiles/Microsoft Office/Root/Office16/EXCEL.EXE")
$ErrorActionPreference='Stop'
. "$PSScriptRoot/../fixtures/system-fixture.ps1"
. "$PSScriptRoot/office-package.ps1"
if(-not $Actl){$Actl="$PSScriptRoot/../target/validation/release/actl.exe"}
$base=if($env:ACTL_TEST_DIR){$env:ACTL_TEST_DIR}else{"$PSScriptRoot/../target"}
$root=Join-Path $base ('excel-table-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $root -Force|Out-Null
$root=(Resolve-Path $root).Path
$c=@{root=$root;binary=(Resolve-Path $Actl).Path;calls=[Collections.Generic.List[object]]::new();owned=$null;app=$null;uncertain=$false;cleanup='not_started';case='excel-table'}
$report=@{passed=$false;reason='';checks=[Collections.Generic.List[object]]::new();application=$null}
$code=1
function Open-OwnedWorkbook {
    $baseline=@((Invoke-FixtureCli $c @('list-windows')).windows|ForEach-Object hwnd)
    $null=Invoke-FixtureCli $c @('launch',$Program,'/x',$file)
    $null=Invoke-FixtureCli $c @('wait','--window',$marker,'--timeout','20000')
    $windows=@(Get-FixtureCandidates (Invoke-FixtureCli $c @('list-windows')).windows $baseline $marker)
    Assert-Fixture ($windows.Count -eq 1 -and $windows[0].class -eq 'XLMAIN') 'Workbook ownership ambiguous'
    $process=Get-Process -Id $windows[0].pid
    Assert-Fixture ($process.Path -eq $Program) 'Workbook executable mismatch'
    Save-FixtureOwner $c $windows[0] $marker
}
function Select-Range([string]$Address) {
    $null=Invoke-FixtureCli $c @('type','id:1001',$Address,'--app',$c.app)
    $null=Invoke-FixtureCli $c @('press','enter','--app',$c.app)
}
function Paste-Range([string]$Address,[string]$Text) {
    Select-Range $Address
    $null=Invoke-FixtureCli $c @('set-clipboard',$Text)
    $null=Invoke-FixtureCli $c @('paste','--app',$c.app)
    $null=Invoke-FixtureCli $c @('press','esc','--app',$c.app)
}
function Assert-Workbook([string]$Name,[hashtable]$Expected) {
    $null=Invoke-FixtureCli $c @('press','ctrl+s','--app',$c.app)
    $deadline=[DateTime]::UtcNow.AddSeconds(5)
    do {
        $actual=Read-OfficeContent $file excel
        $match=$true
        foreach($address in $Expected.Keys) {
            if(-not $actual.ContainsKey($address) -or $actual[$address].value -cne $Expected[$address]){$match=$false}
        }
        if($match){break}
        Start-Sleep -Milliseconds 100
    }while([DateTime]::UtcNow -lt $deadline)
    Assert-Fixture $match "Saved cells differ: $Name"
    $report.checks.Add(@{name=$Name;passed=$true;cells=$Expected.Clone();sha256=(Get-OfficeHash $file)})
}
try {
    if(Test-Path (Join-Path $env:LOCALAPPDATA 'actl/stop-requested')){$code=77;throw 'Emergency stop active'}
    if(-not (Test-Path -LiteralPath $Program)){$code=77;throw 'Excel not installed'}
    $Program=(Resolve-Path $Program).Path
    $marker='actl-excel-'+[guid]::NewGuid().ToString('N');$file=Join-Path $root "$marker.xlsx"
    $c.file=$file
    New-OfficeSeed $file excel
    $report.application=@{program=$Program;version=(Get-Item $Program).VersionInfo.FileVersion;file=$file;os=[Environment]::OSVersion.VersionString}
    Open-OwnedWorkbook
    Paste-Range 'A1' "Item`tQty`tPrice`tTotal`r`nAlpha`t2`t12.5`t=B2*C2`r`n中文`t3`t7`t=B3*C3`r`nSum`t`t`t=SUM(D2:D3)"
    $expected=@{A1='Item';B1='Qty';C1='Price';D1='Total';A2='Alpha';B2='2';C2='12.5';D2='25';A3='中文';B3='3';C3='7';D3='21';A4='Sum';D4='46'}
    Assert-Workbook 'tsv_and_formulas' $expected
    $cells=Read-OfficeContent $file excel
    Assert-Fixture ($cells.D2.formula -eq 'B2*C2' -and $cells.D3.formula -eq 'B3*C3' -and $cells.D4.formula -eq 'SUM(D2:D3)') 'Formula expressions missing from workbook'
    $report.checks.Add(@{name='formula_expressions';passed=$true})
    Paste-Range 'B2' '4'
    $expected.B2='4';$expected.D2='50';$expected.D4='71'
    Assert-Workbook 'recalculate' $expected
    $null=Invoke-FixtureCli $c @('press','ctrl+z','--app',$c.app)
    $expected.B2='2';$expected.D2='25';$expected.D4='46'
    Assert-Workbook 'undo' $expected
    $null=Invoke-FixtureCli $c @('press','ctrl+y','--app',$c.app)
    $expected.B2='4';$expected.D2='50';$expected.D4='71'
    Assert-Workbook 'redo' $expected
    Select-Range 'A1:D4'
    $null=Invoke-FixtureCli $c @('press','ctrl+c','--app',$c.app)
    $clip=Invoke-FixtureCli $c @('get-clipboard')
    $clip|ConvertTo-Json -Depth 5|Set-Content "$root/copied-range.json" -Encoding utf8
    Assert-Fixture ($clip.text.Replace("`r`n","`n").TrimEnd("`n") -ceq "Item`tQty`tPrice`tTotal`nAlpha`t4`t12.5`t50`n中文`t3`t7`t21`nSum`t`t`t71") 'Range clipboard differs'
    $report.checks.Add(@{name='copy_range';passed=$true})
    $null=Invoke-FixtureCli $c @('press','esc','--app',$c.app)
    Close-OfficeWindow $c
    $c.owned=$null
    Open-OwnedWorkbook
    Assert-Workbook 'reopen' $expected
    Select-Range 'D4'
    $null=Invoke-FixtureCli $c @('press','ctrl+c','--app',$c.app)
    $clip=Invoke-FixtureCli $c @('get-clipboard')
    Assert-Fixture ($clip.text.Trim() -eq '71') 'Reopened calculation readback differs'
    $report.checks.Add(@{name='reopen_uia_keyboard_readback';passed=$true})
    $null=Invoke-FixtureCli $c @('press','esc','--app',$c.app)
    $code=0
}catch{$report.reason=$_.Exception.Message}
finally {
    try{Close-OfficeWindow $c}catch{$code=1;$report.reason+="; cleanup: $($_.Exception.Message)"}
    $report.passed=$code -eq 0;$report.cleanup=$c.cleanup;$report.calls=$c.calls.ToArray();$report.exit_code=$code;$report.owned=$c.owned
    $report|ConvertTo-Json -Depth 12|Set-Content "$root/result.json" -Encoding utf8
    Write-Host "Report: $root/result.json"
    Write-Host ($report|Select-Object passed,reason,cleanup|ConvertTo-Json -Compress)
}
exit $code
