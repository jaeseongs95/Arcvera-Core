$ErrorActionPreference = 'Stop'
$productCommit = 'd7b592f56cb3a8e357e362c90e326819a6a62a03'
$target = 'x86_64-pc-windows-msvc'
$output = Join-Path $env:GITHUB_WORKSPACE 'r40-release-artifact'
New-Item -ItemType Directory -Path $output | Out-Null
$receipt = [ordered]@{
    productCommit = $productCommit
    ciCommit = $env:GITHUB_SHA
    runId = $env:GITHUB_RUN_ID
    runAttempt = $env:GITHUB_RUN_ATTEMPT
    target = $target
    profile = 'release'
    status = 'failed'
    scope = 'Windows optimized release build of source27 plus schema-only synchronization; final package/runtime acceptance is separate.'
    binaries = @()
}
$buildProcess = $null
try {
    $admissionDrive = Get-PSDrive -Name ([IO.Path]::GetPathRoot($env:CARGO_TARGET_DIR).Substring(0, 1))
    $admissionMemory = (Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory * 1KB
    $receipt.admission = @{ diskFree = $admissionDrive.Free; memoryFree = $admissionMemory }
    if ($admissionDrive.Free -lt 12GB -or $admissionMemory -lt 3GB) { throw 'Insufficient runner disk or memory before build' }
    # This branch may change only its CI recipe. Product source remains source27.
    $allowed = @('.github/workflows/r40-release-windows.yml', '.github/scripts/r40-release-windows.ps1')
    $changed = @(git -c maintenance.auto=false -c gc.auto=0 diff --name-only $productCommit HEAD)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot compare the fixed product commit' }
    if (@($changed | Where-Object { $_ -notin $allowed }).Count) { throw 'Product files differ from source27' }
    git -c maintenance.auto=false -c gc.auto=0 diff --exit-code $productCommit -- codex-rs third_party/v8
    if ($LASTEXITCODE -ne 0) { throw 'Product worktree differs from source27' }
    $receipt.productRustTree = (git -c maintenance.auto=false -c gc.auto=0 rev-parse "${productCommit}:codex-rs").Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Cannot identify the product Rust tree' }
    $receipt.cargoLockSha256 = (Get-FileHash (Join-Path $env:GITHUB_WORKSPACE 'codex-rs/Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant()

    # Same release manifests and checksum trust root as setup-rusty-v8; no interpreter aliases.
    $lock = Get-Content (Join-Path $env:GITHUB_WORKSPACE 'codex-rs/Cargo.lock') -Raw
    $versionMatch = [regex]::Match($lock, '(?ms)^name = "v8"\r?\nversion = "([0-9.]+)"')
    if (-not $versionMatch.Success) { throw 'Cannot resolve the locked V8 version' }
    $version = $versionMatch.Groups[1].Value
    $prefix = "rusty_v8_ptrcomp_sandbox_release_$target"
    $checksumName = "$prefix.sha256"
    $archiveName = "$prefix.lib.gz"
    $bindingName = "src_binding_ptrcomp_sandbox_release_$target.rs"
    $directory = Join-Path $env:RUNNER_TEMP 'r40-v8'
    New-Item -ItemType Directory -Path $directory | Out-Null
    $base = "https://github.com/openai/codex/releases/download/rusty-v8-v$version"
    $checksumPath = Join-Path $directory $checksumName
    Invoke-WebRequest "$base/$checksumName" -OutFile $checksumPath
    $trustedPath = Join-Path $env:GITHUB_WORKSPACE "third_party/v8/rusty_v8_$($version.Replace('.', '_'))_release_manifests.sha256"
    $trustedLines = @(Get-Content $trustedPath | Where-Object { $_ -match "^[0-9a-f]{64}  $([regex]::Escape($checksumName))$" })
    if ($trustedLines.Count -ne 1) { throw 'Missing or ambiguous trusted V8 manifest' }
    $manifestDigest = (Get-FileHash $checksumPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($manifestDigest -ne $trustedLines[0].Substring(0, 64)) { throw 'V8 manifest checksum mismatch' }
    $checksums = @(Get-Content $checksumPath)
    if ($checksums.Count -ne 2) { throw 'Expected exactly two V8 input checksums' }
    $v8Inputs = @()
    foreach ($name in @($archiveName, $bindingName)) {
        $expected = @($checksums | Where-Object { $_ -match "^[0-9a-f]{64}  $([regex]::Escape($name))$" })
        if ($expected.Count -ne 1) { throw "Missing or ambiguous V8 checksum: $name" }
        $path = Join-Path $directory $name
        Invoke-WebRequest "$base/$name" -OutFile $path
        $digest = (Get-FileHash $path -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($digest -ne $expected[0].Substring(0, 64)) { throw "V8 input checksum mismatch: $name" }
        $v8Inputs += @{ name = $name; sha256 = $digest }
    }
    $receipt.v8Inputs = $v8Inputs
    $env:RUSTY_V8_ARCHIVE = Join-Path $directory $archiveName
    $env:RUSTY_V8_SRC_BINDING_PATH = Join-Path $directory $bindingName
    $env:STABLE_GIT_COMMIT = $productCommit
    $command = @('build', '--locked', '--profile', 'release', '--target', $target, '-j1', '-p', 'codex-cli', '--bin', 'codex', '-p', 'codex-app-server', '--bin', 'codex-app-server', '-p', 'codex-code-mode-host', '--bin', 'codex-code-mode-host')
    $receipt.command = @('cargo') + $command
    $cargo = (Get-Command cargo -CommandType Application).Source
    $buildProcess = Start-Process -FilePath $cargo -ArgumentList $command -WorkingDirectory (Join-Path $env:GITHUB_WORKSPACE 'codex-rs') -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $output 'cargo.stdout.log') -RedirectStandardError (Join-Path $output 'cargo.stderr.log')
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $resources = Join-Path $output 'resources.jsonl'
    while (-not $buildProcess.HasExited) {
        $drive = Get-PSDrive -Name ([IO.Path]::GetPathRoot($env:CARGO_TARGET_DIR).Substring(0, 1))
        $memoryBytes = (Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory * 1KB
        @{ seconds = $watch.Elapsed.TotalSeconds; diskFree = $drive.Free; memoryFree = $memoryBytes } | ConvertTo-Json -Compress | Add-Content $resources
        $logBytes = ((Get-Item (Join-Path $output 'cargo.stdout.log')).Length + (Get-Item (Join-Path $output 'cargo.stderr.log')).Length)
        if ($watch.Elapsed.TotalSeconds -gt 9900 -or $drive.Free -lt 4GB -or $memoryBytes -lt 1GB -or $logBytes -gt 64MB) {
            throw 'Build deadline or resource floor reached'
        }
        Start-Sleep -Seconds 5
        $buildProcess.Refresh()
    }
    $buildProcess.WaitForExit()
    $receipt.elapsedSeconds = $watch.Elapsed.TotalSeconds
    $receipt.exitCode = $buildProcess.ExitCode
    if ($buildProcess.ExitCode -ne 0) { throw "Cargo build failed: $($buildProcess.ExitCode)" }
    git -c maintenance.auto=false -c gc.auto=0 diff --exit-code $productCommit -- codex-rs third_party/v8
    if ($LASTEXITCODE -ne 0) { throw 'Build changed product source' }
    foreach ($name in @('codex', 'codex-app-server', 'codex-code-mode-host')) {
        $binary = Join-Path $env:CARGO_TARGET_DIR "$target/release/$name.exe"
        $saved = Join-Path $output "$name.exe"
        Copy-Item -LiteralPath $binary -Destination $saved
        $digest = (Get-FileHash $saved -Algorithm SHA256).Hash.ToLowerInvariant()
        $argument = if ($name -eq 'codex-code-mode-host') { '--help' } else { '--version' }
        # Process execution is bounded; code-mode-host has no --version option.
        $entry = Start-Process -FilePath $saved -ArgumentList $argument -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $output "$name.entry.stdout.log") -RedirectStandardError (Join-Path $output "$name.entry.stderr.log")
        if (-not $entry.WaitForExit(30000)) {
            & taskkill.exe /PID $entry.Id /T /F | Out-Null
            throw "Entrypoint timeout: $name"
        }
        $entry.WaitForExit()
        if ($entry.ExitCode -ne 0) { throw "Entrypoint failed: $name" }
        $entryText = Get-Content (Join-Path $output "$name.entry.stdout.log") -Raw
        if ($name -ne 'codex-code-mode-host' -and $entryText -notmatch '0\.167\.0\+native-r40\.20261108') { throw "Unexpected product version: $name" }
        if ($name -eq 'codex-code-mode-host' -and $entryText -notmatch '--listen') { throw 'Code-mode-host help is missing --listen' }
        $receipt.binaries += @{ name = "$name.exe"; sha256 = $digest; bytes = (Get-Item $saved).Length; entrypointArgument = $argument; entrypointExitCode = $entry.ExitCode }
    }
    $receipt.symbols = @()
    foreach ($name in @('codex', 'codex_app_server', 'codex_code_mode_host')) {
        $pdb = Join-Path $env:CARGO_TARGET_DIR "$target/release/$name.pdb"
        if (Test-Path -LiteralPath $pdb) {
            $savedPdb = Join-Path $output "$name.pdb"
            Copy-Item -LiteralPath $pdb -Destination $savedPdb
            $receipt.symbols += @{ name = "$name.pdb"; bytes = (Get-Item -LiteralPath $savedPdb).Length; sha256 = (Get-FileHash -LiteralPath $savedPdb -Algorithm SHA256).Hash.ToLowerInvariant() }
        }
    }
    $receipt.status = 'passed'
} catch {
    $receipt.failure = $_.Exception.Message
    Write-Error -Message $_ -ErrorAction Continue
} finally {
    if ($null -ne $buildProcess -and -not $buildProcess.HasExited) {
        & taskkill.exe /PID $buildProcess.Id /T /F | Out-Null
        $buildProcess.WaitForExit(30000) | Out-Null
        $receipt.buildTreeTerminationRequested = $true
    }
    $receipt | ConvertTo-Json -Depth 12 | Set-Content (Join-Path $output 'receipt.json') -Encoding utf8
}
if ($receipt.status -ne 'passed') { exit 1 }
