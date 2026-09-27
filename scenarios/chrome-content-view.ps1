# Real installed application lane; not a CI dependency.
param([string]$Actl=$env:ACTL)
& "$PSScriptRoot/../fixtures/browser-probe.ps1" -Browser chrome -Scenario content-view -Actl $Actl
exit $LASTEXITCODE
