// Modified for OpenDIY Codex R33 (2026-11-02); see MODIFICATIONS.md.
use super::*;
use serde_json::json;
use tempfile::TempDir;

struct Fixture {
    directory: TempDir,
    package: PathBuf,
}

impl Fixture {
    fn new() -> anyhow::Result<Self> {
        let directory = tempfile::tempdir()?;
        let package = directory.path().join("versions/r33");
        fs::create_dir_all(package.join("bin"))?;
        fs::create_dir_all(package.join("updater"))?;
        fs::write(package.join("bin/codex.exe"), b"fixture executable")?;
        fs::write(package.join("bin/codex-app-server.exe"), b"fixture server")?;
        fs::write(package.join("updater/update.ps1"), b"fixture updater")?;
        let fixture = Self { directory, package };
        fixture.write_metadata()?;
        Ok(fixture)
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn write_metadata(&self) -> anyhow::Result<()> {
        let binary_hash = file_sha256(&self.package.join("bin/codex.exe"))?;
        let receipt = json!({
            "version": "codex-cli 0.166.0+native-r33.20261102",
            "sourceCommit": "a".repeat(40),
            "sourceFreezeSha256": "b".repeat(64),
            "binarySha256": binary_hash,
            "files": [
                {"path": "bin/codex.exe", "sha256": binary_hash},
                {"path": "bin/codex-app-server.exe", "sha256": file_sha256(&self.package.join("bin/codex-app-server.exe"))?},
                {"path": "updater/update.ps1", "sha256": file_sha256(&self.package.join("updater/update.ps1"))?}
            ]
        });
        let receipt_bytes = serde_json::to_vec(&receipt)?;
        fs::write(self.package.join("package-receipt.json"), &receipt_bytes)?;
        self.write_pointer(json!({
            "schema": 1,
            "installationRoot": self.root().canonicalize()?,
            "bootstrapManifestSha256": "d".repeat(64),
            "packageRoot": self.package.canonicalize()?,
            "manifestSha256": format!("{:x}", Sha256::digest(&receipt_bytes)),
            "revision": 33,
            "version": "0.166.0+native-r33.20261102",
            "sourceCommit": "a".repeat(40)
        }))
    }

    fn write_pointer(&self, value: serde_json::Value) -> anyhow::Result<()> {
        fs::write(
            self.root().join("current-package.json"),
            serde_json::to_vec(&value)?,
        )?;
        Ok(())
    }

    fn pointer(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::from_slice(&fs::read(
            self.root().join("current-package.json"),
        )?)?)
    }
}

#[test]
fn installed_package_validates_and_resolves_local_updater() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let package = verify_current_package(fixture.root())?;
    assert_eq!(
        package.file("bin/codex.exe")?,
        fixture.package.canonicalize()?.join("bin/codex.exe")
    );
    assert_eq!(
        package.file("updater/update.ps1")?,
        fixture.package.canonicalize()?.join("updater/update.ps1")
    );
    assert_eq!(
        installation_root_for_cli(&package.file("bin/codex.exe")?)?,
        fixture.root().canonicalize()?
    );
    assert!(installation_root_for_cli(&fixture.root().join("bin/codex.exe")).is_err());
    Ok(())
}

#[test]
fn pointer_escape_schema_identity_and_digest_are_rejected() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let original = fixture.pointer()?;
    let outside = tempfile::tempdir()?;
    for (field, invalid) in [
        ("packageRoot", json!(outside.path())),
        ("packageRoot", json!("versions/r33")),
        ("schema", json!(2)),
        ("installationRoot", json!(outside.path())),
        ("bootstrapManifestSha256", json!("invalid")),
        ("revision", json!(34)),
        ("version", json!("0.166.0+native-r33.changed")),
        ("sourceCommit", json!("c".repeat(40))),
        ("manifestSha256", json!("0".repeat(64))),
    ] {
        let mut pointer = original.clone();
        pointer[field] = invalid;
        fixture.write_pointer(pointer)?;
        assert!(
            verify_current_package(fixture.root()).is_err(),
            "invalid {field} must be rejected"
        );
    }
    fixture.write_pointer(original)?;
    fs::write(fixture.root().join("current-package.json"), b"{broken")?;
    assert!(verify_current_package(fixture.root()).is_err());
    Ok(())
}

#[test]
fn missing_or_modified_script_and_binary_are_rejected() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let package = verify_current_package(fixture.root())?;
    fs::write(fixture.package.join("updater/update.ps1"), b"tampered")?;
    assert!(package.file("updater/update.ps1").is_err());
    fs::remove_file(fixture.package.join("updater/update.ps1"))?;
    assert!(package.file("updater/update.ps1").is_err());
    fs::write(fixture.package.join("bin/codex.exe"), b"tampered")?;
    assert!(selected_command(fixture.root(), []).is_err());
    fs::remove_file(fixture.package.join("package-receipt.json"))?;
    assert!(verify_current_package(fixture.root()).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn package_directory_symlink_cannot_escape_versions() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let outside = tempfile::tempdir()?;
    let link = fixture.root().join("versions").join("escape");
    std::os::unix::fs::symlink(outside.path(), &link)?;
    let mut pointer = fixture.pointer()?;
    pointer["packageRoot"] = json!(link);
    fixture.write_pointer(pointer)?;
    assert!(verify_current_package(fixture.root()).is_err());
    Ok(())
}

#[cfg(windows)]
#[test]
fn package_directory_junction_cannot_escape_versions() -> anyhow::Result<()> {
    use std::os::windows::process::CommandExt;
    let fixture = Fixture::new()?;
    let outside = tempfile::tempdir()?;
    let link = fixture.root().join("versions").join("escape");
    let cmd = system_powershell()?
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .context("PowerShell system directory")?
        .join("cmd.exe");
    let status = Command::new(cmd)
        .args(["/D", "/C", "mklink", "/J"])
        .arg(&link)
        .arg(outside.path())
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
        .status()?;
    assert!(status.success());
    assert_eq!(link.canonicalize()?, outside.path().canonicalize()?);
    let mut pointer = fixture.pointer()?;
    pointer["packageRoot"] = json!(link);
    fixture.write_pointer(pointer)?;
    assert!(verify_current_package(fixture.root()).is_err());
    Ok(())
}

// This same test executable becomes the fixture codex.exe. A private child-only
// environment value selects fixture behavior; the parent test never mutates its
// own process environment or launches a real Codex session.
#[test]
fn launcher_child_fixture() -> anyhow::Result<()> {
    use std::io::Write;
    let Some(snapshot) = std::env::var_os("OPENDIY_LAUNCHER_TEST_SNAPSHOT") else {
        return Ok(());
    };
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    fs::write(
        snapshot,
        serde_json::to_vec(&json!({
            "args": std::env::args().skip(1).collect::<Vec<_>>(),
            "cwd": std::env::current_dir()?,
            "input": input
        }))?,
    )?;
    std::io::stdout().write_all(b"fixture stdout\n")?;
    std::io::stderr().write_all(b"fixture stderr\n")?;
    std::process::exit(37);
}

#[test]
fn launcher_preserves_arguments_cwd_stdio_and_exit_status() -> anyhow::Result<()> {
    use std::io::Write;
    use std::process::Stdio;
    let fixture = Fixture::new()?;
    fs::copy(
        std::env::current_exe()?,
        fixture.package.join("bin/codex.exe"),
    )?;
    fixture.write_metadata()?;
    let cwd = fixture.root().join("working directory");
    fs::create_dir(&cwd)?;
    let snapshot_path = fixture.root().join("child.json");
    let args = [
        "--exact",
        "opendiy_update::tests::launcher_child_fixture",
        "--nocapture",
        "--skip",
        "argument with spaces 한글 & symbols",
    ];
    let mut command = selected_command(fixture.root(), args.iter().map(OsString::from))?;
    let mut child = command
        .env("OPENDIY_LAUNCHER_TEST_SNAPSHOT", &snapshot_path)
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("child stdin")?
        .write_all(b"fixture stdin")?;
    let output = child.wait_with_output()?;
    assert_eq!(output.status.code(), Some(37));
    assert!(String::from_utf8(output.stdout)?.contains("fixture stdout"));
    assert!(String::from_utf8(output.stderr)?.contains("fixture stderr"));
    let snapshot: serde_json::Value = serde_json::from_slice(&fs::read(snapshot_path)?)?;
    assert_eq!(snapshot["args"], json!(args));
    assert_eq!(
        PathBuf::from(snapshot["cwd"].as_str().context("snapshot cwd")?).canonicalize()?,
        cwd.canonicalize()?
    );
    assert_eq!(snapshot["input"], "fixture stdin");
    Ok(())
}

#[test]
fn interval_is_rejected_before_any_package_or_process_access() {
    assert!(run_action(Action::Check, Some(60)).is_err());
    assert!(run_action(Action::Enable, Some(4)).is_err());
    assert!(run_action(Action::Enable, Some(1441)).is_err());
}

#[cfg(windows)]
#[test]
fn updater_uses_system_powershell_and_preserves_script_output_and_exit() -> anyhow::Result<()> {
    if std::env::var_os("OPENDIY_TEST_MODULE_PATH_CHILD").is_none() {
        // Pollute only a nested test process, never the test runner's environment.
        let inherited_modules = tempfile::tempdir()?;
        let utility = inherited_modules
            .path()
            .join("Microsoft.PowerShell.Utility");
        fs::create_dir(&utility)?;
        fs::write(utility.join("Microsoft.PowerShell.Utility.psd1"), b"@{ ModuleVersion='99.0.0'; PowerShellVersion='7.0'; RootModule='Microsoft.PowerShell.Utility.psm1'; FunctionsToExport=@('Get-FileHash') }")?;
        fs::write(utility.join("Microsoft.PowerShell.Utility.psm1"), b"function Get-FileHash { throw 'untrusted fixture module imported' }; Export-ModuleMember -Function Get-FileHash")?;
        let output = Command::new(std::env::current_exe()?)
            .args(["--exact", "opendiy_update::tests::updater_uses_system_powershell_and_preserves_script_output_and_exit", "--nocapture"])
            .env("OPENDIY_TEST_MODULE_PATH_CHILD", "1")
            .env("OPENDIY_TEST_BAD_MODULE_PATH", inherited_modules.path())
            .env("PSMODULEPATH", inherited_modules.path())
            .output()?;
        ensure!(
            output.status.success(),
            "Updater must work with a hostile inherited PSMODULEPATH: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(());
    }
    let fixture = Fixture::new()?;
    fs::write(fixture.package.join("updater/update.ps1"), br#"
param([string]$Action, [string]$InstallationRoot, [string]$PackageRoot, [string]$ManifestSha256, [int]$IntervalMinutes = 60)
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
@{action=$Action; installationRoot=$InstallationRoot; packageRoot=$PackageRoot; manifestSha256=$ManifestSha256; intervalMinutes=$IntervalMinutes; cwd=(Get-Location).Path; modulePath=$env:PSModulePath; hashCommand=(Get-Command Get-FileHash -ErrorAction Stop).Name; scriptSha256=(Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256 -ErrorAction Stop).Hash} | ConvertTo-Json -Compress
if ($Action -eq 'Disable') { [Console]::Error.WriteLine('fixture stderr'); exit 23 }
exit 0
"#)?;
    fixture.write_metadata()?;
    let package = verify_current_package(fixture.root())?;
    let script = package.file("updater/update.ps1")?;
    for action in [
        Action::Update,
        Action::Check,
        Action::Status,
        Action::Enable,
        Action::Disable,
    ] {
        let interval = (action == Action::Enable).then_some(5);
        let mut command = updater_command(&package, &script, action, interval)?;
        assert_eq!(command.get_program(), system_powershell()?.as_os_str());
        assert_eq!(
            command.get_current_dir(),
            Some(
                codex_utils_path::normalize_for_native_workdir(&package.installation_root)
                    .as_path()
            )
        );
        let args: Vec<_> = command
            .get_args()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-WindowStyle", "Hidden"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-Action", action.as_str()])
        );
        let module_path = system_powershell()?
            .parent()
            .context("PowerShell directory")?
            .join("Modules");
        let child_environment: Vec<_> = command.get_envs().collect();
        assert!(child_environment.iter().any(|(key, value)| {
            key.to_string_lossy().eq_ignore_ascii_case("PSModulePath")
                && *value == Some(module_path.as_os_str())
        }));
        let output = command.output()?;
        assert_eq!(
            output.status.code(),
            Some(if action == Action::Disable { 23 } else { 0 })
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(value["action"], action.as_str());
        // Windows PowerShell may add its standard user/all-users module paths
        // at startup. The explicit process environment is checked above; its
        // runtime path must retain system modules and discard inherited poison.
        let runtime_modules: Vec<_> = std::env::split_paths(std::ffi::OsStr::new(
            value["modulePath"]
                .as_str()
                .context("fixture module path")?,
        ))
        .collect();
        assert!(runtime_modules.iter().any(|path| {
            path.to_string_lossy()
                .eq_ignore_ascii_case(&module_path.to_string_lossy())
        }));
        let bad_path = std::env::var_os("OPENDIY_TEST_BAD_MODULE_PATH")
            .context("hostile inherited module fixture")?;
        assert!(!runtime_modules.iter().any(|path| {
            path.to_string_lossy()
                .eq_ignore_ascii_case(&bad_path.to_string_lossy())
        }));
        assert_eq!(value["hashCommand"], "Get-FileHash");
        assert_eq!(
            value["scriptSha256"]
                .as_str()
                .context("fixture script hash")?
                .to_ascii_lowercase(),
            file_sha256(&script)?
        );
        assert_eq!(value["manifestSha256"], package.manifest_sha256);
        assert_eq!(value["intervalMinutes"], interval.unwrap_or(60));
        for key in ["installationRoot", "cwd"] {
            assert_eq!(
                PathBuf::from(value[key].as_str().context("fixture installation path")?)
                    .canonicalize()?,
                package.installation_root
            );
        }
        assert_eq!(
            PathBuf::from(
                value["packageRoot"]
                    .as_str()
                    .context("fixture package path")?
            )
            .canonicalize()?,
            package.package_root
        );
        if action == Action::Disable {
            assert!(String::from_utf8(output.stderr)?.contains("fixture stderr"));
        }
    }
    Ok(())
}

#[test]
fn resolve_reports_verified_pair_without_spawning_and_rejects_tampering() -> anyhow::Result<()> {
    let fixture = Fixture::new()?;
    let value = resolve_selected(fixture.root())?;
    assert_eq!(value["schema"], 1);
    assert_eq!(value["version"], "0.166.0+native-r33.20261102");
    assert_eq!(value["revision"], 33);
    assert_eq!(value["sourceCommit"], "a".repeat(40));
    assert_eq!(
        value["manifestSha256"],
        fixture.pointer()?["manifestSha256"]
    );
    for (key, expected) in [
        ("installationRoot", fixture.root()),
        ("packageRoot", fixture.package.as_path()),
    ] {
        assert_eq!(
            PathBuf::from(value[key].as_str().context("resolved package path")?).canonicalize()?,
            expected.canonicalize()?
        );
    }
    for (path_key, hash_key, relative) in [
        ("cliPath", "cliSha256", "bin/codex.exe"),
        (
            "appServerPath",
            "appServerSha256",
            "bin/codex-app-server.exe",
        ),
    ] {
        assert_eq!(
            PathBuf::from(
                value[path_key]
                    .as_str()
                    .context("resolved executable path")?
            )
            .canonicalize()?,
            fixture.package.canonicalize()?.join(relative)
        );
        assert_eq!(
            value[hash_key],
            file_sha256(&fixture.package.join(relative))?
        );
    }
    // These fixture bytes are not runnable executables. Successful resolution
    // therefore proves the exact query did not attempt to start either child.
    assert!(
        launch_or_resolve(fixture.root(), [OsString::from("--opendiy-resolve-json")])?.is_none()
    );
    let forwarded = selected_command(fixture.root(), [OsString::from("--version")])?;
    assert_eq!(
        forwarded.get_args().collect::<Vec<_>>(),
        vec![std::ffi::OsStr::new("--version")]
    );
    for relative in ["bin/codex-app-server.exe", "bin/codex.exe"] {
        fs::write(fixture.package.join(relative), b"tampered")?;
        assert!(resolve_selected(fixture.root()).is_err());
        fs::write(
            fixture.package.join(relative),
            if relative == "bin/codex.exe" {
                b"fixture executable".as_slice()
            } else {
                b"fixture server".as_slice()
            },
        )?;
    }
    Ok(())
}
