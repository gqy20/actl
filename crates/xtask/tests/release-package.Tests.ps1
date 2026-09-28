param([string]$Archive)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot/../../..").Path
Add-Type -AssemblyName System.IO.Compression.FileSystem
function Assert-Package([string]$Path) {
    $zip = [IO.Compression.ZipFile]::OpenRead($Path)
    try {
        $names = @($zip.Entries | ForEach-Object { $_.FullName.Replace('\', '/') })
        if ($names | Where-Object { $_ -like 'docs/*' }) { throw 'Full docs directory must not be packaged' }
        foreach ($required in @('actl.exe', 'actl-signal.exe', 'README.md', 'LICENSE', 'skills/actl/SKILL.md', 'skills/actl/PACKS.md', 'skills/actl/references/error-codes.md', 'skills/actl/workflows/README.md', 'skills/actl/references/flows.md', 'skills/actl/references/task-records.md')) {
            if ($required -notin $names) { throw "Package missing: $required" }
        }
        foreach ($folder in @('skills')) {
            foreach ($file in Get-ChildItem -LiteralPath "$root/$folder" -Recurse -File) {
                if ((Get-Content -LiteralPath $file.FullName -Raw) -match '(?i)([a-z][a-z0-9+.-]*://|www\.|mailto:)') {
                    throw "Skill contains an external URL: $($file.FullName)"
                }
                $relative = [IO.Path]::GetRelativePath($root, $file.FullName).Replace('\', '/')
                if ($relative -notin $names) { throw "Package missing: $relative" }
                $entry = $zip.Entries | Where-Object { $_.FullName.Replace('\', '/') -eq $relative }
                $stream = $entry.Open()
                $sha = [Security.Cryptography.SHA256]::Create()
                try { $actual = [Convert]::ToHexString($sha.ComputeHash($stream)) }
                finally { $stream.Dispose(); $sha.Dispose() }
                if ($actual -ne (Get-FileHash -LiteralPath $file.FullName).Hash) { throw "Content mismatch: $relative" }
            }
        }
        # A skill copied out of the ZIP must not depend on sibling repository folders.
        foreach ($entry in $zip.Entries | Where-Object { $_.FullName.Replace('\', '/') -like 'skills/actl/*.md' }) {
            $reader = [IO.StreamReader]::new($entry.Open())
            try { $markdown = $reader.ReadToEnd() } finally { $reader.Dispose() }
            foreach ($match in [regex]::Matches($markdown, '\]\(([^)]+)\)')) {
                $link = $match.Groups[1].Value
                if ($link -match '^#') { continue }
                $link = ($link -split '#', 2)[0]
                $entryPath = $entry.FullName.Replace('\', '/')
                $resolved = [IO.Path]::GetFullPath((Join-Path (Split-Path (Join-Path $root $entryPath)) $link))
                $relative = [IO.Path]::GetRelativePath($root, $resolved).Replace('\', '/')
                if (-not $relative.StartsWith('skills/actl/') -or $relative -notin $names) {
                    throw "Skill link escapes package or is missing: $entryPath -> $link"
                }
            }
        }
    } finally { $zip.Dispose() }
}
if ($Archive) {
    Assert-Package $Archive
    Write-Output 'Release archive contents verified'
    exit 0
}
$testDir = Join-Path $root ('target/package-tests-' + [guid]::NewGuid().ToString('N'))
$binaryDir = Join-Path $testDir 'bin'
$null = New-Item -ItemType Directory -Path $binaryDir
# Content-only fixtures; this test never executes either fake binary.
Set-Content -LiteralPath "$binaryDir/actl.exe" -Value 'cli fixture'
Set-Content -LiteralPath "$binaryDir/actl-signal.exe" -Value 'signal fixture'
& "$root/crates/xtask/package-release.ps1" -Version test -BinaryDir $binaryDir -OutputDir "$testDir/dist"
Assert-Package "$testDir/dist/actl_test_windows_amd64.zip"
if (@(Get-ChildItem -LiteralPath "$testDir/dist").Count -ne 3) { throw 'Unexpected release assets' }
foreach ($line in Get-Content -LiteralPath "$testDir/dist/checksums.txt") {
    $parts = $line -split '\s+', 2
    if ((Get-FileHash -LiteralPath "$testDir/dist/$($parts[1])").Hash -ne $parts[0]) { throw 'Invalid package checksum' }
}
$old = Join-Path $testDir 'old.zip'
Compress-Archive -Path "$binaryDir/*" -DestinationPath $old
$rejected = $false
try { Assert-Package $old } catch { $rejected = $_.Exception.Message -like 'Package missing:*' }
if (-not $rejected) { throw 'Incomplete archive was accepted' }
Write-Output 'Release packaging preserves standalone skills and rejects incomplete archives'
