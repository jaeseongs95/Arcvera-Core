# Modified for OpenDIY Codex R33 (2026-11-03); see MODIFICATIONS.md.
[CmdletBinding()]
param(
    [ValidateSet('Update', 'Check', 'Status', 'Enable', 'Disable')][string]$Action = 'Update',
    [string]$InstallationRoot,
    [string]$PackageRoot,
    [string]$ManifestSha256,
    [ValidateRange(5, 1440)][int]$IntervalMinutes = 60
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$script:Repository = 'jaeseongs95/opendiy-codex'
$script:FeedUri = "https://api.github.com/repos/$script:Repository/releases/latest"
$script:Utf8 = New-Object System.Text.UTF8Encoding($false)

function Stop-Update([string]$Code, [string]$Message) {
    $failure = New-Object System.InvalidOperationException($Message)
    $failure.Data['UpdateCode'] = $Code
    throw $failure
}

function Get-FileDigest([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-Revision([string]$Version) {
    $matches = [regex]::Matches($Version, '(?i)(?:^|[+.-])r([0-9]+)(?=$|[+.-])')
    $revision = 0
    if ($matches.Count -ne 1 -or -not [int]::TryParse($matches[0].Groups[1].Value, [ref]$revision) -or $revision -le 0) {
        Stop-Update 'invalid_revision' "Expected exactly one positive rNN revision: $Version"
    }
    return $revision
}

function Get-LocalRoot([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path) -or $Path -notmatch '^[A-Za-z]:[\\/]' -or $Path -match '["\x00-\x1f]') {
        Stop-Update 'invalid_root' 'An absolute local Windows path is required.'
    }
    $full = [IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
    if ($full.Length -lt 4) { Stop-Update 'invalid_root' 'A drive root cannot be an installation.' }
    $ancestor = $full
    while ($ancestor) {
        if (Test-Path -LiteralPath $ancestor) {
            if ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                Stop-Update 'reparse_point' "Reparse points are not supported: $ancestor"
            }
        }
        $parent = [IO.Path]::GetDirectoryName($ancestor)
        if ($parent -eq $ancestor) { break }
        $ancestor = $parent
    }
    return $full
}

function Get-SafeRelativePath([string]$Path) {
    if ([string]::IsNullOrEmpty($Path) -or $Path.Contains('\') -or $Path.StartsWith('/') -or $Path -match '[<>:"|?*\x00-\x1f]') {
        Stop-Update 'unsafe_path' "Unsafe package path: $Path"
    }
    foreach ($part in $Path.Split('/')) {
        if ($part -eq '' -or $part -eq '.' -or $part -eq '..' -or $part -match '[. ]$' -or $part -match '^(?i:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)') {
            Stop-Update 'unsafe_path' "Unsafe package path: $Path"
        }
    }
    return $Path
}

function Read-Package([string]$Root, [string]$ExpectedManifest) {
    $Root = Get-LocalRoot $Root
    $receiptPath = Join-Path $Root 'package-receipt.json'
    if (-not (Test-Path -LiteralPath $receiptPath -PathType Leaf)) { Stop-Update 'missing_receipt' 'Package receipt is missing.' }
    $null = Get-LocalRoot $receiptPath
    if ((Get-Item -LiteralPath $receiptPath).Length -gt 4MB) { Stop-Update 'invalid_receipt' 'Package receipt is too large.' }
    $manifestDigest = Get-FileDigest $receiptPath
    if ($ExpectedManifest -and $manifestDigest -cne $ExpectedManifest.ToLowerInvariant()) {
        Stop-Update 'manifest_mismatch' 'Package receipt does not match the supplied manifest SHA-256.'
    }
    $receipt = [IO.File]::ReadAllText($receiptPath) | ConvertFrom-Json
    if ($receipt.sourceCommit -notmatch '^[a-fA-F0-9]{40}$' -or $receipt.sourceFreezeSha256 -notmatch '^[a-fA-F0-9]{64}$' -or $receipt.binarySha256 -notmatch '^[a-fA-F0-9]{64}$') {
        Stop-Update 'invalid_receipt' 'Package source or binary identity is invalid.'
    }
    $version = ([string]$receipt.version) -creplace '^codex-cli ', ''
    if ($version -cnotmatch '^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-alpha(?:\.(?:0|[1-9][0-9]*)){0,2})?\+native-r[1-9][0-9]*\.[0-9]{8}$') {
        Stop-Update 'invalid_package_version' 'Package version must be a stable or alpha Cargo base plus native-rNN and an eight-digit date.'
    }
    $revision = Get-Revision $version
    $files = @{}
    foreach ($entry in $receipt.files) {
        $relative = Get-SafeRelativePath ([string]$entry.path)
        if ($relative -ieq 'package-receipt.json' -or $files.ContainsKey($relative) -or $entry.sha256 -notmatch '^[a-fA-F0-9]{64}$') {
            Stop-Update 'invalid_receipt' "Duplicate or invalid receipt entry: $relative"
        }
        $files[$relative] = ([string]$entry.sha256).ToLowerInvariant()
    }
    $receiptPaths = @($receipt.files | ForEach-Object { [string]$_.path })
    foreach ($required in @('codex-package.json', 'bin/codex.exe', 'bin/codex-app-server.exe', 'tools/opendiy-codex-launcher.exe', 'updater/update.ps1', 'updater/update.vbs')) {
        if ($receiptPaths -cnotcontains $required) { Stop-Update 'incomplete_package' "Required exact receipt path is missing: $required" }
    }
    $directories = New-Object 'System.Collections.Generic.Queue[string]'
    $directories.Enqueue($Root)
    $actualFiles = New-Object 'System.Collections.Generic.List[object]'
    while ($directories.Count) {
        foreach ($item in Get-ChildItem -LiteralPath $directories.Dequeue() -Force) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { Stop-Update 'reparse_point' 'A package contains a reparse point.' }
        if ($item.PSIsContainer) { $directories.Enqueue($item.FullName); continue }
        $actualFiles.Add($item)
        $relative = $item.FullName.Substring($Root.Length + 1).Replace('\', '/')
        if ($relative -ieq 'package-receipt.json') { continue }
        if (-not $files.ContainsKey($relative)) { Stop-Update 'unlisted_file' "File is outside the receipt: $relative" }
        if ((Get-FileDigest $item.FullName) -cne $files[$relative]) { Stop-Update 'file_mismatch' "Package hash mismatch: $relative" }
        }
    }
    foreach ($relative in $files.Keys) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $relative) -PathType Leaf)) { Stop-Update 'incomplete_package' "Missing package file: $relative" }
    }
    $package = [IO.File]::ReadAllText((Join-Path $Root 'codex-package.json')) | ConvertFrom-Json
    if ($package.target -cne 'x86_64-pc-windows-msvc') { Stop-Update 'unsupported_target' 'Only Windows x86_64 packages are supported.' }
    if ($package.entrypoint -cne 'bin/codex.exe' -or $package.version -cne $version -or $files['bin/codex.exe'] -cne ([string]$receipt.binarySha256).ToLowerInvariant()) {
        Stop-Update 'package_identity_mismatch' 'Package metadata and receipt identity differ.'
    }
    $contract = $package.PSObject.Properties['opendiyUpdate']
    if (-not $contract -or $contract.Value -isnot [pscustomobject]) { Stop-Update 'unsupported_package_contract' 'OpenDIY update schema and launcher requirements are missing.' }
    foreach ($member in @('schema', 'minLauncher')) {
        $field = $contract.Value.PSObject.Properties[$member]
        if (-not $field -or ($field.Value -isnot [int] -and $field.Value -isnot [long]) -or $field.Value -ne 1) {
            Stop-Update 'unsupported_package_contract' 'Only integer update schema 1 and launcher requirement 1 are supported.'
        }
    }
    return [ordered]@{ packageRoot = $Root; manifestSha256 = $manifestDigest; revision = $revision; version = $version; sourceCommit = $receipt.sourceCommit }
}

function Assert-RemoteUri([uri]$Uri, [bool]$Redirect = $false) {
    if ($Uri.Scheme -cne 'https' -or -not $Uri.IsDefaultPort -or $Uri.UserInfo -or $Uri.Fragment) {
        Stop-Update 'untrusted_origin' 'Updater requests require the fixed HTTPS GitHub origin.'
    }
    $valid = ($Uri.AbsoluteUri -ceq $script:FeedUri) -or
        ($Uri.Host -ceq 'github.com' -and $Uri.AbsolutePath.StartsWith("/$script:Repository/releases/download/", [StringComparison]::Ordinal)) -or
        ($Redirect -and $Uri.Host -ceq 'release-assets.githubusercontent.com')
    if (-not $valid) { Stop-Update 'untrusted_origin' 'Release asset or redirect is outside the fixed repository origin.' }
}

# The only transport seam; fixture tests replace it without changing validation or selection.
function Invoke-FeedRequest([uri]$Uri, [string]$Destination, [long]$MaximumBytes) {
    Assert-RemoteUri $Uri
    for ($attempt = 0; $attempt -lt 2; $attempt++) {
        try {
            $next = $Uri
            for ($redirects = 0; $redirects -le 3; $redirects++) {
                Assert-RemoteUri $next ($redirects -gt 0)
                $request = [Net.HttpWebRequest]::Create($next)
                $request.AllowAutoRedirect = $false
                $request.Timeout = 30000
                $request.ReadWriteTimeout = 30000
                $request.UserAgent = 'OpenDIY-Codex-Updater/1'
                $request.Accept = 'application/vnd.github+json'
                $response = $request.GetResponse()
                try {
                    $code = [int]$response.StatusCode
                    if ($code -ge 300 -and $code -lt 400) {
                        if ($redirects -eq 3 -or -not $response.Headers['Location']) { Stop-Update 'redirect_limit' 'Release download exceeded its redirect limit.' }
                        $next = New-Object Uri($next, $response.Headers['Location'])
                        continue
                    }
                    if ($code -ne 200) { Stop-Update 'http_failure' "Unexpected HTTP status $code." }
                    if ($response.ContentLength -gt $MaximumBytes) { Stop-Update 'download_too_large' 'Release download exceeds its size limit.' }
                    $inputStream = $response.GetResponseStream()
                    $outputStream = [IO.File]::Open($Destination, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::None)
                    try {
                        $buffer = New-Object byte[] 65536
                        [long]$total = 0
                        $deadline = [DateTime]::UtcNow.AddMinutes(5)
                        while (($count = $inputStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                            $total += $count
                            if ($total -gt $MaximumBytes -or [DateTime]::UtcNow -gt $deadline) { Stop-Update 'download_limit' 'Release download exceeded its byte or time limit.' }
                            $outputStream.Write($buffer, 0, $count)
                        }
                    } finally { $outputStream.Dispose(); $inputStream.Dispose() }
                    return
                } finally { $response.Dispose() }
            }
        } catch [Net.WebException] {
            $response = $_.Exception.Response
            $httpCode = if ($response) { [int]$response.StatusCode } else { 0 }
            if ($response) { $response.Dispose() }
            if ($attempt -eq 1 -or ($httpCode -ge 400 -and $httpCode -lt 500 -and $httpCode -ne 429)) {
                Stop-Update 'download_failed' "Release download failed (HTTP $httpCode): $($_.Exception.Status)"
            }
            Start-Sleep -Milliseconds 250
        }
    }
}

function Expand-CheckedArchive([string]$Archive, [string]$Destination) {
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        $paths = @{}
        [long]$expanded = 0
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName.TrimEnd('/')
            $null = Get-SafeRelativePath $name
            $unixType = ($entry.ExternalAttributes -shr 16) -band 0xf000
            if (($entry.FullName.EndsWith('/') -and $entry.Length -ne 0) -or ($unixType -eq 0x4000 -and -not $entry.FullName.EndsWith('/'))) {
                Stop-Update 'invalid_archive_entry' 'Archive directory metadata and contents differ.'
            }
            if (($unixType -ne 0 -and $unixType -ne 0x8000 -and $unixType -ne 0x4000) -or ($entry.ExternalAttributes -band 0x400)) {
                Stop-Update 'archive_link' 'Archive links and special files are not supported.'
            }
            if ($paths.ContainsKey($name)) { Stop-Update 'archive_duplicate' "Duplicate archive path: $name" }
            $paths[$name] = $true
            $expanded += $entry.Length
            if ($expanded -gt 3GB -or $entry.Length -gt 1GB -or $paths.Count -gt 20000) { Stop-Update 'archive_limit' 'Archive exceeds its extraction limit.' }
        }
        foreach ($entry in $zip.Entries) {
            $path = Join-Path $Destination $entry.FullName
            if ($entry.FullName.EndsWith('/')) { New-Item -ItemType Directory -Path $path -Force | Out-Null; continue }
            New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($path)) -Force | Out-Null
            $inputStream = $entry.Open()
            $outputStream = [IO.File]::Open($path, [IO.FileMode]::CreateNew)
            try { $inputStream.CopyTo($outputStream) } finally { $outputStream.Dispose(); $inputStream.Dispose() }
        }
    } finally { $zip.Dispose() }
    return Read-Package $Destination ''
}

function Write-Selection([string]$Root, $Selected, [string]$BootstrapDigest, [string]$BootstrapRoot) {
    $pointer = [ordered]@{ schema = 1; installationRoot = $Root; bootstrapManifestSha256 = $BootstrapDigest; bootstrapPackageRoot = $BootstrapRoot }
    foreach ($key in $Selected.Keys) { $pointer[$key] = $Selected[$key] }
    $path = Join-Path $Root 'current-package.json'
    $temporary = Join-Path $Root ('.selection.' + [Guid]::NewGuid().ToString('N'))
    try {
        [IO.File]::WriteAllText($temporary, ($pointer | ConvertTo-Json -Depth 8), $script:Utf8)
        if ([IO.File]::Exists($path)) { [IO.File]::Replace($temporary, $path, [NullString]::Value) }
        else { [IO.File]::Move($temporary, $path) }
    } catch {
        Stop-Update 'selection_write_failed' "Could not atomically select the verified package: $($_.Exception.Message)"
    } finally { if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) } }
}

function Read-Selection([string]$Root, $Bootstrap) {
    $path = Join-Path $Root 'current-package.json'
    if (-not [IO.File]::Exists($path)) { return $Bootstrap }
    $null = Get-LocalRoot $path
    $pointer = [IO.File]::ReadAllText($path) | ConvertFrom-Json
    $initialRoot = Get-LocalRoot ([string]$pointer.bootstrapPackageRoot)
    $versions = Join-Path $Root 'versions'
    if ([IO.Path]::GetDirectoryName($initialRoot) -ine $versions) { Stop-Update 'bootstrap_outside_root' 'Bootstrap package must be a direct child of this installation versions folder.' }
    $initialInput = $pointer.bootstrapManifestSha256 -ceq $Bootstrap.manifestSha256 -and $initialRoot -ieq $Bootstrap.packageRoot
    $selectedInput = $pointer.manifestSha256 -ceq $Bootstrap.manifestSha256 -and $pointer.packageRoot -ieq $Bootstrap.packageRoot
    if ($pointer.schema -ne 1 -or $pointer.installationRoot -ine $Root -or (-not $initialInput -and -not $selectedInput) -or $pointer.bootstrapManifestSha256 -notmatch '^[a-f0-9]{64}$') {
        Stop-Update 'selection_binding_mismatch' 'Selection is bound to another installation or bootstrap manifest.'
    }
    $selectedRoot = Get-LocalRoot ([string]$pointer.packageRoot)
    if ([IO.Path]::GetDirectoryName($selectedRoot) -ine $versions) {
        Stop-Update 'selection_outside_root' 'Selected package must be a direct child of this installation versions folder.'
    }
    $selected = Read-Package $selectedRoot ([string]$pointer.manifestSha256)
    if ($selected.revision -ne $pointer.revision -or $selected.version -cne $pointer.version -or $selected.sourceCommit -cne $pointer.sourceCommit) {
        Stop-Update 'selection_identity_mismatch' 'Selection and package identity differ.'
    }
    if ($pointer.PSObject.Properties['archiveSha256']) { $selected['archiveSha256'] = [string]$pointer.archiveSha256 }
    return $selected
}

function Get-UpdateResult([string]$Status, $Selected) {
    $cli = [ordered]@{ selectedPackageRoot = $Selected.packageRoot }
    foreach ($key in $Selected.Keys) { $cli[$key] = $Selected[$key] }
    return [ordered]@{ schema = 1; status = $Status; cli = $cli; daemon = @{ status = 'preserved'; updateRequested = $false } }
}

function Get-UpdateFailure($Failure) {
    $code = $Failure.Exception.Data['UpdateCode']
    if (-not $code) { $code = 'update_failed' }
    # Persist only a stable reason code; raw HTTP exceptions and responses may contain URL tokens.
    $errorInfo = @{ code = $code; message = "OpenDIY package update failed ($code)." }
    if ($Failure.Exception.Data['StateWriteFailed']) { $errorInfo['stateWriteFailed'] = $true }
    return [ordered]@{ schema = 1; status = 'error'; daemon = @{ status = 'preserved'; updateRequested = $false }; error = $errorInfo }
}

function Write-LastResult([string]$Root, $Result) {
    $path = Join-Path $Root 'last-update.json'
    $null = Get-LocalRoot $path
    $record = [ordered]@{ recordedAtUtc = [DateTime]::UtcNow.ToString('o') }
    foreach ($key in $Result.Keys) { $record[$key] = $Result[$key] }
    $temporary = Join-Path $Root ('.result.' + [Guid]::NewGuid().ToString('N'))
    try {
        [IO.File]::WriteAllText($temporary, ($record | ConvertTo-Json -Depth 12), $script:Utf8)
        if ([IO.File]::Exists($path)) { [IO.File]::Replace($temporary, $path, [NullString]::Value) }
        else { [IO.File]::Move($temporary, $path) }
    } catch { Stop-Update 'result_write_failed' 'Could not atomically record the update result.' }
    finally { if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) } }
}

function Complete-Update([string]$Root, $Result) {
    Write-LastResult $Root $Result
    return $Result
}

function Read-LastResult([string]$Root) {
    $path = Join-Path $Root 'last-update.json'
    if (-not [IO.File]::Exists($path)) { return $null }
    try {
        $null = Get-LocalRoot $path
        if ((Get-Item -LiteralPath $path).Length -gt 1MB) { throw 'Invalid last result size.' }
        $last = [IO.File]::ReadAllText($path) | ConvertFrom-Json
        if ($last.schema -ne 1 -or -not $last.recordedAtUtc) { throw 'Invalid last result.' }
        return $last
    } catch { return @{ status = 'unavailable'; error = @{ code = 'invalid_last_result'; message = 'The previous update result could not be read safely.' } } }
}

function Get-ScheduleDefinition([string]$Root) {
    $hash = [Security.Cryptography.SHA256]::Create()
    try { $identity = ([BitConverter]::ToString($hash.ComputeHash($script:Utf8.GetBytes($Root.ToLowerInvariant())))).Replace('-', '').Substring(0, 16) }
    finally { $hash.Dispose() }
    $taskName = "OpenDIY-Codex-Update-$identity"
    $stable = Join-Path $Root 'updater'
    $null = Get-LocalRoot $stable
    $wscript = Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::System)) 'wscript.exe'
    $arguments = '"' + (Join-Path $stable 'update.vbs') + '" "-Dispatch" "' + $Root + '"'
    $currentUser = [Security.Principal.WindowsIdentity]::GetCurrent()
    try { $user = $currentUser.Name; $sid = $currentUser.User.Value } finally { $currentUser.Dispose() }
    return @{ taskName = $taskName; stable = $stable; wscript = $wscript; arguments = $arguments; user = $user; sid = $sid }
}

function Assert-OwnedSchedule($Existing, $Definition) {
    if ($Existing -and (@($Existing.Actions).Count -ne 1 -or $Existing.Actions[0].Execute -ine $Definition.wscript -or $Existing.Actions[0].Arguments -cne $Definition.arguments)) {
        Stop-Update 'schedule_conflict' 'The installation task name belongs to a different action.'
    }
    if ($Existing) {
        # Task Scheduler can return DOMAIN\user, a local short name, or a SID
        # for the same identity. Compare resolved SIDs and fail closed otherwise.
        try {
            $owner = [string]$Existing.Principal.UserId
            if ($owner -match '^S-1-') {
                $ownerSid = (New-Object Security.Principal.SecurityIdentifier($owner)).Value
            } else {
                $ownerSid = (New-Object Security.Principal.NTAccount($owner)).Translate([Security.Principal.SecurityIdentifier]).Value
            }
        } catch {
            Stop-Update 'schedule_conflict' 'The installation task owner cannot be resolved.'
        }
        if ($ownerSid -ine $Definition.sid -or [string]$Existing.Principal.RunLevel -ne 'Limited') {
            Stop-Update 'schedule_conflict' 'The installation task belongs to a different user or privilege level.'
        }
    }
}

function Read-UpdateSchedule([string]$Root) {
    $definition = Get-ScheduleDefinition $Root
    $result = [ordered]@{ taskName = $definition.taskName; state = 'unavailable' }
    try {
        $queryErrors = @()
        $existing = Get-ScheduledTask -TaskName $definition.taskName -TaskPath '\' -ErrorAction SilentlyContinue -ErrorVariable queryErrors
        if (-not $existing) {
            if (@($queryErrors | Where-Object { $_.CategoryInfo.Category -ne 'ObjectNotFound' }).Count -eq 0) { $result.state = 'not_registered' }
            return $result
        }
        Assert-OwnedSchedule $existing $definition
        switch ([string]$existing.State) {
            'Disabled' { $result.state = 'disabled' }
            { $_ -in @('Ready', 'Running', 'Queued') } { $result.state = 'enabled' }
        }
        if (@($existing.Triggers).Count -eq 1 -and $existing.Triggers[0].Repetition.Interval) {
            try {
                $minutes = [Xml.XmlConvert]::ToTimeSpan([string]$existing.Triggers[0].Repetition.Interval).TotalMinutes
                if ($minutes -gt 0) { $result['intervalMinutes'] = $minutes }
            } catch {} # State remains observable even when the scheduler cannot supply an interval.
        }
    } catch {
        if ($_.Exception.Data['UpdateCode'] -eq 'schedule_conflict') { $result.state = 'conflict' }
    }
    return $result
}

function Set-UpdateSchedule([string]$Mode, [string]$Root, $Bootstrap, [int]$Minutes) {
    $definition = Get-ScheduleDefinition $Root
    $taskName, $stable, $wscript, $arguments, $user = $definition.taskName, $definition.stable, $definition.wscript, $definition.arguments, $definition.user
    $existing = Get-ScheduledTask -TaskName $taskName -TaskPath '\' -ErrorAction SilentlyContinue
    Assert-OwnedSchedule $existing $definition
    if ($Mode -eq 'Disable') {
        if ($existing) { Disable-ScheduledTask -TaskName $taskName -TaskPath '\' | Out-Null }
    } else {
        $stableLauncher = Join-Path $Root 'opendiy-codex-launcher.exe'
        $null = Get-LocalRoot $stableLauncher
        if (-not [IO.File]::Exists($stableLauncher) -or (Get-FileDigest $stableLauncher) -cne (Get-FileDigest (Join-Path $Bootstrap.packageRoot 'tools/opendiy-codex-launcher.exe'))) {
            Stop-Update 'bootstrap_launcher_mismatch' 'The stable launcher differs from the verified bootstrap package.'
        }
        New-Item -ItemType Directory -Path $stable -Force | Out-Null
        foreach ($name in @('update.ps1', 'update.vbs')) {
            Copy-Item -LiteralPath (Join-Path $Bootstrap.packageRoot "updater/$name") -Destination (Join-Path $stable $name) -Force
        }
        $action = New-ScheduledTaskAction -Execute $wscript -Argument $arguments
        $trigger = New-ScheduledTaskTrigger -Once -At ([DateTime]::Now.AddMinutes(1)) -RepetitionInterval ([TimeSpan]::FromMinutes($Minutes))
        $settings = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -ExecutionTimeLimit ([TimeSpan]::FromMinutes(30)) -StartWhenAvailable
        Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Settings $settings -User $user -RunLevel Limited -Description 'OpenDIY Codex package update; preserves running app servers.' -Force | Out-Null
    }
    return $taskName
}

function Invoke-OpenDiyUpdate([string]$Mode, [string]$RootPath, [string]$BootstrapPath, [string]$BootstrapManifest, [int]$Minutes = 60) {
    if ($BootstrapManifest -notmatch '^[a-fA-F0-9]{64}$') { Stop-Update 'invalid_manifest_digest' 'A bootstrap package receipt SHA-256 is required.' }
    $root = Get-LocalRoot $RootPath
    $bootstrap = Read-Package $BootstrapPath $BootstrapManifest
    if ([IO.Path]::GetDirectoryName($bootstrap.packageRoot) -ine (Join-Path $root 'versions')) { Stop-Update 'bootstrap_outside_root' 'The provided package must be a direct child of this installation versions folder.' }
    if ($root -ieq $bootstrap.packageRoot -or $root.StartsWith($bootstrap.packageRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
        Stop-Update 'invalid_root' 'Installation root must be outside its bootstrap package.'
    }
    if ($Mode -eq 'Status') {
        $result = Get-UpdateResult 'current' (Read-Selection $root $bootstrap)
        $result['lastResult'] = Read-LastResult $root
        $result['schedule'] = Read-UpdateSchedule $root
        return $result
    }
    New-Item -ItemType Directory -Path $root -Force | Out-Null
    $lock = $null
    $scratch = $null
    $operationStarted = $false
    try {
        $lockPath = Join-Path $root 'update.lock'
        $null = Get-LocalRoot $lockPath
        try { $lock = [IO.File]::Open($lockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None) }
        catch [IO.IOException] { Stop-Update 'update_busy' 'Another updater owns this installation.' }
        $selected = Read-Selection $root $bootstrap
        $bindingDigest = $bootstrap.manifestSha256
        $bindingRoot = $bootstrap.packageRoot
        $pointerPath = Join-Path $root 'current-package.json'
        if ([IO.File]::Exists($pointerPath)) {
            $binding = [IO.File]::ReadAllText($pointerPath) | ConvertFrom-Json
            $bindingDigest = $binding.bootstrapManifestSha256
            $bindingRoot = $binding.bootstrapPackageRoot
        }
        $operationStarted = $true
        if ($Mode -eq 'Enable' -or $Mode -eq 'Disable') {
            if ($Mode -eq 'Enable' -and -not [IO.File]::Exists($pointerPath)) { Write-Selection $root $selected $bindingDigest $bindingRoot }
            $initialPackage = $bootstrap
            if ($Mode -eq 'Enable') { $initialPackage = Read-Package $bindingRoot $bindingDigest }
            $taskName = Set-UpdateSchedule $Mode $root $initialPackage $Minutes
            $result = Get-UpdateResult $(if ($Mode -eq 'Enable') { 'enabled' } else { 'disabled' }) $selected
            $result['taskName'] = $taskName
            return Complete-Update $root $result
        }
        $scratch = Join-Path $root ('.update.' + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $scratch | Out-Null
        $metadataPath = Join-Path $scratch 'release.json'
        Invoke-FeedRequest $script:FeedUri $metadataPath 4MB
        $release = [IO.File]::ReadAllText($metadataPath) | ConvertFrom-Json
        if ($release.draft -or $release.prerelease) { Stop-Update 'unstable_release' 'Only published stable OpenDIY releases are eligible.' }
        $revision = Get-Revision ([string]$release.tag_name)
        if ($revision -lt $selected.revision) { Stop-Update 'downgrade_refused' 'The release feed is older than the selected OpenDIY revision.' }
        if ($revision -eq $selected.revision -and $release.tag_name -cne ('v' + $selected.version.Replace('+native-r', '-r'))) {
            Stop-Update 'release_identity_mismatch' 'Release tag differs from the selected Cargo package version.'
        }
        $assetName = "opendiy-codex-r$revision-windows-x86_64.zip"
        $assets = @($release.assets | Where-Object { $_.name -ceq $assetName })
        $checksums = @($release.assets | Where-Object { $_.name -ceq 'SHA256SUMS.txt' })
        if ($assets.Count -ne 1 -or $checksums.Count -ne 1) { Stop-Update 'missing_asset' 'Release must contain exactly one Windows x86_64 ZIP and SHA256SUMS.txt.' }
        $asset = $assets[0]
        $checksum = $checksums[0]
        $assetPrefix = "https://github.com/$script:Repository/releases/download/$([uri]::EscapeDataString([string]$release.tag_name))/"
        foreach ($pair in @(@($asset, $assetName), @($checksum, 'SHA256SUMS.txt'))) {
            $uri = [uri]$pair[0].browser_download_url
            Assert-RemoteUri $uri
            if ($uri.AbsoluteUri -cne ($assetPrefix + $pair[1])) { Stop-Update 'asset_origin_mismatch' 'Asset name, release tag, and download origin differ.' }
        }
        $checksumPath = Join-Path $scratch 'SHA256SUMS.txt'
        Invoke-FeedRequest $checksum.browser_download_url $checksumPath 1MB
        $lines = @([IO.File]::ReadAllLines($checksumPath) | Where-Object { $_ -cmatch ('^[a-fA-F0-9]{64} [ *]' + [regex]::Escape($assetName) + '$') })
        if ($lines.Count -ne 1) { Stop-Update 'invalid_checksums' 'Expected one exact ZIP entry in SHA256SUMS.txt.' }
        $archiveDigest = $lines[0].Substring(0, 64).ToLowerInvariant()
        if ($asset.PSObject.Properties['digest'] -and $asset.digest -and $asset.digest -cne "sha256:$archiveDigest") { Stop-Update 'asset_digest_mismatch' 'GitHub asset digest and checksum manifest differ.' }
        if ($revision -eq $selected.revision -and $selected.Contains('archiveSha256')) {
            if ($selected.archiveSha256 -cne $archiveDigest) { Stop-Update 'revision_mutated' 'An existing release revision changed its archive digest.' }
            return Complete-Update $root (Get-UpdateResult 'current' $selected)
        }
        $archive = Join-Path $scratch $assetName
        Invoke-FeedRequest $asset.browser_download_url $archive 1GB
        if ((Get-FileDigest $archive) -cne $archiveDigest) { Stop-Update 'archive_digest_mismatch' 'Downloaded ZIP differs from SHA256SUMS.txt.' }
        $stage = Join-Path $scratch 'package'
        New-Item -ItemType Directory -Path $stage | Out-Null
        $candidate = Expand-CheckedArchive $archive $stage
        if ($candidate.revision -ne $revision -or $release.tag_name -cne ('v' + $candidate.version.Replace('+native-r', '-r'))) {
            Stop-Update 'release_identity_mismatch' 'Release tag differs from the full Cargo package version.'
        }
        $candidate['archiveSha256'] = $archiveDigest
        if ($revision -eq $selected.revision) {
            if ($candidate.manifestSha256 -cne $selected.manifestSha256) { Stop-Update 'revision_mutated' 'An existing revision changed its package receipt.' }
            if ($Mode -eq 'Update') { $selected['archiveSha256'] = $archiveDigest; Write-Selection $root $selected $bindingDigest $bindingRoot }
            return Complete-Update $root (Get-UpdateResult 'current' $selected)
        }
        if ($Mode -eq 'Check') {
            $result = Get-UpdateResult 'available' $selected
            $result['available'] = $candidate
            $result.available['packageRoot'] = $null
            return Complete-Update $root $result
        }
        $versions = Join-Path $root 'versions'
        $null = Get-LocalRoot $versions
        New-Item -ItemType Directory -Path $versions -Force | Out-Null
        $destination = Join-Path $versions ("r$revision-" + $archiveDigest.Substring(0, 16) + '-' + [Guid]::NewGuid().ToString('N'))
        [IO.Directory]::Move($stage, $destination)
        $candidate['packageRoot'] = $destination
        # Selection is the commit point. Cleanup failure before it preserves the old selection.
        Remove-Item -LiteralPath $scratch -Recurse -Force
        $scratch = $null
        Write-Selection $root $candidate $bindingDigest $bindingRoot
        return Complete-Update $root (Get-UpdateResult 'updated' $candidate)
    } catch {
        $failure = $_
        if ($operationStarted) {
            try { Write-LastResult $root (Get-UpdateFailure $failure) }
            catch { $failure.Exception.Data['StateWriteFailed'] = $true }
        }
        throw $failure
    } finally {
        if ($scratch -and [IO.Directory]::Exists($scratch)) { Remove-Item -LiteralPath $scratch -Recurse -Force }
        if ($lock) { $lock.Dispose() }
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    try {
        # The native CLI forwards these bytes as UTF-8, including Korean paths.
        # Keep this process-local; dot-sourcing must preserve the caller's encoding.
        [Console]::OutputEncoding = $script:Utf8
        $result = Invoke-OpenDiyUpdate $Action $InstallationRoot $PackageRoot $ManifestSha256 $IntervalMinutes
        $result | ConvertTo-Json -Depth 12 -Compress
        exit 0
    } catch {
        Get-UpdateFailure $_ | ConvertTo-Json -Depth 8 -Compress
        exit 1
    }
}
