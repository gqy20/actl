$ErrorActionPreference='Stop'
[Console]::InputEncoding=[Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
try {
 $request=[Console]::In.ReadToEnd()|ConvertFrom-Json
 if($request.kind -eq 'process'){
  $process=Get-CimInstance Win32_Process -Filter ('ProcessId='+[uint32]$request.path)
  if(-not $process){[Console]::Write('{"state":"absent"}');exit 0}
  if(-not $process.ExecutablePath -or -not $process.CommandLine -or -not $process.CreationDate){throw 'process unavailable'}
  [Console]::Write((@{program=$process.ExecutablePath;command_line=$process.CommandLine;start=$process.CreationDate.ToUniversalTime().ToString('o')}|ConvertTo-Json -Compress))
  exit 0
 }
 if($request.kind -notin @('docx','xlsx')){throw 'kind'}
 Add-Type -AssemblyName System.IO.Compression
 $stream=[IO.File]::Open($request.path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
 $zip=[IO.Compression.ZipArchive]::new($stream,[IO.Compression.ZipArchiveMode]::Read)
 function Read-Part([string]$Name){
  $entry=$zip.GetEntry($Name)
  if(-not $entry -or $entry.Length -gt 4194304){throw 'part missing or oversized'}
  $settings=[Xml.XmlReaderSettings]::new();$settings.DtdProcessing=[Xml.DtdProcessing]::Prohibit;$settings.XmlResolver=$null;$settings.MaxCharactersInDocument=4194304
  $source=$entry.Open();$reader=[Xml.XmlReader]::Create($source,$settings)
  try{$xml=[Xml.XmlDocument]::new();$xml.XmlResolver=$null;$xml.Load($reader);return ,$xml}finally{$reader.Dispose();$source.Dispose()}
 }
 try {
  if($request.kind -eq 'docx'){
   $xml=Read-Part 'word/document.xml'
   $paragraphs=@($xml.SelectNodes('//*[local-name()="body"]//*[local-name()="p"]')|ForEach-Object {($_.SelectNodes('.//*[local-name()="t"]')|ForEach-Object InnerText)-join ''})
   $runs=@($xml.SelectNodes('//*[local-name()="body"]//*[local-name()="r"][*[local-name()="t"]]'))
   $bold=$runs.Count -gt 0
   foreach($run in $runs){$b=$run.SelectSingleNode('./*[local-name()="rPr"]/*[local-name()="b"]');if(-not $b -or $b.GetAttribute('val','http://schemas.openxmlformats.org/wordprocessingml/2006/main') -in @('0','false','off')){$bold=$false}}
   $result=@{text=($paragraphs -join "`n");bold=$bold}
  }else{
   $strings=@();if($zip.GetEntry('xl/sharedStrings.xml')){$shared=Read-Part 'xl/sharedStrings.xml';$strings=@($shared.SelectNodes('//*[local-name()="si"]')|ForEach-Object {($_.SelectNodes('.//*[local-name()="t"]')|ForEach-Object InnerText)-join ''})}
   $xml=Read-Part 'xl/worksheets/sheet1.xml';$result=@{}
   foreach($cell in $xml.SelectNodes('//*[local-name()="c"]')){
    $v=$cell.SelectSingleNode('./*[local-name()="v"]');$value=if($v){$v.InnerText}else{''}
    if($cell.GetAttribute('t') -eq 's'){$index=[int]$value;if($index -lt 0 -or $index -ge $strings.Count){throw 'bad shared string'};$value=$strings[$index]}
    elseif($cell.GetAttribute('t') -eq 'inlineStr'){$value=($cell.SelectNodes('.//*[local-name()="t"]')|ForEach-Object InnerText)-join ''}
    $formula=$cell.SelectSingleNode('./*[local-name()="f"]')
    $result[$cell.GetAttribute('r')]=@{value=[string]$value;formula=$(if($formula){$formula.InnerText}else{''})}
   }
  }
  $json=ConvertTo-Json -InputObject $result -Depth 30 -Compress
  if([Text.Encoding]::UTF8.GetByteCount($json) -gt 4194304){throw 'output oversized'}
  [Console]::Write($json)
 }finally{$zip.Dispose();$stream.Dispose()}
}catch{exit 1}
