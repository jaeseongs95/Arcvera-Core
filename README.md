# OpenDIY Codex

[English](README.md) · [한국어](README.ko.md)

An unofficial Windows edition of Codex CLI and Rust App Server, with changes to cancellation, process cleanup, conversation controls, scheduling, and optional diagnostics.

[Download R33](https://github.com/jaeseongs95/opendiy-codex/releases/tag/v0.166.0-r33.20261102) · [Build from source](docs/source-import.md) · [Validation](docs/TESTING.md) · [Changes](MODIFICATIONS.md)

## Releases and development

| Channel | Official base | Status |
| --- | --- | --- |
| **R33 published package** | 0.166.0 | `0.166.0+native-r33.20261102`; Windows CLI, standalone App Server, and portable package |
| **R33 updater fix candidate** | 0.166.0 | Output encoding and task-owner fixes; source and isolated tests reviewed, unreleased |
| **R32** | 0.166.0 | Previous published package; retain existing installations for recovery |
| **R31** | 0.166.0 | Separate release cancelled; changes integrated into R32 |

**R33 automatic updates have known Windows defects.** Output containing Korean paths can fail during hidden execution, and the updater can mistake an equivalent user name for a task-owner conflict. Current source `bd2ed8ec7854d64905caa67d0f77758e758a930c` contains fixes for these defects. Isolated tests fail with the original helper and pass with the candidate; a fresh source review also passed. These results do not mean the fixes are in the published R33 binaries or installed package. Real task execution and follow-up packaging and deployment remain unverified. See the [fix and validation scope](docs/r33-updater-postinstall-fix.ko.md).

The published R33 CLI and server were built from `36ae395a552fd55962e19409cbf29b1fdf03b0ec`. All five public assets were anonymously downloaded and hash-checked; Windows installation, shortcut selection, and joint tests with OpenDIY Desktop 1.0.2 were checked. The automatic-update post-install audit failed on the two defects above. Earlier failures and remaining limitations are retained in the [validation record](docs/r33-development-verification.ko.md). [Source provenance](SOURCE_BASELINE.json) identifies the unreleased fix candidate.

## Features

- **Cancellation and cleanup:** propagate cancellation to pending requests, limit capacity-error retries, and clean up owned child processes.
- **Conversation controls:** manage context-window settings and memory corrections, and inspect subagent status.
- **Scheduling:** schedule prompts while the CLI is open; busy conversations skip that occurrence.
- **Display history and language:** limit retained display history and choose English or Korean UI. The model's original conversation is preserved.
- **Optional diagnostics:** buffer events and publish completed files. R32 improves cache-response and wire-diagnostic delivery into batches and identifies their originating run.

R33 adds duplicate-display filtering when history and buffered events overlap, drain preservation on repeated graceful shutdown requests, a shared cleanup startup path, and full-version checks for local CLI/server connections. Its Windows updater is designed to select a verified package for the next CLI launch while retaining existing packages and the shared daemon. The published R33 package includes these changes; its updater defects are listed above.

## Run on Windows

1. Download `opendiy-codex-r33-windows-x86_64.zip` and `SHA256SUMS.txt` from the [R33 release](https://github.com/jaeseongs95/opendiy-codex/releases/tag/v0.166.0-r33.20261102).
2. Verify SHA-256 and extract into a new folder, keeping your existing installation.
3. Run `codex-local.cmd` for a local engine separate from an existing shared server. See the package's `USAGE.ko.md`.

Individual `codex.exe` and `codex-app-server.exe` downloads are also available; features needing helpers require the full package. The default CLI and `codex-patched.cmd` may connect to an existing shared server, whose version must be checked separately.

## Where to enter commands

These conversation-input commands were available in R30 and remain in R33. They are not PowerShell commands.

| Command | Purpose |
| --- | --- |
| `/cron add --every 10m -- Check whether the build has finished` | Schedule a repeating prompt |
| `/cron list` · `/cron remove 1` | List or remove schedules; use the actual ID |
| `/context 120000` · `/context reset` | Set or clear this conversation's context-window override |
| `/memory view` · `/memory edit <correction>` | View memory or record a correction for later consolidation |
| `/subagents` | Inspect subagent conversations |
| `/background` or `F6` | Return control while a supported terminal task continues |
| `/reload_plugin` | Request plugin reload for the current conversation |
| `/language ko` · `/language en` | Save the global UI language for the next launch |

Context settings cannot raise the model's catalog limit. Memory corrections do not immediately rewrite the stored summary. Schedules disappear when the CLI exits; natural-language scheduling requires the conversation's `cron` tool. See [scheduling](docs/native-cli-cron.ko.md) and [context, language, and logging](docs/native-cli-r27.ko.md), currently in Korean.

The following are **PowerShell commands in R33**. They are not conversation slash commands and are unavailable in the R32 download.

```powershell
codex update --check
codex update --status
codex update --enable-updates --interval-minutes 60
codex update --disable-updates
codex update
```

These commands require an installation configured for automatic updates. They are separate from the global feature options `--enable` and `--disable`. See [updater requirements](tools/windows-cli/UPDATE.ko.md) for helpers, scheduling, and package requirements.

## Validation and integration

Logging requires explicit selection. File publication does not confirm cloud upload, and cache-diagnostic tests do not establish improved cache hit rates on the actual service. Display-history limits are not a total process-memory cap.

[OpenDIY Desktop](https://github.com/jaeseongs95/opendiy-desktop) is the separate UI and multi-session project. Joint tests of published R33 (`36ae395a552fd55962e19409cbf29b1fdf03b0ec`) and Desktop 1.0.2 (`7069c80463cf60b2cf5f55023a42ca93f0bf57c2`) checked stopped-server transitions, existing-server connections, isolated GUI restoration, and child-session management. After an external server replacement, Desktop can retain an unknown state and require a manual app reopen. The installed updater-fix combination, actual mobile access, sustained workloads, and resolution of the original latency and memory-growth issues remain unverified.

## Build and license

See the [source build guide](docs/source-import.md) and [Windows packaging tools](tools/windows-cli/README.md). Source-provenance checks do not replace a full public-checkout build or runtime validation. Git contains source and tools; Releases contains executable packages. Credentials and operational logs are excluded.

Independent work based on [OpenAI Codex](https://github.com/openai/codex), not an official OpenAI product. See [Apache-2.0](LICENSE), [NOTICE](NOTICE), and component-specific licenses.
