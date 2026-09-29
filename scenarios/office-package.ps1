# Synthetic Office seed files. No Office COM automation or user documents.
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
