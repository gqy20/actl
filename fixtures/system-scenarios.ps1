param($Context)
$ErrorActionPreference = 'Stop'
$baseline = @((Invoke-FixtureCli $Context @('list-windows')).windows | Where-Object { $_.PSObject.Properties['hwnd'] } | ForEach-Object hwnd)
switch ($Context.case) {
    'golden-task-1' {
        $runs = 3
        if ($env:RUNS) {
            Assert-Fixture ([int]::TryParse($env:RUNS,[ref]$runs) -and $runs -ge 1 -and $runs -le 100) 'RUNS must be 1..100'
        }
        $package = Get-AppxPackage Microsoft.WindowsCalculator | Select-Object -First 1
        Assert-Fixture ($null -ne $package) 'Windows Calculator package is not installed'
        $program = Join-Path $package.InstallLocation 'CalculatorApp.exe'
        Assert-Fixture (Test-Path -LiteralPath $program -PathType Leaf) 'Cannot locate CalculatorApp.exe'
        $Context.application = @{name='Calculator';version=[string]$package.Version;program=$program}
        $activationStarted = [DateTime]::UtcNow
        $null = Invoke-FixtureCli $Context @('launch',(Join-Path $env:WINDIR 'explorer.exe'),"shell:AppsFolder\$($package.PackageFamilyName)!App")
        $null = Invoke-FixtureCli $Context @('wait','--window','计算器','--timeout','12000')
        $windows = (Invoke-FixtureCli $Context @('list-windows')).windows
        $candidates = @(Get-FixtureCandidates $windows $baseline '计算器' | Where-Object {
            $process = Get-Process -Id $_.pid -ErrorAction Stop
            $process.Path -eq $program -and $process.StartTime.ToUniversalTime() -ge $activationStarted
        })
        Assert-Fixture ($candidates.Count -eq 1) 'Calculator ownership is not unique'
        $activated = Get-Process -Id $candidates[0].pid -ErrorAction Stop
        Assert-Fixture ($activated.Path -eq $program -and $activated.StartTime.ToUniversalTime() -ge $activationStarted) 'Calculator process identity does not match this activation'
        Save-FixtureOwner $Context $candidates[0] '计算器'
        $buttons = @('num1Button','num2Button','num3Button','multiplyButton','num4Button','num5Button','num6Button','plusButton','num7Button','num8Button','num9Button','equalButton')
        for ($round=1; $round -le $runs; $round++) {
            $timer = [Diagnostics.Stopwatch]::StartNew()
            $null = Invoke-FixtureCli $Context @('click','id:clearButton','--app',$Context.app)
            $snapshot = Invoke-FixtureCli $Context @('snapshot','--app',$Context.app,'--skeleton')
            foreach ($button in $buttons) {
                $targets = @($snapshot.elements | Where-Object { $_.PSObject.Properties['automation_id'] -and $_.automation_id -eq $button })
                Assert-Fixture ($targets.Count -eq 1 -and [bool]$targets[0].ref) "Missing unique button: $button (standard mode required)"
                $null = Invoke-FixtureCli $Context @('click',$targets[0].ref,'--app',$Context.app)
            }
            $null = Invoke-FixtureCli $Context @('verify','--value','id:CalculatorResults','显示为 56,877','--property','name','--app',$Context.app)
            $Context.rounds += @{round=$round;passed=$true;elapsed_ms=$timer.ElapsedMilliseconds}
        }
    }
    'win32-table-extract' {
        $program = Join-Path $env:WINDIR 'System32/msinfo32.exe'
        $Context.application = @{name='System Information';program=$program;version=(Get-Item -LiteralPath $program).VersionInfo.FileVersion}
        $launched = Invoke-FixtureCli $Context @('launch',$program)
        $null = Invoke-FixtureCli $Context @('wait','--window','系统信息','--timeout','20000')
        $candidates = @(Get-FixtureCandidates (Invoke-FixtureCli $Context @('list-windows')).windows $baseline '系统信息' $launched.pid)
        Assert-Fixture ($candidates.Count -eq 1) 'System Information ownership is not unique'
        Save-FixtureOwner $Context $candidates[0] '系统信息'
        $null = Invoke-FixtureCli $Context @('wait','--element','role:List','--app',$Context.app,'--timeout','10000')
        $table = Invoke-FixtureCli $Context @('extract','role:List','--app',$Context.app)
        Assert-FixtureSummaryTable $table
    }
    'virtualized-list' {
        $marker = 'actl-vlist-' + [guid]::NewGuid().ToString('N')
        $directory = Join-Path $Context.root $marker
        New-Item -ItemType Directory -Path $directory | Out-Null
        $Context.artifacts += $directory # Retain evidence; never recursively delete a computed directory.
        1..60 | ForEach-Object { [IO.File]::WriteAllText((Join-Path $directory ('f{0:d2}_padding.txt' -f $_)),"pad $_") }
        $program = Join-Path $env:WINDIR 'explorer.exe'
        $Context.application = @{name='Explorer';program=$program;version=(Get-Item -LiteralPath $program).VersionInfo.FileVersion}
        $null = Invoke-FixtureCli $Context @('launch',$program,"/n,$directory")
        $null = Invoke-FixtureCli $Context @('wait','--window',$marker,'--timeout','12000')
        $candidates = @(Get-FixtureCandidates (Invoke-FixtureCli $Context @('list-windows')).windows $baseline $marker)
        Assert-Fixture ($candidates.Count -eq 1) 'Explorer reused a pre-existing HWND or did not expose the owned folder'
        Save-FixtureOwner $Context $candidates[0] $marker
        $target = 'zzz_late_' + [guid]::NewGuid().ToString('N') + '.txt'
        [IO.File]::WriteAllText((Join-Path $directory $target),'late content')
        $found = Invoke-FixtureCli $Context @('find','--name',$target,'--app',$Context.app)
        Assert-Fixture ($found.PSObject.Properties['resolved_by'] -and $found.resolved_by -eq 'item-container') 'L4 path was not exercised; ordinary visibility does not prove virtualized lookup'
        Assert-Fixture (@($found.elements | Where-Object name -EQ $target).Count -eq 1) 'Wrong virtualized file'
        $null = Invoke-FixtureCli $Context @('get',"name:$target",'--property','name','--app',$Context.app)
        $null = Invoke-FixtureCli $Context @('verify','--value',"name:$target",$target,'--property','name','--app',$Context.app)
    }
    default { throw "Unknown system fixture: $($Context.case)" }
}
