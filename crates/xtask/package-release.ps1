param(
    [Parameter(Mandatory)][ValidatePattern('^[A-Za-z0-9][A-Za-z0-9._-]*$')][string]$Version,
    [Parameter(Mandatory)][string]$BinaryDir,
    [Parameter(Mandatory)][string]$OutputDir
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot/../..").Path
$binaryRoot = (Resolve-Path -LiteralPath $BinaryDir).Path
$null = New-Item -ItemType Directory -Force -Path $OutputDir
$outputRoot = (Resolve-Path -LiteralPath $OutputDir).Path
$base = "actl_${Version}_windows_amd64"
$stage = Join-Path $root ('target/release-package-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $stage
foreach ($name in @('actl.exe', 'actl-signal.exe')) {
    Copy-Item -LiteralPath (Join-Path $binaryRoot $name) -Destination $stage
}
foreach ($name in @('README.md', 'LICENSE', 'skills')) {
    Copy-Item -LiteralPath (Join-Path $root $name) -Destination $stage -Recurse
}
# Fail early if the skill entry or its required references disappear from the source.
foreach ($required in @('skills/actl/SKILL.md', 'skills/actl/PACKS.md', 'skills/actl/apps', 'skills/actl/references/error-codes.md', 'skills/actl/workflows/README.md', 'skills/actl/references/flows.md', 'skills/actl/references/task-records.md')) {
    if (-not (Test-Path -LiteralPath (Join-Path $stage $required))) { throw "Package missing: $required" }
}
$archive = Join-Path $outputRoot "$base.zip"
Compress-Archive -Path "$stage/*" -DestinationPath $archive
# Check the actual archive, not just the staging directory, before making checksums.
& "$PSScriptRoot/tests/release-package.Tests.ps1" -Archive $archive
Copy-Item -LiteralPath (Join-Path $binaryRoot 'actl.exe') -Destination (Join-Path $outputRoot "$base.exe")
$checksums = foreach ($name in @("$base.exe", "$base.zip")) {
    $hash = (Get-FileHash -LiteralPath (Join-Path $outputRoot $name) -Algorithm SHA256).Hash.ToLowerInvariant()
    "$hash  $name"
}
$checksums | Set-Content -LiteralPath (Join-Path $outputRoot 'checksums.txt') -Encoding utf8NoBOM
Write-Output "Package ready: $archive"
