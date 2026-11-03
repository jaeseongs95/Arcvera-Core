//! Localize Clap presentation while preserving argument IDs, names, values and parsers.

use clap::Command;
use clap::builder::StyledStr;
use clap::error::ContextKind;
use clap::error::ContextValue;
use clap::error::ErrorFormatter;
use clap::error::ErrorKind;
use codex_tui::i18n::Locale;
use codex_tui::i18n::message_for;

pub(super) fn localize_help(mut command: Command, locale: Locale) -> Command {
    // Materialize generated help/version arguments and deferred command metadata
    // before translating. Argument parsers and all syntax remain owned by Clap.
    command.build();
    localize_built_help(command, locale)
}

fn localize_built_help(mut command: Command, locale: Locale) -> Command {
    if let Some(about) = command.get_about() {
        let localized = localize_metadata(locale, &about.to_string());
        command = command.about(localized);
    }
    if let Some(about) = command.get_long_about() {
        let localized = localize_metadata(locale, &about.to_string());
        command = command.long_about(localized);
    }
    if let Some(value) = command.get_before_help() {
        let value = localize_metadata(locale, &value.to_string());
        command = command.before_help(value);
    }
    if let Some(value) = command.get_before_long_help() {
        let value = localize_metadata(locale, &value.to_string());
        command = command.before_long_help(value);
    }
    if let Some(value) = command.get_after_help() {
        let value = localize_metadata(locale, &value.to_string());
        command = command.after_help(value);
    }
    if let Some(value) = command.get_after_long_help() {
        let value = localize_metadata(locale, &value.to_string());
        command = command.after_long_help(value);
    }
    if locale == Locale::Korean {
        let mut visible_aliases = command
            .get_visible_short_flag_aliases()
            .map(|alias| format!("-{alias}"))
            .collect::<Vec<_>>();
        visible_aliases.extend(
            command
                .get_visible_long_flag_aliases()
                .map(|alias| format!("--{alias}")),
        );
        visible_aliases.extend(command.get_visible_aliases().map(str::to_owned));
        if !visible_aliases.is_empty() {
            let all_aliases = command
                .get_all_aliases()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let all_short_aliases = command.get_all_short_flag_aliases().collect::<Vec<_>>();
            let all_long_aliases = command
                .get_all_long_flag_aliases()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let suffix = message_for(
                locale,
                "cli-aliases",
                &[("value", &visible_aliases.join(", "))],
            );
            let about = command.get_about().map(ToString::to_string);
            let long_about = command.get_long_about().map(ToString::to_string);
            // Clap has no alias-label setter. Move only the visible description
            // into localized metadata, then restore every accepted alias token.
            command = command
                .alias(None::<&'static str>)
                .aliases(all_aliases)
                .short_flag_alias(None::<char>)
                .short_flag_aliases(all_short_aliases)
                .long_flag_alias(None::<&'static str>)
                .long_flag_aliases(all_long_aliases)
                .about(append_help(about, &suffix));
            if let Some(long_about) = long_about {
                command = command.long_about(append_help(Some(long_about), &suffix));
            }
        }
        let heading = localize_metadata(
            locale,
            command.get_subcommand_help_heading().unwrap_or("Commands"),
        );
        let usage_heading = message_for(locale, "cli-usage-heading", &[]);
        command = command
            .subcommand_help_heading(heading)
            .help_template(format!("{{before-help}}{{name}} {{version}}\n{{about-with-newline}}\n{usage_heading} {{usage}}\n\n{{all-args}}{{after-help}}"));
    }
    command
        .mut_args(|mut arg| {
            let help = arg
                .get_help()
                .map(|help| localize_metadata(locale, &help.to_string()));
            let long_help = arg
                .get_long_help()
                .map(|help| localize_metadata(locale, &help.to_string()));
            if let Some(help) = help {
                arg = arg.help(help);
            }
            if let Some(long_help) = long_help {
                arg = arg.long_help(long_help);
            }
            if locale == Locale::Korean {
                let heading = localize_metadata(
                    locale,
                    arg.get_help_heading().unwrap_or(if arg.is_positional() {
                        "Arguments"
                    } else {
                        "Options"
                    }),
                );
                arg = arg.help_heading(heading);
                let mut metadata = Vec::new();
                let mut visible_aliases = arg
                    .get_visible_short_aliases()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|alias| format!("-{alias}"))
                    .collect::<Vec<_>>();
                visible_aliases.extend(
                    arg.get_visible_aliases()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|alias| format!("--{alias}")),
                );
                if !visible_aliases.is_empty() {
                    let all_aliases = arg
                        .get_all_aliases()
                        .unwrap_or_default()
                        .into_iter()
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    let all_short_aliases = arg.get_all_short_aliases().unwrap_or_default();
                    metadata.push(message_for(
                        locale,
                        "cli-aliases",
                        &[("value", &visible_aliases.join(", "))],
                    ));
                    arg = arg
                        .alias(None::<&'static str>)
                        .aliases(all_aliases)
                        .short_alias(None::<char>)
                        .short_aliases(all_short_aliases);
                }
                if !arg.is_hide_default_value_set() && arg.get_action().takes_values() {
                    let defaults = arg
                        .get_default_values()
                        .iter()
                        .map(|value| {
                            let value = value.to_string_lossy();
                            if value.contains(char::is_whitespace) {
                                format!("{value:?}")
                            } else {
                                value.into_owned()
                            }
                        })
                        .collect::<Vec<_>>();
                    if !defaults.is_empty() {
                        metadata.push(message_for(
                            locale,
                            "cli-default-value",
                            &[("value", &defaults.join(" "))],
                        ));
                        arg = arg.hide_default_value(true);
                    }
                }
                if !arg.is_hide_env_set()
                    && let Some(name) = arg.get_env()
                {
                    // Publish the variable name, not credentials captured by
                    // Clap while building the command. Parsing still uses it.
                    metadata.push(message_for(
                        locale,
                        "cli-environment",
                        &[("value", &name.to_string_lossy())],
                    ));
                    arg = arg.hide_env(true);
                }
                // ValueParser's erased parse method is private. Keep the exact
                // typed parser and translate its reflected descriptions only.
                if !arg.is_hide_possible_values_set() {
                    let values = arg
                        .get_value_parser()
                        .possible_values()
                        .map(|values| {
                            values
                                .filter(|value| !value.is_hide_set())
                                .map(|value| match value.get_help() {
                                    Some(help) => format!(
                                        "  {}: {}",
                                        value.get_name(),
                                        localize_metadata(locale, &help.to_string())
                                    ),
                                    None => format!("  {}", value.get_name()),
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if !values.is_empty() {
                        metadata.push(format!(
                            "{}\n{}",
                            message_for(locale, "cli-possible-values", &[]),
                            values.join("\n")
                        ));
                        arg = arg.hide_possible_values(true);
                    }
                }
                if !metadata.is_empty() {
                    let suffix = metadata.join("\n\n");
                    let short = arg.get_help().map(ToString::to_string);
                    let long = arg
                        .get_long_help()
                        .map(ToString::to_string)
                        .or_else(|| short.clone());
                    arg = arg
                        .help(append_help(short, &suffix))
                        .long_help(append_help(long, &suffix));
                }
            }
            arg
        })
        .mut_subcommands(|child| localize_built_help(child, locale))
}

fn append_help(help: Option<String>, suffix: &str) -> String {
    match help {
        Some(help) => format!("{help}\n\n{suffix}"),
        None => suffix.to_owned(),
    }
}

fn localize_metadata(locale: Locale, original: &str) -> String {
    match help_message_id(original) {
        // English metadata remains byte-for-byte unchanged, except legacy
        // Korean-only declarations that now have an English catalog entry.
        Some(id)
            if locale == Locale::Korean || original.chars().any(|c| ('가'..='힣').contains(&c)) =>
        {
            message_for(locale, id, &[])
        }
        _ => original.to_owned(),
    }
}

fn help_message_id(original: &str) -> Option<&'static str> {
    match original {
        "Observe GitHub Actions runs without changing remote state" => Some("ci-help"),
        "Repository in [HOST/]OWNER/REPO format (defaults to the current repository)" => {
            Some("ci-help-repo")
        }
        "Query CI for the current HEAD or --commit once" => Some("ci-help-status"),
        "Query recent CI runs once" => Some("ci-help-list"),
        "Watch one run until completion or timeout without rerunning it" => Some("ci-help-watch"),
        "Maximum observation time in seconds" => Some("ci-help-timeout"),
        "Observation interval in seconds" => Some("ci-help-interval"),
        "Print each observation as one JSON line" => Some("ci-help-json-lines"),
        "Full commit SHA (defaults to local HEAD for status)" => Some("ci-help-commit"),
        "Additionally filter by branch" => Some("ci-help-branch"),
        "Maximum runs to return (does not enumerate required checks)" => Some("ci-help-limit"),
        "Workspace path to open in the Desktop app" => {
            Some("cli-help-app-cmd-workspace-path-to-open-in-the-desktop-app")
        }
        "Override the app installer download URL (advanced)" => {
            Some("cli-help-app-cmd-override-the-app-installer-download-url-advanced")
        }
        "현재 대화 UUID. 생략하면 CODEX_THREAD_ID를 사용합니다" => {
            Some("cli-cleanup-current")
        }
        "정리 범위의 부모 UUID. 생략하면 현재 대화입니다" => {
            Some("cli-cleanup-parent")
        }
        "정리할 정확한 UUID. 반복할 수 있으며 생략하면 미리보기의 모든 종료 후보입니다" => {
            Some("cli-cleanup-threads")
        }
        "미리보기 후 확인을 받아 실제로 정리합니다. 생략하면 읽기 전용입니다" => {
            Some("cli-cleanup-apply")
        }
        "미리보기의 선택 항목 삭제를 명시적으로 승인합니다" => {
            Some("cli-cleanup-confirm")
        }
        "현재 부모 아래 로컬 2세대 서브에이전트의 종료 대화를 미리 보고 정리합니다." => {
            Some("cli-cleanup-about")
        }
        "미리보기가 기본이며 각 항목의 reason에 정리·보존 사유를 표시합니다. --apply는 미리보기 뒤 명시 확인을 받습니다. 메모리에 없는 과거 종료 항목은 참조 안전성을 입증할 수 없어 reference_participants_unverified로 보존합니다. 해당 항목의 운영 정리 지원은 미완입니다. 실행 중이거나 상태가 불명확한 항목, 다른 세대·원격 소유·이전 방식 항목은 보존합니다. --thread를 반복하면 정확한 선택 집합만 정리하며 선택하지 않은 후손은 보존합니다." => {
            Some("cli-cleanup-long-about")
        }
        "Options for building a local Codex diagnostic report" => {
            Some("cli-help-doctor-options-for-building-a-local-codex-diagnostic-report")
        }
        "Options for building a local Codex diagnostic report.\n\nThe command runs the full diagnostic set unless `--network-only` is selected. Human output includes detailed diagnostics by default; --summary keeps the terminal output compact." => {
            Some("cli-help-doctor-options-for-building-a-local-codex-diagnostic-report-details")
        }
        "Internal isolated filesystem probe; exits before loading configuration" => Some(
            "cli-help-doctor-internal-isolated-filesystem-probe-exits-before-loading-configuration",
        ),
        "Emit a redacted machine-readable report" => {
            Some("cli-help-doctor-emit-a-redacted-machine-readable-report")
        }
        "Run config, network environment, provider HTTP, and Responses WebSocket checks" => {
            Some("cli-help-doctor-run-config-network-environment-provider-http-and-responses")
        }
        "Limit database integrity scans when collecting a feedback attachment" => {
            Some("cli-help-doctor-limit-database-integrity-scans-when-collecting-a-feedback")
        }
        "Only show grouped check rows and the final count summary" => {
            Some("cli-help-doctor-only-show-grouped-check-rows-and-the-final")
        }
        "Expand long lists in detailed human output" => {
            Some("cli-help-doctor-expand-long-lists-in-detailed-human-output")
        }
        "Disable ANSI color in human output" => {
            Some("cli-help-doctor-disable-ansi-color-in-human-output")
        }
        "Use ASCII status labels and separators in human output" => {
            Some("cli-help-doctor-use-ascii-status-labels-and-separators-in-human")
        }
        "Machine-readable doctor output shared by human and JSON renderers" => {
            Some("cli-help-doctor-machine-readable-doctor-output-shared-by-human-and")
        }
        "Machine-readable doctor output shared by human and JSON renderers.\n\nThe schema is intentionally flat: each check carries its own category, status, details, remediation, and duration so support tooling can filter or redact individual rows without understanding the renderer's section layout." => {
            Some("cli-help-doctor-machine-readable-doctor-output-shared-by-human-and-details")
        }
        "One diagnostic result in the doctor report" => {
            Some("cli-help-doctor-one-diagnostic-result-in-the-doctor-report")
        }
        "One diagnostic result in the doctor report.\n\nSummaries are safe for compact human output. Details may include local paths or command output and are redacted before rendering or JSON serialization." => {
            Some("cli-help-doctor-one-diagnostic-result-in-the-doctor-report-summaries")
        }
        "Structured cause/remedy metadata for a non-ok doctor check" => {
            Some("cli-help-doctor-structured-cause-remedy-metadata-for-a-non-ok")
        }
        "Structured cause/remedy metadata for a non-ok doctor check.\n\nHuman output uses issues to make warnings and failures self-explanatory: the row headline says what is wrong, matching detail rows show measured vs. expected values, and remedies are printed as explicit next actions." => {
            Some("cli-help-doctor-structured-cause-remedy-metadata-for-a-non-ok-details")
        }
        "JSON support report emitted by `codex doctor --json`" => {
            Some("cli-help-doctor-json-support-report-emitted-by-codex-doctor-json")
        }
        "JSON support report emitted by `codex doctor --json`.\n\nThe report is keyed by check id so support tooling can fetch paths like `checks[\"terminal.metadata\"]` without scanning arrays. Human rendering can reorder or group rows independently, but this JSON shape should stay stable across cosmetic output changes." => {
            Some("cli-help-doctor-json-support-report-emitted-by-codex-doctor-json-details")
        }
        "One redacted check in the JSON support report" => {
            Some("cli-help-doctor-one-redacted-check-in-the-json-support-report")
        }
        "One redacted issue in the JSON support report" => {
            Some("cli-help-doctor-one-redacted-issue-in-the-json-support-report")
        }
        "JSON detail value that preserves repeated detail keys without inventing names" => {
            Some("cli-help-doctor-json-detail-value-that-preserves-repeated-detail-keys")
        }
        "Opaque JSON value from `codex/sandbox-state` to apply directly" => {
            Some("cli-help-lib-opaque-json-value-from-codex-sandbox-state-to")
        }
        "Add a readable root to the supplied sandbox state. Repeat for multiple roots" => {
            Some("cli-help-lib-add-a-readable-root-to-the-supplied-sandbox")
        }
        "Disable direct network access in the supplied sandbox state" => {
            Some("cli-help-lib-disable-direct-network-access-in-the-supplied-sandbox")
        }
        "Named permissions profile to apply from the active configuration stack" => {
            Some("cli-help-lib-named-permissions-profile-to-apply-from-the-active")
        }
        "Layer $CODEX_HOME/<name>.config.toml on top of the base user config" => {
            Some("cli-help-lib-layer-codex-home-name-config-toml-on-top")
        }
        "Working directory used for profile resolution and command execution" => {
            Some("cli-help-lib-working-directory-used-for-profile-resolution-and-command")
        }
        "Include managed requirements while resolving an explicit permissions profile" => Some(
            "cli-help-lib-include-managed-requirements-while-resolving-an-explicit-permissions",
        ),
        "Allow the sandboxed command to bind/connect AF_UNIX sockets rooted at this path. Relative paths are resolved against the current directory. Repeat to allow multiple paths" => {
            Some("cli-help-lib-allow-the-sandboxed-command-to-bind-connect-af")
        }
        "While the command runs, capture macOS sandbox denials via `log stream` and print them after exit" => {
            Some("cli-help-lib-while-the-command-runs-capture-macos-sandbox-denials")
        }
        "Full command args to run under seatbelt" => {
            Some("cli-help-lib-full-command-args-to-run-under-seatbelt")
        }
        "Full command args to run under the Linux sandbox" => {
            Some("cli-help-lib-full-command-args-to-run-under-the-linux")
        }
        "Full command args to run under Windows restricted token sandbox" => {
            Some("cli-help-lib-full-command-args-to-run-under-windows-restricted")
        }
        "Codex CLI" => Some("cli-help-main-codex-cli"),
        "Codex CLI\n\nIf no subcommand is specified, options will be forwarded to the interactive CLI." => {
            Some("cli-help-main-codex-cli-if-no-subcommand-is-specified-options")
        }
        "Browse all agent sessions on the shared local app-server daemon" => {
            Some("cli-help-main-browse-all-agent-sessions-on-the-shared-local")
        }
        "Internal: forward a local TCP socket through an HTTP/3 CONNECT proxy" => {
            Some("cli-help-main-internal-forward-a-local-tcp-socket-through-an")
        }
        "Run Codex non-interactively" => Some("cli-help-main-run-codex-non-interactively"),
        "Run a code review non-interactively" => {
            Some("cli-help-main-run-a-code-review-non-interactively")
        }
        "Manage login" => Some("cli-help-main-manage-login"),
        "Remove stored authentication credentials" => {
            Some("cli-help-main-remove-stored-authentication-credentials")
        }
        "Manage external MCP servers for Codex" => {
            Some("cli-help-main-manage-external-mcp-servers-for-codex")
        }
        "Manage Codex plugins" => Some("cli-help-main-manage-codex-plugins"),
        "[experimental] Run the app server or related tooling" => {
            Some("cli-help-main-experimental-run-the-app-server-or-related-tooling")
        }
        "[experimental] Manage the app-server daemon with remote control enabled" => {
            Some("cli-help-main-experimental-manage-the-app-server-daemon-with-remote")
        }
        "Launch the Desktop app (opens the app installer if missing)" => {
            Some("cli-help-main-launch-the-desktop-app-opens-the-app-installer")
        }
        "Generate shell completion scripts" => {
            Some("cli-help-main-generate-shell-completion-scripts")
        }
        "Update Codex to the latest version" => {
            Some("cli-help-main-update-codex-to-the-latest-version")
        }
        "Diagnose local Codex installation, config, auth, and runtime health" => {
            Some("cli-help-main-diagnose-local-codex-installation-config-auth-and-runtime")
        }
        "Run commands within a Codex-provided sandbox" => {
            Some("cli-help-main-run-commands-within-a-codex-provided-sandbox")
        }
        "Debugging tools" => Some("cli-help-main-debugging-tools"),
        "Execpolicy tooling" => Some("cli-help-main-execpolicy-tooling"),
        "Apply the latest diff produced by Codex agent as a `git apply` to your local working tree" => {
            Some("cli-help-main-apply-the-latest-diff-produced-by-codex-agent")
        }
        "Resume a previous interactive session (picker by default; use --last to continue the most recent)" => {
            Some("cli-help-main-resume-a-previous-interactive-session-picker-by-default")
        }
        "Queue a message for an existing session" => {
            Some("cli-help-main-queue-a-message-for-an-existing-session")
        }
        "현재 부모 아래 종료된 서브에이전트를 미리 보거나 확인 후 정리합니다" => {
            Some("cli-cleanup-command")
        }
        "Archive a saved session by id or session name" => {
            Some("cli-help-main-archive-a-saved-session-by-id-or-session")
        }
        "Permanently delete a saved session by id or session name" => {
            Some("cli-help-main-permanently-delete-a-saved-session-by-id-or")
        }
        "Inspect or migrate legacy local sessions to paginated thread history" => {
            Some("cli-help-main-inspect-or-migrate-legacy-local-sessions-to-paginated")
        }
        "[experimental] Migrate configured SQLite databases to DELETE mode in place" => {
            Some("cli-help-main-experimental-migrate-configured-sqlite-databases-to-delete-mode")
        }
        "Unarchive a saved session by id or session name" => {
            Some("cli-help-main-unarchive-a-saved-session-by-id-or-session")
        }
        "Fork a previous interactive session (picker by default; use --last to fork the most recent)" => {
            Some("cli-help-main-fork-a-previous-interactive-session-picker-by-default")
        }
        "[EXPERIMENTAL] Browse tasks from Codex Cloud and apply changes locally" => {
            Some("cli-help-main-experimental-browse-tasks-from-codex-cloud-and-apply")
        }
        "Internal: run the responses API proxy" => {
            Some("cli-help-main-internal-run-the-responses-api-proxy")
        }
        "Internal: relay stdio to a Unix domain socket" => {
            Some("cli-help-main-internal-relay-stdio-to-a-unix-domain-socket")
        }
        "[EXPERIMENTAL] Run the standalone exec-server service" => {
            Some("cli-help-main-experimental-run-the-standalone-exec-server-service")
        }
        "Inspect feature flags" => Some("cli-help-main-inspect-feature-flags"),
        "Shell to generate completions for" => {
            Some("cli-help-main-shell-to-generate-completions-for")
        }
        "Render the raw model catalog as JSON" => {
            Some("cli-help-main-render-the-raw-model-catalog-as-json")
        }
        "Tooling: helps debug the app server" => {
            Some("cli-help-main-tooling-helps-debug-the-app-server")
        }
        "Render the model-visible prompt input list as JSON" => {
            Some("cli-help-main-render-the-model-visible-prompt-input-list-as")
        }
        "Replay a rollout trace bundle and write reduced state JSON" => {
            Some("cli-help-main-replay-a-rollout-trace-bundle-and-write-reduced")
        }
        "Internal: reset local memory state for a fresh start" => {
            Some("cli-help-main-internal-reset-local-memory-state-for-a-fresh")
        }
        "Optional user prompt to append after session context" => {
            Some("cli-help-main-optional-user-prompt-to-append-after-session-context")
        }
        "Optional image(s) to attach to the user prompt" => {
            Some("cli-help-main-optional-image-s-to-attach-to-the-user")
        }
        "Skip refresh and dump only the bundled catalog shipped with this binary" => {
            Some("cli-help-main-skip-refresh-and-dump-only-the-bundled-catalog")
        }
        "Error out when config.toml contains fields that are not recognized by this version of Codex" => {
            Some("cli-help-main-error-out-when-config-toml-contains-fields-that")
        }
        "Trace bundle directory containing manifest.json and trace.jsonl" => {
            Some("cli-help-main-trace-bundle-directory-containing-manifest-json-and-trace")
        }
        "Output path for reduced RolloutTrace JSON. Defaults to TRACE_BUNDLE/state.json" => {
            Some("cli-help-main-output-path-for-reduced-rollouttrace-json-defaults-to")
        }
        "The agents overview requires a shared server; this option is rejected" => {
            Some("cli-help-main-the-agents-overview-requires-a-shared-server-this")
        }
        "Use this directory for new tasks on a remote server" => {
            Some("cli-help-main-use-this-directory-for-new-tasks-on-a")
        }
        "Disable alternate screen mode" => Some("cli-help-main-disable-alternate-screen-mode"),
        "Session id (UUID) or session name. UUIDs take precedence if it parses. If omitted, use --last to pick the most recent recorded session" => {
            Some("cli-help-main-session-id-uuid-or-session-name-uuids-take")
        }
        "Continue the most recent session without showing the picker" => {
            Some("cli-help-main-continue-the-most-recent-session-without-showing-the")
        }
        "Show all sessions (disables cwd filtering and shows CWD column)" => {
            Some("cli-help-main-show-all-sessions-disables-cwd-filtering-and-shows")
        }
        "Include non-interactive sessions in the resume picker and --last selection" => {
            Some("cli-help-main-include-non-interactive-sessions-in-the-resume-picker")
        }
        "Session id (UUID) or session name. UUIDs take precedence if it parses" => {
            Some("cli-help-main-session-id-uuid-or-session-name-uuids-take-details")
        }
        "Delete without prompting. SESSION must be a UUID" => {
            Some("cli-help-main-delete-without-prompting-session-must-be-a-uuid")
        }
        "Conversation/session id (UUID). When provided, forks this session. If omitted, use --last to pick the most recent recorded session" => {
            Some("cli-help-main-conversation-session-id-uuid-when-provided-forks-this")
        }
        "Fork the most recent session without showing the picker" => {
            Some("cli-help-main-fork-the-most-recent-session-without-showing-the")
        }
        "TUI arguments for session commands where a parsed prompt implies an explicit session id" => {
            Some("cli-help-main-tui-arguments-for-session-commands-where-a-parsed")
        }
        "TUI arguments for session commands where a parsed prompt implies an explicit session id.\n\nThis keeps `--last PROMPT` valid while rejecting `--last SESSION_ID PROMPT`." => {
            Some("cli-help-main-tui-arguments-for-session-commands-where-a-parsed-details")
        }
        "Full command args to run under the host sandbox" => {
            Some("cli-help-main-full-command-args-to-run-under-the-host")
        }
        "Check execpolicy files against a command" => {
            Some("cli-help-main-check-execpolicy-files-against-a-command")
        }
        "EXPERIMENTAL: Use custom OAuth issuer base URL (advanced) Override the OAuth issuer base URL (advanced)" => {
            Some("cli-help-main-experimental-use-custom-oauth-issuer-base-url-advanced")
        }
        "EXPERIMENTAL: Use custom OAuth client ID (advanced)" => {
            Some("cli-help-main-experimental-use-custom-oauth-client-id-advanced")
        }
        "Show login status" => Some("cli-help-main-show-login-status"),
        "Load frozen cloud skills from a JSON snapshot instead of a live skill service" => {
            Some("cli-help-main-load-frozen-cloud-skills-from-a-json-snapshot")
        }
        "Omit to run the app server; specify a subcommand for tooling" => {
            Some("cli-help-main-omit-to-run-the-app-server-specify-a")
        }
        "Transport endpoint URL. Supported values: `stdio://` (default), `unix://`, `unix://PATH`, `ws://IP:PORT`, `off`" => {
            Some("cli-help-main-transport-endpoint-url-supported-values-stdio-default-unix")
        }
        "Use stdio as the transport (equivalent to `--listen stdio://`)" => {
            Some("cli-help-main-use-stdio-as-the-transport-equivalent-to-listen")
        }
        "Enable remote control for this app-server process without changing persistence" => {
            Some("cli-help-main-enable-remote-control-for-this-app-server-process")
        }
        "Start with remote control disabled, preserving saved registrations and settings" => {
            Some("cli-help-main-start-with-remote-control-disabled-preserving-saved-registrations")
        }
        "Save loaded threads during managed daemon shutdown" => {
            Some("cli-help-main-save-loaded-threads-during-managed-daemon-shutdown")
        }
        "Controls whether analytics are enabled by default" => {
            Some("cli-help-main-controls-whether-analytics-are-enabled-by-default")
        }
        "Controls whether analytics are enabled by default.\n\nAnalytics are disabled by default for app-server. Users have to explicitly opt in via the `analytics` section in the config.toml file.\n\nHowever, for first-party use cases like the VSCode IDE extension, we default analytics to be enabled by default by setting this flag. Users can still opt out by setting this in their config.toml:\n\n```toml [analytics] enabled = false ```\n\nSee https://developers.openai.com/codex/config-advanced/#metrics for more details." => {
            Some("cli-help-main-controls-whether-analytics-are-enabled-by-default-analytics")
        }
        "Manage the local app-server daemon" => {
            Some("cli-help-main-manage-the-local-app-server-daemon")
        }
        "Proxy stdio bytes to the running app-server control socket" => {
            Some("cli-help-main-proxy-stdio-bytes-to-the-running-app-server")
        }
        "[experimental] Generate TypeScript bindings for the app server protocol" => {
            Some("cli-help-main-experimental-generate-typescript-bindings-for-the-app-server")
        }
        "[experimental] Generate JSON Schema for the app server protocol" => {
            Some("cli-help-main-experimental-generate-json-schema-for-the-app-server")
        }
        "[internal] Generate internal JSON Schema artifacts for Codex tooling" => {
            Some("cli-help-main-internal-generate-internal-json-schema-artifacts-for-codex")
        }
        "Install durable local app-server management for SSH-driven use" => {
            Some("cli-help-main-install-durable-local-app-server-management-for-ssh")
        }
        "Start the local app server daemon if it is not already running" => {
            Some("cli-help-main-start-the-local-app-server-daemon-if-it")
        }
        "Install only if no installation, settings, or recovery state exists under the lifecycle lock"
        | "Install only if no installation, settings, or recovery state exists under the lifecycle lock." => {
            Some("cli-help-first-installation-only")
        }
        "Use only the selected installation; preserve settings and updater state" => {
            Some("cli-help-main-use-only-the-selected-installation-preserve-settings-and")
        }
        "Restart the local app server daemon" => {
            Some("cli-help-main-restart-the-local-app-server-daemon")
        }
        "Update the daemon package (may interrupt running work)" => {
            Some("cli-help-main-update-the-daemon-package-may-interrupt-running-work")
        }
        "Copy and pin this CLI package" => Some("cli-help-main-copy-and-pin-this-cli-package"),
        "Confirm replacing the daemon package without an interactive prompt" => {
            Some("cli-help-main-confirm-replacing-the-daemon-package-without-an-interactive")
        }
        "Never force shutdown. On an unconfirmed stop, keep the selected package and do not retry automatically" => {
            Some("cli-help-main-never-force-shutdown-on-an-unconfirmed-stop-keep")
        }
        "Enable remote control for future starts and a currently running managed daemon" => {
            Some("cli-help-main-enable-remote-control-for-future-starts-and-a")
        }
        "Disable remote control for future starts and a currently running managed daemon" => {
            Some("cli-help-main-disable-remote-control-for-future-starts-and-a")
        }
        "Stop the local app server daemon" => {
            Some("cli-help-main-stop-the-local-app-server-daemon")
        }
        "Print local CLI and running app-server versions as JSON" => {
            Some("cli-help-main-print-local-cli-and-running-app-server-versions")
        }
        "Observe the selected installation and daemon without starting or repairing it" => {
            Some("cli-help-main-observe-the-selected-installation-and-daemon-without-starting")
        }
        "[internal] Run the detached pid-backed standalone updater loop" => {
            Some("cli-help-main-internal-run-the-detached-pid-backed-standalone-updater")
        }
        "Check support for daemon-owned packages without starting the updater" => {
            Some("cli-help-main-check-support-for-daemon-owned-packages-without-starting")
        }
        "Authorize one production restoration of this selected release" => {
            Some("cli-help-main-authorize-one-production-restoration-of-this-selected-release")
        }
        "Path to the app-server Unix domain socket to connect to" => {
            Some("cli-help-main-path-to-the-app-server-unix-domain-socket")
        }
        "Launch the managed app-server with remote control enabled" => {
            Some("cli-help-main-launch-the-managed-app-server-with-remote-control")
        }
        "Output directory where .ts files will be written" => {
            Some("cli-help-main-output-directory-where-ts-files-will-be-written")
        }
        "Optional path to the Prettier executable to format generated files" => {
            Some("cli-help-main-optional-path-to-the-prettier-executable-to-format")
        }
        "Include experimental methods and fields in the generated output" => {
            Some("cli-help-main-include-experimental-methods-and-fields-in-the-generated")
        }
        "Output directory where the schema bundle will be written" => {
            Some("cli-help-main-output-directory-where-the-schema-bundle-will-be")
        }
        "Output directory where internal JSON Schema artifacts will be written" => {
            Some("cli-help-main-output-directory-where-internal-json-schema-artifacts-will")
        }
        "Path to the Unix domain socket to connect to" => {
            Some("cli-help-main-path-to-the-unix-domain-socket-to-connect")
        }
        "Check for an OpenDIY package update without applying it" => {
            Some("cli-help-main-check-for-an-opendiy-package-update-without-applying")
        }
        "Show the selected OpenDIY package and automatic update status" => {
            Some("cli-help-main-show-the-selected-opendiy-package-and-automatic-update")
        }
        "Enable automatic OpenDIY package updates (every 60 minutes by default)" => {
            Some("cli-help-main-enable-automatic-opendiy-package-updates-every-60-minutes")
        }
        "Disable automatic OpenDIY package updates" => {
            Some("cli-help-main-disable-automatic-opendiy-package-updates")
        }
        "Automatic update interval in minutes (OpenDIY only; requires --enable-updates)" => {
            Some("cli-help-main-automatic-update-interval-in-minutes-opendiy-only-requires")
        }
        "Enable a feature (repeatable). Equivalent to `-c features.<name>=true`" => {
            Some("cli-help-main-enable-a-feature-repeatable-equivalent-to-c-features")
        }
        "Disable a feature (repeatable). Equivalent to `-c features.<name>=false`" => {
            Some("cli-help-main-disable-a-feature-repeatable-equivalent-to-c-features")
        }
        "Connect the TUI to a remote app server endpoint" => {
            Some("cli-help-main-connect-the-tui-to-a-remote-app-server")
        }
        "Connect the TUI to a remote app server endpoint.\n\nAccepted forms: `ws://host:port`, `wss://host:port`, `unix://`, or `unix://PATH`." => {
            Some("cli-help-main-connect-the-tui-to-a-remote-app-server-details")
        }
        "Name of the environment variable containing the bearer token to send to a remote app server websocket" => {
            Some("cli-help-main-name-of-the-environment-variable-containing-the-bearer")
        }
        "List known features with their stage and effective state" => {
            Some("cli-help-main-list-known-features-with-their-stage-and-effective")
        }
        "Enable a feature in config.toml" => Some("cli-help-main-enable-a-feature-in-config-toml"),
        "Disable a feature in config.toml" => {
            Some("cli-help-main-disable-a-feature-in-config-toml")
        }
        "Feature key to update (for example: unified_exec)" => {
            Some("cli-help-main-feature-key-to-update-for-example-unified-exec")
        }
        "Read the API key from stdin (e.g. `printenv OPENAI_API_KEY | codex login --with-api-key`)" => {
            Some("cli-help-main-read-the-api-key-from-stdin-e-g")
        }
        "Read the access token from stdin (e.g. `printenv CODEX_ACCESS_TOKEN | codex login --with-access-token`)" => {
            Some("cli-help-main-read-the-access-token-from-stdin-e-g")
        }
        "(deprecated) Previously accepted the API key directly; now exits with guidance to use --with-api-key" => {
            Some("cli-help-main-deprecated-previously-accepted-the-api-key-directly-now")
        }
        "Add a local or Git marketplace to the configured marketplace sources" => {
            Some("cli-help-marketplace-cmd-add-a-local-or-git-marketplace-to-the")
        }
        "List plugin marketplaces Codex is currently considering and their roots" => Some(
            "cli-help-marketplace-cmd-list-plugin-marketplaces-codex-is-currently-considering-and",
        ),
        "Refresh configured Git marketplace snapshots" => {
            Some("cli-help-marketplace-cmd-refresh-configured-git-marketplace-snapshots")
        }
        "Refresh configured Git marketplace snapshots.\n\nOmit MARKETPLACE_NAME to upgrade all configured Git marketplaces." => {
            Some(
                "cli-help-marketplace-cmd-refresh-configured-git-marketplace-snapshots-omit-marketplace-name",
            )
        }
        "Remove a configured marketplace source by name" => {
            Some("cli-help-marketplace-cmd-remove-a-configured-marketplace-source-by-name")
        }
        "Marketplace source: a local path, owner/repo[@ref], HTTPS Git URL, or SSH Git URL" => {
            Some("cli-help-marketplace-cmd-marketplace-source-a-local-path-owner-repo-ref")
        }
        "Git ref to fetch for Git marketplace sources" => {
            Some("cli-help-marketplace-cmd-git-ref-to-fetch-for-git-marketplace-sources")
        }
        "Sparse checkout path for Git marketplace sources. Can be repeated" => {
            Some("cli-help-marketplace-cmd-sparse-checkout-path-for-git-marketplace-sources-can")
        }
        "Output add result as JSON" => Some("cli-help-marketplace-cmd-output-add-result-as-json"),
        "Output marketplace list as JSON" => {
            Some("cli-help-marketplace-cmd-output-marketplace-list-as-json")
        }
        "Optional configured marketplace name to upgrade. Omit to upgrade all Git marketplaces" => {
            Some("cli-help-marketplace-cmd-optional-configured-marketplace-name-to-upgrade-omit-to")
        }
        "Output upgrade result as JSON" => {
            Some("cli-help-marketplace-cmd-output-upgrade-result-as-json")
        }
        "Configured marketplace name to remove" => {
            Some("cli-help-marketplace-cmd-configured-marketplace-name-to-remove")
        }
        "Output remove result as JSON" => {
            Some("cli-help-marketplace-cmd-output-remove-result-as-json")
        }
        "Examples:\n  codex plugin marketplace add ./path/to/marketplace\n  codex plugin marketplace add owner/repo --ref main\n  codex plugin marketplace add https://github.com/owner/repo --sparse plugins/foo" => {
            Some(
                "cli-help-marketplace-cmd-examples-codex-plugin-marketplace-add-path-to-marketplace",
            )
        }
        "Examples:\n  codex plugin marketplace upgrade\n  codex plugin marketplace upgrade debug" => {
            Some(
                "cli-help-marketplace-cmd-examples-codex-plugin-marketplace-upgrade-codex-plugin-marketplace",
            )
        }
        "Example:\n  codex plugin marketplace remove debug" => {
            Some("cli-help-marketplace-cmd-example-codex-plugin-marketplace-remove-debug")
        }
        "Subcommands: - `list` — list configured servers (with `--json`) - `get` — show a single server (with `--json`) - `add` — add a server launcher entry to `~/.codex/config.toml` - `remove` — delete a server entry - `login` — authenticate with MCP server using OAuth - `logout` — remove OAuth credentials for MCP server" => {
            Some("cli-help-mcp-cmd-subcommands-list-list-configured-servers-with-json-get")
        }
        "Output the configured servers as JSON" => {
            Some("cli-help-mcp-cmd-output-the-configured-servers-as-json")
        }
        "Name of the MCP server to display" => {
            Some("cli-help-mcp-cmd-name-of-the-mcp-server-to-display")
        }
        "Output the server configuration as JSON" => {
            Some("cli-help-mcp-cmd-output-the-server-configuration-as-json")
        }
        "Name for the MCP server configuration" => {
            Some("cli-help-mcp-cmd-name-for-the-mcp-server-configuration")
        }
        "Command to launch the MCP server. Use --url for a streamable HTTP server" => {
            Some("cli-help-mcp-cmd-command-to-launch-the-mcp-server-use-url")
        }
        "Environment variables to set when launching the server. Only valid with stdio servers" => {
            Some("cli-help-mcp-cmd-environment-variables-to-set-when-launching-the-server")
        }
        "URL for a streamable HTTP MCP server" => {
            Some("cli-help-mcp-cmd-url-for-a-streamable-http-mcp-server")
        }
        "Optional environment variable to read for a bearer token. Only valid with streamable HTTP servers" => {
            Some("cli-help-mcp-cmd-optional-environment-variable-to-read-for-a-bearer")
        }
        "Optional OAuth client identifier to use for this MCP server" => {
            Some("cli-help-mcp-cmd-optional-oauth-client-identifier-to-use-for-this")
        }
        "Optional OAuth client secret for the pre-registered client" => {
            Some("cli-help-mcp-cmd-optional-oauth-client-secret-for-the-pre-registered")
        }
        "OAuth client-registration strategy for the immediate login only" => {
            Some("cli-help-mcp-cmd-oauth-client-registration-strategy-for-the-immediate-login")
        }
        "Optional OAuth resource parameter to include during MCP login" => {
            Some("cli-help-mcp-cmd-optional-oauth-resource-parameter-to-include-during-mcp")
        }
        "Name of the MCP server configuration to remove" => {
            Some("cli-help-mcp-cmd-name-of-the-mcp-server-configuration-to-remove")
        }
        "Name of the MCP server to authenticate with oauth" => {
            Some("cli-help-mcp-cmd-name-of-the-mcp-server-to-authenticate-with")
        }
        "Print the authorization URL and accept the callback URL without opening a browser" => {
            Some("cli-help-mcp-cmd-print-the-authorization-url-and-accept-the-callback")
        }
        "Comma-separated list of OAuth scopes to request" => {
            Some("cli-help-mcp-cmd-comma-separated-list-of-oauth-scopes-to-request")
        }
        "OAuth client-registration strategy for this login only" => {
            Some("cli-help-mcp-cmd-oauth-client-registration-strategy-for-this-login-only")
        }
        "Name of the MCP server to deauthenticate" => {
            Some("cli-help-mcp-cmd-name-of-the-mcp-server-to-deauthenticate")
        }
        "Retry provider-rejected discovered scopes once, retaining the selected input mode" => {
            Some("cli-help-mcp-login-retry-provider-rejected-discovered-scopes-once-retaining-the")
        }
        "Use a local directory with space for the database and WAL files. The default is the system temporary directory" => {
            Some("cli-help-migrate-nfs-db-use-a-local-directory-with-space-for-the")
        }
        "Stop SQLite clients on every host and keep them stopped through cutover.\nThe NFS mount must support cross-client POSIX locks (no nolock/local_lock=all).\nThis command replaces configured databases in place and retains their WAL backups." => {
            Some("cli-help-migrate-nfs-db-stop-sqlite-clients-on-every-host-and-keep")
        }
        "Publish the migration. Without this flag the command only reports eligible sessions" => {
            Some("cli-help-migrate-rollouts-publish-the-migration-without-this-flag-the-command")
        }
        "Restrict inspection or migration to one or more thread IDs" => {
            Some("cli-help-migrate-rollouts-restrict-inspection-or-migration-to-one-or-more")
        }
        "Limit aggregate rollout read and write throughput, in MiB per second" => {
            Some("cli-help-migrate-rollouts-limit-aggregate-rollout-read-and-write-throughput-in")
        }
        "Emit the complete per-thread report as JSON" => {
            Some("cli-help-migrate-rollouts-emit-the-complete-per-thread-report-as-json")
        }
        "Print one line for every inspected rollout" => {
            Some("cli-help-migrate-rollouts-print-one-line-for-every-inspected-rollout")
        }
        "Actions supported by the packaged updater" => {
            Some("cli-help-opendiy-update-actions-supported-by-the-packaged-updater")
        }
        "Install a plugin from a configured or remote marketplace" => {
            Some("cli-help-plugin-cmd-install-a-plugin-from-a-configured-or-remote")
        }
        "Install a plugin from a configured or remote marketplace.\n\nPass either `PLUGIN@MARKETPLACE` or pass `PLUGIN` with `--marketplace MARKETPLACE`." => {
            Some("cli-help-plugin-cmd-install-a-plugin-from-a-configured-or-remote-details")
        }
        "List plugins available from configured and remote marketplaces" => Some(
            "cli-help-plugin-cmd-list-plugins-available-from-configured-and-remote-marketplaces",
        ),
        "Add, list, upgrade, or remove configured plugin marketplaces" => {
            Some("cli-help-plugin-cmd-add-list-upgrade-or-remove-configured-plugin-marketplaces")
        }
        "Uninstall a plugin and remove its local cache" => {
            Some("cli-help-plugin-cmd-uninstall-a-plugin-and-remove-its-local-cache")
        }
        "Uninstall a plugin and remove its local cache.\n\nPass either `PLUGIN@MARKETPLACE` or pass `PLUGIN` with `--marketplace MARKETPLACE`." => {
            Some("cli-help-plugin-cmd-uninstall-a-plugin-and-remove-its-local-cache-details")
        }
        "Plugin selector to install: either PLUGIN@MARKETPLACE or PLUGIN with --marketplace" => {
            Some("cli-help-plugin-cmd-plugin-selector-to-install-either-plugin-marketplace-or")
        }
        "Marketplace name to use when PLUGIN does not include @MARKETPLACE" => {
            Some("cli-help-plugin-cmd-marketplace-name-to-use-when-plugin-does-not")
        }
        "Output install result as JSON" => {
            Some("cli-help-plugin-cmd-output-install-result-as-json")
        }
        "Only list plugins from this marketplace name" => {
            Some("cli-help-plugin-cmd-only-list-plugins-from-this-marketplace-name")
        }
        "Output plugin list as JSON" => Some("cli-help-plugin-cmd-output-plugin-list-as-json"),
        "Include uninstalled marketplace plugins in the JSON output" => {
            Some("cli-help-plugin-cmd-include-uninstalled-marketplace-plugins-in-the-json-output")
        }
        "Plugin selector to remove: either PLUGIN@MARKETPLACE or PLUGIN with --marketplace" => {
            Some("cli-help-plugin-cmd-plugin-selector-to-remove-either-plugin-marketplace-or")
        }
        "Examples:\n  codex plugin add sample@debug\n  codex plugin add sample --marketplace debug" => {
            Some("cli-help-plugin-cmd-examples-codex-plugin-add-sample-debug-codex-plugin")
        }
        "Examples:\n  codex plugin list\n  codex plugin list --marketplace debug\n  codex plugin list --json\n  codex plugin list --available --json" => {
            Some("cli-help-plugin-cmd-examples-codex-plugin-list-codex-plugin-list-marketplace")
        }
        "Examples:\n  codex plugin remove sample@debug\n  codex plugin remove sample --marketplace debug" => {
            Some("cli-help-plugin-cmd-examples-codex-plugin-remove-sample-debug-codex-plugin")
        }
        "Session UUID or exact session name" => {
            Some("cli-help-queue-cmd-session-uuid-or-exact-session-name")
        }
        "Message text to queue" => Some("cli-help-queue-cmd-message-text-to-queue"),
        "After queueing, try to resume the thread on the local daemon or selected --remote server. A failed resume produces a warning and the command still succeeds" => {
            Some("cli-help-queue-cmd-after-queueing-try-to-resume-the-thread-on")
        }
        "Emit machine-readable JSON" => {
            Some("cli-help-remote-control-cmd-emit-machine-readable-json")
        }
        "Run an embedded app-server in the foreground until Ctrl-C" => {
            Some("cli-help-remote-control-cmd-run-an-embedded-app-server-in-the-foreground")
        }
        "Start the app-server daemon with remote control enabled" => {
            Some("cli-help-remote-control-cmd-start-the-app-server-daemon-with-remote-control")
        }
        "Stop the app-server daemon" => {
            Some("cli-help-remote-control-cmd-stop-the-app-server-daemon")
        }
        "Create and print a short-lived manual pairing code" => {
            Some("cli-help-remote-control-cmd-create-and-print-a-short-lived-manual-pairing")
        }
        "Read relay status from the existing shared daemon without starting it" => {
            Some("cli-help-remote-control-cmd-read-relay-status-from-the-existing-shared-daemon")
        }
        "Set up the elevated Windows sandbox" => {
            Some("cli-help-sandbox-setup-set-up-the-elevated-windows-sandbox")
        }
        "Windows user that will run Codex after managed deployment" => {
            Some("cli-help-sandbox-setup-windows-user-that-will-run-codex-after-managed")
        }
        "Use the current Windows user as the Codex user" => {
            Some("cli-help-sandbox-setup-use-the-current-windows-user-as-the-codex")
        }
        "CODEX_HOME for the Codex user. Required with --user" => {
            Some("cli-help-sandbox-setup-codex-home-for-the-codex-user-required-with")
        }
        "Remove the legacy Windows sandbox's machine-wide accounts and network rules" => {
            Some("cli-help-sandbox-uninstall-remove-the-legacy-windows-sandbox-s-machine-wide")
        }
        "Run as administrator after stopping apps using the legacy sandbox and its provisioning service, if installed.\nPreserves Codex home, including sandbox directories and filesystem permissions." => {
            Some("cli-help-sandbox-uninstall-run-as-administrator-after-stopping-apps-using-the")
        }
        "인증된 로컬 통로로 Desktop이 소유한 예약 제어를 켭니다" => {
            Some("cli-desktop-cron-control")
        }
        "Internal: launching CLI that handles daemon updates after the TUI exits" => {
            Some("cli-help-cli-internal-launching-cli-that-handles-daemon-updates-after")
        }
        "Optional user prompt to start the session" => {
            Some("cli-help-cli-optional-user-prompt-to-start-the-session")
        }
        "Internal: resume a specific recorded session by id (UUID). Set by the top-level `codex resume <SESSION_ID>` wrapper; not exposed as a public flag" => {
            Some("cli-help-cli-internal-resume-a-specific-recorded-session-by-id")
        }
        "Internal: show all sessions (disables cwd filtering and shows CWD column)" => {
            Some("cli-help-cli-internal-show-all-sessions-disables-cwd-filtering-and")
        }
        "Internal: include non-interactive sessions in resume listings" => {
            Some("cli-help-cli-internal-include-non-interactive-sessions-in-resume-listings")
        }
        "Internal: open the daemon-wide agents overview instead of starting a thread" => {
            Some("cli-help-cli-internal-open-the-daemon-wide-agents-overview-instead")
        }
        "Internal: the launching CLI verified that the local daemon has a managed backend" => {
            Some("cli-help-cli-internal-the-launching-cli-verified-that-the-local")
        }
        "Internal: fork a specific recorded session by id (UUID). Set by the top-level `codex fork <SESSION_ID>` wrapper; not exposed as a public flag" => {
            Some("cli-help-cli-internal-fork-a-specific-recorded-session-by-id")
        }
        "Configure when the model requires human approval before executing a command" => {
            Some("cli-help-cli-configure-when-the-model-requires-human-approval-before")
        }
        "Enable live web search. When enabled, the native Responses `web_search` tool is available to the model (no per‑call approval)" => {
            Some("cli-help-cli-enable-live-web-search-when-enabled-the-native")
        }
        "Disable alternate screen mode\n\nRuns the TUI in inline mode, preserving terminal scrollback history." => {
            Some("cli-help-cli-disable-alternate-screen-mode-runs-the-tui-in")
        }
        "Run without the shared background server, even if it is already running" => {
            Some("cli-help-cli-run-without-the-shared-background-server-even-if")
        }
        "Action to perform. If omitted, runs a new non-interactive session" => {
            Some("cli-help-cli-action-to-perform-if-omitted-runs-a-new")
        }
        "Source classification for newly created or forked threads" => {
            Some("cli-help-cli-source-classification-for-newly-created-or-forked-threads")
        }
        "Request an experimental Cyber access program for this turn (OpenAI provider only). Omit to use server defaults. Not supported with review; fork requires a prompt" => {
            Some("cli-help-cli-request-an-experimental-cyber-access-program-for-this")
        }
        "Allow running Codex outside a Git repository" => {
            Some("cli-help-cli-allow-running-codex-outside-a-git-repository")
        }
        "Run without persisting session files to disk" => {
            Some("cli-help-cli-run-without-persisting-session-files-to-disk")
        }
        "Do not load `$CODEX_HOME/config.toml`; auth still uses `CODEX_HOME`" => {
            Some("cli-help-cli-do-not-load-codex-home-config-toml-auth")
        }
        "Do not load user or project execpolicy `.rules` files" => {
            Some("cli-help-cli-do-not-load-user-or-project-execpolicy-rules")
        }
        "Path to a JSON Schema file describing the model's final response shape" => {
            Some("cli-help-cli-path-to-a-json-schema-file-describing-the")
        }
        "Specifies color settings for use in the output" => {
            Some("cli-help-cli-specifies-color-settings-for-use-in-the-output")
        }
        "Print events to stdout as JSONL" => Some("cli-help-cli-print-events-to-stdout-as-jsonl"),
        "Specifies file where the last message from the agent should be written" => {
            Some("cli-help-cli-specifies-file-where-the-last-message-from-the")
        }
        "Initial instructions for the agent. If not provided as an argument (or if `-` is used), instructions are read from stdin. If stdin is piped and a prompt is also provided, stdin is appended as a `<stdin>` block" => {
            Some("cli-help-cli-initial-instructions-for-the-agent-if-not-provided")
        }
        "Resume a previous session by id or pick the most recent with --last" => {
            Some("cli-help-cli-resume-a-previous-session-by-id-or-pick")
        }
        "Fork a previous session by id into a new session" => {
            Some("cli-help-cli-fork-a-previous-session-by-id-into-a")
        }
        "Run a code review against the current repository" => {
            Some("cli-help-cli-run-a-code-review-against-the-current-repository")
        }
        "Conversation/session id (UUID) or thread name to fork" => {
            Some("cli-help-cli-conversation-session-id-uuid-or-thread-name-to")
        }
        "Optional image(s) to attach to the prompt sent after forking" => {
            Some("cli-help-cli-optional-image-s-to-attach-to-the-prompt")
        }
        "Optional prompt to send after forking. If `-` is used, read from stdin" => {
            Some("cli-help-cli-optional-prompt-to-send-after-forking-if-is")
        }
        "Conversation/session id (UUID) or thread name. UUIDs take precedence if it parses. If omitted, use --last to pick the most recent recorded session" => {
            Some("cli-help-cli-conversation-session-id-uuid-or-thread-name-uuids")
        }
        "Resume the most recent recorded session (newest) without specifying an id" => {
            Some("cli-help-cli-resume-the-most-recent-recorded-session-newest-without")
        }
        "Show all sessions (disables cwd filtering)" => {
            Some("cli-help-cli-show-all-sessions-disables-cwd-filtering")
        }
        "Optional image(s) to attach to the prompt sent after resuming" => {
            Some("cli-help-cli-optional-image-s-to-attach-to-the-prompt-details")
        }
        "Prompt to send after resuming the session. If `-` is used, read from stdin" => {
            Some("cli-help-cli-prompt-to-send-after-resuming-the-session-if")
        }
        "Review staged, unstaged, and untracked changes" => {
            Some("cli-help-cli-review-staged-unstaged-and-untracked-changes")
        }
        "Review changes against the given base branch" => {
            Some("cli-help-cli-review-changes-against-the-given-base-branch")
        }
        "Review the changes introduced by a commit" => {
            Some("cli-help-cli-review-the-changes-introduced-by-a-commit")
        }
        "Optional commit title to display in the review summary" => {
            Some("cli-help-cli-optional-commit-title-to-display-in-the-review")
        }
        "Custom review instructions. If `-` is used, read from stdin" => {
            Some("cli-help-cli-custom-review-instructions-if-is-used-read-from")
        }
        "Submit a new Codex Cloud task without launching the TUI" => {
            Some("cli-help-cli-submit-a-new-codex-cloud-task-without-launching")
        }
        "Show the status of a Codex Cloud task" => {
            Some("cli-help-cli-show-the-status-of-a-codex-cloud-task")
        }
        "List Codex Cloud tasks" => Some("cli-help-cli-list-codex-cloud-tasks"),
        "List accessible Codex Cloud environments" => {
            Some("cli-help-cli-list-accessible-codex-cloud-environments")
        }
        "Apply the diff for a Codex Cloud task locally" => {
            Some("cli-help-cli-apply-the-diff-for-a-codex-cloud-task")
        }
        "Show the unified diff for a Codex Cloud task" => {
            Some("cli-help-cli-show-the-unified-diff-for-a-codex-cloud")
        }
        "Task prompt to run in Codex Cloud" => {
            Some("cli-help-cli-task-prompt-to-run-in-codex-cloud")
        }
        "Target environment identifier (see `codex cloud` to browse)" => {
            Some("cli-help-cli-target-environment-identifier-see-codex-cloud-to-browse")
        }
        "Number of assistant attempts (best-of-N)" => {
            Some("cli-help-cli-number-of-assistant-attempts-best-of-n")
        }
        "Git branch to run in Codex Cloud (defaults to current branch)" => {
            Some("cli-help-cli-git-branch-to-run-in-codex-cloud-defaults")
        }
        "Codex Cloud task identifier to inspect" => {
            Some("cli-help-cli-codex-cloud-task-identifier-to-inspect")
        }
        "Filter tasks by environment identifier" => {
            Some("cli-help-cli-filter-tasks-by-environment-identifier")
        }
        "Maximum number of tasks to return (1-20)" => {
            Some("cli-help-cli-maximum-number-of-tasks-to-return-1-20")
        }
        "Pagination cursor returned by a previous call" => {
            Some("cli-help-cli-pagination-cursor-returned-by-a-previous-call")
        }
        "Emit JSON instead of plain text" => Some("cli-help-cli-emit-json-instead-of-plain-text"),
        "Codex Cloud task identifier to apply" => {
            Some("cli-help-cli-codex-cloud-task-identifier-to-apply")
        }
        "Attempt number to apply (1-based)" => Some("cli-help-cli-attempt-number-to-apply-1-based"),
        "Codex Cloud task identifier to display" => {
            Some("cli-help-cli-codex-cloud-task-identifier-to-display")
        }
        "Attempt number to display (1-based)" => {
            Some("cli-help-cli-attempt-number-to-display-1-based")
        }
        "Optional image(s) to attach to the initial prompt" => {
            Some("cli-help-shared-options-optional-image-s-to-attach-to-the-initial")
        }
        "Model the agent should use" => Some("cli-help-shared-options-model-the-agent-should-use"),
        "Use open-source provider" => Some("cli-help-shared-options-use-open-source-provider"),
        "Specify which local provider to use (lmstudio or ollama). If not specified with --oss, will use config default or show selection" => {
            Some("cli-help-shared-options-specify-which-local-provider-to-use-lmstudio-or")
        }
        "Select the sandbox policy to use when executing model-generated shell commands" => {
            Some("cli-help-shared-options-select-the-sandbox-policy-to-use-when-executing")
        }
        "Route approval requests through automatic review using the workspace-write sandbox" => {
            Some(
                "cli-help-shared-options-route-approval-requests-through-automatic-review-using-the",
            )
        }
        "Skip all confirmation prompts and execute commands without sandboxing. EXTREMELY DANGEROUS. Intended solely for running in environments that are externally sandboxed" => {
            Some(
                "cli-help-shared-options-skip-all-confirmation-prompts-and-execute-commands-without",
            )
        }
        "Run enabled hooks without requiring persisted hook trust for this invocation. DANGEROUS. Intended only for automation that already vets hook sources" => {
            Some("cli-help-shared-options-run-enabled-hooks-without-requiring-persisted-hook-trust")
        }
        "Tell the agent to use the specified directory as its working root" => {
            Some("cli-help-shared-options-tell-the-agent-to-use-the-specified-directory")
        }
        "Run the session in a new managed Git worktree" => {
            Some("cli-help-shared-options-run-the-session-in-a-new-managed-git")
        }
        "Additional directories that should be writable alongside the primary workspace" => Some(
            "cli-help-shared-options-additional-directories-that-should-be-writable-alongside-the",
        ),
        "The model decides when to ask the user for approval" => {
            Some("cli-help-approval-mode-cli-arg-the-model-decides-when-to-ask-the-user")
        }
        "Never ask for user approval Execution failures are immediately returned to the model" => {
            Some(
                "cli-help-approval-mode-cli-arg-never-ask-for-user-approval-execution-failures-are",
            )
        }
        "CLI option that captures arbitrary configuration overrides specified as `-c key=value`. It intentionally keeps both halves **unparsed** so that the calling code can decide how to interpret the right-hand side" => {
            Some(
                "cli-help-config-override-cli-option-that-captures-arbitrary-configuration-overrides-specified",
            )
        }
        "Override a configuration value that would otherwise be loaded from `~/.codex/config.toml`. Use a dotted path (`foo.bar.baz`) to override nested values. The `value` portion is parsed as TOML. If it fails to parse as TOML, the raw string is used as a literal" => {
            Some("cli-help-config-override-override-a-configuration-value-that-would-otherwise-be")
        }
        "Override a configuration value that would otherwise be loaded from `~/.codex/config.toml`. Use a dotted path (`foo.bar.baz`) to override nested values. The `value` portion is parsed as TOML. If it fails to parse as TOML, the raw string is used as a literal.\n\nExamples: - `-c model=\"o3\"` - `-c 'sandbox_permissions=[\"disk-full-read-access\"]'` - `-c shell_environment_policy.inherit=all`" => {
            Some(
                "cli-help-config-override-override-a-configuration-value-that-would-otherwise-be-details",
            )
        }
        "Linux PID namespace: isolate (default) or inherit. Inherit allows signals to other same-UID processes; enable only when provisioning a dedicated environment" => {
            Some("cli-help-command-linux-pid-namespace-isolate-default-or-inherit-inherit")
        }
        "Allow permitted private IP destinations to use the configured upstream proxy. If no valid upstream proxy applies to the request protocol, connect directly. Loopback stays local. This flag does not require upstream routing" => {
            Some("cli-help-command-allow-permitted-private-ip-destinations-to-use-the")
        }
        "Maximum number of requests to process concurrently on each connection" => {
            Some("cli-help-command-maximum-number-of-requests-to-process-concurrently-on")
        }
        "Transport endpoint URL. Supported values: `ws://IP:PORT` (default), `stdio`, `stdio://`" => {
            Some("cli-help-command-transport-endpoint-url-supported-values-ws-ip-port")
        }
        "Register this exec-server as a remote environment using the given base URL" => {
            Some("cli-help-command-register-this-exec-server-as-a-remote-environment")
        }
        "Transport used for the remote executor connection" => {
            Some("cli-help-command-transport-used-for-the-remote-executor-connection")
        }
        "Environment id to attach to when registering remotely" => {
            Some("cli-help-command-environment-id-to-attach-to-when-registering-remotely")
        }
        "Human-readable environment name" => {
            Some("cli-help-command-human-readable-environment-name")
        }
        "Use Agent Identity auth from CODEX_ACCESS_TOKEN for remote registration" => {
            Some("cli-help-command-use-agent-identity-auth-from-codex-access-token")
        }
        "Read rotating executor credentials from stdin; exit when the pipe closes" => {
            Some("cli-help-command-read-rotating-executor-credentials-from-stdin-exit-when")
        }
        "Sign Direct registration and WebSocket handshake requests with AWS SigV4" => {
            Some("cli-help-command-sign-direct-registration-and-websocket-handshake-requests-with")
        }
        "AWS profile used for SigV4 authentication" => {
            Some("cli-help-command-aws-profile-used-for-sigv4-authentication")
        }
        "AWS signing region. Uses the SDK region chain when omitted" => {
            Some("cli-help-command-aws-signing-region-uses-the-sdk-region-chain")
        }
        "AWS signing service" => Some("cli-help-command-aws-signing-service"),
        "Exit when the parent-owned standard-input pipe closes" => {
            Some("cli-help-command-exit-when-the-parent-owned-standard-input-pipe")
        }
        "Register an existing WebSocket exec-server as a remote environment" => {
            Some("cli-help-command-register-an-existing-websocket-exec-server-as-a")
        }
        "Destination exec-server WebSocket URL" => {
            Some("cli-help-command-destination-exec-server-websocket-url")
        }
        "Sign out using the managed auth path before loading fresh startup requirements" => {
            Some("cli-help-startup-config-sign-out-using-the-managed-auth-path-before")
        }
        "Validate configuration and print requirements as JSON without starting the app server" => {
            Some(
                "cli-help-startup-config-validate-configuration-and-print-requirements-as-json-without",
            )
        }
        "ClientInfo JSON used for configuration/auth request identity" => Some(
            "cli-help-startup-config-clientinfo-json-used-for-configuration-auth-request-identity",
        ),
        "Selects the code-mode host for a single app-server process" => {
            Some("cli-help-code-mode-host-selects-the-code-mode-host-for-a-single")
        }
        "Connect to a remote code-mode host instead of starting a local host" => {
            Some("cli-help-code-mode-host-connect-to-a-remote-code-mode-host-instead")
        }
        "Process-scoped transport used to reach the code-mode host" => {
            Some("cli-help-code-mode-host-process-scoped-transport-used-to-reach-the-code")
        }
        "Start and own the default local code-mode host" => {
            Some("cli-help-code-mode-host-start-and-own-the-default-local-code-mode")
        }
        "Share an HTTP/2 gRPC connection to the specified remote code-mode host" => {
            Some("cli-help-code-mode-host-share-an-http-2-grpc-connection-to-the")
        }
        "Authentication mode for incoming WebSocket connections" => {
            Some("cli-help-lib-authentication-mode-for-incoming-websocket-connections")
        }
        "Absolute path to the capability-token file" => {
            Some("cli-help-lib-absolute-path-to-the-capability-token-file")
        }
        "Hex-encoded SHA-256 digest of the capability token" => {
            Some("cli-help-lib-hex-encoded-sha-256-digest-of-the-capability")
        }
        "Absolute path to the shared secret file for signed JWT bearer tokens" => {
            Some("cli-help-lib-absolute-path-to-the-shared-secret-file-for")
        }
        "Expected issuer for signed JWT bearer tokens" => {
            Some("cli-help-lib-expected-issuer-for-signed-jwt-bearer-tokens")
        }
        "Expected audience for signed JWT bearer tokens" => {
            Some("cli-help-lib-expected-audience-for-signed-jwt-bearer-tokens")
        }
        "Maximum clock skew when validating signed JWT bearer tokens" => {
            Some("cli-help-lib-maximum-clock-skew-when-validating-signed-jwt-bearer")
        }
        "Loaded upgrade policy. The default permits unauthenticated connections; each listener decides whether that policy is allowed for its bind address" => {
            Some(
                "cli-help-lib-loaded-upgrade-policy-the-default-permits-unauthenticated-connections",
            )
        }
        "Arguments for evaluating a command against one or more execpolicy files" => {
            Some("cli-help-execpolicycheck-arguments-for-evaluating-a-command-against-one-or")
        }
        "Paths to execpolicy rule files to evaluate (repeatable)" => {
            Some("cli-help-execpolicycheck-paths-to-execpolicy-rule-files-to-evaluate-repeatable")
        }
        "Pretty-print the JSON output" => {
            Some("cli-help-execpolicycheck-pretty-print-the-json-output")
        }
        "Resolve absolute program paths against basename rules, gated by any `host_executable()` definitions in the loaded policy files" => {
            Some(
                "cli-help-execpolicycheck-resolve-absolute-program-paths-against-basename-rules-gated",
            )
        }
        "Command tokens to check against the policy" => {
            Some("cli-help-execpolicycheck-command-tokens-to-check-against-the-policy")
        }
        "CLI arguments for the proxy" => Some("cli-help-lib-cli-arguments-for-the-proxy"),
        "Port to listen on. If not set, an ephemeral port is used" => {
            Some("cli-help-lib-port-to-listen-on-if-not-set-an")
        }
        "Path to a JSON file to write startup info (single line). Includes {\"port\": <u16>}" => {
            Some("cli-help-lib-path-to-a-json-file-to-write-startup")
        }
        "Enable HTTP shutdown endpoint at GET /shutdown" => {
            Some("cli-help-lib-enable-http-shutdown-endpoint-at-get-shutdown")
        }
        "Absolute URL the proxy should forward requests to (defaults to OpenAI)" => {
            Some("cli-help-lib-absolute-url-the-proxy-should-forward-requests-to")
        }
        "Directory where request/response dumps should be written as JSON" => {
            Some("cli-help-lib-directory-where-request-response-dumps-should-be-written")
        }
        "Authenticate to the OpenAI API using GitHub Actions workload identity. In this mode stdin supplies the GitHub OIDC request token, not an API key" => {
            Some("cli-help-lib-authenticate-to-the-openai-api-using-github-actions")
        }
        "Audience configured on the OpenAI workload identity provider" => {
            Some("cli-help-lib-audience-configured-on-the-openai-workload-identity-provider")
        }
        "OpenAI workload identity provider ID" => {
            Some("cli-help-lib-openai-workload-identity-provider-id")
        }
        "OpenAI service account ID selected by the provider mapping" => {
            Some("cli-help-lib-openai-service-account-id-selected-by-the-provider")
        }
        "HTTPS token-request URL from GitHub's runner-provided ACTIONS_ID_TOKEN_REQUEST_URL" => {
            Some("cli-help-lib-https-token-request-url-from-github-s-runner")
        }
        "HTTPS token-request URL from GitHub's runner-provided ACTIONS_ID_TOKEN_REQUEST_URL.\n\nUse the value directly from the trusted runner environment, never from a workflow input or repository content. The proxy sends the GitHub request credential to this URL." => {
            Some("cli-help-lib-https-token-request-url-from-github-s-runner-details")
        }
        "Minimal OpenAI responses proxy" => Some("cli-help-lib-minimal-openai-responses-proxy"),
        "HTTPS origin of the HTTP/3 proxy" => Some("cli-help-lib-https-origin-of-the-http-3-proxy"),
        "File containing one exact approved HTTPS proxy origin per line" => {
            Some("cli-help-lib-file-containing-one-exact-approved-https-proxy-origin")
        }
        "CONNECT target authority, including a nonzero port" => {
            Some("cli-help-lib-connect-target-authority-including-a-nonzero-port")
        }
        "Read the initial bearer from stdin" => {
            Some("cli-help-lib-read-the-initial-bearer-from-stdin")
        }
        "Read replacement bearers from stdin and stop when the controlling pipe closes" => {
            Some("cli-help-lib-read-replacement-bearers-from-stdin-and-stop-when")
        }
        "Read a JSON list of extension-header name/value pairs before the first bearer" => {
            Some("cli-help-lib-read-a-json-list-of-extension-header-name")
        }
        "Emit credential-safe JSON diagnostics on stderr instead of human-readable errors" => {
            Some("cli-help-lib-emit-credential-safe-json-diagnostics-on-stderr-instead")
        }
        "Print help" => Some("cli-print-help"),
        "Print version" => Some("cli-print-version"),
        "Print help (see more with '--help')" => Some("cli-print-help-long-hint"),
        "Print help (see a summary with '-h')" => Some("cli-print-help-short-hint"),
        "Print this message or the help of the given subcommand(s)" => Some("cli-help-command"),
        "Print help for the subcommand(s)" => Some("cli-help-subcommands"),
        "Commands" => Some("cli-commands-heading"),
        "Arguments" => Some("cli-arguments-heading"),
        "Options" => Some("cli-options-heading"),
        "Local server options (without --remote)" => Some("cli-local-server-heading"),
        "Override a startup configuration value (key=value). Queued messages use the thread's model." => {
            Some("cli-queue-config-help")
        }
        _ => None,
    }
}

pub(super) fn exit_error(error: clap::Error, locale: Locale) -> ! {
    if locale == Locale::Korean {
        error.apply::<KoreanErrorFormatter>().exit()
    }
    error.exit()
}

struct KoreanErrorFormatter;

impl ErrorFormatter for KoreanErrorFormatter {
    fn format_error(error: &clap::error::Error<Self>) -> StyledStr {
        let locale = Locale::Korean;
        let context = |kind| error.get(kind).map(ToString::to_string).unwrap_or_default();
        let arg = context(ContextKind::InvalidArg);
        let value = context(ContextKind::InvalidValue);
        let subcommand = context(ContextKind::InvalidSubcommand);
        let prior = context(ContextKind::PriorArg);
        let actual = context(ContextKind::ActualNumValues);
        let expected = context(ContextKind::ExpectedNumValues);
        let minimum = context(ContextKind::MinValues);
        let id = match error.kind() {
            ErrorKind::ArgumentConflict if !arg.is_empty() && arg == prior => {
                "cli-error-repeated-argument"
            }
            ErrorKind::ArgumentConflict => "cli-error-conflict",
            ErrorKind::NoEquals => "cli-error-equals",
            ErrorKind::InvalidValue if value.is_empty() => "cli-error-missing-value",
            ErrorKind::InvalidValue | ErrorKind::ValueValidation => "cli-error-invalid-value",
            ErrorKind::InvalidSubcommand => "cli-error-invalid-subcommand",
            ErrorKind::MissingRequiredArgument => "cli-error-missing-required",
            ErrorKind::MissingSubcommand => "cli-error-missing-subcommand",
            ErrorKind::InvalidUtf8 => "cli-error-invalid-utf8",
            ErrorKind::TooManyValues => "cli-error-too-many-values",
            ErrorKind::TooFewValues => "cli-error-too-few-values",
            ErrorKind::WrongNumberOfValues => "cli-error-wrong-number-values",
            ErrorKind::UnknownArgument => "cli-error-unknown-argument",
            _ => "cli-error-other",
        };
        let subject = if arg.is_empty() { &subcommand } else { &arg };
        let mut output = format!(
            "{} {}",
            message_for(locale, "cli-error-heading", &[]),
            message_for(
                locale,
                id,
                &[
                    ("arg", subject.as_str()),
                    ("value", value.as_str()),
                    ("subcommand", subcommand.as_str()),
                    ("prior", prior.as_str()),
                    ("actual", actual.as_str()),
                    ("expected", expected.as_str()),
                    ("minimum", minimum.as_str()),
                ]
            )
        );
        // A parser/validator owns its source text. Preserve it verbatim, including
        // user values that happen to contain English error or help phrases.
        if let Some(source) = std::error::Error::source(error) {
            output.push_str(&format!("\n  {source}"));
        }
        for (kind, id) in [
            (ContextKind::ValidValue, "cli-error-valid-values"),
            (ContextKind::ValidSubcommand, "cli-error-valid-subcommands"),
            (
                ContextKind::SuggestedSubcommand,
                "cli-error-suggested-subcommand",
            ),
            (ContextKind::SuggestedArg, "cli-error-suggested-argument"),
            (ContextKind::SuggestedValue, "cli-error-suggested-value"),
        ] {
            if let Some(value) = error.get(kind) {
                output.push_str(&format!(
                    "\n\n  {}",
                    message_for(locale, id, &[("value", &value.to_string())])
                ));
            }
        }
        for kind in [
            ContextKind::SuggestedCommand,
            ContextKind::Suggested,
            ContextKind::Custom,
            ContextKind::TrailingArg,
        ] {
            if let Some(value) = error.get(kind) {
                output.push_str(&format!(
                    "\n\n  {}",
                    message_for(
                        locale,
                        "cli-error-context",
                        &[("value", &value.to_string())]
                    )
                ));
            }
        }
        if let Some(ContextValue::StyledStr(usage)) = error.get(ContextKind::Usage) {
            // Usage is structured context. Translate only Clap's leading title,
            // never occurrences inside user values, command names or paths.
            let usage = usage.to_string();
            let body = usage.strip_prefix("Usage: ").unwrap_or(&usage);
            output.push_str(&format!(
                "\n\n{} {body}",
                message_for(locale, "cli-usage-heading", &[])
            ));
        }
        output.push_str(&format!(
            "\n\n{}\n",
            message_for(locale, "cli-error-try-help", &[])
        ));
        StyledStr::from(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Arg;
    use clap::CommandFactory;

    #[test]
    fn entire_help_tree_has_catalog_entries_and_preserves_syntax_metadata() {
        fn visit(before: &Command, after: &Command, path: &str) {
            assert_eq!(before.get_name(), after.get_name(), "{path}");
            assert_eq!(
                before.get_all_aliases().collect::<Vec<_>>(),
                after.get_all_aliases().collect::<Vec<_>>(),
                "{path}"
            );
            assert_eq!(
                before.get_all_short_flag_aliases().collect::<Vec<_>>(),
                after.get_all_short_flag_aliases().collect::<Vec<_>>(),
                "{path}"
            );
            assert_eq!(
                before.get_all_long_flag_aliases().collect::<Vec<_>>(),
                after.get_all_long_flag_aliases().collect::<Vec<_>>(),
                "{path}"
            );
            if let Some(heading) = before.get_subcommand_help_heading() {
                assert!(
                    help_message_id(heading).is_some(),
                    "uncataloged heading {path}: {heading}"
                );
            }
            for value in [
                before.get_about(),
                before.get_long_about(),
                before.get_before_help(),
                before.get_before_long_help(),
                before.get_after_help(),
                before.get_after_long_help(),
            ]
            .into_iter()
            .flatten()
            {
                let value = value.to_string();
                assert!(
                    help_message_id(&value).is_some()
                        || value.chars().any(|c| ('가'..='힣').contains(&c)),
                    "uncataloged {path}: {value:?}"
                );
            }
            assert_eq!(
                before.get_arguments().count(),
                after.get_arguments().count(),
                "{path}"
            );
            for (a, b) in before.get_arguments().zip(after.get_arguments()) {
                assert_eq!(a.get_id(), b.get_id(), "{path}");
                assert_eq!(a.get_long(), b.get_long(), "{path}");
                assert_eq!(a.get_short(), b.get_short(), "{path}");
                assert_eq!(a.get_all_aliases(), b.get_all_aliases(), "{path}");
                assert_eq!(
                    a.get_all_short_aliases(),
                    b.get_all_short_aliases(),
                    "{path}"
                );
                assert_eq!(a.get_default_values(), b.get_default_values(), "{path}");
                assert_eq!(a.get_env(), b.get_env(), "{path}");
                if let Some(heading) = a.get_help_heading() {
                    assert!(
                        help_message_id(heading).is_some(),
                        "uncataloged heading {path}/{}: {heading}",
                        a.get_id()
                    );
                }
                assert_eq!(
                    a.get_value_parser().type_id(),
                    b.get_value_parser().type_id(),
                    "{path}"
                );
                for value in [a.get_help(), a.get_long_help()].into_iter().flatten() {
                    let value = value.to_string();
                    assert!(
                        help_message_id(&value).is_some()
                            || value.chars().any(|c| ('가'..='힣').contains(&c)),
                        "uncataloged {path}/{}: {value:?}",
                        a.get_id()
                    );
                }
                if let Some(values) = a.get_value_parser().possible_values() {
                    for value in values {
                        if let Some(help) = value.get_help() {
                            assert!(
                                help_message_id(&help.to_string()).is_some(),
                                "uncataloged {path}/{} value {}: {help}",
                                a.get_id(),
                                value.get_name()
                            );
                        }
                    }
                }
            }
            assert_eq!(
                before.get_subcommands().count(),
                after.get_subcommands().count(),
                "{path}"
            );
            for (before, after) in before.get_subcommands().zip(after.get_subcommands()) {
                visit(before, after, &format!("{path}/{}", before.get_name()));
            }
        }
        let mut original = crate::MultitoolCli::command();
        original.build();
        let localized = localize_help(original.clone(), Locale::Korean);
        visit(&original, &localized, "codex");
    }

    #[test]
    fn english_tree_matches_clap_except_preexisting_korean_declarations() {
        fn visit(mut original: Command, mut localized: Command) {
            for long in [false, true] {
                let before = if long {
                    original.render_long_help()
                } else {
                    original.render_help()
                }
                .to_string();
                let after = if long {
                    localized.render_long_help()
                } else {
                    localized.render_help()
                }
                .to_string();
                if !before.chars().any(|c| ('가'..='힣').contains(&c)) {
                    assert_eq!(before, after, "{} long={long}", original.get_name());
                }
                assert!(
                    !after.chars().any(|c| ('가'..='힣').contains(&c)),
                    "{after}"
                );
            }
            for (a, b) in original.get_subcommands().zip(localized.get_subcommands()) {
                visit(a.clone(), b.clone());
            }
        }
        let mut original = crate::MultitoolCli::command();
        original.build();
        visit(
            original,
            localize_help(crate::MultitoolCli::command(), Locale::English),
        );
    }

    #[test]
    fn help_extensions_translate_without_replacing_typed_value_parsers() {
        use clap::builder::PossibleValue;
        let original = Command::new("metadata")
            .before_help("Print help")
            .before_long_help("Print version")
            .after_help("Print version")
            .after_long_help("Print help")
            .arg(
                Arg::new("number")
                    .long("number")
                    .value_parser(clap::value_parser!(u16))
                    .default_value("42")
                    .env("R35_I18N_ISOLATED_NUMBER"),
            )
            .arg(
                Arg::new("mode").long("mode").value_parser([
                    PossibleValue::new("auto")
                        .alias("automatic")
                        .help("The model decides when to ask the user for approval"),
                    PossibleValue::new("hidden").hide(true),
                ]),
            );
        let mut localized = localize_help(original.clone(), Locale::Korean);
        let short = localized.render_help().to_string();
        let long = localized.render_long_help().to_string();
        assert!(short.starts_with("도움말 표시"), "{short}");
        assert!(short.contains("기본값: 42"), "{short}");
        assert!(
            short.contains("환경 변수: R35_I18N_ISOLATED_NUMBER"),
            "{short}"
        );
        assert!(
            short.contains("auto: 사용자 승인 요청 시점을 모델이 결정합니다"),
            "{short}"
        );
        assert!(!short.contains("  hidden"), "{short}");
        assert!(long.starts_with("버전 표시"), "{long}");
        assert!(long.contains("도움말 표시"), "{long}");
        let parsed = localized
            .try_get_matches_from(["metadata", "--number", "65535", "--mode", "automatic"])
            .unwrap();
        assert_eq!(parsed.get_one::<u16>("number"), Some(&65535));
        assert_eq!(
            parsed.get_one::<String>("mode").map(String::as_str),
            Some("automatic")
        );
        for locale in [Locale::English, Locale::Korean] {
            let error = localize_help(original.clone(), locale)
                .try_get_matches_from(["metadata", "--number", "65536"])
                .unwrap_err();
            assert_eq!(error.kind(), ErrorKind::ValueValidation);
        }
    }

    #[test]
    fn actual_command_aliases_use_localized_labels_and_keep_e_and_a_parsing() {
        let original = crate::MultitoolCli::command().term_width(200);
        for locale in [Locale::English, Locale::Korean] {
            let mut localized = localize_help(original.clone(), locale);
            for long in [false, true] {
                let help = if long {
                    localized.render_long_help()
                } else {
                    localized.render_help()
                }
                .to_string();
                for alias in ["e", "a"] {
                    if locale == Locale::Korean {
                        assert!(help.contains(&format!("별칭: {alias}")), "{help}");
                        assert!(!help.contains("[aliases:"), "{help}");
                    } else {
                        assert!(help.contains(&format!("[aliases: {alias}]")), "{help}");
                        assert!(!help.contains("별칭:"), "{help}");
                    }
                }
                for (name, alias) in [("exec", "e"), ("apply", "a")] {
                    let command = localized.find_subcommand_mut(name).unwrap();
                    let help = if long {
                        command.render_long_help()
                    } else {
                        command.render_help()
                    }
                    .to_string();
                    assert!(
                        !help.contains(if locale == Locale::Korean {
                            "[aliases:"
                        } else {
                            "별칭:"
                        }),
                        "{help}"
                    );
                    if locale == Locale::Korean {
                        assert!(help.contains(&format!("별칭: {alias}")), "{help}");
                    }
                }
            }
            let prompt = "원문 Options: { $value } --help 🦀";
            let parsed = localized
                .clone()
                .try_get_matches_from(["codex", "e", prompt])
                .unwrap();
            assert_eq!(parsed.subcommand_name(), Some("exec"));
            assert_eq!(
                parsed
                    .subcommand_matches("exec")
                    .unwrap()
                    .get_one::<String>("prompt")
                    .map(String::as_str),
                Some(prompt)
            );
            let task_id = "r35-task-한글";
            let parsed = localized
                .try_get_matches_from(["codex", "a", task_id])
                .unwrap();
            assert_eq!(parsed.subcommand_name(), Some("apply"));
            assert_eq!(
                parsed
                    .subcommand_matches("apply")
                    .unwrap()
                    .get_one::<String>("task_id")
                    .map(String::as_str),
                Some(task_id)
            );
        }
    }

    #[test]
    fn synthetic_visible_and_hidden_aliases_preserve_all_tokens_and_typed_values() {
        let mut original = Command::new("alias-fixture")
            .term_width(200)
            .arg(
                Arg::new("number")
                    .long("number")
                    .short('n')
                    .visible_alias("visible-number")
                    .alias("hidden-number")
                    .visible_short_alias('x')
                    .short_alias('y')
                    .value_parser(clap::value_parser!(u16)),
            )
            .subcommand(
                Command::new("run")
                    .about("Run Codex non-interactively")
                    .long_about("Run a code review non-interactively")
                    .visible_alias("visible-run")
                    .alias("hidden-run")
                    .short_flag('r')
                    .long_flag("run-command")
                    .visible_short_flag_alias('s')
                    .short_flag_alias('t')
                    .visible_long_flag_alias("visible-run-flag")
                    .long_flag_alias("hidden-run-flag"),
            );
        original.build();
        for locale in [Locale::English, Locale::Korean] {
            let mut localized = localize_help(original.clone(), locale);
            let before = original
                .get_arguments()
                .find(|arg| arg.get_id() == "number")
                .unwrap();
            let after = localized
                .get_arguments()
                .find(|arg| arg.get_id() == "number")
                .unwrap();
            assert_eq!(before.get_all_aliases(), after.get_all_aliases());
            assert_eq!(
                before.get_all_short_aliases(),
                after.get_all_short_aliases()
            );
            assert_eq!(
                before.get_value_parser().type_id(),
                after.get_value_parser().type_id()
            );
            let before = original.find_subcommand("run").unwrap();
            let after = localized.find_subcommand("run").unwrap();
            assert_eq!(
                before.get_all_aliases().collect::<Vec<_>>(),
                after.get_all_aliases().collect::<Vec<_>>()
            );
            assert_eq!(
                before.get_all_short_flag_aliases().collect::<Vec<_>>(),
                after.get_all_short_flag_aliases().collect::<Vec<_>>()
            );
            assert_eq!(
                before.get_all_long_flag_aliases().collect::<Vec<_>>(),
                after.get_all_long_flag_aliases().collect::<Vec<_>>()
            );
            for long in [false, true] {
                let help = if long {
                    localized.render_long_help()
                } else {
                    localized.render_help()
                }
                .to_string();
                assert!(help.contains("--visible-number"), "{help}");
                assert!(help.contains("-x"), "{help}");
                assert!(help.contains("visible-run"), "{help}");
                assert!(help.contains("--visible-run-flag"), "{help}");
                assert!(help.contains("-s"), "{help}");
                assert!(!help.contains("hidden-number"), "{help}");
                assert!(!help.contains("hidden-run"), "{help}");
                assert!(
                    !help.contains(if locale == Locale::Korean {
                        "[aliases:"
                    } else {
                        "별칭:"
                    }),
                    "{help}"
                );
                assert!(
                    help.contains(if locale == Locale::Korean {
                        "별칭:"
                    } else {
                        "[aliases:"
                    }),
                    "{help}"
                );
                let child = localized.find_subcommand_mut("run").unwrap();
                let help = if long {
                    child.render_long_help()
                } else {
                    child.render_help()
                }
                .to_string();
                if locale == Locale::Korean {
                    assert!(
                        help.contains("별칭: -s, --visible-run-flag, visible-run"),
                        "{help}"
                    );
                }
                assert!(
                    !help.contains(if locale == Locale::Korean {
                        "[aliases:"
                    } else {
                        "별칭:"
                    }),
                    "{help}"
                );
            }
            for alias in [
                "--number",
                "--visible-number",
                "--hidden-number",
                "-n",
                "-x",
                "-y",
            ] {
                let before = original
                    .clone()
                    .try_get_matches_from(["alias-fixture", alias, "65535"])
                    .unwrap();
                let after = localized
                    .clone()
                    .try_get_matches_from(["alias-fixture", alias, "65535"])
                    .unwrap();
                assert_eq!(
                    before.get_one::<u16>("number"),
                    after.get_one::<u16>("number")
                );
                assert_eq!(after.get_one::<u16>("number"), Some(&65535));
            }
            for alias in [
                "run",
                "visible-run",
                "hidden-run",
                "--run-command",
                "--visible-run-flag",
                "--hidden-run-flag",
                "-r",
                "-s",
                "-t",
            ] {
                let before = original
                    .clone()
                    .try_get_matches_from(["alias-fixture", alias])
                    .unwrap();
                let after = localized
                    .clone()
                    .try_get_matches_from(["alias-fixture", alias])
                    .unwrap();
                assert_eq!(before.subcommand_name(), after.subcommand_name());
                assert_eq!(after.subcommand_name(), Some("run"));
            }
        }
    }

    #[test]
    fn structured_errors_translate_clap_scaffolding_and_keep_opaque_values() {
        let value = "Options: error: { $value } 한글 🦀";
        let original = Command::new("metadata")
            .arg(
                Arg::new("choice")
                    .long("choice")
                    .value_parser(["one", "two"]),
            )
            .arg(
                Arg::new("number")
                    .long("number")
                    .value_parser(clap::value_parser!(u16)),
            )
            .arg(
                Arg::new("first")
                    .long("first")
                    .action(clap::ArgAction::SetTrue)
                    .conflicts_with("second"),
            )
            .arg(
                Arg::new("second")
                    .long("second")
                    .action(clap::ArgAction::SetTrue),
            );
        for (args, kind, expected) in [
            (
                vec!["metadata", "--choice", value],
                ErrorKind::InvalidValue,
                "올바르지 않습니다",
            ),
            (
                vec!["metadata", "--choice"],
                ErrorKind::InvalidValue,
                "값이 필요합니다",
            ),
            (
                vec!["metadata", "--number", value],
                ErrorKind::ValueValidation,
                "올바르지 않습니다",
            ),
            (
                vec!["metadata", "--first", "--second"],
                ErrorKind::ArgumentConflict,
                "함께 사용할 수 없습니다",
            ),
            (
                vec!["metadata", "--unknown"],
                ErrorKind::UnknownArgument,
                "예상하지 않은 인수",
            ),
        ] {
            let error = localize_help(original.clone(), Locale::Korean)
                .try_get_matches_from(args.clone())
                .unwrap_err();
            assert_eq!(error.kind(), kind);
            let localized = error.apply::<KoreanErrorFormatter>();
            assert_eq!(localized.kind(), kind);
            assert_eq!(localized.exit_code(), 2);
            assert!(localized.use_stderr());
            let output = localized.to_string();
            assert!(output.starts_with("오류:"), "{output}");
            assert!(output.contains(expected), "{output}");
            assert!(output.contains("'--help'"), "{output}");
            if args.contains(&value) {
                assert!(output.contains(value), "{output}");
            }
        }
        let custom: clap::Error = clap::Error::raw(ErrorKind::ValueValidation, value);
        let custom = custom.apply::<KoreanErrorFormatter>();
        // Clap's public formatter deliberately does not reinterpret preformatted
        // custom errors. Never replace English substrings inside opaque values.
        assert!(custom.to_string().contains(value));
    }

    #[test]
    fn structured_required_subcommand_and_value_count_errors_keep_their_kinds() {
        let cases = [
            (
                Command::new("fixture").arg(Arg::new("item").long("item").require_equals(true)),
                vec!["fixture", "--item", "one"],
                ErrorKind::NoEquals,
                "등호가 필요합니다",
            ),
            (
                Command::new("fixture").arg(Arg::new("item").long("item").required(true)),
                vec!["fixture"],
                ErrorKind::MissingRequiredArgument,
                "필수 인수가 없습니다",
            ),
            (
                Command::new("fixture")
                    .subcommand_required(true)
                    .subcommand(Command::new("known")),
                vec!["fixture"],
                ErrorKind::MissingSubcommand,
                "하위 명령이 필요합니다",
            ),
            (
                Command::new("fixture").subcommand(Command::new("known")),
                vec!["fixture", "unknown"],
                ErrorKind::InvalidSubcommand,
                "인식할 수 없는 하위 명령",
            ),
            (
                Command::new("fixture").arg(Arg::new("item").long("item").num_args(2..=3)),
                vec!["fixture", "--item", "one"],
                ErrorKind::TooFewValues,
                "최소 2개의 값이 필요",
            ),
            (
                Command::new("fixture").arg(Arg::new("item").long("item").num_args(2)),
                vec!["fixture", "--item", "one"],
                ErrorKind::WrongNumberOfValues,
                "2개의 값이 필요",
            ),
            (
                Command::new("fixture").arg(
                    Arg::new("item")
                        .long("item")
                        .action(clap::ArgAction::SetTrue),
                ),
                vec!["fixture", "--item=one"],
                ErrorKind::TooManyValues,
                "예상하지 않은 추가 값",
            ),
            (
                Command::new("fixture").arg(Arg::new("item").long("item")),
                vec!["fixture", "--item", "one", "--item", "two"],
                ErrorKind::ArgumentConflict,
                "여러 번 사용할 수 없습니다",
            ),
        ];
        for (command, args, kind, expected) in cases {
            let before = command
                .clone()
                .try_get_matches_from(args.clone())
                .unwrap_err();
            let after = localize_help(command, Locale::Korean)
                .try_get_matches_from(args)
                .unwrap_err()
                .apply::<KoreanErrorFormatter>();
            assert_eq!(before.kind(), kind);
            assert_eq!(after.kind(), before.kind());
            assert_eq!(after.exit_code(), before.exit_code());
            assert!(after.to_string().contains(expected), "{after}");
        }
        // Clap checks raw argument counts before splitting delimiters. Preserve
        // that accepted input instead of treating three split values as an error.
        let delimited = Command::new("fixture").arg(
            Arg::new("item")
                .long("item")
                .num_args(1..=2)
                .value_delimiter(','),
        );
        for command in [delimited.clone(), localize_help(delimited, Locale::Korean)] {
            let parsed = command
                .try_get_matches_from(["fixture", "--item", "one,two,three"])
                .expect("delimiter splitting preserves Clap's accepted values");
            assert_eq!(
                parsed
                    .get_many::<String>("item")
                    .expect("string values")
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["one", "two", "three"]
            );
        }
        let invalid_utf8: clap::Error = clap::Error::new(ErrorKind::InvalidUtf8);
        let invalid_utf8 = invalid_utf8.apply::<KoreanErrorFormatter>();
        assert!(invalid_utf8.to_string().contains("올바르지 않은 UTF-8"));
    }

    #[test]
    fn desktop_help_contract_fixtures_keep_real_option_rows_in_both_languages() {
        for (locale, usage, options, commands) in [
            (Locale::English, "Usage:", "Options:", "Commands:"),
            (Locale::Korean, "사용법:", "옵션:", "명령어:"),
        ] {
            let mut cli = localize_help(crate::MultitoolCli::command(), locale);
            let server = command_at(&mut cli, &["app-server"]);
            let help = server.render_help().to_string();
            assert!(help.contains(usage), "{help}");
            assert!(help.contains(commands), "{help}");
            assert!(
                help.lines()
                    .any(|line| line.trim_start().starts_with("daemon ")),
                "{help}"
            );
            assert!(
                help.lines()
                    .any(|line| line.trim_start().starts_with("proxy ")),
                "{help}"
            );
            let option_rows = help
                .split_once(options)
                .expect("localized options section")
                .1;
            for token in ["--no-remote-control", "--help"] {
                assert!(
                    option_rows
                        .lines()
                        .any(|line| line.trim_start().starts_with(token)
                            || (token == "--help" && line.trim_start().starts_with("-h, --help"))),
                    "missing {token}: {help}"
                );
            }
            let start = command_at(&mut cli, &["app-server", "daemon", "start"]);
            let help = start.render_help().to_string();
            let option_rows = help
                .split_once(options)
                .expect("localized options section")
                .1;
            assert!(
                option_rows
                    .lines()
                    .any(|line| line.trim_start().starts_with("--existing-installation")),
                "{help}"
            );
        }
    }

    #[test]
    fn synthetic_desktop_option_fixture_preserves_tokens_and_user_values() {
        // This is a parser-preservation fixture, not a claim that every option
        // is supported by the current product's app-server command.
        let original = Command::new("synthetic-desktop")
            .arg(Arg::new("remote_control_client_name").long("remote-control-client-name"))
            .arg(
                Arg::new("existing_installation")
                    .long("existing-installation")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                Arg::new("no_remote_control")
                    .long("no-remote-control")
                    .action(clap::ArgAction::SetTrue),
            );
        let value = "client --help Options: error: { $value } 한글 🦀";
        for locale in [Locale::English, Locale::Korean] {
            let parsed = localize_help(original.clone(), locale)
                .try_get_matches_from([
                    "synthetic-desktop",
                    "--existing-installation",
                    "--no-remote-control",
                    "--remote-control-client-name",
                    value,
                ])
                .unwrap();
            assert!(parsed.get_flag("existing_installation"));
            assert!(parsed.get_flag("no_remote_control"));
            assert_eq!(
                parsed
                    .get_one::<String>("remote_control_client_name")
                    .map(String::as_str),
                Some(value)
            );
        }
    }

    #[test]
    fn korean_help_keeps_cli_argument_and_subcommand_tokens() {
        let mut command = localize_help(crate::MultitoolCli::command(), Locale::Korean);
        let help = command.render_help().to_string();
        assert!(help.contains("사용법:"));
        assert!(help.contains("명령어:"));
        assert!(help.contains("비대화형으로 Codex 실행"));
        assert!(help.contains("--no-alt-screen"));
        assert!(help.contains("exec"));
        let parsed = command
            .try_get_matches_from(["codex", "exec", "hello 원문"])
            .expect("localized command parses the original syntax");
        let exec = parsed
            .subcommand_matches("exec")
            .expect("exec token unchanged");
        assert_eq!(
            exec.get_one::<String>("prompt").map(String::as_str),
            Some("hello 원문")
        );
    }

    fn command_at<'a>(mut command: &'a mut Command, path: &[&str]) -> &'a mut Command {
        for name in path {
            command = command
                .find_subcommand_mut(name)
                .expect("existing command token");
        }
        command
    }

    #[test]
    fn r28_korean_help_covers_remote_startup_status_and_cloud_environments() {
        let mut command = localize_help(crate::MultitoolCli::command(), Locale::Korean);
        let server_help = command_at(&mut command, &["app-server"])
            .render_help()
            .to_string();
        assert!(server_help.contains("--no-remote-control"));
        assert!(
            server_help.contains("저장된 등록과 설정을 유지하며"),
            "{server_help}"
        );
        let remote_help = command_at(&mut command, &["remote-control"])
            .render_help()
            .to_string();
        assert!(remote_help.contains("status"));
        assert!(
            remote_help
                .split_whitespace()
                .collect::<String>()
                .contains("relay상태를조회합니다"),
            "{remote_help}"
        );
        let cloud_help = command_at(&mut command, &["cloud"])
            .render_help()
            .to_string();
        assert!(cloud_help.contains("environments"));
        assert!(
            cloud_help.contains("접근 가능한 Codex Cloud 환경 목록 표시"),
            "{cloud_help}"
        );
        let environments_help = command_at(&mut command, &["cloud", "environments"])
            .render_help()
            .to_string();
        assert!(environments_help.contains("--json"));
        assert!(
            environments_help.contains("일반 텍스트 대신 JSON 출력"),
            "{environments_help}"
        );

        let parsed = command
            .clone()
            .try_get_matches_from(["codex", "app-server", "--no-remote-control"])
            .expect("localized startup flag retains its parser");
        assert!(
            parsed
                .subcommand_matches("app-server")
                .expect("app-server")
                .get_flag("no_remote_control")
        );
        let parsed = command
            .clone()
            .try_get_matches_from(["codex", "remote-control", "status", "--json"])
            .expect("localized status command retains its parser");
        let remote = parsed
            .subcommand_matches("remote-control")
            .expect("remote-control");
        assert!(remote.get_flag("json"));
        assert!(remote.subcommand_matches("status").is_some());
        let parsed = command
            .try_get_matches_from(["codex", "cloud", "environments", "--json"])
            .expect("localized environments command retains its parser");
        assert!(
            parsed
                .subcommand_matches("cloud")
                .expect("cloud")
                .subcommand_matches("environments")
                .expect("environments")
                .get_flag("json")
        );
    }

    #[test]
    fn r28_english_help_preserves_new_command_and_flag_descriptions() {
        let mut original = crate::MultitoolCli::command();
        original.build();
        let mut english = localize_help(crate::MultitoolCli::command(), Locale::English);
        for path in [
            vec!["app-server"],
            vec!["remote-control"],
            vec!["remote-control", "status"],
            vec!["cloud"],
            vec!["cloud", "environments"],
        ] {
            assert_eq!(
                command_at(&mut original, &path).render_help().to_string(),
                command_at(&mut english, &path).render_help().to_string()
            );
        }
        let help = command_at(&mut english, &["app-server"])
            .render_help()
            .to_string();
        assert!(help.contains("Start with remote control disabled"));
        assert!(help.contains("--no-remote-control"));
    }

    #[test]
    fn localized_arguments_preserve_optional_help_and_custom_headings() {
        let mut original = Command::new("metadata")
            .arg(Arg::new("absent").long("absent"))
            .arg(Arg::new("short").long("short").help("Print help"))
            .arg(Arg::new("long").long("long").long_help("Print version"))
            .arg(
                Arg::new("both")
                    .long("both")
                    .help("custom short")
                    .long_help("custom long")
                    .help_heading("Original section"),
            );
        original.build();
        let localized = localize_help(original.clone(), Locale::Korean);
        assert_eq!(
            original.get_arguments().count(),
            localized.get_arguments().count()
        );
        for (before, after) in original.get_arguments().zip(localized.get_arguments()) {
            assert_eq!(before.get_id(), after.get_id());
            assert_eq!(before.get_long(), after.get_long());
            assert_eq!(before.get_help().is_some(), after.get_help().is_some());
            assert_eq!(
                before.get_long_help().is_some(),
                after.get_long_help().is_some()
            );
            if before.get_help_heading().is_some() {
                assert_eq!(before.get_help_heading(), after.get_help_heading());
            } else {
                assert_eq!(after.get_help_heading(), Some("옵션"));
            }
            match after.get_id().as_str() {
                "short" => assert_eq!(
                    after.get_help().expect("short help").to_string(),
                    "도움말 표시"
                ),
                "long" => assert_eq!(
                    after.get_long_help().expect("long help").to_string(),
                    "버전 표시"
                ),
                "both" => {
                    assert_eq!(
                        after.get_help().expect("short help").to_string(),
                        "custom short"
                    );
                    assert_eq!(
                        after.get_long_help().expect("long help").to_string(),
                        "custom long"
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn r36_cleanup_localized_long_help_explains_default_unloaded_preservation() {
        for (locale, expected) in [
            (
                Locale::English,
                "Operational cleanup support for these items is incomplete.",
            ),
            (Locale::Korean, "해당 항목의 운영 정리 지원은 미완입니다."),
        ] {
            let error = localize_help(crate::MultitoolCli::command(), locale)
                .try_get_matches_from(["codex", "cleanup-agents", "--help"])
                .expect_err("the actual help argument prints localized long help");
            assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
            let help = error.to_string();
            let normalized = help.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(normalized.contains(expected), "{help}");
            assert!(help.contains("reference_participants_unverified"), "{help}");
            assert!(help.contains("--apply"));
            assert!(help.contains("--thread"));
            if locale == Locale::English {
                assert!(!help.chars().any(|c| ('가'..='힣').contains(&c)), "{help}");
            }
        }
    }

    #[test]
    fn english_help_and_unknown_descriptions_are_preserved() {
        let mut english = localize_help(crate::MultitoolCli::command(), Locale::English);
        let help = english.render_help().to_string();
        assert!(help.contains("Usage:"));
        assert!(help.contains("Enable Desktop-owned scheduled controls"));
        assert!(help.contains("Preview completed subagents under the current parent"));
        assert!(!help.chars().any(|c| ('가'..='힣').contains(&c)), "{help}");
        let mut unknown = localize_help(
            Command::new("example")
                .about("custom original")
                .arg(Arg::new("custom").long("custom").help("unknown original")),
            Locale::Korean,
        );
        let help = unknown.render_help().to_string();
        assert!(help.contains("custom original"));
        assert!(help.contains("unknown original"));
        assert!(help.contains("--custom"));
    }
}
