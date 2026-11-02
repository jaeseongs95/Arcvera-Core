[CmdletBinding()]
param(
    [string]$UpdaterPath,
    [string]$WorkRoot = ('D:\codex\build\opendiy-r33-updater-tests-' + [Guid]::NewGuid().ToString('N'))
)

# Native PowerShell 5.1 integration tests. Only the HTTP transport is replaced;
# package/ZIP validation, independent-process locking, and selection use real files.
# C1=one selector, C2=daemon preserved, C3=revision identity, C4=bounded/untrusted input.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $UpdaterPath) { $UpdaterPath = Join-Path $PSScriptRoot 'update.ps1' }
$UpdaterPath = [IO.Path]::GetFullPath($UpdaterPath)
$WorkRoot = [IO.Path]::GetFullPath($WorkRoot)
if (-not $WorkRoot.StartsWith('D:\codex\build\', [StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $WorkRoot)) { throw 'Tests require a new directory under D:\codex\build\.' }
New-Item -ItemType Directory -Path $WorkRoot | Out-Null
. $UpdaterPath
$script:TestResults = New-Object 'System.Collections.Generic.List[object]'
$script:Fixtures = @{}
$script:Requests = New-Object 'System.Collections.Generic.List[string]'
$script:OnArchiveRequest = $null
$script:TransportFailure = $false
$script:TestUtf8 = New-Object Text.UTF8Encoding($false)

function Assert-Equal($Actual, $Expected, [string]$Why) {
    if ($Actual -cne $Expected) { throw "$Why (actual=$Actual expected=$Expected)" }
}
function Assert-Fails([scriptblock]$Run, [string]$Code) {
    $caught = $false
    try { & $Run | Out-Null } catch {
        $caught = $true
        Assert-Equal $_.Exception.Data['UpdateCode'] $Code 'Failure must identify the real boundary'
    }
    if (-not $caught) { throw "Expected failure: $Code" }
}
function Test-Case([string]$Name, [scriptblock]$Run) {
    try {
        & $Run
        $script:TestResults.Add(@{ name = $Name; status = 'PASS' })
        Write-Host "PASS $Name"
    } catch {
        $script:TestResults.Add(@{ name = $Name; status = 'FAIL'; error = $_.Exception.Message })
        Write-Host "FAIL $Name : $($_.Exception.Message)"
        throw
    }
}
function Write-TestJson([string]$Path, $Value) { [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 12), $script:TestUtf8) }
function New-TestPackage([int]$Revision, [string]$Name = '', [string]$Version = '') {
    if (-not $Name) { $Name = "package-r$Revision" }
    $path = Join-Path $WorkRoot $Name
    New-Item -ItemType Directory -Path (Join-Path $path 'bin'), (Join-Path $path 'updater'), (Join-Path $path 'tools') | Out-Null
    if (-not $Version) { $version = "0.166.0+native-r$Revision.20261102" }
    [IO.File]::WriteAllText((Join-Path $path 'bin/codex.exe'), "Not executable; daemon requests must never run. Revision $Revision", $script:TestUtf8)
    [IO.File]::WriteAllText((Join-Path $path 'bin/codex-app-server.exe'), 'Not executable; standalone server must not be launched.', $script:TestUtf8)
    $launcherFixture = if ($Revision -eq 33) { 'where.exe' } else { 'hostname.exe' }
    Copy-Item -LiteralPath (Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::System)) $launcherFixture) -Destination (Join-Path $path 'tools/opendiy-codex-launcher.exe')
    Write-TestJson (Join-Path $path 'codex-package.json') @{ version = $version; target = 'x86_64-pc-windows-msvc'; entrypoint = 'bin/codex.exe'; opendiyUpdate = @{ schema = 1; minLauncher = 1 } }
    Copy-Item -LiteralPath $UpdaterPath -Destination (Join-Path $path 'updater/update.ps1')
    Copy-Item -LiteralPath (Join-Path ([IO.Path]::GetDirectoryName($UpdaterPath)) 'update.vbs') -Destination (Join-Path $path 'updater/update.vbs')
    $files = @(Get-ChildItem -LiteralPath $path -Recurse -File | Sort-Object FullName | ForEach-Object { @{ path = $_.FullName.Substring($path.Length + 1).Replace('\', '/'); sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() } })
    $receipt = @{ sourceCommit = ('1' * 40); sourceFreezeSha256 = ('2' * 64); binarySha256 = (Get-FileHash -LiteralPath (Join-Path $path 'bin/codex.exe') -Algorithm SHA256).Hash.ToLowerInvariant(); version = "codex-cli $version"; files = $files }
    Write-TestJson (Join-Path $path 'package-receipt.json') $receipt
    return @{ root = $path; manifest = (Get-FileHash -LiteralPath (Join-Path $path 'package-receipt.json') -Algorithm SHA256).Hash.ToLowerInvariant(); version = $version }
}
function New-TestArchive($Package, [string]$Name, [string]$Extra = '', [int]$Attributes = 0) {
    $archive = Join-Path $WorkRoot $Name
    $zip = [IO.Compression.ZipFile]::Open($archive, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($item in Get-ChildItem -LiteralPath $Package.root -Recurse -File) {
            $relative = $item.FullName.Substring($Package.root.Length + 1).Replace('\', '/')
            [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $item.FullName, $relative) | Out-Null
        }
        if ($Extra) {
            $entry = $zip.CreateEntry($Extra)
            $entry.ExternalAttributes = $Attributes
            $stream = New-Object IO.StreamWriter($entry.Open())
            try { $stream.Write('malicious fixture') } finally { $stream.Dispose() }
        }
    } finally { $zip.Dispose() }
    return $archive
}
function Set-TestRelease([int]$Revision, [string]$Archive, [string]$Tag = '') {
    if (-not $Tag) { $Tag = "v0.166.0-r$Revision.20261102" }
    $name = "opendiy-codex-r$Revision-windows-x86_64.zip"
    $prefix = "https://github.com/jaeseongs95/opendiy-codex/releases/download/$Tag/"
    $digest = (Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash.ToLowerInvariant()
    $metadata = @{ tag_name = $Tag; draft = $false; prerelease = $false; assets = @(@{ name = $name; browser_download_url = "$prefix$name"; digest = "sha256:$digest" }, @{ name = 'SHA256SUMS.txt'; browser_download_url = "${prefix}SHA256SUMS.txt" }) }
    $metadataPath = Join-Path $WorkRoot 'fixture-release.json'
    Write-TestJson $metadataPath $metadata
    $sums = Join-Path $WorkRoot 'fixture-SHA256SUMS.txt'
    [IO.File]::WriteAllText($sums, "$digest  $name`n", $script:TestUtf8)
    $script:Fixtures = @{ $script:FeedUri = $metadataPath; "$prefix$name" = $Archive; "${prefix}SHA256SUMS.txt" = $sums }
}
function Invoke-FeedRequest([uri]$Uri, [string]$Destination, [long]$MaximumBytes) {
    Assert-RemoteUri $Uri
    if ($script:TransportFailure) { Stop-Update 'download_failed' 'RAW PRIVATE RESPONSE https://example.invalid/?token=secret' }
    $script:Requests.Add($Uri.AbsoluteUri)
    if (-not $script:Fixtures.ContainsKey($Uri.AbsoluteUri)) { throw "Fixture does not allow request: $Uri" }
    if ($Uri.AbsolutePath.EndsWith('.zip') -and $script:OnArchiveRequest) { & $script:OnArchiveRequest }
    $source = $script:Fixtures[$Uri.AbsoluteUri]
    if ((Get-Item -LiteralPath $source).Length -gt $MaximumBytes) { Stop-Update 'download_too_large' 'Fixture size exceeds request budget.' }
    Copy-Item -LiteralPath $source -Destination $Destination
}

$bootstrap = New-TestPackage 33
$next = New-TestPackage 34
$future = New-TestPackage 35
$archive34 = New-TestArchive $next 'r34.zip'
$install = Join-Path $WorkRoot 'installation with spaces'
$bootstrapDestination = Join-Path $install 'versions/bootstrap-r33'
New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($bootstrapDestination)) -Force | Out-Null
[IO.Directory]::Move($bootstrap.root, $bootstrapDestination)
$bootstrap.root = $bootstrapDestination
Copy-Item -LiteralPath (Join-Path $bootstrap.root 'tools/opendiy-codex-launcher.exe') -Destination (Join-Path $install 'opendiy-codex-launcher.exe')
$pointerPath = Join-Path $install 'current-package.json'
$sentinel = Join-Path $WorkRoot 'daemon-and-user-data.txt'
[IO.File]::WriteAllText($sentinel, 'daemon PID, auth, settings and DB are outside updater scope', $script:TestUtf8)
$sentinelDigest = Get-FileDigest $sentinel
$exitCode = 0
try {
    Test-Case 'C2-C3 automatic R33 to R34 selects the verified next CLI without executing it' {
        Set-TestRelease 34 $archive34
        $result = Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest
        Assert-Equal $result.status 'updated' 'Automatic update must commit the next launch selection'
        Assert-Equal $result.cli.revision 34 'Build metadata revision must advance on the same official base'
        Assert-Equal $result.daemon.updateRequested $false 'No daemon update may be requested'
        $pointer = [IO.File]::ReadAllText($pointerPath) | ConvertFrom-Json
        Assert-Equal $pointer.packageRoot $result.cli.selectedPackageRoot 'Launcher pointer must select the actual package'
        Assert-Equal (Get-FileDigest (Join-Path $pointer.packageRoot 'bin/codex.exe')) (Get-FileDigest (Join-Path $next.root 'bin/codex.exe')) 'Selected bytes must be the new fixture'
        Assert-Equal (Get-FileDigest $sentinel) $sentinelDigest 'External data must be unchanged'
    }
    $selected = [IO.File]::ReadAllText($pointerPath) | ConvertFrom-Json
    Test-Case 'C1-C3 replay and both scheduler bootstrap and current CLI inputs are stable' {
        $before = [IO.File]::ReadAllText($pointerPath)
        $result = Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest
        Assert-Equal $result.status 'current' 'Identical revision must be a no-op'
        Assert-Equal ([IO.File]::ReadAllText($pointerPath)) $before 'No-op must not rewrite selection'
        $result = Invoke-OpenDiyUpdate 'Status' $install $selected.packageRoot $selected.manifestSha256
        Assert-Equal $result.cli.revision 34 'A new selected CLI can invoke its updater'
        $result = Invoke-OpenDiyUpdate 'Status' $install $bootstrap.root $bootstrap.manifest
        Assert-Equal $result.cli.revision 34 'Pinned scheduler bootstrap must resolve current selection'
    }
    Test-Case 'C3 downgrade and same-revision mutation preserve the pointer' {
        $before = [IO.File]::ReadAllText($pointerPath)
        Set-TestRelease 33 (New-TestArchive $bootstrap 'r33.zip')
        Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } 'downgrade_refused'
        $changed = New-TestArchive $next 'changed-r34.zip' 'unlisted.txt'
        Set-TestRelease 34 $changed
        Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } 'revision_mutated'
        Assert-Equal ([IO.File]::ReadAllText($pointerPath)) $before 'Failed revisions must preserve selection'
    }
    Test-Case 'C4 hostile archive paths, duplicate names, links and unlisted files are rejected' {
        $before = [IO.File]::ReadAllText($pointerPath)
        $cases = @(@('../escaped.txt', 0, 'unsafe_path'), @('BIN/CODEX.EXE', 0, 'archive_duplicate'), @('link', [int]0xA1FF0000, 'archive_link'), @('unlisted.txt', 0, 'unlisted_file'))
        $i = 0
        foreach ($case in $cases) {
            $hostile = New-TestArchive $future "hostile-$i.zip" $case[0] $case[1]
            Set-TestRelease 35 $hostile
            Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } $case[2]
            Assert-Equal ([IO.File]::ReadAllText($pointerPath)) $before 'Rejected ZIP must preserve selection'
            $i++
        }
        foreach ($required in @('bin/codex-app-server.exe', 'tools/opendiy-codex-launcher.exe')) {
            $incomplete = New-TestArchive $future ("incomplete-$i.zip")
            $zip = [IO.Compression.ZipFile]::Open($incomplete, [IO.Compression.ZipArchiveMode]::Update)
            try { $zip.GetEntry($required).Delete() } finally { $zip.Dispose() }
            Set-TestRelease 35 $incomplete
            Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } 'incomplete_package'
            Assert-Equal ([IO.File]::ReadAllText($pointerPath)) $before 'Incomplete CLI/server/launcher package must preserve selection'
            $i++
        }
        Assert-Equal (Test-Path -LiteralPath (Join-Path $WorkRoot 'escaped.txt')) $false 'Path traversal must not write outside stage'
    }
    Test-Case 'C4 asset origin, archive digest and package digest mismatches are distinct' {
        $before = [IO.File]::ReadAllText($pointerPath)
        $good = New-TestArchive $future 'r35-good.zip'
        Set-TestRelease 35 $good
        $metadata = [IO.File]::ReadAllText($script:Fixtures[$script:FeedUri]) | ConvertFrom-Json
        $metadata.assets[0].browser_download_url = 'https://github.com/other/repo/releases/download/r35/untrusted.zip'
        Write-TestJson $script:Fixtures[$script:FeedUri] $metadata
        Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } 'untrusted_origin'
        Set-TestRelease 35 $good
        $assetUri = @($script:Fixtures.Keys | Where-Object { $_.EndsWith('.zip') })[0]
        $script:Fixtures[$assetUri] = $archive34
        Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } 'archive_digest_mismatch'
        [IO.File]::AppendAllText((Join-Path $future.root 'bin/codex.exe'), 'changed after receipt')
        Set-TestRelease 35 (New-TestArchive $future 'r35-bad-package.zip')
        Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } 'file_mismatch'
        Assert-Equal ([IO.File]::ReadAllText($pointerPath)) $before 'Bad origins and digests must preserve selection'
    }
    Test-Case 'C1 failed atomic pointer replacement preserves the selected package' {
        $before = [IO.File]::ReadAllText($pointerPath)
        $next36 = New-TestPackage 36
        Set-TestRelease 36 (New-TestArchive $next36 'r36.zip')
        $script:PointerGuard = $null
        $script:OnArchiveRequest = { $script:PointerGuard = [IO.File]::Open($pointerPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read) }
        try { Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } 'selection_write_failed' }
        finally { if ($script:PointerGuard) { $script:PointerGuard.Dispose() }; $script:OnArchiveRequest = $null }
        Assert-Equal ([IO.File]::ReadAllText($pointerPath)) $before 'Atomic replacement failure must preserve old pointer bytes'
    }
    Test-Case 'C3 package admission matches native Cargo versions, exact receipt paths and release tags' {
        $before = [IO.File]::ReadAllText($pointerPath)
        $badVersion = New-TestPackage 38 'bad-cargo-version' '0.166.0-r38.20261102'
        Set-TestRelease 38 (New-TestArchive $badVersion 'bad-cargo-version.zip')
        Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } 'invalid_package_version'
        Assert-Equal ([IO.File]::ReadAllText($pointerPath)) $before 'Non-native Cargo format must be rejected before selection'
        $badCase = New-TestPackage 38 'bad-receipt-case'
        $receiptPath = Join-Path $badCase.root 'package-receipt.json'
        $receipt = [IO.File]::ReadAllText($receiptPath) | ConvertFrom-Json
        ($receipt.files | Where-Object { $_.path -ceq 'bin/codex.exe' }).path = 'BIN/CODEX.EXE'
        Write-TestJson $receiptPath $receipt
        Set-TestRelease 38 (New-TestArchive $badCase 'bad-receipt-case.zip')
        Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } 'incomplete_package'
        Assert-Equal ([IO.File]::ReadAllText($pointerPath)) $before 'Wrong required path case must be rejected before selection'
        $wrongTag = New-TestPackage 38 'wrong-release-tag' '0.167.0+native-r38.20261103'
        Set-TestRelease 38 (New-TestArchive $wrongTag 'wrong-release-tag.zip')
        Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } 'release_identity_mismatch'
        Assert-Equal ([IO.File]::ReadAllText($pointerPath)) $before 'Full version/tag mismatch must preserve selection'
        $i = 0
        foreach ($base in @('0.167.0', '0.167.0-alpha.1')) {
            $package = New-TestPackage 38 "future-base-$i" "$base+native-r38.20261103"
            Set-TestRelease 38 (New-TestArchive $package "future-base-$i.zip") "v$base-r38.20261103"
            $check = Invoke-OpenDiyUpdate 'Check' $install $bootstrap.root $bootstrap.manifest
            Assert-Equal $check.status 'available' 'New stable and alpha Cargo bases must remain supported'
            Assert-Equal $check.available.version "$base+native-r38.20261103" 'Full Cargo version must be preserved'
            $i++
        }
        Assert-Equal ([IO.File]::ReadAllText($pointerPath)) $before 'Admission checks must not change actual selection'
    }
    Test-Case 'C3 unsupported or missing package and launcher contracts preserve the selected pointer' {
        $before = [IO.File]::ReadAllText($pointerPath)
        $contracts = @(
            @{ name = 'missing-contract'; value = $null },
            @{ name = 'schema-two'; value = @{ schema = 2; minLauncher = 1 } },
            @{ name = 'launcher-two'; value = @{ schema = 1; minLauncher = 2 } },
            @{ name = 'missing-launcher'; value = @{ schema = 1 } },
            @{ name = 'missing-schema'; value = @{ minLauncher = 1 } },
            @{ name = 'string-launcher'; value = @{ schema = 1; minLauncher = '1' } },
            @{ name = 'boolean-schema'; value = @{ schema = $true; minLauncher = 1 } }
        )
        foreach ($contract in $contracts) {
            $package = New-TestPackage 39 $contract.name
            $metadataPath = Join-Path $package.root 'codex-package.json'
            $metadata = [IO.File]::ReadAllText($metadataPath) | ConvertFrom-Json
            if ($null -eq $contract.value) { $metadata.PSObject.Properties.Remove('opendiyUpdate') }
            else { $metadata.opendiyUpdate = $contract.value }
            Write-TestJson $metadataPath $metadata
            $receiptPath = Join-Path $package.root 'package-receipt.json'
            $receipt = [IO.File]::ReadAllText($receiptPath) | ConvertFrom-Json
            ($receipt.files | Where-Object { $_.path -ceq 'codex-package.json' }).sha256 = Get-FileDigest $metadataPath
            Write-TestJson $receiptPath $receipt
            Set-TestRelease 39 (New-TestArchive $package ($contract.name + '.zip'))
            Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } 'unsupported_package_contract'
            Assert-Equal ([IO.File]::ReadAllText($pointerPath)) $before 'Unsupported bootstrap requirements must be rejected before pointer selection'
        }
    }
    Test-Case 'C1 independent Windows process excludes a concurrent update' {
        $lastBefore = [IO.File]::ReadAllText((Join-Path $install 'last-update.json'))
        $ready = Join-Path $WorkRoot 'lock.ready'
        $release = Join-Path $WorkRoot 'lock.release'
        $workerPath = Join-Path $WorkRoot 'lock-worker.ps1'
        $worker = @'
param($LockPath, $ReadyPath, $ReleasePath)
$lock = [IO.File]::Open($LockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
try {
    [IO.File]::WriteAllText($ReadyPath, 'ready')
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    while (-not [IO.File]::Exists($ReleasePath) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 30 }
} finally { $lock.Dispose() }
'@
        [IO.File]::WriteAllText($workerPath, $worker, $script:TestUtf8)
        $arguments = '-NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $workerPath + '" "' + (Join-Path $install 'update.lock') + '" "' + $ready + '" "' + $release + '"'
        $process = Start-Process -FilePath (Join-Path $env:WINDIR 'System32/WindowsPowerShell/v1.0/powershell.exe') -ArgumentList $arguments -WindowStyle Hidden -PassThru
        try {
            $deadline = [DateTime]::UtcNow.AddSeconds(10)
            while (-not [IO.File]::Exists($ready) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 30 }
            if (-not [IO.File]::Exists($ready)) { throw 'Lock worker did not become ready.' }
            Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } 'update_busy'
            Assert-Equal ([IO.File]::ReadAllText((Join-Path $install 'last-update.json'))) $lastBefore 'Lock contention must not overwrite a real operation result'
        } finally {
            [IO.File]::WriteAllText($release, 'release')
            if (-not $process.WaitForExit(10000)) { throw 'Fixture lock worker did not exit.' }
            $process.Dispose()
        }
    }
    Test-Case 'C2 error is visible through Status and a later success replaces it without private response data' {
        $script:TransportFailure = $true
        try { Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest } 'download_failed' }
        finally { $script:TransportFailure = $false }
        $status = Invoke-OpenDiyUpdate 'Status' $install $bootstrap.root $bootstrap.manifest
        Assert-Equal $status.cli.revision 34 'Status must retain the actual pointer fact'
        Assert-Equal $status.lastResult.status 'error' 'A previous failed check must remain visible'
        Assert-Equal $status.lastResult.error.code 'download_failed' 'Status must expose the failure reason'
        $lastText = [IO.File]::ReadAllText((Join-Path $install 'last-update.json'))
        if ($lastText.Contains('secret') -or $lastText.Contains('PRIVATE') -or $lastText.Contains('example.invalid')) { throw 'Private transport text leaked into state.' }
        Set-TestRelease 34 $archive34
        Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest | Out-Null
        $status = Invoke-OpenDiyUpdate 'Status' $install $bootstrap.root $bootstrap.manifest
        Assert-Equal $status.lastResult.status 'current' 'Later success must atomically replace the last failure'
        if (-not $status.lastResult.recordedAtUtc.EndsWith('Z')) { throw 'Result timestamp must be UTC.' }
        $before = [IO.File]::ReadAllText((Join-Path $install 'last-update.json'))
        Assert-Fails { Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root ('0' * 64) } 'manifest_mismatch'
        Assert-Equal ([IO.File]::ReadAllText((Join-Path $install 'last-update.json'))) $before 'Invalid bootstrap input must not overwrite operation state'
    }
    Test-Case 'C2 schedule contract uses one hidden user action and supports disable (scheduler mocked)' {
        $script:Scheduled = $null
        $script:Disabled = $false
        $script:ScheduleReadFailure = $false
        $script:ScheduleWrites = 0
        function Get-ScheduledTask { param($TaskName, $TaskPath, $ErrorAction, $ErrorVariable); if ($script:ScheduleReadFailure) { throw 'Fixture scheduler unavailable.' }; return $script:Scheduled }
        function New-ScheduledTaskAction { param($Execute, $Argument); return [pscustomobject]@{ Execute = $Execute; Arguments = $Argument } }
        function New-ScheduledTaskTrigger { param([switch]$Once, $At, $RepetitionInterval); return [pscustomobject]@{ Repetition = [pscustomobject]@{ Interval = [Xml.XmlConvert]::ToString($RepetitionInterval) } } }
        function New-ScheduledTaskSettingsSet { param($MultipleInstances, $ExecutionTimeLimit, [switch]$StartWhenAvailable); return @{} }
        function Register-ScheduledTask {
            param($TaskName, $Action, $Trigger, $Settings, $User, $RunLevel, $Description, [switch]$Force)
            Assert-Equal $RunLevel 'Limited' 'Scheduled update must run without elevation'
            $script:ScheduleWrites++
            $script:Scheduled = [pscustomobject]@{ TaskName = $TaskName; State = 'Ready'; Triggers = @($Trigger); Actions = @($Action); Principal = [pscustomobject]@{ UserId = $User; RunLevel = $RunLevel } }
        }
        function Disable-ScheduledTask { param($TaskName, $TaskPath); Assert-Equal $TaskName $script:Scheduled.TaskName 'Disable must address the same task'; $script:Disabled = $true; $script:Scheduled.State = 'Disabled'; $script:ScheduleWrites++ }
        $status = Invoke-OpenDiyUpdate 'Status' $install $bootstrap.root $bootstrap.manifest
        Assert-Equal $status.schedule.state 'not_registered' 'A missing task must be reported independently of last result'
        $enabled = Invoke-OpenDiyUpdate 'Enable' $install $selected.packageRoot $selected.manifestSha256 60
        Assert-Equal $enabled.status 'enabled' 'Enable must configure the updater'
        Assert-Equal $script:Scheduled.Actions[0].Execute (Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::System)) 'wscript.exe') 'Exactly one wscript action is required'
        if (-not $script:Scheduled.Actions[0].Arguments.Contains('update.vbs')) { throw 'Hidden action must use the VBS runner.' }
        if (-not $script:Scheduled.Actions[0].Arguments.Contains('"-Dispatch"')) { throw 'Task must dispatch the currently selected helper through the stable launcher.' }
        $status = Invoke-OpenDiyUpdate 'Status' $install $bootstrap.root $bootstrap.manifest
        Assert-Equal $status.schedule.state 'enabled' 'Status must observe the live enabled task'
        Assert-Equal $status.schedule.intervalMinutes 60 'Status must report the actual repetition interval'
        Assert-Equal $status.schedule.taskName $enabled.taskName 'Read and write must use the same task identity'
        Set-TestRelease 34 $archive34
        Invoke-OpenDiyUpdate 'Update' $install $bootstrap.root $bootstrap.manifest | Out-Null
        $status = Invoke-OpenDiyUpdate 'Status' $install $bootstrap.root $bootstrap.manifest
        Assert-Equal $status.lastResult.status 'current' 'A check result must replace the previous enable result'
        Assert-Equal $status.schedule.state 'enabled' 'A check result must not hide the actual enabled task'
        Assert-Equal $script:ScheduleWrites 1 'Status and Update must not modify task state'
        $disabled = Invoke-OpenDiyUpdate 'Disable' $install $bootstrap.root $bootstrap.manifest 60
        Assert-Equal $disabled.taskName $enabled.taskName 'Enable and disable must use one installation identity'
        Assert-Equal $script:Disabled $true 'Disable must be implemented'
        $status = Invoke-OpenDiyUpdate 'Status' $install $bootstrap.root $bootstrap.manifest
        Assert-Equal $status.schedule.state 'disabled' 'Status must observe the live disabled task'
        $script:Scheduled.Principal.UserId = 'another-user'
        $status = Invoke-OpenDiyUpdate 'Status' $install $bootstrap.root $bootstrap.manifest
        Assert-Equal $status.schedule.state 'conflict' 'A different task owner must not be reported as this updater'
        $script:ScheduleReadFailure = $true
        $status = Invoke-OpenDiyUpdate 'Status' $install $bootstrap.root $bootstrap.manifest
        Assert-Equal $status.schedule.state 'unavailable' 'Query failure must not be reported as an absent or disabled task'
        Assert-Equal $script:ScheduleWrites 2 'Read-only schedule status must never register or disable tasks'
        Assert-Equal (Get-FileDigest (Join-Path $install 'opendiy-codex-launcher.exe')) (Get-FileDigest (Join-Path $bootstrap.root 'tools/opendiy-codex-launcher.exe')) 'Enable from a new package must preserve the bootstrap launcher'
    }
    Test-Case 'C2 native script JSON and hidden VBS runner preserve daemon state' {
        $pointerBeforeStatus = Get-FileDigest $pointerPath
        $resultBeforeStatus = Get-FileDigest (Join-Path $install 'last-update.json')
        $systemPowerShell = Join-Path $env:WINDIR 'System32/WindowsPowerShell/v1.0/powershell.exe'
        $output = & $systemPowerShell -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $UpdaterPath -Action Status -InstallationRoot $install -PackageRoot $bootstrap.root -ManifestSha256 $bootstrap.manifest
        Assert-Equal $LASTEXITCODE 0 'Native CLI process must succeed'
        $json = $output | ConvertFrom-Json
        Assert-Equal $json.cli.revision 34 'JSON must resolve the selected package'
        Assert-Equal $json.daemon.updateRequested $false 'JSON must explicitly preserve daemon'
        if ($json.schedule.state -notin @('not_registered', 'unavailable')) { throw 'Native fixture query must not find a registered task.' }
        if (-not $json.schedule.taskName.StartsWith('OpenDIY-Codex-Update-')) { throw 'Native fixture query must use the installation task identity.' }
        Assert-Equal (Get-FileDigest $pointerPath) $pointerBeforeStatus 'Native Status must not modify the pointer'
        Assert-Equal (Get-FileDigest (Join-Path $install 'last-update.json')) $resultBeforeStatus 'Native Status must not modify operation state'
        & (Join-Path $env:WINDIR 'System32/cscript.exe') //nologo (Join-Path ([IO.Path]::GetDirectoryName($UpdaterPath)) 'update.vbs') -Action Status -InstallationRoot $install -PackageRoot $bootstrap.root -ManifestSha256 $bootstrap.manifest
        Assert-Equal $LASTEXITCODE 0 'VBS must forward paths with spaces and child exit code'
        $oldPath = $env:PATH
        Push-Location $WorkRoot
        try {
            $env:PATH = $WorkRoot
            & (Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::System)) 'cscript.exe') //nologo (Join-Path ([IO.Path]::GetDirectoryName($UpdaterPath)) 'update.vbs') -Dispatch $install
            Assert-Equal $LASTEXITCODE 1 'Dispatcher must run the fixed fixture launcher (where.exe update returns 1)'
        } finally { $env:PATH = $oldPath; Pop-Location }
        Assert-Equal (Get-FileDigest $sentinel) $sentinelDigest 'User data sentinel must remain unchanged'
    }
    Test-Case 'C1-C3 a later update atomically replaces an existing pointer and preserves bootstrap identity' {
        $latest = New-TestPackage 37
        Set-TestRelease 37 (New-TestArchive $latest 'r37.zip')
        $result = Invoke-OpenDiyUpdate 'Update' $install $selected.packageRoot $selected.manifestSha256
        Assert-Equal $result.status 'updated' 'Replacing an existing pointer must succeed'
        $pointer = [IO.File]::ReadAllText($pointerPath) | ConvertFrom-Json
        Assert-Equal $pointer.revision 37 'The pointer must advance to the new CLI'
        Assert-Equal $pointer.bootstrapManifestSha256 $bootstrap.manifest 'Bootstrap manifest binding must remain stable'
        Assert-Equal $pointer.bootstrapPackageRoot $bootstrap.root 'Bootstrap package binding must remain stable'
        $status = Invoke-OpenDiyUpdate 'Status' $install $bootstrap.root $bootstrap.manifest
        Assert-Equal $status.lastResult.status 'updated' 'A successful selection must be the latest diagnostic result'
        Assert-Equal $status.cli.revision 37 'Status must read the actual new pointer'
    }
} catch { $exitCode = 1; Write-Host $_.ScriptStackTrace }
finally {
    $receipt = @{ schema = 1; updaterSha256 = (Get-FileHash -LiteralPath $UpdaterPath -Algorithm SHA256).Hash.ToLowerInvariant(); testSha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant(); powershellVersion = $PSVersionTable.PSVersion.ToString(); workRoot = $WorkRoot; exitCode = $exitCode; cases = @($script:TestResults.ToArray()); excluded = @('production network, redirects and rate limiting', 'real scheduled task registration (contract tested with command mocks)', 'real Codex executable and user profile'); transport = 'local file fixtures; fixed-origin validation retained' }
    Write-TestJson (Join-Path $WorkRoot 'test.receipt.json') $receipt
    Write-Host "Receipt: $WorkRoot\test.receipt.json"
}
exit $exitCode
