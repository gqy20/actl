param([string]$Actl=$env:ACTL)
& "$PSScriptRoot/browser-probe.ps1" -Browser edge -Scenario deep-tree -Actl $Actl
exit $LASTEXITCODE
