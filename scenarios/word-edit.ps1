param([string]$Actl=$env:ACTL,[string]$Program="$env:ProgramFiles/Microsoft Office/Root/Office16/WINWORD.EXE")
$ErrorActionPreference='Stop'
. "$PSScriptRoot/../fixtures/system-fixture.ps1"
. "$PSScriptRoot/office-package.ps1"
if(-not $Actl){$Actl="$PSScriptRoot/../target/validation/release/actl.exe"}
$base=if($env:ACTL_TEST_DIR){$env:ACTL_TEST_DIR}else{"$PSScriptRoot/../target"}
$root=Join-Path $base ('word-edit-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $root -Force|Out-Null
$root=(Resolve-Path $root).Path
$c=@{root=$root;binary=(Resolve-Path $Actl).Path;calls=[Collections.Generic.List[object]]::new();owned=$null;app=$null;uncertain=$false;cleanup='not_started';case='word-edit'}
$report=@{passed=$false;reason='';checks=[Collections.Generic.List[object]]::new();application=$null}
$code=1
function Normalize-WordText([string]$Text){$Text.Replace("`r`n","`n").Replace("`r","`n").TrimEnd("`n")}
function Open-OwnedDocument {
    $baseline=@((Invoke-FixtureCli $c @('list-windows')).windows|ForEach-Object hwnd)
    $null=Invoke-FixtureCli $c @('launch',$Program,'/n',$file)
    $null=Invoke-FixtureCli $c @('wait','--window',$marker,'--timeout','20000')
    $windows=@(Get-FixtureCandidates (Invoke-FixtureCli $c @('list-windows')).windows $baseline $marker)
    Assert-Fixture ($windows.Count -eq 1 -and $windows[0].class -eq 'OpusApp') 'Document ownership ambiguous'
    $process=Get-Process -Id $windows[0].pid
    Assert-Fixture ($process.Path -eq $Program) 'Document executable mismatch'
    Save-FixtureOwner $c $windows[0] $marker
}
function Assert-Document([string]$Name,[string]$Expected) {
    $null=Invoke-FixtureCli $c @('press','ctrl+s','--app',$c.app)
    $deadline=[DateTime]::UtcNow.AddSeconds(5)
    do {
        $actual=Read-OfficeContent $file word
        if($actual -ceq $Expected){break}
        Start-Sleep -Milliseconds 100
    }while([DateTime]::UtcNow -lt $deadline)
    Assert-Fixture ($actual -ceq $Expected) "Saved document differs: $Name"
    $read=Invoke-FixtureCli $c @('get','role:Document','--property','text','--app',$c.app)
    Assert-Fixture ((Normalize-WordText $read.properties.text) -ceq $Expected) "UIA document differs: $Name"
    $report.checks.Add(@{name=$Name;passed=$true;sha256=(Get-OfficeHash $file)})
}
try {
    if(Test-Path (Join-Path $env:LOCALAPPDATA 'actl/stop-requested')){$code=77;throw 'Emergency stop active'}
    if(-not (Test-Path -LiteralPath $Program)){$code=77;throw 'Word not installed'}
    $Program=(Resolve-Path $Program).Path
    $marker='actl-word-'+[guid]::NewGuid().ToString('N');$file=Join-Path $root "$marker.docx"
    New-OfficeSeed $file word
    $report.application=@{program=$Program;version=(Get-Item $Program).VersionInfo.FileVersion;file=$file;os=[Environment]::OSVersion.VersionString}
    Open-OwnedDocument
    $null=Invoke-FixtureCli $c @('type','role:Document','ACTL prefix ','--paste','--app',$c.app)
    Assert-Document 'insert_preserves_seed' 'ACTL prefix ACTL seed'
    $null=Invoke-FixtureCli $c @('press','ctrl+a','--app',$c.app)
    $body="ACTL report 2026`n第一段 中文 12345`nSecond paragraph 67890"
    $null=Invoke-FixtureCli $c @('set-clipboard',$body.Replace("`n","`r`n"))
    $null=Invoke-FixtureCli $c @('paste','--app',$c.app)
    Assert-Document 'replace_multiple_paragraphs' $body
    $null=Invoke-FixtureCli $c @('press','ctrl+end','--app',$c.app)
    $suffix="`nAppend 中文 Ω 24680"
    $null=Invoke-FixtureCli $c @('set-clipboard',$suffix.Replace("`n","`r`n"))
    $null=Invoke-FixtureCli $c @('paste','--app',$c.app)
    Assert-Document 'append_at_end' ($body+$suffix)
    $null=Invoke-FixtureCli $c @('press','ctrl+z','--app',$c.app)
    Assert-Document 'undo_append' $body
    $null=Invoke-FixtureCli $c @('press','ctrl+y','--app',$c.app)
    Assert-Document 'redo_append' ($body+$suffix)
    $null=Invoke-FixtureCli $c @('press','ctrl+a','--app',$c.app)
    $null=Invoke-FixtureCli $c @('press','ctrl+b','--app',$c.app)
    Assert-Document 'bold_preserves_text' ($body+$suffix)
    $stream=Open-OfficeReadStream $file
    $zip=[IO.Compression.ZipArchive]::new($stream,[IO.Compression.ZipArchiveMode]::Read)
    try {
        $xml=Read-OfficePart $zip 'word/document.xml'
        $runs=@($xml.SelectNodes('//*[local-name()="body"]//*[local-name()="r"][*[local-name()="t"]]'))
        Assert-Fixture ($runs.Count -gt 0) 'No text runs'
        foreach($run in $runs) {
            $bold=$run.SelectSingleNode('./*[local-name()="rPr"]/*[local-name()="b"]')
            Assert-Fixture ($null -ne $bold -and $bold.GetAttribute('val','http://schemas.openxmlformats.org/wordprocessingml/2006/main') -notin @('0','false','off')) 'Text run not bold'
        }
    }finally{$zip.Dispose();$stream.Dispose()}
    $report.checks.Add(@{name='bold_in_saved_xml';passed=$true})
    $null=Invoke-FixtureCli $c @('press','right','--app',$c.app)
    Close-OfficeWindow $c
    $c.owned=$null
    Open-OwnedDocument
    Assert-Document 'reopen_exact_text' ($body+$suffix)
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
