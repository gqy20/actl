$ErrorActionPreference='Stop'
. "$PSScriptRoot/../../../fixtures/system-fixture.ps1"
. "$PSScriptRoot/../../../scenarios/office-package.ps1"
$root=Join-Path "$PSScriptRoot/../../../target" ('office-package-tests-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $root -Force|Out-Null
$previousLocal=$env:LOCALAPPDATA
$env:LOCALAPPDATA=$root
try {
foreach($kind in @('excel','word')) {
    $file=Join-Path $root "$kind.zip"
    New-OfficeSeed $file $kind
    $content=Read-OfficeContent $file $kind
    if($kind -eq 'excel'){Assert-Fixture ($content.A1.value -ceq 'ACTL seed') 'Spreadsheet seed unreadable'}
    else{Assert-Fixture ($content -ceq 'ACTL seed') 'Document seed unreadable'}
    # Reproduce Office's open writer: the verifier must share read/write access.
    $writer=[IO.File]::Open($file,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::ReadWrite)
    try {
        $null=Read-OfficeContent $file $kind
        $hash=Get-OfficeHash $file
    }finally{$writer.Dispose()}
    Assert-Fixture ($hash -eq (Get-FileHash $file).Hash) 'Shared hash differs from disk hash'
}
$file=Join-Path $root 'mixed.xlsx'
Write-OfficePackage $file @{
    'xl/sharedStrings.xml'='<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><si><r><t>中</t></r><r><t>文</t></r></si></sst>'
    'xl/worksheets/sheet1.xml'='<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><f>2*3</f><v>6</v></c><c r="C1"/></row></sheetData></worksheet>'
}
$cells=Read-OfficeContent $file excel
Assert-Fixture ($cells.A1.value -ceq '中文' -and $cells.B1.value -eq '6' -and $cells.B1.formula -eq '2*3' -and $cells.C1.value -eq '') 'Shared strings/formulas/empty cells parsed incorrectly'
$file=Join-Path $root 'paragraphs.docx'
Write-OfficePackage $file @{'word/document.xml'='<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>First</w:t></w:r><w:r><w:t xml:space="preserve"> 中文</w:t></w:r></w:p><w:p/><w:p><w:r><w:t>Last</w:t></w:r></w:p></w:body></w:document>'}
Assert-Fixture ((Read-OfficeContent $file word) -ceq "First 中文`n`nLast") 'Paragraph boundaries or split runs lost'
# Delayed close: observe until gone, issue the close only once; preserve dialogs.
function Close-FixtureWindow($Context) {$script:closes++;throw 'Owned window did not close'}
function Invoke-FixtureCli($Context,$Arguments) {
    $script:reads++
    return @{windows=@()}
}
$script:closes=0;$script:reads=0
$context=@{owned=@{hwnd=42;pid=1;started=123};cleanup='not_started';uncertain=$false}
# UIA omission with a live original owner is not proof of closure.
function Get-Process {param($Id,$ErrorAction) return [pscustomobject]@{StartTime=[datetime]::new(123,[DateTimeKind]::Utc)}}
function Invoke-FixtureCli($Context,$Arguments) {
    $script:reads++
    if('--native' -in $Arguments){throw 'native probe failed'}
    return @{windows=@()}
}
$rejected=$false
try{Close-OfficeWindow $context}catch{$rejected=$true}
Assert-Fixture ($rejected -and $context.cleanup -ne 'closed_verified') 'Missing UIA window was falsely reported closed'
Remove-Item Function:Get-Process
$script:closes=0;$script:reads=0;$context.cleanup='not_started'
function Invoke-FixtureCli($Context,$Arguments) {$script:reads++;return @{windows=@()}}
Close-OfficeWindow $context
Assert-Fixture ($script:closes -eq 1 -and $script:reads -eq 1 -and $context.cleanup -eq 'closed_verified') 'Delayed close was replayed or not verified'
function Close-FixtureWindow($Context) {throw 'Cleanup deferred while emergency stop is active'}
$blocked=$false
try{Close-OfficeWindow $context}catch{$blocked=$true}
Assert-Fixture ($blocked -and $script:reads -eq 1) 'Stop refusal was swallowed'
$hostWindow=[pscustomobject]@{class='XLMAIN';title='Excel';visible=$false}
$emptyTree=[pscustomobject]@{truncated=$false;elements=@([pscustomobject]@{role='Window';name='Excel'},[pscustomobject]@{role='TitleBar'})}
Assert-Fixture (Test-OfficeEmptyHost $hostWindow $emptyTree) 'Empty hidden Excel host was not classified'
$emptyTree.elements+=[pscustomobject]@{role='MenuBar';name='系统'}
Assert-Fixture (Test-OfficeEmptyHost $hostWindow $emptyTree) 'Native system menu in full snapshot was rejected (skeleton omits it)'
$hostWindow.visible=$true
Assert-Fixture (-not (Test-OfficeEmptyHost $hostWindow $emptyTree)) 'Visible host accepted as closed document'
$hostWindow.visible=$false
$emptyTree.elements+= [pscustomobject]@{role='DataGrid';name='Grid'}
Assert-Fixture (-not (Test-OfficeEmptyHost $hostWindow $emptyTree)) 'Workbook grid accepted as empty host'
$script:emptyHost=[pscustomobject]@{hwnd=42;pid=1;class='XLMAIN';title='Excel';visible=$false}
$script:emptySnapshot=[pscustomobject]@{truncated=$false;elements=@([pscustomobject]@{role='Window';name='Excel'})}
function Close-FixtureWindow($Context) {$script:closes++;throw 'Owned window did not close'}
function Invoke-FixtureCli($Context,$Arguments) {
    if($Arguments[0] -eq 'snapshot'){return $script:emptySnapshot}
    return @{windows=@($script:emptyHost)}
}
$context=@{owned=@{hwnd=42;pid=1;started=123};app='hwnd:42';cleanup='not_started';file=$file;uncertain=$false}
function Get-Process {param($Id,$ErrorAction) return [pscustomobject]@{StartTime=[datetime]::new(123,[DateTimeKind]::Utc)}}
Close-OfficeWindow $context
Assert-Fixture ($context.cleanup -eq 'document_closed_host_retained' -and $script:closes -eq 2) 'Retained host was misreported or closed repeatedly'
Remove-Item Function:Get-Process
Write-Output 'Office package oracles, shared file access and delayed close checks passed'
}finally{$env:LOCALAPPDATA=$previousLocal}
