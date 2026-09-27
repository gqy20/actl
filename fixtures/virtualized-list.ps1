param([string]$Actl = $env:ACTL)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/system-fixture.ps1"
exit (Invoke-SystemFixture -Case 'virtualized-list' -Actl $Actl)