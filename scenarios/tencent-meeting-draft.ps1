# Assisted scenario: operate only an already observed schedule form; never submit.
param(
 [Parameter(Mandatory)][long]$ScheduleHwnd,
 [Parameter(Mandatory)][ValidateNotNullOrEmpty()][string]$Topic,
 [Parameter(Mandatory)][ValidatePattern('^\d{4}/\d{1,2}/\d{1,2}$')][string]$ExpectedDate,
 [string]$SetDate,
 [string]$Actl=$env:ACTL
)
$ErrorActionPreference='Stop'
. "$PSScriptRoot/../fixtures/system-fixture.ps1"
if(-not $Actl){$Actl="$PSScriptRoot/../target/validation/release/actl.exe"}
$base=if($env:ACTL_TEST_DIR){$env:ACTL_TEST_DIR}else{"$PSScriptRoot/../target"}
$root=Join-Path $base ('meeting-draft-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root -Force|Out-Null
$root=(Resolve-Path -LiteralPath $root).Path
$c=@{root=$root;binary=(Resolve-Path -LiteralPath $Actl).Path;calls=[Collections.Generic.List[object]]::new();uncertain=$false;app="hwnd:$ScheduleHwnd"}
$report=@{passed=$false;submitted=$false;reason='';scope='topic_and_date_only';remaining_review=@('time','duration','timezone','repeat','calendar','invitees');screenshot=$null}
$code=1
try{
 if(Test-Path -LiteralPath "$env:LOCALAPPDATA/actl/stop-requested"){$code=77;throw 'Emergency stop active'}
 $windows=@((Invoke-FixtureCli $c @('list-windows')).windows|Where-Object hwnd -EQ $ScheduleHwnd)
 Assert-Fixture ($windows.Count -eq 1 -and $windows[0].class -eq 'schedule_meeting::ScheduleMeetingDialog' -and $windows[0].title -eq '预定会议') 'Expected schedule form not found'
 $process=Get-Process -Id $windows[0].pid
 Assert-Fixture ($process.ProcessName -eq 'WeMeetApp') 'Unexpected schedule process'
 $null=Invoke-FixtureCli $c @('snapshot','--app',$c.app)
 # Read first: QDateEdit SetValue may silently do nothing.
 $null=Invoke-FixtureCli $c @('verify','--value','name:开始日期',$ExpectedDate,'--app',$c.app)
 $finalDate=$ExpectedDate
 if($SetDate){
  $date=[DateTime]::ParseExact($SetDate,'yyyy/M/d',[Globalization.CultureInfo]::InvariantCulture)
  $finalDate=$date.ToString('yyyy/M/d',[Globalization.CultureInfo]::InvariantCulture)
  # The first type takes focus; replace the complete date instead of one Qt date segment.
  $null=Invoke-FixtureCli $c @('type','name:开始日期',$finalDate,'--paste','--app',$c.app)
  $null=Invoke-FixtureCli $c @('press','ctrl+a','--app',$c.app)
  $null=Invoke-FixtureCli $c @('paste','--app',$c.app)
  $null=Invoke-FixtureCli $c @('press','tab','--app',$c.app)
  $null=Invoke-FixtureCli $c @('verify','--value','name:开始日期',$finalDate,'--app',$c.app)
 }
 $null=Invoke-FixtureCli $c @('set-value','name:请输入会议主题',$Topic,'--app',$c.app)
 $null=Invoke-FixtureCli $c @('verify','--value','name:请输入会议主题',$Topic,'--app',$c.app)
 $null=Invoke-FixtureCli $c @('verify','--value','name:开始日期',$finalDate,'--app',$c.app)
 $report.screenshot=Invoke-FixtureCli $c @('screenshot','--app',$c.app)
 $code=0
}catch{$report.reason=$_.Exception.Message}
finally{
 $report.passed=$code -eq 0;$report.exit_code=$code;$report.uncertain=$c.uncertain
 $report.calls=$c.calls.ToArray();$report.window_retained=$true
 $report|ConvertTo-Json -Depth 12|Set-Content -LiteralPath "$root/result.json" -Encoding utf8
 Write-Host "Draft report: $root/result.json"
}
exit $code
