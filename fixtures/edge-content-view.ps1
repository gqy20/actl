param([string]$Actl=$env:ACTL)
& "$PSScriptRoot/browser-probe.ps1" -Browser edge -Scenario content-view -Actl $Actl
exit $LASTEXITCODE
