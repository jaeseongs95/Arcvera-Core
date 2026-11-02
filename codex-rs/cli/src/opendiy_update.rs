//! OpenDIY's local package boundary. Never resolve an updater through PATH or a
//! user configuration directory, and never fall back to the upstream installer.
use anyhow::Context;
#[cfg(not(windows))]
use anyhow::bail;
use anyhow::ensure;
use serde::Deserialize;
use sha2::Digest;
use sha2::Sha256;
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::ExitStatus;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentPackage {
    schema: u32,
    installation_root: PathBuf,
    bootstrap_manifest_sha256: String,
    package_root: PathBuf,
    manifest_sha256: String,
    revision: u64,
    version: String,
    source_commit: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageReceipt {
    version: String,
    source_commit: String,
    source_freeze_sha256: String,
    binary_sha256: String,
    files: Vec<ReceiptFile>,
}

#[derive(Deserialize)]
struct ReceiptFile {
    path: String,
    sha256: String,
}

struct VerifiedPackage {
    installation_root: PathBuf,
    package_root: PathBuf,
    manifest_sha256: String,
    receipt: PackageReceipt,
}

fn valid_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn read_json(path: &Path) -> anyhow::Result<Vec<u8>> {
    ensure!(
        fs::metadata(path)
            .with_context(|| format!("Missing OpenDIY package metadata: {}", path.display()))?
            .len()
            <= 16 * 1024 * 1024,
        "OpenDIY package metadata exceeds 16 MiB: {}",
        path.display()
    );
    fs::read(path).with_context(|| format!("Cannot read OpenDIY metadata: {}", path.display()))
}

fn file_sha256(path: &Path) -> anyhow::Result<String> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("Cannot open packaged OpenDIY file: {}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn verified_file(package_root: &Path, relative: &str, digest: &str) -> anyhow::Result<PathBuf> {
    ensure!(
        valid_hex(digest, 64),
        "Invalid OpenDIY SHA256 for {relative}"
    );
    let path = package_root.join(relative);
    let canonical = path
        .canonicalize()
        .with_context(|| format!("Missing packaged OpenDIY file: {}", path.display()))?;
    // These fixed executable paths must not traverse symlinks or junctions.
    ensure!(
        canonical == path && canonical.is_file(),
        "OpenDIY packaged file escapes its expected path: {relative}"
    );
    ensure!(
        file_sha256(&canonical)?.eq_ignore_ascii_case(digest),
        "OpenDIY packaged file SHA256 mismatch: {relative}"
    );
    Ok(canonical)
}

fn native_revision(version: &str) -> Option<u64> {
    let (_, suffix) = version.split_once("+native-r")?;
    suffix.split('.').next()?.parse().ok()
}

fn verify_current_package(installation_root: &Path) -> anyhow::Result<VerifiedPackage> {
    let installation_root = installation_root.canonicalize()?;
    let pointer_path = installation_root.join("current-package.json");
    ensure!(
        pointer_path.canonicalize().with_context(|| format!(
            "Missing OpenDIY installation pointer: {}",
            pointer_path.display()
        ))? == pointer_path,
        "OpenDIY installation pointer must not be a symlink or junction"
    );
    let pointer: CurrentPackage = serde_json::from_slice(&read_json(&pointer_path)?)
        .context("Invalid OpenDIY current-package.json")?;
    ensure!(
        pointer.schema == 1,
        "Unsupported OpenDIY installation pointer schema"
    );
    ensure!(
        pointer.installation_root.is_absolute()
            && pointer.installation_root.canonicalize()? == installation_root,
        "OpenDIY pointer installation root mismatch"
    );
    ensure!(
        valid_hex(&pointer.bootstrap_manifest_sha256, 64),
        "Invalid OpenDIY bootstrap manifest SHA256"
    );
    ensure!(
        pointer.package_root.is_absolute(),
        "OpenDIY selected package path must be absolute"
    );
    ensure!(
        valid_hex(&pointer.manifest_sha256, 64),
        "Invalid OpenDIY manifest SHA256"
    );
    ensure!(
        valid_hex(&pointer.source_commit, 40),
        "Invalid OpenDIY source commit"
    );
    ensure!(
        native_revision(&pointer.version) == Some(pointer.revision),
        "OpenDIY pointer revision/version mismatch"
    );
    let versions = installation_root.join("versions");
    ensure!(
        versions.canonicalize()? == versions,
        "OpenDIY versions directory must not be a symlink or junction"
    );
    let package_root = pointer.package_root.canonicalize()?;
    ensure!(
        package_root.parent() == Some(versions.as_path()),
        "OpenDIY selected package must be directly under this installation's versions directory"
    );
    let manifest_path = package_root.join("package-receipt.json");
    ensure!(
        manifest_path.canonicalize()? == manifest_path,
        "OpenDIY receipt must not be a symlink or junction"
    );
    let manifest = read_json(&manifest_path)?;
    ensure!(
        format!("{:x}", Sha256::digest(&manifest)).eq_ignore_ascii_case(&pointer.manifest_sha256),
        "OpenDIY package receipt SHA256 mismatch"
    );
    let receipt: PackageReceipt =
        serde_json::from_slice(&manifest).context("Invalid OpenDIY package-receipt.json")?;
    let version = receipt
        .version
        .strip_prefix("codex-cli ")
        .unwrap_or(&receipt.version);
    ensure!(
        version == pointer.version && receipt.source_commit == pointer.source_commit,
        "OpenDIY pointer/receipt identity mismatch"
    );
    ensure!(
        valid_hex(&receipt.source_freeze_sha256, 64),
        "Invalid OpenDIY source freeze SHA256"
    );
    Ok(VerifiedPackage {
        installation_root,
        package_root,
        manifest_sha256: pointer.manifest_sha256,
        receipt,
    })
}

impl VerifiedPackage {
    fn file(&self, relative: &str) -> anyhow::Result<PathBuf> {
        let mut entries = self
            .receipt
            .files
            .iter()
            .filter(|entry| entry.path == relative);
        let entry = entries
            .next()
            .with_context(|| format!("OpenDIY receipt does not list {relative}"))?;
        ensure!(
            entries.next().is_none(),
            "Duplicate OpenDIY receipt entry: {relative}"
        );
        if relative == "bin/codex.exe" {
            ensure!(
                entry
                    .sha256
                    .eq_ignore_ascii_case(&self.receipt.binary_sha256),
                "OpenDIY binary SHA256 disagrees with file receipt"
            );
        }
        verified_file(&self.package_root, relative, &entry.sha256)
    }
}

fn installation_root_for_cli(executable: &Path) -> anyhow::Result<PathBuf> {
    let package = executable
        .parent()
        .filter(|p| p.file_name().is_some_and(|n| n == "bin"))
        .and_then(Path::parent);
    let versions = package
        .and_then(Path::parent)
        .filter(|p| p.file_name().is_some_and(|n| n == "versions"));
    versions.and_then(Path::parent).map(Path::to_path_buf).context(
        "OpenDIY update requires an installation under <installation>/versions/<package>/bin/codex.exe with current-package.json; the official installer will not be used"
    )
}

/// Resolve the verified CLI/server pair for the exact private query, or start
/// the selected CLI with the original arguments, working directory and handles.
/// None means a successful resolution with no child; Some preserves child status.
pub fn launch() -> anyhow::Result<Option<ExitStatus>> {
    let executable = std::env::current_exe()?.canonicalize()?;
    let installation_root = executable
        .parent()
        .context("OpenDIY launcher has no installation directory")?;
    launch_or_resolve(installation_root, std::env::args_os().skip(1))
}

fn launch_or_resolve(
    installation_root: &Path,
    args: impl IntoIterator<Item = OsString>,
) -> anyhow::Result<Option<ExitStatus>> {
    use std::io::Write;
    let args: Vec<_> = args.into_iter().collect();
    if args.len() == 1 && args[0] == "--opendiy-resolve-json" {
        let resolved = resolve_selected(installation_root)?;
        let mut stdout = std::io::stdout().lock();
        serde_json::to_writer(&mut stdout, &resolved)?;
        stdout.write_all(b"\n")?;
        return Ok(None);
    }
    launch_selected(installation_root, args).map(Some)
}

fn resolve_selected(installation_root: &Path) -> anyhow::Result<serde_json::Value> {
    let package = verify_current_package(installation_root)?;
    let cli = package.file("bin/codex.exe")?;
    let server = package.file("bin/codex-app-server.exe")?;
    let server_sha256 = &package
        .receipt
        .files
        .iter()
        .find(|entry| entry.path == "bin/codex-app-server.exe")
        .context("Missing verified OpenDIY app-server receipt entry")?
        .sha256;
    let version = package
        .receipt
        .version
        .strip_prefix("codex-cli ")
        .unwrap_or(&package.receipt.version);
    Ok(serde_json::json!({
        "schema": 1,
        "installationRoot": codex_utils_path::normalize_for_native_workdir(&package.installation_root),
        "packageRoot": codex_utils_path::normalize_for_native_workdir(&package.package_root),
        "manifestSha256": package.manifest_sha256,
        "version": version,
        "revision": native_revision(version).context("Invalid OpenDIY package revision")?,
        "sourceCommit": package.receipt.source_commit,
        "cliPath": codex_utils_path::normalize_for_native_workdir(&cli),
        "cliSha256": package.receipt.binary_sha256,
        "appServerPath": codex_utils_path::normalize_for_native_workdir(&server),
        "appServerSha256": server_sha256
    }))
}

fn launch_selected(
    installation_root: &Path,
    args: impl IntoIterator<Item = OsString>,
) -> anyhow::Result<ExitStatus> {
    selected_command(installation_root, args)?
        .status()
        .context("Failed to start the selected OpenDIY CLI")
}

fn selected_command(
    installation_root: &Path,
    args: impl IntoIterator<Item = OsString>,
) -> anyhow::Result<Command> {
    let package = verify_current_package(installation_root)?;
    let mut command = Command::new(package.file("bin/codex.exe")?);
    command.args(args);
    Ok(command)
}

/// Actions supported by the packaged updater.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Update,
    Check,
    Status,
    Enable,
    Disable,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Self::Update => "Update",
            Self::Check => "Check",
            Self::Status => "Status",
            Self::Enable => "Enable",
            Self::Disable => "Disable",
        }
    }
}

/// Run the package updater without a window or a shell command string.
pub fn run_update() -> anyhow::Result<()> {
    run_action(Action::Update, None)
}

/// Execute an OpenDIY update action after validating the installed package.
pub fn run_action(action: Action, interval_minutes: Option<u32>) -> anyhow::Result<()> {
    ensure!(
        interval_minutes
            .is_none_or(|interval| action == Action::Enable && (5..=1440).contains(&interval)),
        "OpenDIY update interval requires Enable and must be between 5 and 1440 minutes"
    );
    let executable = std::env::current_exe()?.canonicalize()?;
    let installation_root = installation_root_for_cli(&executable)?;
    let package = verify_current_package(&installation_root)?;
    ensure!(
        executable == package.file("bin/codex.exe")?,
        "This OpenDIY CLI is no longer the selected package; restart Codex before updating"
    );
    let version = package
        .receipt
        .version
        .strip_prefix("codex-cli ")
        .unwrap_or(&package.receipt.version);
    ensure!(
        version == env!("CARGO_PKG_VERSION"),
        "Running OpenDIY build version differs from its package receipt"
    );
    let script = package.file("updater/update.ps1")?;
    run_verified_updater(&package, &script, action, interval_minutes)
}

#[cfg(windows)]
fn system_powershell() -> anyhow::Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    let mut buffer = [0_u16; 32768];
    // Obtain the trusted system directory from Windows, not PATH/SystemRoot.
    let length = unsafe {
        windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
    } as usize;
    ensure!(
        length > 0 && length < buffer.len(),
        "Cannot locate the Windows system directory"
    );
    let system_directory = PathBuf::from(OsString::from_wide(&buffer[..length]));
    let powershell = system_directory.join("WindowsPowerShell/v1.0/powershell.exe");
    ensure!(
        powershell.is_file(),
        "System Windows PowerShell is missing: {}",
        powershell.display()
    );
    Ok(powershell)
}

#[cfg(windows)]
fn updater_command(
    package: &VerifiedPackage,
    script: &Path,
    action: Action,
    interval_minutes: Option<u32>,
) -> anyhow::Result<Command> {
    use std::os::windows::process::CommandExt;
    // Windows PowerShell 5.1 expects ordinary drive/UNC paths rather than
    // Rust's extended-length canonical path prefix.
    let script = codex_utils_path::normalize_for_native_workdir(script);
    let installation_root =
        codex_utils_path::normalize_for_native_workdir(&package.installation_root);
    let package_root = codex_utils_path::normalize_for_native_workdir(&package.package_root);
    let mut command = Command::new(system_powershell()?);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
        ])
        .arg(script)
        .args(["-Action", action.as_str(), "-InstallationRoot"])
        .arg(&installation_root)
        .arg("-PackageRoot")
        .arg(&package_root)
        .arg("-ManifestSha256")
        .arg(&package.manifest_sha256)
        .current_dir(&installation_root)
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    if let Some(interval) = interval_minutes {
        command.arg("-IntervalMinutes").arg(interval.to_string());
    }
    Ok(command)
}

#[cfg(windows)]
fn run_verified_updater(
    package: &VerifiedPackage,
    script: &Path,
    action: Action,
    interval_minutes: Option<u32>,
) -> anyhow::Result<()> {
    use std::io::Write;
    let output = updater_command(package, script, action, interval_minutes)?
        .output()
        .context("Failed to start the packaged OpenDIY updater")?;
    std::io::stdout().write_all(&output.stdout)?;
    std::io::stderr().write_all(&output.stderr)?;
    ensure!(
        output.status.success(),
        "Packaged OpenDIY updater failed with status {}",
        output.status
    );
    Ok(())
}

#[cfg(not(windows))]
fn run_verified_updater(
    _package: &VerifiedPackage,
    _script: &Path,
    _action: Action,
    _interval_minutes: Option<u32>,
) -> anyhow::Result<()> {
    bail!(
        "Packaged OpenDIY updates currently support Windows only; the official installer will not be used"
    )
}

#[cfg(test)]
#[path = "opendiy_update_tests.rs"]
mod tests;
