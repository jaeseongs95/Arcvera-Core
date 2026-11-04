# OpenDIY Codex

[English](README.md) · [한국어](README.ko.md)

An unofficial Windows edition of Codex CLI and Rust App Server. It repairs confirmed defects and reuses upstream functionality rather than duplicating it.

[Download R34](https://github.com/jaeseongs95/opendiy-codex/releases/tag/v0.166.0-r34.20261103) · [Build from source](docs/source-import.md) · [Validation](docs/TESTING.md) · [Changes](MODIFICATIONS.md)

## Release status

| Version | Official base | State |
| --- | --- | --- |
| R36 | 0.166.0 + selected upstream changes | Candidate integrating R35 changes. Joint testing uses dev-small binaries; final release build, publication, and installation remain pending. |
| R35 | 0.166.0 | Integrated into R36 without a separate release. |
| R34 | 0.166.0 + selected upstream changes | Published Windows CLI, standalone App Server, and full package. Frozen after publication. |
| R33 | 0.166.0 | Historical package; Windows updater defects were repaired in R34. |
| R32 | 0.166.0 | Historical package. |
| R31 | 0.166.0 | No separate release; integrated into R32. |

The published executable release is R34 (`v0.166.0-r34.20261103`). A merged R36 source PR or successful dev-small build does not mean final binaries are published or installed. The [R35 guide](docs/native-cli-r35.ko.md) describes the memory, language, and update changes now integrated into R36.

## Features

- Cancellation propagation, bounded capacity retries, and owned-child process cleanup.
- Per-conversation context/tier controls, memory activation/corrections, subagent display, and confirmed cleanup.
- CLI-lifetime scheduling: busy conversations skip an occurrence; schedules disappear when the CLI exits.
- R36 candidate: one-line subagent summary with at most three expanded rows, distinct Cloud discovery errors, read-only `codex ci` observation, and fast propagation/compaction phase diagnostics.
- Bounded display history independent of model context, and globally selected English/Korean UI.
- Optional asynchronous diagnostics published as completed immutable batch files.
- A package launcher/updater that retains previous packages and does not automatically replace an active shared daemon.

The changes integrated into R36 distinguish a server's optional capability declaration from a successful attachment or completed model turn. Its cache change fixes missing completed-usage observations; it does not change cache policy or establish a better live hit rate.

## Run on Windows

1. Download the complete ZIP and `SHA256SUMS.txt` from the release.
2. Verify SHA-256 and follow the package's `USAGE.ko.md`; retain your existing installation.
3. Use `codex-local.cmd` for an engine separate from an existing shared server.

Individual `codex.exe` and `codex-app-server.exe` downloads omit helpers. The default CLI may attach to an existing server: verify its identity separately from the selected CLI.

## Conversation commands

These commands go in the CLI conversation input, not PowerShell.

| Command | Purpose |
| --- | --- |
| `/cron add --every 10m -- Check whether the build has finished` | Repeating prompt |
| `/cron list` · `/cron remove 1` | List/remove the actual schedule ID |
| `/context 120000` · `/context reset` | Conversation context override |
| `/memory view` · `/memory enable on` | Inspect memory / explicitly enable it |
| `/subagents` | Inspect child sessions |
| `/background` or `F6` | Background a supported terminal task |
| `/reload_plugin` | Request plugin reload |
| `/language ko` · `/language en` | Save global language for next launch |

Context overrides cannot exceed the model limit. Saving a memory correction is not immediate summary consolidation. Natural-language controls require the tools supplied by the running CLI version. Plugin reload acknowledgement does not prove every MCP server is ready.

Updater commands run in PowerShell: `codex update --check`, `--status`, `--enable-updates`, `--disable-updates`, or `codex update`. They require a supported installed package; see [updater requirements](tools/windows-cli/UPDATE.ko.md).

## OpenDIY Desktop compatibility

[OpenDIY Desktop](https://github.com/jaeseongs95/opendiy-desktop) owns the desktop UI and multi-session workflows. This repository owns the CLI, App Server, and their package. The projects coordinate protocol and release compatibility.

| CLI/server | Desktop | Verification scope |
| --- | --- | --- |
| R33 | 1.0.2 | Historical bounded joint tests; see the R33 release record. |
| R34 | Release-specific tested candidate | Startup, isolated engine/catalog, and restoration checks; see the R34 release record. |
| R36 trial CLI/server | 1.1.0 candidate `23eba648` | Actual GUI input, HTTP/SSE, same-backend completion, normal exit, and restart/restore passed. OS Korean IME/copy checks and final-release binary joint tests remain pending. |
| R36 trial CLI/isolated R28 server | Same 1.1.0 candidate | Conversation completion and normal exit passed. Unsupported server features are hidden; this does not test replacing the operational shared daemon. |

A connection, initialize response, or build-string match alone is not proof of full compatibility. Actual mobile connection, sustained workloads, and live cache effects need separate evidence. File creation does not prove cloud upload, and display-history limits are not total process-memory caps.

## Build and license

[Source build guide](docs/source-import.md) · [Windows packaging](tools/windows-cli/README.md). Git stores source and build tools; Releases stores executable assets. Source provenance is not a fresh public-checkout build. Credentials and operational logs are excluded.

Independent work based on [OpenAI Codex](https://github.com/openai/codex), not an official OpenAI product. [Apache-2.0](LICENSE) · [NOTICE](NOTICE).
