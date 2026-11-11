# Arcvera-Core

[English](README.md) · [한국어](README.ko.md)

An independently maintained Windows CLI and Rust App Server derived from OpenAI Codex. It adds conversation controls and repairs confirmed defects while preserving authentication, permission checks, stored conversations, and ongoing work.

Arcvera-Core supports communication with OpenAI servers. Compatibility or interchangeability with the official Codex CLI is outside its support scope. Upstream version references identify source provenance; Arcvera-Core maintains its own commands, protocol, storage and release contracts.

[Download latest release](https://github.com/jaeseongs95/Arcvera-Core/releases/latest) · [Release history](https://github.com/jaeseongs95/Arcvera-Core/releases) · [Build from source](docs/source-import.md)

## Start on Windows

1. Download the complete Windows x86-64 ZIP and `SHA256SUMS.txt` from the release page.
2. Compare the ZIP's SHA-256 with the checksum file, then extract it into a new folder with its directory layout intact.
3. Run `bin\codex.exe`. Source compilation and CMD launchers are optional.

For R40, run the first command in the download folder and the second in the extracted package folder:

```powershell
Get-FileHash .\opendiy-codex-r40-windows-x86_64.zip -Algorithm SHA256
```

```powershell
.\bin\codex.exe
```

Keep `codex-package.json`, `package-receipt.json`, and the `bin` folder together. Retain the previous package for recovery.

### Choose a connection

| Entry point | Behavior |
| --- | --- |
| `bin\codex.exe` or `codex-patched.cmd` | Default startup; may reuse a compatible local shared server, depending on installation and settings. |
| `codex-local.cmd` | Uses `--no-daemon` to start a separate embedded backend. |
| `bin\codex.exe --remote <address>` | Connects to an explicit remote server; requires the appropriate authentication. |

The ZIP already includes `codex-patched.cmd`, which passes arguments to `bin\codex.exe`. It provides a convenient default launch that can reuse the shared server. It does not replace a server version, configure a remote address, or pair a mobile device. An incompatible or unverified implicit server can cause startup to use the embedded backend. Check the actual server identity separately from the selected CLI file.

To replace the managed shared server, follow the [R40 server update procedure](https://github.com/jaeseongs95/Arcvera-Core/releases/tag/v0.167.0-r40.20261108). Use the complete ZIP and a separate PowerShell window after finishing active work. Changing an EXE path alone does not update a running server.

### Individual executables

Releases also provide `codex.exe`, `codex-app-server.exe`, and `codex-code-mode-host.exe` separately. Use the same release for all three. Keep the helper beside the executable being run, with the exact name `codex-code-mode-host.exe`; a missing helper can cause `os error 2`. If the CLI and server share a folder, put all three files there.

These three files are not a complete managed-server installation package. Voice resources, launchers, updater support, and `app-server daemon update --from-cli` require the complete ZIP. Download and version checks do not establish first-time installation on a new PC.

## Current releases

Arcvera-Core 0.168.0 (R41 scope) is in development: [draft notes and SemVer policy (EN/KO)](docs/releases/R41.md), [indexed-context design and recovery](docs/context-index.md). A selected task and its dependency closure define eligible model API context, alongside preserved user/control and unclassified inputs. Cache observations report response-level counters; unprovided TTL/expiry remains unknown. This is not a verified downloadable release. The broader architecture work previously assigned to R41 moves to [R42](docs/roadmap/R42.md). Release versions are independent of upstream Codex 0.167.0; R41/R42 track scope. Final-source local regression passed 89 distinct tests in 20 suites, and 180 synthetic projection samples reduced request bytes by 89.0–90.0%. Final executables and independent audit remain pending; actual app/model/network performance is unmeasured; R39/R40 results do not establish acceptance. Builds and tests run locally; GitHub/Actions builds are excluded.

| Release | Main change | Verification boundary |
| --- | --- | --- |
| [R40](https://github.com/jaeseongs95/Arcvera-Core/releases/tag/v0.167.0-r40.20261108) · [Notes (EN/KO)](docs/releases/R40.md) | Temporary MCP reconnection, nonblocking context lookup, hook boundaries, scrolling, Korean text, and confirmed memory/provenance fixes | Final CLI, RPC, ConPTY, and isolated TEST Desktop checks; see the notes for failures and unrun cases. |
| [R39](https://github.com/jaeseongs95/Arcvera-Core/releases/tag/v0.167.0-r39.20261107) · [Notes (EN/KO)](docs/releases/R39.md) | Compaction focused on unfinished work | 15 isolated final-package checks plus source compaction tests. Final Desktop joint testing was not run. |

Both are based on official Codex 0.167.0. Publication does not establish the version installed on a PC. Earlier versions remain in the [release history](https://github.com/jaeseongs95/Arcvera-Core/releases).

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

Run updater commands in PowerShell: `codex update --check`, `codex update --status`, or `codex update`. They require a supported installed package; see the [updater guide](tools/windows-cli/UPDATE.ko.md).

## Desktop and shared servers

[OpenDIY Desktop](https://github.com/jaeseongs95/opendiy-desktop) is a separate project for the desktop UI and multi-session workflows. This repository supplies the CLI, App Server, and Windows package. Selecting a new CLI in Desktop does not replace an already running shared server.

Desktop 1.1.0 OpenDIY extensions require at least CLI and App Server R36 (`0.166.0+native-r36.20261103`). This minimum does not prove compatibility with every later combination. R40 integration checks cover the isolated TEST Desktop candidate used in the tests, not every published Desktop installation. R39 final Desktop joint testing was not run.

A connection, initialize response, or matching version alone does not prove full compatibility. Existing data and active work are preserved without automatic server replacement.

## Validation and limits

Tagged release notes identify tested binaries, original failures, and unrun cases. Real-model compaction quality, physical keyboard/IME and voice behavior, first-time installation, and sustained workloads have the gaps specified there. These tests do not establish that all GUI delays or memory growth have been resolved.

R40 excludes a full memory redesign, a new CodeMode architecture, broad refactoring, and structural caching improvements. Display-history bounds are not total process-memory limits; file creation is not proof of cloud upload.

[Validation](docs/TESTING.md) · [Release note format](docs/releases/FORMAT.md) · [Source build](docs/source-import.md) · [Windows packaging](tools/windows-cli/README.md) · [Modifications](MODIFICATIONS.md)

## License

Independent work based on [OpenAI Codex](https://github.com/openai/codex); not an official OpenAI product. Git stores source and build tools; Releases stores executable assets. Credentials and operational logs are excluded.

[Apache-2.0](LICENSE) · [NOTICE](NOTICE)
