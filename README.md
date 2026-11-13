# Arcvera-Core

[English](README.md) · [한국어](README.ko.md)

An independently maintained Windows CLI and Rust App Server derived from OpenAI Codex. The public command is `arcvera`. Arcvera-Core currently communicates with OpenAI servers; upstream names retained in protocol identifiers and helper components identify implementation provenance. It is not an official OpenAI product.

[Download releases](https://github.com/jaeseongs95/Arcvera-Core/releases) · [Build from source](docs/source-import.md)

## Start on Windows

Download the complete Windows x86-64 ZIP and `SHA256SUMS.txt`, compare the ZIP hash, and extract to a new folder. For version 1.0.1:

```powershell
Get-FileHash .\arcvera-core-1.0.1-windows-x86_64.zip -Algorithm SHA256
```

In the extracted folder:

```powershell
.\bin\arcvera.exe
```

| Entry point | Behavior |
| --- | --- |
| `bin\arcvera.exe` or `arcvera.cmd` | Default startup using Arcvera's local shared server. |
| `arcvera-local.cmd` | Starts an embedded backend with `--no-daemon`. |
| `bin\arcvera.exe --remote <address>` | Connects to the explicitly selected remote server with appropriate authentication. |

The default home is `~/.arcvera`; set `ARCVERA_HOME` to select an existing directory. `CODEX_HOME` is ignored. Existing Codex data is not imported automatically. Sign in separately for the new Arcvera home. An official Codex or ChatGPT installation is not required.

Keep `codex-package.json`, `package-receipt.json`, the `bin` folder and all helper resources together. Individual executables are not a complete managed-server package. The internal helpers remain named `codex-app-server.exe` and `codex-code-mode-host.exe` and must come from the same release. Retain the previous package for recovery.

Native voice requires the Microsoft Visual C++ v14 **X64** runtime installed separately. Microsoft DLLs and installers are not bundled; see [runtime requirements](tools/windows-cli/RUNTIME.md).

## Current releases

Arcvera-Core **1.0.1** is the native home-isolation and update-policy hotfix. It introduces `arcvera`, `ARCVERA_HOME` and `~/.arcvera`, and disables inherited automatic installers and update feeds. Download new versions manually from this repository's releases. See the [bilingual release note](docs/releases/v1.0.1.md) for the source snapshot and final executable verification record.

The previous [1.0.0 release](docs/releases/R41.md) introduced the R41 conversation and version-contract changes. Its historical test results do not establish the correctness of 1.0.1. Broader architecture work remains in [R42](docs/roadmap/R42.md). Release versions are independent of upstream Codex 0.167.0.

## Conversation controls

Enter these in the CLI conversation input, not PowerShell.

| Command | Purpose |
| --- | --- |
| `/context 120000` · `/context reset` | Set or reset the conversation context override |
| `/memory view` · `/memory enable on` | Inspect memory or explicitly enable it |
| `/cron add --every 10m -- Check whether the build has finished` | Schedule a repeating prompt for this CLI session |
| `/cron list` · `/cron remove 1` | Inspect schedules or remove the actual schedule ID |
| `/subagents` | Inspect child sessions |
| `/background` or `F6` | Background a supported terminal task |
| `/reload_plugin` | Request plugin reload |
| `/language ko` · `/language en` | Save the global UI language for the next launch |

Context overrides cannot exceed the model limit. Memory corrections are not immediate summary consolidation. Schedules skip busy occurrences and disappear when the CLI exits. Natural-language controls need the running CLI's tools. Reload acknowledgement does not prove every MCP server is ready.


Automatic updates are disabled. `arcvera update` and inherited daemon updater commands report the manual-download policy. Explicit `app-server daemon update --from-cli` remains available for installing the complete local package into Arcvera's own managed-server directory.

## Desktop and shared servers

[OpenDIY Desktop](https://github.com/jaeseongs95/opendiy-desktop) is a separate client. It must launch `arcvera` and explicitly select `ARCVERA_HOME` when using a nondefault profile. Selecting a new CLI file does not replace a running server. Clients must declare the server's exact full Core version at initialization.

Finish active work before an explicit managed-server replacement. Follow the [shared-server procedure](docs/releases/v1.0.1.md#shared-server-update), then verify the actual server version and path. No automatic profile migration or running-server replacement is performed by extracting the ZIP.

## Validation and limits

The [release note](docs/releases/v1.0.1.md) separates source tests, final binaries and unrun cases. The originally reported nested SQLite/socket error has not been reproduced in a fresh 1.0.0 fixture; home collision and inherited updater behavior are independently confirmed defects. Their correction is not proof of the original error's direct cause.

Real-account/model behavior, physical IME, voice, sustained workloads, and clean-PC runtime installation require separate evidence. Full removal of upstream implementation names and multi-vendor login are not included in this hotfix.

[Validation](docs/TESTING.md) · [Source build](docs/source-import.md) · [Windows packaging](tools/windows-cli/README.md) · [Modifications](MODIFICATIONS.md)

The [Core 1.0 workflow](.github/workflows/core-1.0.yml) checks the source inventory, patch replay and packaging fixtures on Linux. It does not compile product binaries or establish native Windows runtime acceptance. Product builds and runtime tests run locally.

## License

Independent work based on [OpenAI Codex](https://github.com/openai/codex). Source and build tools are stored in Git; executable assets are published in Releases. Original copyright, attribution and third-party notices are retained.

[AGPL-3.0-or-later](LICENSE) · [Licensing](LICENSING.md) · [Original Apache-2.0](LICENSE-APACHE-2.0) · [NOTICE](NOTICE)
