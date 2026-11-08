//! Pure presentation mapping for known doctor checks. Raw reports stay unchanged.
//! Unknown check IDs, surfaces and values are opaque. Never use this for JSON.

use super::super::DoctorHumanOrigin;
use super::CheckStatus;
use super::DoctorCheck;
use codex_tui::i18n::Locale;
use codex_tui::i18n::message_for;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Surface<'a> {
    Summary,
    Cause,
    Remedy,
    Expected,
    Detail(&'a str),
    Label(&'a str),
}

pub(super) fn for_display<'a>(
    check: &DoctorCheck,
    surface: Surface<'_>,
    raw: &'a str,
) -> Cow<'a, str> {
    for_check_locale(codex_tui::i18n::locale(), check, surface, raw)
}

fn for_check_locale<'a>(
    locale: Locale,
    check: &DoctorCheck,
    surface: Surface<'_>,
    raw: &'a str,
) -> Cow<'a, str> {
    if locale == Locale::English {
        return Cow::Borrowed(raw);
    }
    let id = match (&check.human_origin, surface, raw) {
        (
            DoctorHumanOrigin::SystemEnvironment {
                os_language_missing: true,
                ..
            },
            Surface::Detail("os language" | "OS language"),
            "unavailable",
        ) if check.id == "system.environment" => Some("r39-doctor-human-ba691ba042bc"),
        (
            DoctorHumanOrigin::SystemEnvironment {
                os_language_missing: true,
                ..
            },
            Surface::Summary,
            "OS language unavailable",
        ) if check.id == "system.environment" => Some("r39-doctor-human-0f06928a2eba"),
        (
            DoctorHumanOrigin::SystemEnvironment {
                missing_editor_env, ..
            },
            Surface::Detail(field @ ("EDITOR" | "VISUAL")),
            "not set",
        ) if check.id == "system.environment" && missing_editor_env.contains(&field) => {
            Some("r39-doctor-human-1aef93991721")
        }
        (
            DoctorHumanOrigin::SearchPathNotFile,
            Surface::Detail("search command readiness"),
            "path is not a file",
        ) if check.id == "runtime.search" => Some("r39-doctor-human-cd6101db7c9d"),
        _ => None,
    };
    if let Some(id) = id {
        return Cow::Owned(message_for(locale, id, &[]));
    }
    for_locale(locale, &check.id, check.status, surface, raw)
}

fn for_locale<'a>(
    locale: Locale,
    check_id: &str,
    status: CheckStatus,
    surface: Surface<'_>,
    raw: &'a str,
) -> Cow<'a, str> {
    if locale == Locale::English {
        return Cow::Borrowed(raw);
    }
    let message_id = match (check_id, surface, raw) {
        (
            "app_server.status",
            Surface::Summary,
            "background server socket path could not be resolved",
        ) => Some("r39-doctor-human-4418f17f19f2"),
        ("app_server.status", Surface::Summary, "background server is not running") => {
            Some("r39-doctor-human-d89d01d8e46d")
        }
        ("app_server.status", Surface::Summary, "background server is running") => {
            Some("r39-doctor-human-e19714ee45f2")
        }
        (
            "app_server.status",
            Surface::Summary,
            "background server socket is stale or unreachable",
        ) => Some("r39-doctor-human-b88354e8e6e3"),
        (
            "app_server.status",
            Surface::Remedy,
            "Run codex app-server daemon version for more details.",
        ) => Some("r39-doctor-human-b33cd9659492"),
        ("app_server.status", Surface::Detail("status"), "not running") => {
            Some("r39-doctor-human-415ed7340303")
        }
        ("app_server.status", Surface::Detail("status"), "running") => {
            Some("r39-doctor-human-c071cf5f5ed6")
        }
        ("app_server.status", Surface::Detail("status"), "stale or unreachable") => {
            Some("r39-doctor-human-0a0ec9aaf795")
        }
        ("app_server.status", Surface::Detail("mode"), "persistent") => {
            Some("r39-doctor-human-75b07bb3ffb3")
        }
        ("app_server.status", Surface::Detail("mode"), "ephemeral") => {
            Some("r39-doctor-human-8341425cafed")
        }
        (
            "app_server.status",
            Surface::Detail("configured updater settings"),
            "unreadable or invalid",
        ) => Some("r39-doctor-human-59b5f40613a2"),
        ("app_server.status", Surface::Detail("automatic updates"), "enabled (configured)") => {
            Some("r39-doctor-human-2f8b074215cc")
        }
        ("app_server.status", Surface::Detail("automatic updates"), "disabled (configured)") => {
            Some("r39-doctor-human-6083425cc74b")
        }
        ("system.disk", Surface::Summary, "sufficient free disk space") => {
            Some("r39-doctor-human-9ed31a4c13d5")
        }
        ("system.disk", Surface::Summary, "disk capacity could not be fully verified") => {
            Some("r39-doctor-human-316eca4f662a")
        }
        ("system.disk", Surface::Summary, "disk capacity could not be verified") => {
            Some("r39-doctor-human-1b9dfb2457a9")
        }
        ("system.disk", Surface::Expected, "readable filesystem capacity") => {
            Some("r39-doctor-human-c97ae7978e33")
        }
        (
            "system.disk",
            Surface::Remedy,
            "Free disk space or move the worktree to a larger volume.",
        ) => Some("r39-doctor-human-5b5fa118a954"),
        ("system.disk", Surface::Remedy, "Check filesystem access and available disk space.") => {
            Some("r39-doctor-human-200a5dfc4600")
        }
        (
            "sandbox.filesystem_paths",
            Surface::Summary,
            "configured filesystem paths respond promptly",
        ) => Some("r39-doctor-human-22288bd4c38c"),
        ("sandbox.filesystem_paths", Surface::Summary, "no explicit filesystem paths to probe") => {
            Some("r39-doctor-human-6f88c22952f8")
        }
        (
            "sandbox.filesystem_paths",
            Surface::Summary,
            "configured filesystem paths listed; read restrictions prevent probes",
        ) => Some("r39-doctor-human-3c45274e779c"),
        (
            "sandbox.filesystem_paths",
            Surface::Summary,
            "configured filesystem paths listed; probes disabled on Windows",
        ) => Some("r39-doctor-human-66a0c1e12046"),
        (
            "sandbox.filesystem_paths",
            Surface::Summary,
            "filesystem path probes unavailable: cannot locate Codex executable",
        ) => Some("r39-doctor-human-e393b8af5b65"),
        (
            "sandbox.filesystem_paths",
            Surface::Summary,
            "some configured filesystem paths are slow or could not be checked",
        ) => Some("r39-doctor-human-9f21e1d2f480"),
        ("sandbox.filesystem_paths", Surface::Summary, "filesystem path check is incomplete") => {
            Some("r39-doctor-human-e22e4498e7ed")
        }
        (
            "sandbox.filesystem_paths",
            Surface::Detail("operation"),
            "resolve paths only; read/write access is not tested",
        ) => Some("r39-doctor-human-3f66b87cf446"),
        (
            "sandbox.filesystem_paths",
            Surface::Detail("probe budgets"),
            "2 seconds per path, 8 seconds total, at most 32 paths",
        ) => Some("r39-doctor-human-6ac4a8d3deca"),
        (
            "sandbox.filesystem_paths",
            Surface::Remedy,
            "Check this path's mount and host permissions. Ask your administrator whether the entry is needed on this machine; do not remove required restrictions.",
        ) => Some("r39-doctor-human-ecfe4380c865"),
        ("git.environment", Surface::Summary, "git executable found; execution not verified") => {
            Some("r39-doctor-human-23f4ffb66e07")
        }
        ("git.environment", Surface::Summary, "git executable not found") => {
            Some("r39-doctor-human-6c1fe7eed41c")
        }
        (
            "git.environment",
            Surface::Summary | Surface::Cause,
            "Git repository detected but git executable was not found",
        ) => Some("r39-doctor-human-4e210d0d271b"),
        ("git.environment", Surface::Expected, "git available on PATH") => {
            Some("r39-doctor-human-903a0bc74f81")
        }
        (
            "git.environment",
            Surface::Remedy,
            "Install Git or fix PATH so Codex can inspect repository metadata.",
        ) => Some("r39-doctor-human-1b6c65a5ac08"),
        (
            "git.environment",
            Surface::Detail("git execution"),
            "not inspected (PATH helpers are not executed)",
        ) => Some("r39-doctor-human-6ae58c1bf6e7"),
        ("git.environment", Surface::Detail(".git entry"), "directory") => {
            Some("r39-doctor-human-333178788eae")
        }
        ("git.environment", Surface::Detail(".git entry"), "file") => {
            Some("r39-doctor-human-3b9c358f36f0")
        }
        ("git.environment", Surface::Detail(".git entry"), "other") => {
            Some("r39-doctor-human-d9298a10d1b0")
        }
        ("git.environment", Surface::Detail(".git entry"), "missing") => {
            Some("r39-doctor-human-ffa63583dfa6")
        }
        ("runtime.search", Surface::Summary, "search command could not be verified") => {
            Some("r39-doctor-human-4742ff6069d8")
        }
        (
            "runtime.search",
            Surface::Remedy,
            "Install ripgrep or repair the bundled Codex package.",
        ) => Some("r39-doctor-human-90078f9a30d5"),
        ("runtime.search", Surface::Detail("search command readiness"), "file exists")
            if status == CheckStatus::Ok =>
        {
            Some("r39-doctor-human-383e7eb7f901")
        }
        (
            "runtime.search",
            Surface::Detail("search command readiness"),
            "found; execution not verified",
        ) if status == CheckStatus::Ok => Some("r39-doctor-human-9f06479bff83"),
        ("network.env", Surface::Summary, "network-related environment looks readable") => {
            Some("r39-doctor-human-e5a66e38d171")
        }
        ("network.env", Surface::Summary, "custom CA env var points at an unreadable file") => {
            Some("r39-doctor-human-9ce7eeb062b9")
        }
        ("network.env", Surface::Summary, "custom CA env var does not point at a file") => {
            Some("r39-doctor-human-743c76f78ed7")
        }
        ("network.env", Surface::Summary, "custom CA env var points at an unreadable path") => {
            Some("r39-doctor-human-dd08a7350b3f")
        }
        ("network.env", Surface::Detail("system proxy"), "automatic (PAC)") => {
            Some("r39-doctor-human-8f6cafeaf327")
        }
        ("network.env", Surface::Detail("system proxy"), "manual") => {
            Some("r39-doctor-human-36bde66f289a")
        }
        ("network.env", Surface::Detail("system proxy"), "direct") => {
            Some("r39-doctor-human-d15690f08a57")
        }
        ("network.env", Surface::Detail("system proxy"), "unavailable") => {
            Some("r39-doctor-human-ba691ba042bc")
        }
        ("network.env", Surface::Detail("respect system proxy"), "enabled") => {
            Some("r39-doctor-human-fb9cf75606b4")
        }
        ("network.env", Surface::Detail("respect system proxy"), "disabled") => {
            Some("r39-doctor-human-17eb3c0168d0")
        }
        ("network.env", Surface::Detail("managed proxy"), "configured") => {
            Some("r39-doctor-human-201582247500")
        }
        ("network.env", Surface::Detail("managed proxy"), "not configured") => {
            Some("r39-doctor-human-9f33f06843e7")
        }
        (
            "desktop.app.version",
            Surface::Summary,
            "the desktop application installation could not be inspected",
        ) => Some("r39-doctor-human-23f03f7ed718"),
        ("desktop.app.version", Surface::Summary, "the desktop application is installed") => {
            Some("r39-doctor-human-020ebe08eec7")
        }
        (
            "desktop.app_server.handshake",
            Surface::Summary,
            "no desktop app-server handshake was recorded",
        ) => Some("r39-doctor-human-095f9e93540e"),
        (
            "desktop.app_server.handshake",
            Surface::Summary,
            "the desktop app-server initialized successfully",
        ) => Some("r39-doctor-human-6270fc5a9546"),
        (
            "desktop.app_server.handshake",
            Surface::Summary,
            "the desktop app-server failed to initialize",
        ) => Some("r39-doctor-human-58194b53c166"),
        (
            "desktop.app_server.handshake",
            Surface::Summary,
            "the desktop application is not running",
        ) => Some("r39-doctor-human-dcc6dd327452"),
        (
            "desktop.app.version",
            Surface::Remedy,
            "restore desktop diagnostic access and rerun codex doctor",
        ) => Some("r39-doctor-human-3a2daf8b1439"),
        (
            "desktop.app_server.handshake",
            Surface::Remedy,
            "restore desktop diagnostic access and rerun codex doctor",
        ) => Some("r39-doctor-human-3a2daf8b1439"),
        (
            "desktop.security.enforcement",
            Surface::Summary,
            "security event coverage is incomplete",
        ) => Some("r39-doctor-human-cceace342fa1"),
        (
            "desktop.security.enforcement",
            Surface::Summary,
            "no locally visible recent Codex security enforcement was found",
        ) => Some("r39-doctor-human-50ff3e8ce274"),
        (
            "desktop.security.enforcement",
            Surface::Summary,
            "security event channels could not be inspected",
        ) => Some("r39-doctor-human-1b968c0a144a"),
        (
            "desktop.security.enforcement",
            Surface::Summary,
            "recent Codex security audit or detection requires review",
        ) => Some("r39-doctor-human-635d5e8831f1"),
        (
            "desktop.security.enforcement",
            Surface::Summary,
            "endpoint security blocked or quarantined a Codex executable",
        ) => Some("r39-doctor-human-00cb7047a65e"),
        (
            "desktop.security.enforcement",
            Surface::Remedy,
            "ask your organization's security administrator to review endpoint security events and the approved Codex application policy",
        ) => Some("r39-doctor-human-092547fe9a8f"),
        (
            "desktop.security.enforcement",
            Surface::Summary,
            "macos XProtect blocked or remediated the desktop application",
        ) => Some("r39-doctor-human-5d09864723f1"),
        (
            "desktop.security.enforcement",
            Surface::Summary,
            "macos gatekeeper rejected the desktop application",
        ) => Some("r39-doctor-human-5dca586451f6"),
        (
            "desktop.security.enforcement",
            Surface::Summary,
            "a recent macos security event blocked the desktop application",
        ) => Some("r39-doctor-human-db73e3d01d2a"),
        (
            "desktop.security.enforcement",
            Surface::Summary,
            "macos system-policy diagnostics indicate file descriptor exhaustion",
        ) => Some("r39-doctor-human-854b2be9314a"),
        (
            "desktop.security.enforcement",
            Surface::Summary,
            "recent desktop security events are audit-only",
        ) => Some("r39-doctor-human-e97c27e05918"),
        (
            "desktop.security.enforcement",
            Surface::Summary,
            "the desktop security assessment was unavailable",
        ) => Some("r39-doctor-human-7d9adee5864d"),
        (
            "desktop.security.enforcement",
            Surface::Summary,
            "recent macos security enforcement history was unavailable",
        ) => Some("r39-doctor-human-d1ee8af9d6a9"),
        (
            "desktop.security.enforcement",
            Surface::Summary,
            "the desktop application passed available macos security assessments",
        ) => Some("r39-doctor-human-288327f6e86a"),
        (
            "desktop.security.enforcement",
            Surface::Remedy,
            "collect the XProtect detection and ask your security administrator to review the official Codex installation",
        ) => Some("r39-doctor-human-125747937a60"),
        (
            "desktop.security.enforcement",
            Surface::Remedy,
            "ask your security administrator to review the application policy",
        ) => Some("r39-doctor-human-745aadccf224"),
        (
            "desktop.security.enforcement",
            Surface::Remedy,
            "ask your security administrator to review the matching prevention event",
        ) => Some("r39-doctor-human-6a8ef61e1afe"),
        (
            "desktop.security.enforcement",
            Surface::Remedy,
            "restart your Mac, retry Codex once, and contact support if the problem returns",
        ) => Some("r39-doctor-human-3028ca9c12ee"),
        (
            "desktop.security.enforcement",
            Surface::Remedy,
            "ask your security administrator to verify the application policy",
        ) => Some("r39-doctor-human-8cc9e267e5a8"),
        (
            "desktop.security.enforcement",
            Surface::Remedy,
            "check access to macos gatekeeper diagnostics",
        ) => Some("r39-doctor-human-516f6eb97deb"),
        (
            "desktop.security.enforcement",
            Surface::Remedy,
            "check access to macos unified security logs and rerun codex doctor",
        ) => Some("r39-doctor-human-187c29049a29"),
        ("desktop.security.enforcement", Surface::Detail("gatekeeper"), "accepted") => {
            Some("r39-doctor-human-070c160a6299")
        }
        ("sandbox.helpers", Surface::Summary, "sandbox configuration is readable") => {
            Some("r39-doctor-human-f5f32e6e4144")
        }
        ("sandbox.helpers", Surface::Summary, "Linux sandbox helper path does not exist") => {
            Some("r39-doctor-human-19423338b292")
        }
        (
            "sandbox.helpers",
            Surface::Summary,
            "Windows sandbox has multiple configuration or provisioning issues",
        ) => Some("r39-doctor-human-9e46bbd29ceb"),
        (
            "sandbox.helpers",
            Surface::Cause,
            "managed denied-read requirements need the elevated Windows sandbox backend",
        ) => Some("r39-doctor-human-ec10bc9a1209"),
        (
            "sandbox.helpers",
            Surface::Cause,
            "Windows sandbox setup failure report could not be read",
        ) => Some("r39-doctor-human-f8bbab97375a"),
        ("sandbox.helpers", Surface::Cause, "a Windows sandbox account is missing") => {
            Some("r39-doctor-human-3ff2fa3b2fac")
        }
        ("sandbox.helpers", Surface::Cause, "a Windows sandbox account is locked") => {
            Some("r39-doctor-human-7bdf0ea47dea")
        }
        ("sandbox.helpers", Surface::Cause, "a Windows sandbox account is disabled") => {
            Some("r39-doctor-human-d49644c196ff")
        }
        ("sandbox.helpers", Surface::Cause, "a Windows sandbox account password has expired") => {
            Some("r39-doctor-human-2664577dc37e")
        }
        (
            "sandbox.helpers",
            Surface::Cause,
            "Windows sandbox account status could not be inspected",
        ) => Some("r39-doctor-human-c9167cbcf173"),
        (
            "sandbox.helpers",
            Surface::Cause,
            "elevated Windows sandbox provisioning recorded a structured failure",
        ) => Some("r39-doctor-human-280bb82f7e81"),
        (
            "sandbox.helpers",
            Surface::Cause,
            "elevated Windows sandbox provisioning is incomplete or outdated",
        ) => Some("r39-doctor-human-fd37a5b03e75"),
        ("sandbox.helpers", Surface::Expected, "elevated backend") => {
            Some("r39-doctor-human-e6ff8cd00164")
        }
        (
            "sandbox.helpers",
            Surface::Expected,
            "readable setup failure report no larger than 64 KiB",
        ) => Some("r39-doctor-human-f24e52abba1b"),
        ("sandbox.helpers", Surface::Expected, "enabled, unlocked local sandbox account") => {
            Some("r39-doctor-human-a68ffccafa73")
        }
        (
            "sandbox.helpers",
            Surface::Remedy,
            "enable the elevated Windows sandbox backend permitted by managed policy",
        ) => Some("r39-doctor-human-60700a3dc6d7"),
        (
            "sandbox.helpers",
            Surface::Remedy,
            "run codex sandbox setup --elevated --user <end-user> --codex-home <authoritative-home> from an elevated shell",
        ) => Some("r39-doctor-human-2492f1523152"),
        (
            "sandbox.helpers",
            Surface::Remedy,
            "stop sandbox retries and ask IT to unlock the account",
        ) => Some("r39-doctor-human-c673fa8c5d94"),
        (
            "sandbox.helpers",
            Surface::Remedy,
            "ask IT to enable the sandbox account before rerunning setup",
        ) => Some("r39-doctor-human-e785b8df4d37"),
        (
            "sandbox.helpers",
            Surface::Remedy,
            "ask IT to inspect access to the local sandbox accounts",
        ) => Some("r39-doctor-human-2a93bd038fe5"),
        ("sandbox.helpers", Surface::Remedy, "ask IT to inspect the local sandbox accounts") => {
            Some("r39-doctor-human-a32f2559f01d")
        }
        (
            "sandbox.helpers",
            Surface::Remedy,
            "use an elevated shell or your organization's approved elevation workflow",
        ) => Some("r39-doctor-human-d787f068d646"),
        (
            "sandbox.helpers",
            Surface::Remedy,
            "repair the installed Codex helpers or ask IT to allow their execution",
        ) => Some("r39-doctor-human-dd7fdf8711f2"),
        (
            "sandbox.helpers",
            Surface::Remedy,
            "ask IT to allow the managed local sandbox accounts and group",
        ) => Some("r39-doctor-human-28ad39f57741"),
        (
            "sandbox.helpers",
            Surface::Remedy,
            "rerun elevated setup for the authoritative Codex home or ask IT",
        ) => Some("r39-doctor-human-b7fcc4b9200e"),
        (
            "sandbox.helpers",
            Surface::Remedy,
            "ask IT to allow Codex sandbox rules in managed Windows Firewall policy",
        ) => Some("r39-doctor-human-18d58863b386"),
        (
            "sandbox.helpers",
            Surface::Remedy,
            "repair or reinstall the Codex CLI from an approved distribution",
        ) => Some("r39-doctor-human-f3bac734bd71"),
        ("sandbox.helpers", Surface::Detail("sandbox provisioning"), "complete") => {
            Some("r39-doctor-human-eebbf6457e46")
        }
        ("sandbox.helpers", Surface::Detail("sandbox provisioning"), "failed") => {
            Some("r39-doctor-human-5d28a90f4498")
        }
        ("sandbox.helpers", Surface::Detail("sandbox provisioning"), "incomplete") => {
            Some("r39-doctor-human-85554bb9dd4b")
        }
        ("sandbox.helpers", Surface::Detail("setup failure report"), "unreadable") => {
            Some("r39-doctor-human-da3c01050b1f")
        }
        ("security.endpoint", Surface::Summary, "endpoint protection inspection unavailable") => {
            Some("r39-doctor-human-e2429adefd61")
        }
        ("security.endpoint", Surface::Summary, "no supported endpoint protection detected") => {
            Some("r39-doctor-human-19c6e8eaf39a")
        }
        (
            "security.endpoint",
            Surface::Summary,
            "endpoint protection is not inspected on this platform",
        ) => Some("r39-doctor-human-f2c8ae563b90"),
        (
            "security.endpoint",
            Surface::Summary,
            "endpoint protection detected; Codex exclusions are unverified",
        ) => Some("r39-doctor-human-c14743885fe4"),
        ("security.endpoint", Surface::Expected, "Codex application and helper exclusions") => {
            Some("r39-doctor-human-0acdf3ced680")
        }
        (
            "security.endpoint",
            Surface::Remedy,
            "ask your security administrator to verify Codex exclusions and required helper allowances",
        ) => Some("r39-doctor-human-5bb139a6f55e"),
        (
            "security.endpoint",
            Surface::Remedy,
            "CrowdStrike Falcon: Add a certificate or IOA exclusion for Codex. If sensor overhead continues, exclude the Codex agent from sensor visibility. Keep monitoring descendant processes.",
        ) => Some("r39-doctor-human-7d2676f04519"),
        (
            "security.endpoint",
            Surface::Remedy,
            "BeyondTrust: Remove Codex from application blocking rules. Add allow rules for Codex helper executables. Do not grant administrator privileges.",
        ) => Some("r39-doctor-human-2aaa636cb94e"),
        (
            "security.endpoint",
            Surface::Remedy,
            "Microsoft Defender: Add a certificate or executable-path exclusion for Codex and its helpers. If Attack Surface Reduction blocks Codex, add a rule exclusion. If Controlled Folder Access blocks Codex, allow the app.",
        ) => Some("r39-doctor-human-65211cf88b62"),
        (
            "security.endpoint",
            Surface::Remedy,
            "SentinelOne: Add a signer, file-hash, or executable-path exclusion for Codex and its helpers.",
        ) => Some("r39-doctor-human-fc510ce12336"),
        (
            "security.endpoint",
            Surface::Remedy,
            "Jamf Protect: Add an Override Threat Prevention exception for Codex app and helper signing identities. If analytics cause delays, add an Ignore System Events for Analytics exception.",
        ) => Some("r39-doctor-human-48ee23879315"),
        (
            "security.endpoint",
            Surface::Remedy,
            "Add an exclusion for Codex and its helpers. Use the endpoint product instructions.",
        ) => Some("r39-doctor-human-a43e0553c01e"),
        ("security.endpoint", Surface::Detail("Codex exclusions"), "not verified") => {
            Some("r39-doctor-human-64f365a9a989")
        }
        ("security.endpoint", Surface::Detail("endpoint products"), "unavailable") => {
            Some("r39-doctor-human-ba691ba042bc")
        }
        ("security.endpoint", Surface::Detail("additional endpoint products"), "unavailable") => {
            Some("r39-doctor-human-ba691ba042bc")
        }
        ("security.endpoint", Surface::Detail("endpoint products"), "none detected") => {
            Some("r39-doctor-human-3ff7ddf2d4c6")
        }
        (
            "security.endpoint",
            Surface::Detail("endpoint products"),
            "not inspected on this platform",
        ) => Some("r39-doctor-human-80d751856ad5"),
        (
            "state.rollout_db_parity",
            Surface::Summary,
            "state database thread inventory could not be read",
        ) => Some("r39-doctor-human-2e0861171d64"),
        (
            "state.rollout_db_parity",
            Surface::Cause,
            "state DB thread rows could not be queried",
        ) => Some("r39-doctor-human-81eecb09f80f"),
        (
            "state.rollout_db_parity",
            Surface::Summary,
            "no rollout/state DB inventory to compare",
        ) => Some("r39-doctor-human-d0fb02f38ec5"),
        (
            "state.rollout_db_parity",
            Surface::Summary | Surface::Cause,
            "rollout scan was incomplete or found bad files",
        ) => Some("r39-doctor-human-b633759a23cb"),
        (
            "state.rollout_db_parity",
            Surface::Summary,
            "state DB is missing while rollout files exist",
        ) => Some("r39-doctor-human-3b550599ed0a"),
        (
            "state.rollout_db_parity",
            Surface::Cause,
            "rollout files exist but the state DB is missing",
        ) => Some("r39-doctor-human-bf683c855974"),
        (
            "state.rollout_db_parity",
            Surface::Summary,
            "rollout files and state DB thread inventory agree",
        ) => Some("r39-doctor-human-b964fa8d9a6a"),
        (
            "state.rollout_db_parity",
            Surface::Summary,
            "rollout files and state DB thread inventory differ",
        ) => Some("r39-doctor-human-d63018da1dc0"),
        (
            "state.rollout_db_parity",
            Surface::Cause,
            "rollout files are missing from the state DB",
        ) => Some("r39-doctor-human-a65f15038669"),
        (
            "state.rollout_db_parity",
            Surface::Cause,
            "state DB rows point at missing or unusable rollout files",
        ) => Some("r39-doctor-human-4aabe558322d"),
        (
            "state.rollout_db_parity",
            Surface::Cause,
            "state DB archive flags disagree with rollout file locations",
        ) => Some("r39-doctor-human-85230dd710d6"),
        ("state.rollout_db_parity", Surface::Cause, "duplicate thread inventory entries found") => {
            Some("r39-doctor-human-2036771ff458")
        }
        ("state.rollout_db_parity", Surface::Expected, "readable threads table") => {
            Some("r39-doctor-human-b888156c56e0")
        }
        (
            "state.rollout_db_parity",
            Surface::Expected,
            "state DB contains matching thread rows",
        ) => Some("r39-doctor-human-5bef0a6cfbb3"),
        (
            "state.rollout_db_parity",
            Surface::Expected,
            "rollout directories are fully scannable",
        ) => Some("r39-doctor-human-6a56efb17250"),
        (
            "state.rollout_db_parity",
            Surface::Expected,
            "every rollout file has a matching threads row",
        ) => Some("r39-doctor-human-b841066f6b6e"),
        (
            "state.rollout_db_parity",
            Surface::Expected,
            "every state DB rollout path is a file on disk",
        ) => Some("r39-doctor-human-8e8a27e249a3"),
        (
            "state.rollout_db_parity",
            Surface::Expected,
            "rows under archived_sessions are archived and rows under sessions are active",
        ) => Some("r39-doctor-human-de7395110348"),
        (
            "state.rollout_db_parity",
            Surface::Expected,
            "one rollout path and thread id per thread",
        ) => Some("r39-doctor-human-2636ed04cdec"),
        (
            "state.rollout_db_parity",
            Surface::Remedy,
            "Start Codex with no state DB present so startup backfill can create it from rollout files.",
        ) => Some("r39-doctor-human-20e634feadaf"),
        (
            "state.rollout_db_parity",
            Surface::Remedy,
            "Check file permissions and unexpected files under CODEX_HOME sessions.",
        ) => Some("r39-doctor-human-402ec2ef2a25"),
        (
            "state.rollout_db_parity",
            Surface::Remedy,
            "Attach the doctor report to a bug report so support can inspect samples.",
        ) => Some("r39-doctor-human-d58f2fac632c"),
        (
            "terminal.title",
            Surface::Cause,
            "terminal title configuration contains unknown item identifiers",
        ) => Some("r39-doctor-human-c3f9d401a914"),
        ("terminal.title", Surface::Expected, "known terminal title item identifiers") => {
            Some("r39-doctor-human-7ac6c355c038")
        }
        (
            "terminal.title",
            Surface::Remedy,
            "Remove or replace the unknown entries in [tui].terminal_title.",
        ) => Some("r39-doctor-human-d76b66953a7a"),
        ("terminal.title", Surface::Detail("terminal title source"), "configured") => {
            Some("r39-doctor-human-201582247500")
        }
        ("terminal.title", Surface::Detail("terminal title source"), "default") => {
            Some("r39-doctor-human-37a8eec1ce19")
        }
        ("terminal.title", Surface::Detail("terminal title source"), "disabled") => {
            Some("r39-doctor-human-17eb3c0168d0")
        }
        ("terminal.title", Surface::Detail("title source"), "configured") => {
            Some("r39-doctor-human-201582247500")
        }
        ("terminal.title", Surface::Detail("title source"), "default") => {
            Some("r39-doctor-human-37a8eec1ce19")
        }
        ("terminal.title", Surface::Detail("title source"), "disabled") => {
            Some("r39-doctor-human-17eb3c0168d0")
        }
        ("terminal.title", Surface::Detail("terminal title project source"), "git repo root") => {
            Some("r39-doctor-human-9522e24a1fee")
        }
        ("terminal.title", Surface::Detail("terminal title project source"), "project config") => {
            Some("r39-doctor-human-500e4912c3a3")
        }
        ("terminal.title", Surface::Detail("terminal title project source"), "cwd") => {
            Some("r39-doctor-human-6f2f7e6c6558")
        }
        ("terminal.title", Surface::Detail("project source"), "git repo root") => {
            Some("r39-doctor-human-9522e24a1fee")
        }
        ("terminal.title", Surface::Detail("project source"), "project config") => {
            Some("r39-doctor-human-500e4912c3a3")
        }
        ("terminal.title", Surface::Detail("project source"), "cwd") => {
            Some("r39-doctor-human-6f2f7e6c6558")
        }
        ("updates.status", Surface::Summary, "update configuration is locally consistent") => {
            Some("r39-doctor-human-99076a39d638")
        }
        (
            "updates.status",
            Surface::Detail("npm update target"),
            "not inspected (PATH helpers are not executed)",
        ) => Some("r39-doctor-human-6ae58c1bf6e7"),
        (
            "updates.status",
            Surface::Detail("latest version status"),
            "newer version is available",
        ) => Some("r39-doctor-human-c5fc17e47154"),
        (
            "updates.status",
            Surface::Detail("latest version status"),
            "current version is not older",
        ) => Some("r39-doctor-human-9c72a27499d2"),
        ("updates.status", Surface::Detail("desktop update status"), "ready to install") => {
            Some("r39-doctor-human-519c221901fc")
        }
        ("updates.status", Surface::Detail("desktop update status"), "available") => {
            Some("r39-doctor-human-ddd9818abacf")
        }
        ("updates.status", Surface::Detail("version cache"), "missing") => {
            Some("r39-doctor-human-ffa63583dfa6")
        }
        ("updates.status", Surface::Detail("update action"), "standalone installer") => {
            Some("r39-doctor-human-4ac1f5694e41")
        }
        ("updates.status", Surface::Detail("update action"), "manual or unknown") => {
            Some("r39-doctor-human-4bc42d606be7")
        }
        (
            "updates.status",
            Surface::Detail("desktop update manifest"),
            "response could not be read",
        ) => Some("r39-doctor-human-4d37fd711fb0"),
        (
            "updates.status",
            Surface::Detail("desktop update manifest"),
            "invalid Windows Store update manifest",
        ) => Some("r39-doctor-human-14c91ac84e65"),
        (
            "updates.status",
            Surface::Detail("desktop update manifest"),
            "Windows Store update manifest does not target the production application",
        ) => Some("r39-doctor-human-1aae3aa15b46"),
        (
            "updates.status",
            Surface::Detail("desktop update manifest"),
            "Windows Store update manifest contains an invalid build version",
        ) => Some("r39-doctor-human-eec636c317e9"),
        (
            "updates.status",
            Surface::Detail("desktop update manifest"),
            "installed Windows application has an invalid version",
        ) => Some("r39-doctor-human-574d0b956413"),
        (
            "network.provider_reachability",
            Surface::Summary | Surface::Cause,
            "desktop update and runtime CDN is unreachable",
        ) => Some("r39-doctor-human-e6a4d651cef2"),
        (
            "network.provider_reachability",
            Surface::Expected,
            "desktop update and runtime CDN reachable over HTTPS",
        ) => Some("r39-doctor-human-7c9121961685"),
        (
            "network.provider_reachability",
            Surface::Remedy,
            "check proxy, firewall, DNS, and certificate access to chatgpt.com",
        ) => Some("r39-doctor-human-e8b7ede85a8d"),
        (
            "network.provider_reachability",
            Surface::Remedy,
            "check proxy, firewall, DNS, and certificate access to persistent.oaistatic.com",
        ) => Some("r39-doctor-human-0e0642fdff1a"),
        ("git.worktree.dev_drive", Surface::Summary, "no Git worktree is active") => {
            Some("r39-doctor-human-3994e8d3e7b4")
        }
        (
            "git.worktree.dev_drive",
            Surface::Summary,
            "Windows Dev Drives are unavailable on this Windows version",
        ) => Some("r39-doctor-human-eebe14496195"),
        (
            "git.worktree.dev_drive",
            Surface::Summary,
            "this worktree is not on a Windows Dev Drive; moving it to a trusted Dev Drive can significantly improve repository and filesystem performance",
        ) => Some("r39-doctor-human-cdd89d4a4cf3"),
        (
            "git.worktree.dev_drive",
            Surface::Summary,
            "the active Git worktree is on an untrusted Windows Dev Drive",
        ) => Some("r39-doctor-human-ba610bf72136"),
        (
            "git.worktree.dev_drive",
            Surface::Summary,
            "the active Git worktree is on a trusted Windows Dev Drive",
        ) => Some("r39-doctor-human-3f9123ffb046"),
        (
            "git.worktree.dev_drive",
            Surface::Summary,
            "the active Git worktree's Windows Dev Drive state could not be inspected",
        ) => Some("r39-doctor-human-f143c280f267"),
        (
            "git.worktree.dev_drive",
            Surface::Remedy,
            "create a trusted Windows Dev Drive for source repositories: https://learn.microsoft.com/en-us/windows/dev-drive/",
        ) => Some("r39-doctor-human-b81d03bbb2db"),
        (
            "git.worktree.dev_drive",
            Surface::Remedy,
            "ask your administrator to trust the Windows Dev Drive: https://learn.microsoft.com/en-us/windows/dev-drive/#how-do-i-designate-a-dev-drive-as-trusted",
        ) => Some("r39-doctor-human-3a292f88a2e1"),
        (
            "git.worktree.dev_drive",
            Surface::Remedy,
            "check access to the Git worktree and its Windows storage volume",
        ) => Some("r39-doctor-human-02b327d6fdce"),
        ("app_server.status", Surface::Label("daemon state dir"), "daemon state dir") => {
            Some("r39-doctor-human-6808dca9be9a")
        }
        ("app_server.status", Surface::Label("settings"), "settings") => {
            Some("r39-doctor-human-cde0fb0dec14")
        }
        ("app_server.status", Surface::Label("pid file"), "pid file") => {
            Some("r39-doctor-human-7931fdd36439")
        }
        ("app_server.status", Surface::Label("update-loop pid file"), "update-loop pid file") => {
            Some("r39-doctor-human-4904c04e9cc8")
        }
        ("app_server.status", Surface::Label("dedicated pid file"), "dedicated pid file") => {
            Some("r39-doctor-human-b6864a6a282f")
        }
        (
            "app_server.status",
            Surface::Label("dedicated update-loop pid file"),
            "dedicated update-loop pid file",
        ) => Some("r39-doctor-human-e778bb543374"),
        ("app_server.status", Surface::Label("control socket"), "control socket") => {
            Some("r39-doctor-human-a25d1e6b2d9b")
        }
        ("app_server.status", Surface::Label("status"), "status") => {
            Some("r39-doctor-human-073c1634c496")
        }
        ("app_server.status", Surface::Label("mode"), "mode") => {
            Some("r39-doctor-human-e642b12901a6")
        }
        (
            "app_server.status",
            Surface::Label("configured updater settings"),
            "configured updater settings",
        ) => Some("r39-doctor-human-dab9fd65b585"),
        ("app_server.status", Surface::Label("automatic updates"), "automatic updates") => {
            Some("r39-doctor-human-d91007eb4239")
        }
        ("app_server.status", Surface::Label("update interval"), "update interval") => {
            Some("r39-doctor-human-04852235a739")
        }
        ("app_server.status", Surface::Label("app-server version"), "app-server version") => {
            Some("r39-doctor-human-07536781be3b")
        }
        ("system.disk", Surface::Label("warning threshold"), "warning threshold") => {
            Some("r39-doctor-human-007b44b28260")
        }
        ("system.disk", Surface::Label("failure threshold"), "failure threshold") => {
            Some("r39-doctor-human-e59298aab168")
        }
        ("system.disk", Surface::Label("CODEX_HOME available"), "CODEX_HOME available") => {
            Some("r39-doctor-human-62a897e9adbb")
        }
        ("system.disk", Surface::Label("worktree available"), "worktree available") => {
            Some("r39-doctor-human-ffe68f27823a")
        }
        (
            "sandbox.filesystem_paths",
            Surface::Label("permission profile"),
            "permission profile",
        ) => Some("r39-doctor-human-abaa1b134b93"),
        ("sandbox.filesystem_paths", Surface::Label("operation"), "operation") => {
            Some("r39-doctor-human-9bf5a24e4aa7")
        }
        ("sandbox.filesystem_paths", Surface::Label("probe budgets"), "probe budgets") => {
            Some("r39-doctor-human-b195553898b9")
        }
        ("sandbox.filesystem_paths", Surface::Label("unchecked paths"), "unchecked paths") => {
            Some("r39-doctor-human-d292f83dda43")
        }
        ("sandbox.filesystem_paths", Surface::Label("paths checked"), "paths checked") => {
            Some("r39-doctor-human-497822d3ae83")
        }
        ("git.environment", Surface::Label("git execution"), "git execution") => {
            Some("r39-doctor-human-26796f4f192a")
        }
        ("runtime.search", Surface::Label("search command"), "search command") => {
            Some("r39-doctor-human-b2d1bef12585")
        }
        ("runtime.search", Surface::Label("search provider"), "search provider") => {
            Some("r39-doctor-human-1d695f124452")
        }
        (
            "runtime.search",
            Surface::Label("search command readiness"),
            "search command readiness",
        ) => Some("r39-doctor-human-674a5decc35d"),
        ("runtime.search", Surface::Label("search command path"), "search command path") => {
            Some("r39-doctor-human-536d95b57af7")
        }
        ("network.env", Surface::Label("system proxy"), "system proxy") => {
            Some("r39-doctor-human-cbf9ac8d840a")
        }
        ("network.env", Surface::Label("respect system proxy"), "respect system proxy") => {
            Some("r39-doctor-human-99ede5945f92")
        }
        ("network.env", Surface::Label("managed proxy"), "managed proxy") => {
            Some("r39-doctor-human-30100448d41e")
        }
        ("desktop.app.version", Surface::Label("running"), "running") => {
            Some("r39-doctor-human-c071cf5f5ed6")
        }
        ("desktop.app.version", Surface::Label("log directory"), "log directory") => {
            Some("r39-doctor-human-45c691cb27fd")
        }
        ("sandbox.helpers", Surface::Label("approval policy"), "approval policy") => {
            Some("r39-doctor-human-e9b48317e4c1")
        }
        ("sandbox.helpers", Surface::Label("filesystem sandbox"), "filesystem sandbox") => {
            Some("r39-doctor-human-f2b43d9171a4")
        }
        ("sandbox.helpers", Surface::Label("denied-read rules"), "denied-read rules") => {
            Some("r39-doctor-human-7d17f47baaa8")
        }
        ("sandbox.helpers", Surface::Label("denied-read glob rules"), "denied-read glob rules") => {
            Some("r39-doctor-human-a6f3465fddf3")
        }
        ("sandbox.helpers", Surface::Label("glob scan max depth"), "glob scan max depth") => {
            Some("r39-doctor-human-7e5bff37c0da")
        }
        (
            "sandbox.helpers",
            Surface::Label("managed filesystem source"),
            "managed filesystem source",
        ) => Some("r39-doctor-human-df6e7e992e1b"),
        ("sandbox.helpers", Surface::Label("network sandbox"), "network sandbox") => {
            Some("r39-doctor-human-8c286274fa95")
        }
        ("sandbox.helpers", Surface::Label("sandbox backend"), "sandbox backend") => {
            Some("r39-doctor-human-cab0a0755c43")
        }
        (
            "sandbox.helpers",
            Surface::Label("denied-read restrictions"),
            "denied-read restrictions",
        ) => Some("r39-doctor-human-fb131a0ddd18"),
        ("sandbox.helpers", Surface::Label("setup failure report"), "setup failure report") => {
            Some("r39-doctor-human-51bed544ef7d")
        }
        ("sandbox.helpers", Surface::Label("sandbox provisioning"), "sandbox provisioning") => {
            Some("r39-doctor-human-acb233db8248")
        }
        ("sandbox.helpers", Surface::Label("sandbox account"), "sandbox account") => {
            Some("r39-doctor-human-3068d6678ef7")
        }
        ("sandbox.helpers", Surface::Label("account query status"), "account query status") => {
            Some("r39-doctor-human-c034ef76883a")
        }
        ("sandbox.helpers", Surface::Label("error code"), "error code") => {
            Some("r39-doctor-human-a6145c29902b")
        }
        ("security.endpoint", Surface::Label("endpoint products"), "endpoint products") => {
            Some("r39-doctor-human-a2c90a9b1f54")
        }
        (
            "security.endpoint",
            Surface::Label("additional endpoint products"),
            "additional endpoint products",
        ) => Some("r39-doctor-human-989239096be8"),
        ("security.endpoint", Surface::Label("exclusion targets"), "exclusion targets") => {
            Some("r39-doctor-human-0d2fa71bacf2")
        }
        ("security.endpoint", Surface::Label("Codex exclusions"), "Codex exclusions") => {
            Some("r39-doctor-human-101a946be7ef")
        }
        ("state.rollout_db_parity", Surface::Label("rollout DB rows"), "rollout DB rows") => {
            Some("r39-doctor-human-066f74ef49b9")
        }
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB active rows"),
            "rollout DB active rows",
        ) => Some("r39-doctor-human-5ac58b0438f0"),
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB archived rows"),
            "rollout DB archived rows",
        ) => Some("r39-doctor-human-414ac20eeacc"),
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB missing active rows"),
            "rollout DB missing active rows",
        ) => Some("r39-doctor-human-9b2615fff50d"),
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB missing archived rows"),
            "rollout DB missing archived rows",
        ) => Some("r39-doctor-human-13ca420a5a2b"),
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB stale rows"),
            "rollout DB stale rows",
        ) => Some("r39-doctor-human-4634ad05aa38"),
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB archive mismatches"),
            "rollout DB archive mismatches",
        ) => Some("r39-doctor-human-29d3524b1969"),
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB duplicate rollout thread ids"),
            "rollout DB duplicate rollout thread ids",
        ) => Some("r39-doctor-human-5b7d99cd671f"),
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB duplicate DB paths"),
            "rollout DB duplicate DB paths",
        ) => Some("r39-doctor-human-8ecfc5a66e26"),
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB model providers"),
            "rollout DB model providers",
        ) => Some("r39-doctor-human-3694fc00d83b"),
        ("state.rollout_db_parity", Surface::Label("rollout DB sources"), "rollout DB sources") => {
            Some("r39-doctor-human-becb21284ce1")
        }
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB missing active sample"),
            "rollout DB missing active sample",
        ) => Some("r39-doctor-human-c20076a5ee8c"),
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB missing archived sample"),
            "rollout DB missing archived sample",
        ) => Some("r39-doctor-human-27fe6054ff20"),
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB stale row sample"),
            "rollout DB stale row sample",
        ) => Some("r39-doctor-human-7f4066c0fb39"),
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB archive mismatch sample"),
            "rollout DB archive mismatch sample",
        ) => Some("r39-doctor-human-6c74001aa9d5"),
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB duplicate rollout thread id sample"),
            "rollout DB duplicate rollout thread id sample",
        ) => Some("r39-doctor-human-3cdb161d37f4"),
        (
            "state.rollout_db_parity",
            Surface::Label("rollout DB duplicate DB path sample"),
            "rollout DB duplicate DB path sample",
        ) => Some("r39-doctor-human-3bbf04a75bed"),
        ("updates.status", Surface::Label("update action"), "update action") => {
            Some("r39-doctor-human-49e8a04360f8")
        }
        ("updates.status", Surface::Label("version cache"), "version cache") => {
            Some("r39-doctor-human-4ab4d42c36f9")
        }
        ("updates.status", Surface::Label("cached latest version"), "cached latest version") => {
            Some("r39-doctor-human-006a74f95c88")
        }
        ("updates.status", Surface::Label("last checked at"), "last checked at") => {
            Some("r39-doctor-human-f712c736807e")
        }
        ("updates.status", Surface::Label("dismissed version"), "dismissed version") => {
            Some("r39-doctor-human-46dc8051a064")
        }
        ("updates.status", Surface::Label("latest version"), "latest version") => {
            Some("r39-doctor-human-5fe87164c192")
        }
        ("updates.status", Surface::Label("latest version status"), "latest version status") => {
            Some("r39-doctor-human-4b669db73703")
        }
        ("updates.status", Surface::Label("latest version probe"), "latest version probe") => {
            Some("r39-doctor-human-4169ada59d47")
        }
        ("updates.status", Surface::Label("desktop update status"), "desktop update status") => {
            Some("r39-doctor-human-36c01855d161")
        }
        ("updates.status", Surface::Label("desktop latest build"), "desktop latest build") => {
            Some("r39-doctor-human-fafd868923d8")
        }
        ("updates.status", Surface::Label("desktop application"), "desktop application") => {
            Some("r39-doctor-human-48de30184762")
        }
        (
            "updates.status",
            Surface::Label("desktop update manifest"),
            "desktop update manifest",
        ) => Some("r39-doctor-human-fc81fdee7b2b"),
        ("updates.status", Surface::Label("npm update target"), "npm update target") => {
            Some("r39-doctor-human-c3a4ff3ea575")
        }
        ("updates.status", Surface::Label("version cache parse"), "version cache parse") => {
            Some("r39-doctor-human-34ab4672a6ea")
        }
        ("updates.status", Surface::Label("version cache read"), "version cache read") => {
            Some("r39-doctor-human-378df2d364cb")
        }
        ("git.worktree.dev_drive", Surface::Label("filesystem error"), "filesystem error") => {
            Some("r39-doctor-human-ec123adb05af")
        }
        (
            "network.provider_reachability",
            Surface::Remedy,
            "A macOS system proxy is configured but unused. If your organization requires it, ask your administrator whether to enable the experimental System proxy setting in `/experimental` or with `codex features enable respect_system_proxy`. Restart Codex after changing this setting.",
        ) => Some("r39-doctor-human-ac16b90eb673"),
        (
            "network.websocket_reachability",
            Surface::Remedy,
            "A macOS system proxy is configured but unused. If your organization requires it, ask your administrator whether to enable the experimental System proxy setting in `/experimental` or with `codex features enable respect_system_proxy`. Restart Codex after changing this setting.",
        ) => Some("r39-doctor-human-ac16b90eb673"),
        _ => None,
    };
    if let Some(id) = message_id {
        return Cow::Owned(message_for(locale, id, &[]));
    }
    if check_id == "app_server.status" && matches!(surface, Surface::Detail("update interval")) {
        if let Some(value) = capture(raw, "", " minutes (configured)").filter(|value| {
            value
                .parse::<u32>()
                .is_ok_and(|n| n > 0 && n.to_string() == *value)
        }) {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-685c7fa78e60",
                &[("value", value)],
            ));
        }
    }
    if check_id == "system.disk" && matches!(surface, Surface::Summary) {
        if let Some(value) =
            capture(raw, "critically low disk space (", ")").filter(|value| capacity_value(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-6f8a592bdabe",
                &[("value", value)],
            ));
        }
    }
    if check_id == "system.disk" && matches!(surface, Surface::Summary) {
        if let Some(value) =
            capture(raw, "low disk space (", ")").filter(|value| capacity_value(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-dfb87a46d404",
                &[("value", value)],
            ));
        }
    }
    if check_id == "system.disk" && matches!(surface, Surface::Summary) {
        if let Some(value) =
            capture(raw, "sufficient free disk space (", ")").filter(|value| capacity_value(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-d267e2863713",
                &[("value", value)],
            ));
        }
    }
    if check_id == "system.disk" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(raw, "", " has insufficient disk space")
            .filter(|value| ["CODEX_HOME", "worktree"].contains(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-3047508b0d85",
                &[("value", value)],
            ));
        }
    }
    if check_id == "system.disk" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(raw, "disk space for ", " could not be checked")
            .filter(|value| ["CODEX_HOME", "worktree"].contains(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-51ac11eb6852",
                &[("value", value)],
            ));
        }
    }
    if check_id == "system.disk" && matches!(surface, Surface::Expected) {
        if let Some(value) = capture(raw, "at least ", "").filter(|value| capacity_value(value)) {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-eb3b1a05850a",
                &[("value", value)],
            ));
        }
    }
    if check_id == "git.environment" && matches!(surface, Surface::Detail(".git entry")) {
        if let Some(value) = capture(raw, "file -> ", "") {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-15c20cf56019",
                &[("value", value)],
            ));
        }
    }
    if check_id == "git.environment" && matches!(surface, Surface::Detail(".git entry")) {
        if let Some(value) = capture(raw, "unreadable (", ")") {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-2b60f9e0d102",
                &[("value", value)],
            ));
        }
    }
    if check_id == "runtime.search" && matches!(surface, Surface::Summary) {
        if let Some(value) = capture(raw, "search command found (", "); execution not verified") {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-fd91d829b17e",
                &[("value", value)],
            ));
        }
    }
    if check_id == "security.endpoint" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(
            raw,
            "",
            " can interfere with Codex. Verify Codex exclusions.",
        ) {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-1ba8b8c00796",
                &[("value", value)],
            ));
        }
    }
    if check_id == "network.provider_reachability"
        && matches!(surface, Surface::Detail("desktop assets CDN"))
    {
        if let Some(value) = capture(
            raw,
            "",
            " TLS handshake or certificate validation failed (optional)",
        )
        .filter(|value| value.starts_with("https://") && !value.chars().any(char::is_whitespace))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-0913d456ced6",
                &[("value", value)],
            ));
        }
    }
    if check_id == "network.provider_reachability"
        && matches!(surface, Surface::Detail("desktop assets CDN"))
    {
        if let Some(value) =
            capture(raw, "", " proxy authentication required (optional)").filter(|value| {
                value.starts_with("https://") && !value.chars().any(char::is_whitespace)
            })
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-c7be3ee8a146",
                &[("value", value)],
            ));
        }
    }
    if check_id == "network.provider_reachability"
        && matches!(surface, Surface::Detail("desktop assets CDN"))
    {
        if let Some(value) =
            capture(raw, "", " invalid proxy configuration (optional)").filter(|value| {
                value.starts_with("https://") && !value.chars().any(char::is_whitespace)
            })
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-cbb2fe3b0fd0",
                &[("value", value)],
            ));
        }
    }
    if check_id == "network.provider_reachability"
        && matches!(surface, Surface::Detail("desktop assets CDN"))
    {
        if let Some(value) = capture(
            raw,
            "",
            " system proxy configuration unavailable (optional)",
        )
        .filter(|value| value.starts_with("https://") && !value.chars().any(char::is_whitespace))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-57dbdaae8b84",
                &[("value", value)],
            ));
        }
    }
    if check_id == "network.provider_reachability"
        && matches!(surface, Surface::Detail("desktop assets CDN"))
    {
        if let Some(value) = capture(raw, "", " request timed out (optional)").filter(|value| {
            value.starts_with("https://") && !value.chars().any(char::is_whitespace)
        }) {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-ca4cf8184432",
                &[("value", value)],
            ));
        }
    }
    if check_id == "network.provider_reachability"
        && matches!(surface, Surface::Detail("desktop assets CDN"))
    {
        if let Some(value) =
            capture(raw, "", " unsupported proxy configuration (optional)").filter(|value| {
                value.starts_with("https://") && !value.chars().any(char::is_whitespace)
            })
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-1008b1b125aa",
                &[("value", value)],
            ));
        }
    }
    if check_id == "network.provider_reachability"
        && matches!(surface, Surface::Detail("desktop assets CDN"))
    {
        if let Some(value) =
            capture(raw, "", " proxy resolution failed (optional)").filter(|value| {
                value.starts_with("https://") && !value.chars().any(char::is_whitespace)
            })
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-2884aeb02944",
                &[("value", value)],
            ));
        }
    }
    if check_id == "network.provider_reachability"
        && matches!(surface, Surface::Detail("desktop assets CDN"))
    {
        if let Some(value) = capture(raw, "", " connect failed (optional)").filter(|value| {
            value.starts_with("https://") && !value.chars().any(char::is_whitespace)
        }) {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-d6ef4d150df2",
                &[("value", value)],
            ));
        }
    }
    if check_id == "network.provider_reachability"
        && matches!(surface, Surface::Detail("desktop assets CDN"))
    {
        if let Some(value) = capture(raw, "", " request failed (optional)").filter(|value| {
            value.starts_with("https://") && !value.chars().any(char::is_whitespace)
        }) {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-68b025270335",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths"
        && matches!(surface, Surface::Detail(field) if numbered_path_field(field))
    {
        if let Some(value) = capture(raw, "", "; path resolved (read/write access not tested)") {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-de5907f2a2d1",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(raw, "", ": path resolved (read/write access not tested)")
            .filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-507ac8620f72",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(
            raw,
            "",
            ": path resolved (read/write access not tested); slow path probe",
        )
        .filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-c116f3a750f5",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths"
        && matches!(surface, Surface::Detail(field) if numbered_path_field(field))
    {
        if let Some(value) = capture(raw, "", "; missing (may be intentional)") {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-72129b1d7be4",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(raw, "", ": missing (may be intentional)")
            .filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-7d2db1fdcf72",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(raw, "", ": missing (may be intentional); slow path probe")
            .filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-0c2d23c18ba1",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths"
        && matches!(surface, Surface::Detail(field) if numbered_path_field(field))
    {
        if let Some(value) = capture(raw, "", "; access denied to doctor") {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-c3b1a1f8f79e",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) =
            capture(raw, "", ": access denied to doctor").filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-9cbf55492e24",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(raw, "", ": access denied to doctor; slow path probe")
            .filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-7f3273e3083e",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths"
        && matches!(surface, Surface::Detail(field) if numbered_path_field(field))
    {
        if let Some(value) = capture(raw, "", "; path probe timed out") {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-f6661cd2a605",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) =
            capture(raw, "", ": path probe timed out").filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-8b79a6716084",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(raw, "", ": path probe timed out; slow path probe")
            .filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-5dc3db30c01d",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths"
        && matches!(surface, Surface::Detail(field) if numbered_path_field(field))
    {
        if let Some(value) = capture(raw, "", "; path probe failed") {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-4ce4642666a3",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) =
            capture(raw, "", ": path probe failed").filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-b9686e8d0533",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(raw, "", ": path probe failed; slow path probe")
            .filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-1acaefcb0f25",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths"
        && matches!(surface, Surface::Detail(field) if numbered_path_field(field))
    {
        if let Some(value) = capture(raw, "", "; path is not native to this host; not probed") {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-e5ff9b06273a",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(raw, "", ": path is not native to this host; not probed")
            .filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-5f66b2430bc5",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(
            raw,
            "",
            ": path is not native to this host; not probed; slow path probe",
        )
        .filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-5fc49212f218",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths"
        && matches!(surface, Surface::Detail(field) if numbered_path_field(field))
    {
        if let Some(value) = capture(
            raw,
            "",
            "; not probed on Windows (network authentication risk)",
        ) {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-f7ffff79f3bc",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(
            raw,
            "",
            ": not probed on Windows (network authentication risk)",
        )
        .filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-25c7e67cb397",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(
            raw,
            "",
            ": not probed on Windows (network authentication risk); slow path probe",
        )
        .filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-12086f20bb39",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths"
        && matches!(surface, Surface::Detail(field) if numbered_path_field(field))
    {
        if let Some(value) = capture(raw, "", "; not probed (filesystem read restrictions apply)") {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-5acac3e7c7f1",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(raw, "", ": not probed (filesystem read restrictions apply)")
            .filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-5338044cc7fa",
                &[("value", value)],
            ));
        }
    }
    if check_id == "sandbox.filesystem_paths" && matches!(surface, Surface::Cause) {
        if let Some(value) = capture(
            raw,
            "",
            ": not probed (filesystem read restrictions apply); slow path probe",
        )
        .filter(|value| numbered_path_field(value))
        {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-76da871cb7bc",
                &[("value", value)],
            ));
        }
    }
    if check_id == "app_server.status"
        && matches!(surface, Surface::Detail("app-server version"))
        && status == CheckStatus::Warning
    {
        if let Some(value) = capture(raw, "unavailable (", ")") {
            return Cow::Owned(message_for(
                locale,
                "r39-doctor-human-3f7056b9acd7",
                &[("value", value)],
            ));
        }
    }
    Cow::Borrowed(raw)
}

fn capture<'a>(raw: &'a str, before: &str, after: &str) -> Option<&'a str> {
    raw.strip_prefix(before)?
        .strip_suffix(after)
        .filter(|value| !value.is_empty())
}

fn numbered_path_field(field: &str) -> bool {
    field.strip_prefix("path ").is_some_and(|number| {
        number
            .parse::<u8>()
            .is_ok_and(|n| (1..=32).contains(&n) && n.to_string() == number)
    })
}

fn capacity_value(value: &str) -> bool {
    let Some(number) = value
        .strip_suffix(" GiB")
        .or_else(|| value.strip_suffix(" MiB"))
    else {
        return false;
    };
    let Some((whole, fraction)) = number.split_once('.') else {
        return false;
    };
    !whole.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && fraction.len() == 1
        && fraction.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn korean(id: &str, status: CheckStatus, surface: Surface<'_>, raw: &str) -> String {
        for_locale(Locale::Korean, id, status, surface, raw).into_owned()
    }

    #[test]
    fn closed_summary_requires_the_known_check_and_surface() {
        let raw = "background server is running";
        assert_eq!(
            korean("app_server.status", CheckStatus::Ok, Surface::Summary, raw),
            "백그라운드 서버가 실행 중입니다"
        );
        for (id, surface) in [
            ("user.server", Surface::Summary),
            ("app_server.status", Surface::Detail("error")),
            ("app_server.status", Surface::Cause),
        ] {
            assert_eq!(korean(id, CheckStatus::Ok, surface, raw), raw);
        }
    }

    #[test]
    fn ambiguous_runtime_and_environment_values_stay_opaque() {
        for raw in [
            "not set",
            "unavailable",
            "path is not a file",
            "file exists",
            "C:\\자료\\not set.exe",
        ] {
            assert_eq!(
                korean(
                    "system.environment",
                    CheckStatus::Ok,
                    Surface::Detail("EDITOR"),
                    raw
                ),
                raw
            );
            assert_eq!(
                korean(
                    "runtime.search",
                    CheckStatus::Warning,
                    Surface::Detail("search command readiness"),
                    raw
                ),
                raw
            );
        }
        assert_eq!(
            korean(
                "runtime.search",
                CheckStatus::Ok,
                Surface::Detail("search command readiness"),
                "file exists"
            ),
            "파일 있음"
        );
    }

    #[test]
    fn dynamic_error_is_preserved_and_server_version_is_not_an_error() {
        let raw = "unavailable (Access denied {token}; C:\\자료\\x.exe)";
        assert_eq!(
            korean(
                "app_server.status",
                CheckStatus::Warning,
                Surface::Detail("app-server version"),
                raw
            ),
            "확인할 수 없음 (Access denied {token}; C:\\자료\\x.exe)"
        );
        assert_eq!(
            korean(
                "app_server.status",
                CheckStatus::Ok,
                Surface::Detail("app-server version"),
                raw
            ),
            raw
        );
        assert_eq!(
            korean(
                "app_server.status",
                CheckStatus::Warning,
                Surface::Detail("server payload"),
                raw
            ),
            raw
        );
    }

    #[test]
    fn disk_capacity_wrapper_accepts_only_the_producer_shape() {
        assert_eq!(
            korean(
                "system.disk",
                CheckStatus::Warning,
                Surface::Summary,
                "low disk space (2.5 GiB)"
            ),
            "디스크 여유 공간이 부족합니다 (2.5 GiB)"
        );
        for raw in [
            "low disk space (Access denied)",
            "low disk space (2 GiB)",
            "low disk space (2.50 GiB)",
            "low disk space (2.5 GiB) extra",
        ] {
            assert_eq!(
                korean("system.disk", CheckStatus::Warning, Surface::Summary, raw),
                raw
            );
        }
    }

    #[test]
    fn filesystem_outcomes_require_a_canonical_probe_field() {
        let raw = "C:\\자료\\{opaque} (read); path probe failed";
        assert_eq!(
            korean(
                "sandbox.filesystem_paths",
                CheckStatus::Warning,
                Surface::Detail("path 1"),
                raw
            ),
            "C:\\자료\\{opaque} (read); 경로 검사 실패"
        );
        for field in ["path 0", "path 33", "path 01", "path 1 source", "error"] {
            assert_eq!(
                korean(
                    "sandbox.filesystem_paths",
                    CheckStatus::Warning,
                    Surface::Detail(field),
                    raw
                ),
                raw
            );
        }
        assert_eq!(
            korean(
                "sandbox.filesystem_paths",
                CheckStatus::Warning,
                Surface::Cause,
                "path 32: path probe failed; slow path probe"
            ),
            "path 32: 경로 검사 실패; 경로 검사 응답이 느림"
        );
        let raw = "actual error: path probe failed";
        assert_eq!(
            korean(
                "sandbox.filesystem_paths",
                CheckStatus::Warning,
                Surface::Cause,
                raw
            ),
            raw
        );
    }

    #[test]
    fn unknown_values_and_english_are_byte_preserved() {
        for raw in [
            "not set",
            "https://example.test/a?x={value}",
            "codex --model GPT-opaque",
            "C:\\資料\\file.rs",
            "request failed\nraw error",
        ] {
            assert_eq!(
                korean("unknown.check", CheckStatus::Warning, Surface::Summary, raw),
                raw
            );
            assert_eq!(
                for_locale(
                    Locale::English,
                    "app_server.status",
                    CheckStatus::Warning,
                    Surface::Summary,
                    raw
                ),
                raw
            );
        }
        assert_eq!(
            for_locale(
                Locale::English,
                "app_server.status",
                CheckStatus::Ok,
                Surface::Summary,
                "background server is running"
            ),
            "background server is running"
        );
    }

    #[test]
    fn rendering_does_not_modify_source_report_values() {
        let check = DoctorCheck::new(
            "app_server.status",
            "app_server",
            CheckStatus::Warning,
            "background server socket is stale or unreachable",
        )
        .detail("status: stale or unreachable")
        .detail("server payload: {opaque}");
        let summary = check.summary.clone();
        let details = check.details.clone();
        let _ = for_display(&check, Surface::Summary, &check.summary);
        let _ = for_display(&check, Surface::Detail("server payload"), "{opaque}");
        assert_eq!(check.summary, summary);
        assert_eq!(check.details, details);
    }

    #[test]
    fn typed_missing_locale_and_editor_differ_from_actual_sentinel_values() {
        let mut check = DoctorCheck::new(
            "system.environment",
            "system",
            CheckStatus::Ok,
            "OS language unavailable",
        )
        .detail("os language: unavailable")
        .detail("EDITOR: not set")
        .detail("VISUAL: not set");
        let json = serde_json::to_vec(&check).unwrap();
        assert_eq!(
            for_check_locale(
                Locale::Korean,
                &check,
                Surface::Detail("os language"),
                "unavailable"
            ),
            "unavailable"
        );
        assert_eq!(
            for_check_locale(Locale::Korean, &check, Surface::Detail("EDITOR"), "not set"),
            "not set"
        );
        check.human_origin = DoctorHumanOrigin::SystemEnvironment {
            os_language_missing: true,
            missing_editor_env: vec!["EDITOR"],
        };
        assert_eq!(
            for_check_locale(
                Locale::Korean,
                &check,
                Surface::Detail("os language"),
                "unavailable"
            ),
            "확인할 수 없음"
        );
        assert_eq!(
            for_check_locale(
                Locale::Korean,
                &check,
                Surface::Detail("OS language"),
                "unavailable"
            ),
            "확인할 수 없음"
        );
        assert_eq!(
            for_check_locale(Locale::Korean, &check, Surface::Detail("EDITOR"), "not set"),
            "설정되지 않음"
        );
        assert_eq!(
            for_check_locale(Locale::Korean, &check, Surface::Detail("VISUAL"), "not set"),
            "not set"
        );
        assert_eq!(
            for_check_locale(
                Locale::Korean,
                &check,
                Surface::Detail("EDITOR"),
                "raw actual environment"
            ),
            "raw actual environment"
        );
        assert_eq!(
            for_check_locale(
                Locale::English,
                &check,
                Surface::Detail("EDITOR"),
                "not set"
            ),
            "not set"
        );
        assert_eq!(serde_json::to_vec(&check).unwrap(), json);
        check.id = "unknown.check".to_string();
        assert_eq!(
            for_check_locale(Locale::Korean, &check, Surface::Detail("EDITOR"), "not set"),
            "not set"
        );
    }

    #[test]
    fn typed_nonfile_readiness_does_not_translate_a_real_io_error() {
        let mut check = DoctorCheck::new(
            "runtime.search",
            "search",
            CheckStatus::Warning,
            "search command could not be verified",
        )
        .detail("search command readiness: path is not a file");
        let raw = "path is not a file";
        let json = serde_json::to_vec(&check).unwrap();
        assert_eq!(
            for_check_locale(
                Locale::Korean,
                &check,
                Surface::Detail("search command readiness"),
                raw
            ),
            raw
        );
        check.human_origin = DoctorHumanOrigin::SearchPathNotFile;
        assert_eq!(
            for_check_locale(
                Locale::Korean,
                &check,
                Surface::Detail("search command readiness"),
                raw
            ),
            "경로가 파일이 아님"
        );
        assert_eq!(
            for_check_locale(Locale::Korean, &check, Surface::Detail("raw io error"), raw),
            raw
        );
        assert_eq!(
            for_check_locale(
                Locale::English,
                &check,
                Surface::Detail("search command readiness"),
                raw
            ),
            raw
        );
        for locale in [Locale::English, Locale::Korean] {
            let _ = for_check_locale(
                locale,
                &check,
                Surface::Detail("search command readiness"),
                raw,
            );
            assert_eq!(serde_json::to_vec(&check).unwrap(), json);
        }
    }
}
