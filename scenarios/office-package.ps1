# Synthetic Office files and disk oracles. No Office COM automation or user documents.
function Write-OfficePackage([string]$Path,[hashtable]$Parts) {
    $zip=[IO.Compression.ZipFile]::Open($Path,[IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach($name in $Parts.Keys) {
            $stream=$zip.CreateEntry($name).Open()
            $writer=[IO.StreamWriter]::new($stream,[Text.UTF8Encoding]::new($false))
            try {$writer.Write($Parts[$name])} finally {$writer.Dispose()}
        }
    } finally {$zip.Dispose()}
}
function New-OfficeSeed([string]$Path,[ValidateSet('excel','word')][string]$Kind) {
    $target=if($Kind -eq 'excel'){'xl/workbook.xml'}else{'word/document.xml'}
    $parts=@{'_rels/.rels'="<Relationships xmlns=`"http://schemas.openxmlformats.org/package/2006/relationships`"><Relationship Id=`"rId1`" Type=`"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument`" Target=`"$target`"/></Relationships>"}
    $types='<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
    if($Kind -eq 'excel') {
        $parts['[Content_Types].xml']=$types+'<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>'
        $parts['xl/workbook.xml']='<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>'
        $parts['xl/_rels/workbook.xml.rels']='<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>'
        $parts['xl/worksheets/sheet1.xml']='<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>ACTL seed</t></is></c></row></sheetData></worksheet>'
    } else {
        $parts['[Content_Types].xml']=$types+'<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>'
        $parts['word/document.xml']='<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>ACTL seed</w:t></w:r></w:p><w:sectPr/></w:body></w:document>'
    }
    Write-OfficePackage $Path $parts
}
function Read-OfficePart($Zip,[string]$Name) {
    $entry=$Zip.GetEntry($Name)
    if(-not $entry){throw "Missing Office part: $Name"}
    $reader=[IO.StreamReader]::new($entry.Open())
    try {return [xml]$reader.ReadToEnd()} finally {$reader.Dispose()}
}
function Open-OfficeReadStream([string]$Path) {
    $deadline=[DateTime]::UtcNow.AddSeconds(10)
    do {
        try {return [IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))}
        catch [IO.IOException] {
            if([DateTime]::UtcNow -ge $deadline){throw}
            # Saving can briefly lock/replace the package. Retry only the disk read.
            Start-Sleep -Milliseconds 100
        }
    }while($true)
}
function Get-OfficeHash([string]$Path) {
    $stream=Open-OfficeReadStream $Path
    try {return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream))} finally {$stream.Dispose()}
}
function Test-OfficeEmptyHost($Window,$Snapshot) {
    return ($Window.class -eq 'XLMAIN' -and $Window.title -eq 'Excel' -and
        $Window.PSObject.Properties['visible'] -and $Window.visible -eq $false -and
        -not $Snapshot.truncated -and @($Snapshot.elements).Count -gt 0 -and
        @($Snapshot.elements|Where-Object {$_.role -notin @('Window','TitleBar','MenuBar','MenuItem','Button')}).Count -eq 0)
}
function Close-OfficeWindow($Context) {
    if(-not $Context.owned){$Context.cleanup='no_owned_window';return}
    if($Context.uncertain){$Context.cleanup='retained_uncertain';throw 'Cleanup deferred after uncertain CLI result'}
    if(Test-Path (Join-Path $env:LOCALAPPDATA 'actl/stop-requested')){$Context.cleanup='retained_stop';throw 'Cleanup deferred while emergency stop is active'}
    if($Context.cleanup -ne 'retained_for_review') {
        try {Close-FixtureWindow $Context;return} catch {
            # Observe asynchronous close; never replay it or dismiss a save dialog.
            if($_.Exception.Message -ne 'Owned window did not close'){throw}
            $Context.cleanup='retained_for_review'
        }
    }
    $deadline=[DateTime]::UtcNow.AddSeconds(45)
    do {
        if(Test-Path (Join-Path $env:LOCALAPPDATA 'actl/stop-requested')){$Context.cleanup='retained_stop';throw 'Cleanup deferred while emergency stop is active'}
        $windows=@((Invoke-FixtureCli $Context @('list-windows','--native','--hwnd',[string]$Context.owned.hwnd)).windows)
        $same=@($windows|Where-Object {$_.PSObject.Properties['hwnd'] -and $_.hwnd -eq $Context.owned.hwnd})
        if(-not $same.Count) {
            $Context.cleanup='closed_verified';return
        }
        if($same.Count -eq 1 -and $same[0].class -eq 'XLMAIN' -and $same[0].title -eq 'Excel' -and $same[0].pid -eq $Context.owned.pid) {
            $process=Get-Process -Id $Context.owned.pid -ErrorAction Stop
            Assert-Fixture ($process.StartTime.ToUniversalTime().Ticks -eq $Context.owned.started) 'Office process identity changed during close'
            try {$tree=Invoke-FixtureCli $Context @('snapshot','--app',$Context.app)} catch {
                # The same native window may finish closing between the two reads.
                # Recheck native existence; do not replay close or swallow provider errors.
                $last=Get-Content $Context.calls[$Context.calls.Count-1].stdout -Raw|ConvertFrom-Json
                if($last.error.code -ne 'NOT_FOUND'){throw}
                Start-Sleep -Milliseconds 100
                continue
            }
            if(Test-OfficeEmptyHost $same[0] $tree) {
                # Require the task's file to be released as well as an empty hidden UIA host.
                $stream=[IO.File]::Open($Context.file,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::None)
                $stream.Dispose()
                $Context.cleanup='document_closed_host_retained';return
            }
        }
        Start-Sleep -Milliseconds 100
    }while([DateTime]::UtcNow -lt $deadline)
    throw 'Office close pending; inspect the owned window before continuing'
}
function Read-OfficeContent([string]$Path,[ValidateSet('excel','word')][string]$Kind) {
    # Excel holds an open file handle while editing; inspect a shared read stream.
    $stream=Open-OfficeReadStream $Path
    $zip=$null
    try {
        $zip=[IO.Compression.ZipArchive]::new($stream,[IO.Compression.ZipArchiveMode]::Read)
        if($Kind -eq 'word') {
            $xml=Read-OfficePart $zip 'word/document.xml'
            return (@($xml.SelectNodes('//*[local-name()="body"]/*[local-name()="p"]')|ForEach-Object {
                ($_.SelectNodes('.//*[local-name()="t"]')|ForEach-Object InnerText) -join ''
            }) -join "`n")
        }
        $strings=@()
        if($zip.GetEntry('xl/sharedStrings.xml')) {
            $xml=Read-OfficePart $zip 'xl/sharedStrings.xml'
            $strings=@($xml.SelectNodes('//*[local-name()="si"]')|ForEach-Object {
                ($_.SelectNodes('.//*[local-name()="t"]')|ForEach-Object InnerText) -join ''
            })
        }
        $xml=Read-OfficePart $zip 'xl/worksheets/sheet1.xml'
        $cells=@{}
        foreach($cell in $xml.SelectNodes('//*[local-name()="c"]')) {
            $valueNode=$cell.SelectSingleNode('./*[local-name()="v"]')
            $value=if($valueNode){$valueNode.InnerText}else{''}
            if($cell.GetAttribute('t') -eq 's'){$value=$strings[[int]$value]}
            elseif($cell.GetAttribute('t') -eq 'inlineStr'){$value=($cell.SelectNodes('.//*[local-name()="t"]')|ForEach-Object InnerText) -join ''}
            $formula=$cell.SelectSingleNode('./*[local-name()="f"]')
            $cells[$cell.GetAttribute('r')]=@{value=$value;formula=$(if($formula){$formula.InnerText}else{''})}
        }
        return $cells
    } finally {if($zip){$zip.Dispose()};$stream.Dispose()}
}
