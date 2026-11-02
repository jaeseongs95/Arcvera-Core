# OpenDIY Codex

[English](README.md) · [한국어](README.ko.md)

An unofficial Windows edition of Codex CLI and Rust App Server, with changes to cancellation, process cleanup, conversation controls, scheduling, and optional diagnostics.

[Download R32](https://github.com/jaeseongs95/opendiy-codex/releases/tag/v0.166.0-r32.20261101) · [Build from source](docs/source-import.md) · [Validation](docs/TESTING.md) · [Changes](MODIFICATIONS.md)

## Releases and development

| Channel | Official base | Status |
| --- | --- | --- |
| **R32 release** | 0.166.0 | Latest published release: Windows x86-64 CLI, standalone App Server, and portable package |
| **R31** | 0.166.0 | Separate release cancelled; changes integrated into R32 |
| **R33 source candidate** | 0.166.0 | `0.166.0+native-r33.20261102`; not released |

The source is pinned to `3d9eb86445925caa920dd2f958496cf1aa4ad7e2` and differs from the R32 download. Development test compilation and 73 targeted Rust tests passed on the previous `0c588` candidate, as did 13 PowerShell and 13 packaging tests. Development test compilation, 66 daemon tests, and one exact nested-stdio rerun passed on the current candidate; one native fixture was ignored. The exact rerun overlaps the full suite. A separate delivery-preparation checker passed its initial 21 fixtures and 23 fixtures after further hardening. Optimized CLI, App Server, and launcher builds completed, and package03 was created with verified artifact hashes. Desktop joint tests passed for eight natural EOF boundaries and isolated GUI restoration, but the graceful completion-notification failure keeps overall acceptance, release, and installation on hold. The final independent audit is FAIL because of this blocking defect. Binary publication, actual task registration, mobile access, and sustained workloads remain unverified. See the [R33 validation scope](docs/r33-development-verification.ko.md) and [source provenance](SOURCE_BASELINE.json).

## Features

- **Cancellation and cleanup:** propagate cancellation to pending requests, limit capacity-error retries, and clean up owned child processes.
- **Conversation controls:** manage context-window settings and memory corrections, and inspect subagent status.
- **Scheduling:** schedule prompts while the CLI is open; busy conversations skip that occurrence.
- **Display history and language:** limit retained display history and choose English or Korean UI. The model's original conversation is preserved.
- **Optional diagnostics:** buffer events and publish completed files. R32 improves cache-response and wire-diagnostic delivery into batches and identifies their originating run.

The R33 candidate adds duplicate-display filtering when history and buffered events overlap, drain preservation on repeated graceful shutdown requests, a shared cleanup startup path, and full-version checks for local CLI/server connections. Its Windows updater is designed to select a verified package for the next CLI launch while retaining existing packages and the shared daemon. These additions are absent from the current R32 download.

## Run on Windows

1. Download `opendiy-codex-r32-windows-x86_64.zip` and `SHA256SUMS.txt` from the [R32 release](https://github.com/jaeseongs95/opendiy-codex/releases/tag/v0.166.0-r32.20261101).
2. Verify SHA-256 and extract into a new folder, keeping your existing installation.
3. Run `codex-local.cmd` for a local engine separate from an existing shared server. See the package's `USAGE.ko.md`.

Individual `codex.exe` and `codex-app-server.exe` downloads are also available; features needing helpers require the full package. The default CLI and `codex-patched.cmd` may connect to an existing shared server, whose version must be checked separately.

## Where to enter commands

These conversation-input commands were available in R30 and remain in R32. They are not PowerShell commands.

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

The following are **PowerShell commands for the R33 candidate**. They are not conversation slash commands and are unavailable in the R32 download.

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

[OpenDIY Desktop](https://github.com/jaeseongs95/opendiy-desktop) is the separate UI and multi-session project. Joint tests with the Desktop 1.0.1 candidate (`36f54c0d5b8c8ee9ca7fed435c26f8a246bc4985`) passed for eight natural EOF boundaries and isolated GUI restoration. Overall pair acceptance remains on hold because of the graceful completion-notification failure; CI status is not a compatibility guarantee. Actual mobile access, sustained workloads, and resolution of the original latency and memory-growth issues remain unverified.

## Build and license

See the [source build guide](docs/source-import.md) and [Windows packaging tools](tools/windows-cli/README.md). Source-provenance checks do not replace a full public-checkout build or runtime validation. Git contains source and tools; Releases contains executable packages. Credentials and operational logs are excluded.

Independent work based on [OpenAI Codex](https://github.com/openai/codex), not an official OpenAI product. See [Apache-2.0](LICENSE), [NOTICE](NOTICE), and component-specific licenses.
