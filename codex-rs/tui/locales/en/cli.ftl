# Application-owned CLI help and parse-error presentation. Command tokens and values remain unchanged.

cli-help-app-cmd-workspace-path-to-open-in-the-desktop-app = Workspace path to open in the Desktop app

cli-help-app-cmd-override-the-app-installer-download-url-advanced = Override the app installer download URL (advanced)

cli-cleanup-current = Current conversation UUID. Uses CODEX_THREAD_ID when omitted

cli-cleanup-parent = Parent UUID defining the cleanup scope. Defaults to the current conversation

cli-cleanup-threads = Exact UUID to clean up. Repeatable; defaults to all completed candidates in the preview

cli-cleanup-apply = Apply cleanup after preview and confirmation. Defaults to read-only

cli-cleanup-confirm = Explicitly approve deleting the selected preview items

cli-cleanup-about = Preview and clean up completed local second-generation subagent conversations under the current parent.

cli-cleanup-long-about = Read-only preview is the default. --apply requests explicit confirmation after the preview. Running or unknown-state items, other generations, remote-owned items, and legacy items are preserved. Repeated --thread flags clean up only the exact selection and preserve unselected descendants.

cli-help-doctor-options-for-building-a-local-codex-diagnostic-report = Options for building a local Codex diagnostic report

cli-help-doctor-options-for-building-a-local-codex-diagnostic-report-details = Options for building a local Codex diagnostic report.

    The command runs the full diagnostic set unless `--network-only` is selected. Human output includes detailed diagnostics by default; --summary keeps the terminal output compact.

cli-help-doctor-internal-isolated-filesystem-probe-exits-before-loading-configuration = Internal isolated filesystem probe; exits before loading configuration

cli-help-doctor-emit-a-redacted-machine-readable-report = Emit a redacted machine-readable report

cli-help-doctor-run-config-network-environment-provider-http-and-responses = Run config, network environment, provider HTTP, and Responses WebSocket checks

cli-help-doctor-limit-database-integrity-scans-when-collecting-a-feedback = Limit database integrity scans when collecting a feedback attachment

cli-help-doctor-only-show-grouped-check-rows-and-the-final = Only show grouped check rows and the final count summary

cli-help-doctor-expand-long-lists-in-detailed-human-output = Expand long lists in detailed human output

cli-help-doctor-disable-ansi-color-in-human-output = Disable ANSI color in human output

cli-help-doctor-use-ascii-status-labels-and-separators-in-human = Use ASCII status labels and separators in human output

cli-help-doctor-machine-readable-doctor-output-shared-by-human-and = Machine-readable doctor output shared by human and JSON renderers

cli-help-doctor-machine-readable-doctor-output-shared-by-human-and-details = Machine-readable doctor output shared by human and JSON renderers.

    The schema is intentionally flat: each check carries its own category, status, details, remediation, and duration so support tooling can filter or redact individual rows without understanding the renderer's section layout.

cli-help-doctor-one-diagnostic-result-in-the-doctor-report = One diagnostic result in the doctor report

cli-help-doctor-one-diagnostic-result-in-the-doctor-report-summaries = One diagnostic result in the doctor report.

    Summaries are safe for compact human output. Details may include local paths or command output and are redacted before rendering or JSON serialization.

cli-help-doctor-structured-cause-remedy-metadata-for-a-non-ok = Structured cause/remedy metadata for a non-ok doctor check

cli-help-doctor-structured-cause-remedy-metadata-for-a-non-ok-details = Structured cause/remedy metadata for a non-ok doctor check.

    Human output uses issues to make warnings and failures self-explanatory: the row headline says what is wrong, matching detail rows show measured vs. expected values, and remedies are printed as explicit next actions.

cli-help-doctor-json-support-report-emitted-by-codex-doctor-json = JSON support report emitted by `codex doctor --json`

cli-help-doctor-json-support-report-emitted-by-codex-doctor-json-details = JSON support report emitted by `codex doctor --json`.

    The report is keyed by check id so support tooling can fetch paths like `checks["terminal.metadata"]` without scanning arrays. Human rendering can reorder or group rows independently, but this JSON shape should stay stable across cosmetic output changes.

cli-help-doctor-one-redacted-check-in-the-json-support-report = One redacted check in the JSON support report

cli-help-doctor-one-redacted-issue-in-the-json-support-report = One redacted issue in the JSON support report

cli-help-doctor-json-detail-value-that-preserves-repeated-detail-keys = JSON detail value that preserves repeated detail keys without inventing names

cli-help-lib-opaque-json-value-from-codex-sandbox-state-to = Opaque JSON value from `codex/sandbox-state` to apply directly

cli-help-lib-add-a-readable-root-to-the-supplied-sandbox = Add a readable root to the supplied sandbox state. Repeat for multiple roots

cli-help-lib-disable-direct-network-access-in-the-supplied-sandbox = Disable direct network access in the supplied sandbox state

cli-help-lib-named-permissions-profile-to-apply-from-the-active = Named permissions profile to apply from the active configuration stack

cli-help-lib-layer-codex-home-name-config-toml-on-top = Layer $CODEX_HOME/<name>.config.toml on top of the base user config

cli-help-lib-working-directory-used-for-profile-resolution-and-command = Working directory used for profile resolution and command execution

cli-help-lib-include-managed-requirements-while-resolving-an-explicit-permissions = Include managed requirements while resolving an explicit permissions profile

cli-help-lib-allow-the-sandboxed-command-to-bind-connect-af = Allow the sandboxed command to bind/connect AF_UNIX sockets rooted at this path. Relative paths are resolved against the current directory. Repeat to allow multiple paths

cli-help-lib-while-the-command-runs-capture-macos-sandbox-denials = While the command runs, capture macOS sandbox denials via `log stream` and print them after exit

cli-help-lib-full-command-args-to-run-under-seatbelt = Full command args to run under seatbelt

cli-help-lib-full-command-args-to-run-under-the-linux = Full command args to run under the Linux sandbox

cli-help-lib-full-command-args-to-run-under-windows-restricted = Full command args to run under Windows restricted token sandbox

cli-help-main-codex-cli = Codex CLI

cli-help-main-codex-cli-if-no-subcommand-is-specified-options = Codex CLI

    If no subcommand is specified, options will be forwarded to the interactive CLI.

cli-help-main-browse-all-agent-sessions-on-the-shared-local = Browse all agent sessions on the shared local app-server daemon

cli-help-main-internal-forward-a-local-tcp-socket-through-an = Internal: forward a local TCP socket through an HTTP/3 CONNECT proxy

cli-help-main-run-codex-non-interactively = Run Codex non-interactively

cli-help-main-run-a-code-review-non-interactively = Run a code review non-interactively

cli-help-main-manage-login = Manage login

cli-help-main-remove-stored-authentication-credentials = Remove stored authentication credentials

cli-help-main-manage-external-mcp-servers-for-codex = Manage external MCP servers for Codex

cli-help-main-manage-codex-plugins = Manage Codex plugins

cli-help-main-experimental-run-the-app-server-or-related-tooling = [experimental] Run the app server or related tooling

cli-help-main-experimental-manage-the-app-server-daemon-with-remote = [experimental] Manage the app-server daemon with remote control enabled

cli-help-main-launch-the-desktop-app-opens-the-app-installer = Launch the Desktop app (opens the app installer if missing)

cli-help-main-generate-shell-completion-scripts = Generate shell completion scripts

cli-help-main-update-codex-to-the-latest-version = Update Codex to the latest version

cli-help-main-diagnose-local-codex-installation-config-auth-and-runtime = Diagnose local Codex installation, config, auth, and runtime health

cli-help-main-run-commands-within-a-codex-provided-sandbox = Run commands within a Codex-provided sandbox

cli-help-main-debugging-tools = Debugging tools

cli-help-main-execpolicy-tooling = Execpolicy tooling

cli-help-main-apply-the-latest-diff-produced-by-codex-agent = Apply the latest diff produced by Codex agent as a `git apply` to your local working tree

cli-help-main-resume-a-previous-interactive-session-picker-by-default = Resume a previous interactive session (picker by default; use --last to continue the most recent)

cli-help-main-queue-a-message-for-an-existing-session = Queue a message for an existing session

cli-cleanup-command = Preview completed subagents under the current parent, or clean them up after confirmation

cli-help-main-archive-a-saved-session-by-id-or-session = Archive a saved session by id or session name

cli-help-main-permanently-delete-a-saved-session-by-id-or = Permanently delete a saved session by id or session name

cli-help-main-inspect-or-migrate-legacy-local-sessions-to-paginated = Inspect or migrate legacy local sessions to paginated thread history

cli-help-main-experimental-migrate-configured-sqlite-databases-to-delete-mode = [experimental] Migrate configured SQLite databases to DELETE mode in place

cli-help-main-unarchive-a-saved-session-by-id-or-session = Unarchive a saved session by id or session name

cli-help-main-fork-a-previous-interactive-session-picker-by-default = Fork a previous interactive session (picker by default; use --last to fork the most recent)

cli-help-main-experimental-browse-tasks-from-codex-cloud-and-apply = [EXPERIMENTAL] Browse tasks from Codex Cloud and apply changes locally

cli-help-main-internal-run-the-responses-api-proxy = Internal: run the responses API proxy

cli-help-main-internal-relay-stdio-to-a-unix-domain-socket = Internal: relay stdio to a Unix domain socket

cli-help-main-experimental-run-the-standalone-exec-server-service = [EXPERIMENTAL] Run the standalone exec-server service

cli-help-main-inspect-feature-flags = Inspect feature flags

cli-help-main-shell-to-generate-completions-for = Shell to generate completions for

cli-help-main-render-the-raw-model-catalog-as-json = Render the raw model catalog as JSON

cli-help-main-tooling-helps-debug-the-app-server = Tooling: helps debug the app server

cli-help-main-render-the-model-visible-prompt-input-list-as = Render the model-visible prompt input list as JSON

cli-help-main-replay-a-rollout-trace-bundle-and-write-reduced = Replay a rollout trace bundle and write reduced state JSON

cli-help-main-internal-reset-local-memory-state-for-a-fresh = Internal: reset local memory state for a fresh start

cli-help-main-optional-user-prompt-to-append-after-session-context = Optional user prompt to append after session context

cli-help-main-optional-image-s-to-attach-to-the-user = Optional image(s) to attach to the user prompt

cli-help-main-skip-refresh-and-dump-only-the-bundled-catalog = Skip refresh and dump only the bundled catalog shipped with this binary

cli-help-main-error-out-when-config-toml-contains-fields-that = Error out when config.toml contains fields that are not recognized by this version of Codex

cli-help-main-trace-bundle-directory-containing-manifest-json-and-trace = Trace bundle directory containing manifest.json and trace.jsonl

cli-help-main-output-path-for-reduced-rollouttrace-json-defaults-to = Output path for reduced RolloutTrace JSON. Defaults to TRACE_BUNDLE/state.json

cli-help-main-the-agents-overview-requires-a-shared-server-this = The agents overview requires a shared server; this option is rejected

cli-help-main-use-this-directory-for-new-tasks-on-a = Use this directory for new tasks on a remote server

cli-help-main-disable-alternate-screen-mode = Disable alternate screen mode

cli-help-main-session-id-uuid-or-session-name-uuids-take = Session id (UUID) or session name. UUIDs take precedence if it parses. If omitted, use --last to pick the most recent recorded session

cli-help-main-continue-the-most-recent-session-without-showing-the = Continue the most recent session without showing the picker

cli-help-main-show-all-sessions-disables-cwd-filtering-and-shows = Show all sessions (disables cwd filtering and shows CWD column)

cli-help-main-include-non-interactive-sessions-in-the-resume-picker = Include non-interactive sessions in the resume picker and --last selection

cli-help-main-session-id-uuid-or-session-name-uuids-take-details = Session id (UUID) or session name. UUIDs take precedence if it parses

cli-help-main-delete-without-prompting-session-must-be-a-uuid = Delete without prompting. SESSION must be a UUID

cli-help-main-conversation-session-id-uuid-when-provided-forks-this = Conversation/session id (UUID). When provided, forks this session. If omitted, use --last to pick the most recent recorded session

cli-help-main-fork-the-most-recent-session-without-showing-the = Fork the most recent session without showing the picker

cli-help-main-tui-arguments-for-session-commands-where-a-parsed = TUI arguments for session commands where a parsed prompt implies an explicit session id

cli-help-main-tui-arguments-for-session-commands-where-a-parsed-details = TUI arguments for session commands where a parsed prompt implies an explicit session id.

    This keeps `--last PROMPT` valid while rejecting `--last SESSION_ID PROMPT`.

cli-help-main-full-command-args-to-run-under-the-host = Full command args to run under the host sandbox

cli-help-main-check-execpolicy-files-against-a-command = Check execpolicy files against a command

cli-help-main-experimental-use-custom-oauth-issuer-base-url-advanced = EXPERIMENTAL: Use custom OAuth issuer base URL (advanced) Override the OAuth issuer base URL (advanced)

cli-help-main-experimental-use-custom-oauth-client-id-advanced = EXPERIMENTAL: Use custom OAuth client ID (advanced)

cli-help-main-show-login-status = Show login status

cli-help-main-load-frozen-cloud-skills-from-a-json-snapshot = Load frozen cloud skills from a JSON snapshot instead of a live skill service

cli-help-main-omit-to-run-the-app-server-specify-a = Omit to run the app server; specify a subcommand for tooling

cli-help-main-transport-endpoint-url-supported-values-stdio-default-unix = Transport endpoint URL. Supported values: `stdio://` (default), `unix://`, `unix://PATH`, `ws://IP:PORT`, `off`

cli-help-main-use-stdio-as-the-transport-equivalent-to-listen = Use stdio as the transport (equivalent to `--listen stdio://`)

cli-help-main-enable-remote-control-for-this-app-server-process = Enable remote control for this app-server process without changing persistence

cli-help-main-start-with-remote-control-disabled-preserving-saved-registrations = Start with remote control disabled, preserving saved registrations and settings

cli-help-main-save-loaded-threads-during-managed-daemon-shutdown = Save loaded threads during managed daemon shutdown

cli-help-main-controls-whether-analytics-are-enabled-by-default = Controls whether analytics are enabled by default

cli-help-main-controls-whether-analytics-are-enabled-by-default-analytics = Controls whether analytics are enabled by default.

    Analytics are disabled by default for app-server. Users have to explicitly opt in via the `analytics` section in the config.toml file.

    However, for first-party use cases like the VSCode IDE extension, we default analytics to be enabled by default by setting this flag. Users can still opt out by setting this in their config.toml:

    ```toml [analytics] enabled = false ```

    See https://developers.openai.com/codex/config-advanced/#metrics for more details.

cli-help-main-manage-the-local-app-server-daemon = Manage the local app-server daemon

cli-help-main-proxy-stdio-bytes-to-the-running-app-server = Proxy stdio bytes to the running app-server control socket

cli-help-main-experimental-generate-typescript-bindings-for-the-app-server = [experimental] Generate TypeScript bindings for the app server protocol

cli-help-main-experimental-generate-json-schema-for-the-app-server = [experimental] Generate JSON Schema for the app server protocol

cli-help-main-internal-generate-internal-json-schema-artifacts-for-codex = [internal] Generate internal JSON Schema artifacts for Codex tooling

cli-help-main-install-durable-local-app-server-management-for-ssh = Install durable local app-server management for SSH-driven use

cli-help-main-start-the-local-app-server-daemon-if-it = Start the local app server daemon if it is not already running

cli-help-main-use-only-the-selected-installation-preserve-settings-and = Use only the selected installation; preserve settings and updater state

cli-help-main-restart-the-local-app-server-daemon = Restart the local app server daemon

cli-help-main-update-the-daemon-package-may-interrupt-running-work = Update the daemon package (may interrupt running work)

cli-help-main-copy-and-pin-this-cli-package = Copy and pin this CLI package

cli-help-main-confirm-replacing-the-daemon-package-without-an-interactive = Confirm replacing the daemon package without an interactive prompt

cli-help-main-never-force-shutdown-on-an-unconfirmed-stop-keep = Never force shutdown. On an unconfirmed stop, keep the selected package and do not retry automatically

cli-help-main-enable-remote-control-for-future-starts-and-a = Enable remote control for future starts and a currently running managed daemon

cli-help-main-disable-remote-control-for-future-starts-and-a = Disable remote control for future starts and a currently running managed daemon

cli-help-main-stop-the-local-app-server-daemon = Stop the local app server daemon

cli-help-main-print-local-cli-and-running-app-server-versions = Print local CLI and running app-server versions as JSON

cli-help-main-observe-the-selected-installation-and-daemon-without-starting = Observe the selected installation and daemon without starting or repairing it

cli-help-main-internal-run-the-detached-pid-backed-standalone-updater = [internal] Run the detached pid-backed standalone updater loop

cli-help-main-check-support-for-daemon-owned-packages-without-starting = Check support for daemon-owned packages without starting the updater

cli-help-main-authorize-one-production-restoration-of-this-selected-release = Authorize one production restoration of this selected release

cli-help-main-path-to-the-app-server-unix-domain-socket = Path to the app-server Unix domain socket to connect to

cli-help-main-launch-the-managed-app-server-with-remote-control = Launch the managed app-server with remote control enabled

cli-help-main-output-directory-where-ts-files-will-be-written = Output directory where .ts files will be written

cli-help-main-optional-path-to-the-prettier-executable-to-format = Optional path to the Prettier executable to format generated files

cli-help-main-include-experimental-methods-and-fields-in-the-generated = Include experimental methods and fields in the generated output

cli-help-main-output-directory-where-the-schema-bundle-will-be = Output directory where the schema bundle will be written

cli-help-main-output-directory-where-internal-json-schema-artifacts-will = Output directory where internal JSON Schema artifacts will be written

cli-help-main-path-to-the-unix-domain-socket-to-connect = Path to the Unix domain socket to connect to

cli-help-main-check-for-an-opendiy-package-update-without-applying = Check for an OpenDIY package update without applying it

cli-help-main-show-the-selected-opendiy-package-and-automatic-update = Show the selected OpenDIY package and automatic update status

cli-help-main-enable-automatic-opendiy-package-updates-every-60-minutes = Enable automatic OpenDIY package updates (every 60 minutes by default)

cli-help-main-disable-automatic-opendiy-package-updates = Disable automatic OpenDIY package updates

cli-help-main-automatic-update-interval-in-minutes-opendiy-only-requires = Automatic update interval in minutes (OpenDIY only; requires --enable-updates)

cli-help-main-enable-a-feature-repeatable-equivalent-to-c-features = Enable a feature (repeatable). Equivalent to `-c features.<name>=true`

cli-help-main-disable-a-feature-repeatable-equivalent-to-c-features = Disable a feature (repeatable). Equivalent to `-c features.<name>=false`

cli-help-main-connect-the-tui-to-a-remote-app-server = Connect the TUI to a remote app server endpoint

cli-help-main-connect-the-tui-to-a-remote-app-server-details = Connect the TUI to a remote app server endpoint.

    Accepted forms: `ws://host:port`, `wss://host:port`, `unix://`, or `unix://PATH`.

cli-help-main-name-of-the-environment-variable-containing-the-bearer = Name of the environment variable containing the bearer token to send to a remote app server websocket

cli-help-main-list-known-features-with-their-stage-and-effective = List known features with their stage and effective state

cli-help-main-enable-a-feature-in-config-toml = Enable a feature in config.toml

cli-help-main-disable-a-feature-in-config-toml = Disable a feature in config.toml

cli-help-main-feature-key-to-update-for-example-unified-exec = Feature key to update (for example: unified_exec)

cli-help-main-read-the-api-key-from-stdin-e-g = Read the API key from stdin (e.g. `printenv OPENAI_API_KEY | codex login --with-api-key`)

cli-help-main-read-the-access-token-from-stdin-e-g = Read the access token from stdin (e.g. `printenv CODEX_ACCESS_TOKEN | codex login --with-access-token`)

cli-help-main-deprecated-previously-accepted-the-api-key-directly-now = (deprecated) Previously accepted the API key directly; now exits with guidance to use --with-api-key

cli-help-marketplace-cmd-add-a-local-or-git-marketplace-to-the = Add a local or Git marketplace to the configured marketplace sources

cli-help-marketplace-cmd-list-plugin-marketplaces-codex-is-currently-considering-and = List plugin marketplaces Codex is currently considering and their roots

cli-help-marketplace-cmd-refresh-configured-git-marketplace-snapshots = Refresh configured Git marketplace snapshots

cli-help-marketplace-cmd-refresh-configured-git-marketplace-snapshots-omit-marketplace-name = Refresh configured Git marketplace snapshots.

    Omit MARKETPLACE_NAME to upgrade all configured Git marketplaces.

cli-help-marketplace-cmd-remove-a-configured-marketplace-source-by-name = Remove a configured marketplace source by name

cli-help-marketplace-cmd-marketplace-source-a-local-path-owner-repo-ref = Marketplace source: a local path, owner/repo[@ref], HTTPS Git URL, or SSH Git URL

cli-help-marketplace-cmd-git-ref-to-fetch-for-git-marketplace-sources = Git ref to fetch for Git marketplace sources

cli-help-marketplace-cmd-sparse-checkout-path-for-git-marketplace-sources-can = Sparse checkout path for Git marketplace sources. Can be repeated

cli-help-marketplace-cmd-output-add-result-as-json = Output add result as JSON

cli-help-marketplace-cmd-output-marketplace-list-as-json = Output marketplace list as JSON

cli-help-marketplace-cmd-optional-configured-marketplace-name-to-upgrade-omit-to = Optional configured marketplace name to upgrade. Omit to upgrade all Git marketplaces

cli-help-marketplace-cmd-output-upgrade-result-as-json = Output upgrade result as JSON

cli-help-marketplace-cmd-configured-marketplace-name-to-remove = Configured marketplace name to remove

cli-help-marketplace-cmd-output-remove-result-as-json = Output remove result as JSON

cli-help-marketplace-cmd-examples-codex-plugin-marketplace-add-path-to-marketplace = Examples:
      codex plugin marketplace add ./path/to/marketplace
      codex plugin marketplace add owner/repo --ref main
      codex plugin marketplace add https://github.com/owner/repo --sparse plugins/foo

cli-help-marketplace-cmd-examples-codex-plugin-marketplace-upgrade-codex-plugin-marketplace = Examples:
      codex plugin marketplace upgrade
      codex plugin marketplace upgrade debug

cli-help-marketplace-cmd-example-codex-plugin-marketplace-remove-debug = Example:
      codex plugin marketplace remove debug

cli-help-mcp-cmd-subcommands-list-list-configured-servers-with-json-get = Subcommands: - `list` — list configured servers (with `--json`) - `get` — show a single server (with `--json`) - `add` — add a server launcher entry to `~/.codex/config.toml` - `remove` — delete a server entry - `login` — authenticate with MCP server using OAuth - `logout` — remove OAuth credentials for MCP server

cli-help-mcp-cmd-output-the-configured-servers-as-json = Output the configured servers as JSON

cli-help-mcp-cmd-name-of-the-mcp-server-to-display = Name of the MCP server to display

cli-help-mcp-cmd-output-the-server-configuration-as-json = Output the server configuration as JSON

cli-help-mcp-cmd-name-for-the-mcp-server-configuration = Name for the MCP server configuration

cli-help-mcp-cmd-command-to-launch-the-mcp-server-use-url = Command to launch the MCP server. Use --url for a streamable HTTP server

cli-help-mcp-cmd-environment-variables-to-set-when-launching-the-server = Environment variables to set when launching the server. Only valid with stdio servers

cli-help-mcp-cmd-url-for-a-streamable-http-mcp-server = URL for a streamable HTTP MCP server

cli-help-mcp-cmd-optional-environment-variable-to-read-for-a-bearer = Optional environment variable to read for a bearer token. Only valid with streamable HTTP servers

cli-help-mcp-cmd-optional-oauth-client-identifier-to-use-for-this = Optional OAuth client identifier to use for this MCP server

cli-help-mcp-cmd-optional-oauth-client-secret-for-the-pre-registered = Optional OAuth client secret for the pre-registered client

cli-help-mcp-cmd-oauth-client-registration-strategy-for-the-immediate-login = OAuth client-registration strategy for the immediate login only

cli-help-mcp-cmd-optional-oauth-resource-parameter-to-include-during-mcp = Optional OAuth resource parameter to include during MCP login

cli-help-mcp-cmd-name-of-the-mcp-server-configuration-to-remove = Name of the MCP server configuration to remove

cli-help-mcp-cmd-name-of-the-mcp-server-to-authenticate-with = Name of the MCP server to authenticate with oauth

cli-help-mcp-cmd-print-the-authorization-url-and-accept-the-callback = Print the authorization URL and accept the callback URL without opening a browser

cli-help-mcp-cmd-comma-separated-list-of-oauth-scopes-to-request = Comma-separated list of OAuth scopes to request

cli-help-mcp-cmd-oauth-client-registration-strategy-for-this-login-only = OAuth client-registration strategy for this login only

cli-help-mcp-cmd-name-of-the-mcp-server-to-deauthenticate = Name of the MCP server to deauthenticate

cli-help-mcp-login-retry-provider-rejected-discovered-scopes-once-retaining-the = Retry provider-rejected discovered scopes once, retaining the selected input mode

cli-help-migrate-nfs-db-use-a-local-directory-with-space-for-the = Use a local directory with space for the database and WAL files. The default is the system temporary directory

cli-help-migrate-nfs-db-stop-sqlite-clients-on-every-host-and-keep = Stop SQLite clients on every host and keep them stopped through cutover.
    The NFS mount must support cross-client POSIX locks (no nolock/local_lock=all).
    This command replaces configured databases in place and retains their WAL backups.

cli-help-migrate-rollouts-publish-the-migration-without-this-flag-the-command = Publish the migration. Without this flag the command only reports eligible sessions

cli-help-migrate-rollouts-restrict-inspection-or-migration-to-one-or-more = Restrict inspection or migration to one or more thread IDs

cli-help-migrate-rollouts-limit-aggregate-rollout-read-and-write-throughput-in = Limit aggregate rollout read and write throughput, in MiB per second

cli-help-migrate-rollouts-emit-the-complete-per-thread-report-as-json = Emit the complete per-thread report as JSON

cli-help-migrate-rollouts-print-one-line-for-every-inspected-rollout = Print one line for every inspected rollout

cli-help-opendiy-update-actions-supported-by-the-packaged-updater = Actions supported by the packaged updater

cli-help-plugin-cmd-install-a-plugin-from-a-configured-or-remote = Install a plugin from a configured or remote marketplace

cli-help-plugin-cmd-install-a-plugin-from-a-configured-or-remote-details = Install a plugin from a configured or remote marketplace.

    Pass either `PLUGIN@MARKETPLACE` or pass `PLUGIN` with `--marketplace MARKETPLACE`.

cli-help-plugin-cmd-list-plugins-available-from-configured-and-remote-marketplaces = List plugins available from configured and remote marketplaces

cli-help-plugin-cmd-add-list-upgrade-or-remove-configured-plugin-marketplaces = Add, list, upgrade, or remove configured plugin marketplaces

cli-help-plugin-cmd-uninstall-a-plugin-and-remove-its-local-cache = Uninstall a plugin and remove its local cache

cli-help-plugin-cmd-uninstall-a-plugin-and-remove-its-local-cache-details = Uninstall a plugin and remove its local cache.

    Pass either `PLUGIN@MARKETPLACE` or pass `PLUGIN` with `--marketplace MARKETPLACE`.

cli-help-plugin-cmd-plugin-selector-to-install-either-plugin-marketplace-or = Plugin selector to install: either PLUGIN@MARKETPLACE or PLUGIN with --marketplace

cli-help-plugin-cmd-marketplace-name-to-use-when-plugin-does-not = Marketplace name to use when PLUGIN does not include @MARKETPLACE

cli-help-plugin-cmd-output-install-result-as-json = Output install result as JSON

cli-help-plugin-cmd-only-list-plugins-from-this-marketplace-name = Only list plugins from this marketplace name

cli-help-plugin-cmd-output-plugin-list-as-json = Output plugin list as JSON

cli-help-plugin-cmd-include-uninstalled-marketplace-plugins-in-the-json-output = Include uninstalled marketplace plugins in the JSON output

cli-help-plugin-cmd-plugin-selector-to-remove-either-plugin-marketplace-or = Plugin selector to remove: either PLUGIN@MARKETPLACE or PLUGIN with --marketplace

cli-help-plugin-cmd-examples-codex-plugin-add-sample-debug-codex-plugin = Examples:
      codex plugin add sample@debug
      codex plugin add sample --marketplace debug

cli-help-plugin-cmd-examples-codex-plugin-list-codex-plugin-list-marketplace = Examples:
      codex plugin list
      codex plugin list --marketplace debug
      codex plugin list --json
      codex plugin list --available --json

cli-help-plugin-cmd-examples-codex-plugin-remove-sample-debug-codex-plugin = Examples:
      codex plugin remove sample@debug
      codex plugin remove sample --marketplace debug

cli-help-queue-cmd-session-uuid-or-exact-session-name = Session UUID or exact session name

cli-help-queue-cmd-message-text-to-queue = Message text to queue

cli-help-queue-cmd-after-queueing-try-to-resume-the-thread-on = After queueing, try to resume the thread on the local daemon or selected --remote server. A failed resume produces a warning and the command still succeeds

cli-help-remote-control-cmd-emit-machine-readable-json = Emit machine-readable JSON

cli-help-remote-control-cmd-run-an-embedded-app-server-in-the-foreground = Run an embedded app-server in the foreground until Ctrl-C

cli-help-remote-control-cmd-start-the-app-server-daemon-with-remote-control = Start the app-server daemon with remote control enabled

cli-help-remote-control-cmd-stop-the-app-server-daemon = Stop the app-server daemon

cli-help-remote-control-cmd-create-and-print-a-short-lived-manual-pairing = Create and print a short-lived manual pairing code

cli-help-remote-control-cmd-read-relay-status-from-the-existing-shared-daemon = Read relay status from the existing shared daemon without starting it

cli-help-sandbox-setup-set-up-the-elevated-windows-sandbox = Set up the elevated Windows sandbox

cli-help-sandbox-setup-windows-user-that-will-run-codex-after-managed = Windows user that will run Codex after managed deployment

cli-help-sandbox-setup-use-the-current-windows-user-as-the-codex = Use the current Windows user as the Codex user

cli-help-sandbox-setup-codex-home-for-the-codex-user-required-with = CODEX_HOME for the Codex user. Required with --user

cli-help-sandbox-uninstall-remove-the-legacy-windows-sandbox-s-machine-wide = Remove the legacy Windows sandbox's machine-wide accounts and network rules

cli-help-sandbox-uninstall-run-as-administrator-after-stopping-apps-using-the = Run as administrator after stopping apps using the legacy sandbox and its provisioning service, if installed.
    Preserves Codex home, including sandbox directories and filesystem permissions.

cli-desktop-cron-control = Enable Desktop-owned scheduled controls through an authenticated local channel

cli-help-cli-internal-launching-cli-that-handles-daemon-updates-after = Internal: launching CLI that handles daemon updates after the TUI exits

cli-help-cli-optional-user-prompt-to-start-the-session = Optional user prompt to start the session

cli-help-cli-internal-resume-a-specific-recorded-session-by-id = Internal: resume a specific recorded session by id (UUID). Set by the top-level `codex resume <SESSION_ID>` wrapper; not exposed as a public flag

cli-help-cli-internal-show-all-sessions-disables-cwd-filtering-and = Internal: show all sessions (disables cwd filtering and shows CWD column)

cli-help-cli-internal-include-non-interactive-sessions-in-resume-listings = Internal: include non-interactive sessions in resume listings

cli-help-cli-internal-open-the-daemon-wide-agents-overview-instead = Internal: open the daemon-wide agents overview instead of starting a thread

cli-help-cli-internal-the-launching-cli-verified-that-the-local = Internal: the launching CLI verified that the local daemon has a managed backend

cli-help-cli-internal-fork-a-specific-recorded-session-by-id = Internal: fork a specific recorded session by id (UUID). Set by the top-level `codex fork <SESSION_ID>` wrapper; not exposed as a public flag

cli-help-cli-configure-when-the-model-requires-human-approval-before = Configure when the model requires human approval before executing a command

cli-help-cli-enable-live-web-search-when-enabled-the-native = Enable live web search. When enabled, the native Responses `web_search` tool is available to the model (no per‑call approval)

cli-help-cli-disable-alternate-screen-mode-runs-the-tui-in = Disable alternate screen mode

    Runs the TUI in inline mode, preserving terminal scrollback history.

cli-help-cli-run-without-the-shared-background-server-even-if = Run without the shared background server, even if it is already running

cli-help-cli-action-to-perform-if-omitted-runs-a-new = Action to perform. If omitted, runs a new non-interactive session

cli-help-cli-source-classification-for-newly-created-or-forked-threads = Source classification for newly created or forked threads

cli-help-cli-request-an-experimental-cyber-access-program-for-this = Request an experimental Cyber access program for this turn (OpenAI provider only). Omit to use server defaults. Not supported with review; fork requires a prompt

cli-help-cli-allow-running-codex-outside-a-git-repository = Allow running Codex outside a Git repository

cli-help-cli-run-without-persisting-session-files-to-disk = Run without persisting session files to disk

cli-help-cli-do-not-load-codex-home-config-toml-auth = Do not load `$CODEX_HOME/config.toml`; auth still uses `CODEX_HOME`

cli-help-cli-do-not-load-user-or-project-execpolicy-rules = Do not load user or project execpolicy `.rules` files

cli-help-cli-path-to-a-json-schema-file-describing-the = Path to a JSON Schema file describing the model's final response shape

cli-help-cli-specifies-color-settings-for-use-in-the-output = Specifies color settings for use in the output

cli-help-cli-print-events-to-stdout-as-jsonl = Print events to stdout as JSONL

cli-help-cli-specifies-file-where-the-last-message-from-the = Specifies file where the last message from the agent should be written

cli-help-cli-initial-instructions-for-the-agent-if-not-provided = Initial instructions for the agent. If not provided as an argument (or if `-` is used), instructions are read from stdin. If stdin is piped and a prompt is also provided, stdin is appended as a `<stdin>` block

cli-help-cli-resume-a-previous-session-by-id-or-pick = Resume a previous session by id or pick the most recent with --last

cli-help-cli-fork-a-previous-session-by-id-into-a = Fork a previous session by id into a new session

cli-help-cli-run-a-code-review-against-the-current-repository = Run a code review against the current repository

cli-help-cli-conversation-session-id-uuid-or-thread-name-to = Conversation/session id (UUID) or thread name to fork

cli-help-cli-optional-image-s-to-attach-to-the-prompt = Optional image(s) to attach to the prompt sent after forking

cli-help-cli-optional-prompt-to-send-after-forking-if-is = Optional prompt to send after forking. If `-` is used, read from stdin

cli-help-cli-conversation-session-id-uuid-or-thread-name-uuids = Conversation/session id (UUID) or thread name. UUIDs take precedence if it parses. If omitted, use --last to pick the most recent recorded session

cli-help-cli-resume-the-most-recent-recorded-session-newest-without = Resume the most recent recorded session (newest) without specifying an id

cli-help-cli-show-all-sessions-disables-cwd-filtering = Show all sessions (disables cwd filtering)

cli-help-cli-optional-image-s-to-attach-to-the-prompt-details = Optional image(s) to attach to the prompt sent after resuming

cli-help-cli-prompt-to-send-after-resuming-the-session-if = Prompt to send after resuming the session. If `-` is used, read from stdin

cli-help-cli-review-staged-unstaged-and-untracked-changes = Review staged, unstaged, and untracked changes

cli-help-cli-review-changes-against-the-given-base-branch = Review changes against the given base branch

cli-help-cli-review-the-changes-introduced-by-a-commit = Review the changes introduced by a commit

cli-help-cli-optional-commit-title-to-display-in-the-review = Optional commit title to display in the review summary

cli-help-cli-custom-review-instructions-if-is-used-read-from = Custom review instructions. If `-` is used, read from stdin

cli-help-cli-submit-a-new-codex-cloud-task-without-launching = Submit a new Codex Cloud task without launching the TUI

cli-help-cli-show-the-status-of-a-codex-cloud-task = Show the status of a Codex Cloud task

cli-help-cli-list-codex-cloud-tasks = List Codex Cloud tasks

cli-help-cli-list-accessible-codex-cloud-environments = List accessible Codex Cloud environments

cli-help-cli-apply-the-diff-for-a-codex-cloud-task = Apply the diff for a Codex Cloud task locally

cli-help-cli-show-the-unified-diff-for-a-codex-cloud = Show the unified diff for a Codex Cloud task

cli-help-cli-task-prompt-to-run-in-codex-cloud = Task prompt to run in Codex Cloud

cli-help-cli-target-environment-identifier-see-codex-cloud-to-browse = Target environment identifier (see `codex cloud` to browse)

cli-help-cli-number-of-assistant-attempts-best-of-n = Number of assistant attempts (best-of-N)

cli-help-cli-git-branch-to-run-in-codex-cloud-defaults = Git branch to run in Codex Cloud (defaults to current branch)

cli-help-cli-codex-cloud-task-identifier-to-inspect = Codex Cloud task identifier to inspect

cli-help-cli-filter-tasks-by-environment-identifier = Filter tasks by environment identifier

cli-help-cli-maximum-number-of-tasks-to-return-1-20 = Maximum number of tasks to return (1-20)

cli-help-cli-pagination-cursor-returned-by-a-previous-call = Pagination cursor returned by a previous call

cli-help-cli-emit-json-instead-of-plain-text = Emit JSON instead of plain text

cli-help-cli-codex-cloud-task-identifier-to-apply = Codex Cloud task identifier to apply

cli-help-cli-attempt-number-to-apply-1-based = Attempt number to apply (1-based)

cli-help-cli-codex-cloud-task-identifier-to-display = Codex Cloud task identifier to display

cli-help-cli-attempt-number-to-display-1-based = Attempt number to display (1-based)

cli-help-shared-options-optional-image-s-to-attach-to-the-initial = Optional image(s) to attach to the initial prompt

cli-help-shared-options-model-the-agent-should-use = Model the agent should use

cli-help-shared-options-use-open-source-provider = Use open-source provider

cli-help-shared-options-specify-which-local-provider-to-use-lmstudio-or = Specify which local provider to use (lmstudio or ollama). If not specified with --oss, will use config default or show selection

cli-help-shared-options-select-the-sandbox-policy-to-use-when-executing = Select the sandbox policy to use when executing model-generated shell commands

cli-help-shared-options-route-approval-requests-through-automatic-review-using-the = Route approval requests through automatic review using the workspace-write sandbox

cli-help-shared-options-skip-all-confirmation-prompts-and-execute-commands-without = Skip all confirmation prompts and execute commands without sandboxing. EXTREMELY DANGEROUS. Intended solely for running in environments that are externally sandboxed

cli-help-shared-options-run-enabled-hooks-without-requiring-persisted-hook-trust = Run enabled hooks without requiring persisted hook trust for this invocation. DANGEROUS. Intended only for automation that already vets hook sources

cli-help-shared-options-tell-the-agent-to-use-the-specified-directory = Tell the agent to use the specified directory as its working root

cli-help-shared-options-run-the-session-in-a-new-managed-git = Run the session in a new managed Git worktree

cli-help-shared-options-additional-directories-that-should-be-writable-alongside-the = Additional directories that should be writable alongside the primary workspace

cli-help-approval-mode-cli-arg-the-model-decides-when-to-ask-the-user = The model decides when to ask the user for approval

cli-help-approval-mode-cli-arg-never-ask-for-user-approval-execution-failures-are = Never ask for user approval Execution failures are immediately returned to the model

cli-help-config-override-cli-option-that-captures-arbitrary-configuration-overrides-specified = CLI option that captures arbitrary configuration overrides specified as `-c key=value`. It intentionally keeps both halves **unparsed** so that the calling code can decide how to interpret the right-hand side

cli-help-config-override-override-a-configuration-value-that-would-otherwise-be = Override a configuration value that would otherwise be loaded from `~/.codex/config.toml`. Use a dotted path (`foo.bar.baz`) to override nested values. The `value` portion is parsed as TOML. If it fails to parse as TOML, the raw string is used as a literal

cli-help-config-override-override-a-configuration-value-that-would-otherwise-be-details = Override a configuration value that would otherwise be loaded from `~/.codex/config.toml`. Use a dotted path (`foo.bar.baz`) to override nested values. The `value` portion is parsed as TOML. If it fails to parse as TOML, the raw string is used as a literal.

    Examples: - `-c model="o3"` - `-c 'sandbox_permissions=["disk-full-read-access"]'` - `-c shell_environment_policy.inherit=all`

cli-help-command-linux-pid-namespace-isolate-default-or-inherit-inherit = Linux PID namespace: isolate (default) or inherit. Inherit allows signals to other same-UID processes; enable only when provisioning a dedicated environment

cli-help-command-allow-permitted-private-ip-destinations-to-use-the = Allow permitted private IP destinations to use the configured upstream proxy. If no valid upstream proxy applies to the request protocol, connect directly. Loopback stays local. This flag does not require upstream routing

cli-help-command-maximum-number-of-requests-to-process-concurrently-on = Maximum number of requests to process concurrently on each connection

cli-help-command-transport-endpoint-url-supported-values-ws-ip-port = Transport endpoint URL. Supported values: `ws://IP:PORT` (default), `stdio`, `stdio://`

cli-help-command-register-this-exec-server-as-a-remote-environment = Register this exec-server as a remote environment using the given base URL

cli-help-command-transport-used-for-the-remote-executor-connection = Transport used for the remote executor connection

cli-help-command-environment-id-to-attach-to-when-registering-remotely = Environment id to attach to when registering remotely

cli-help-command-human-readable-environment-name = Human-readable environment name

cli-help-command-use-agent-identity-auth-from-codex-access-token = Use Agent Identity auth from CODEX_ACCESS_TOKEN for remote registration

cli-help-command-read-rotating-executor-credentials-from-stdin-exit-when = Read rotating executor credentials from stdin; exit when the pipe closes

cli-help-command-sign-direct-registration-and-websocket-handshake-requests-with = Sign Direct registration and WebSocket handshake requests with AWS SigV4

cli-help-command-aws-profile-used-for-sigv4-authentication = AWS profile used for SigV4 authentication

cli-help-command-aws-signing-region-uses-the-sdk-region-chain = AWS signing region. Uses the SDK region chain when omitted

cli-help-command-aws-signing-service = AWS signing service

cli-help-command-exit-when-the-parent-owned-standard-input-pipe = Exit when the parent-owned standard-input pipe closes

cli-help-command-register-an-existing-websocket-exec-server-as-a = Register an existing WebSocket exec-server as a remote environment

cli-help-command-destination-exec-server-websocket-url = Destination exec-server WebSocket URL

cli-help-startup-config-sign-out-using-the-managed-auth-path-before = Sign out using the managed auth path before loading fresh startup requirements

cli-help-startup-config-validate-configuration-and-print-requirements-as-json-without = Validate configuration and print requirements as JSON without starting the app server

cli-help-startup-config-clientinfo-json-used-for-configuration-auth-request-identity = ClientInfo JSON used for configuration/auth request identity

cli-help-code-mode-host-selects-the-code-mode-host-for-a-single = Selects the code-mode host for a single app-server process

cli-help-code-mode-host-connect-to-a-remote-code-mode-host-instead = Connect to a remote code-mode host instead of starting a local host

cli-help-code-mode-host-process-scoped-transport-used-to-reach-the-code = Process-scoped transport used to reach the code-mode host

cli-help-code-mode-host-start-and-own-the-default-local-code-mode = Start and own the default local code-mode host

cli-help-code-mode-host-share-an-http-2-grpc-connection-to-the = Share an HTTP/2 gRPC connection to the specified remote code-mode host

cli-help-lib-authentication-mode-for-incoming-websocket-connections = Authentication mode for incoming WebSocket connections

cli-help-lib-absolute-path-to-the-capability-token-file = Absolute path to the capability-token file

cli-help-lib-hex-encoded-sha-256-digest-of-the-capability = Hex-encoded SHA-256 digest of the capability token

cli-help-lib-absolute-path-to-the-shared-secret-file-for = Absolute path to the shared secret file for signed JWT bearer tokens

cli-help-lib-expected-issuer-for-signed-jwt-bearer-tokens = Expected issuer for signed JWT bearer tokens

cli-help-lib-expected-audience-for-signed-jwt-bearer-tokens = Expected audience for signed JWT bearer tokens

cli-help-lib-maximum-clock-skew-when-validating-signed-jwt-bearer = Maximum clock skew when validating signed JWT bearer tokens

cli-help-lib-loaded-upgrade-policy-the-default-permits-unauthenticated-connections = Loaded upgrade policy. The default permits unauthenticated connections; each listener decides whether that policy is allowed for its bind address

cli-help-execpolicycheck-arguments-for-evaluating-a-command-against-one-or = Arguments for evaluating a command against one or more execpolicy files

cli-help-execpolicycheck-paths-to-execpolicy-rule-files-to-evaluate-repeatable = Paths to execpolicy rule files to evaluate (repeatable)

cli-help-execpolicycheck-pretty-print-the-json-output = Pretty-print the JSON output

cli-help-execpolicycheck-resolve-absolute-program-paths-against-basename-rules-gated = Resolve absolute program paths against basename rules, gated by any `host_executable()` definitions in the loaded policy files

cli-help-execpolicycheck-command-tokens-to-check-against-the-policy = Command tokens to check against the policy

cli-help-lib-cli-arguments-for-the-proxy = CLI arguments for the proxy

cli-help-lib-port-to-listen-on-if-not-set-an = Port to listen on. If not set, an ephemeral port is used

cli-help-lib-path-to-a-json-file-to-write-startup = Path to a JSON file to write startup info (single line). Includes { "{" }"port": <u16>{ "}" }

cli-help-lib-enable-http-shutdown-endpoint-at-get-shutdown = Enable HTTP shutdown endpoint at GET /shutdown

cli-help-lib-absolute-url-the-proxy-should-forward-requests-to = Absolute URL the proxy should forward requests to (defaults to OpenAI)

cli-help-lib-directory-where-request-response-dumps-should-be-written = Directory where request/response dumps should be written as JSON

cli-help-lib-authenticate-to-the-openai-api-using-github-actions = Authenticate to the OpenAI API using GitHub Actions workload identity. In this mode stdin supplies the GitHub OIDC request token, not an API key

cli-help-lib-audience-configured-on-the-openai-workload-identity-provider = Audience configured on the OpenAI workload identity provider

cli-help-lib-openai-workload-identity-provider-id = OpenAI workload identity provider ID

cli-help-lib-openai-service-account-id-selected-by-the-provider = OpenAI service account ID selected by the provider mapping

cli-help-lib-https-token-request-url-from-github-s-runner = HTTPS token-request URL from GitHub's runner-provided ACTIONS_ID_TOKEN_REQUEST_URL

cli-help-lib-https-token-request-url-from-github-s-runner-details = HTTPS token-request URL from GitHub's runner-provided ACTIONS_ID_TOKEN_REQUEST_URL.

    Use the value directly from the trusted runner environment, never from a workflow input or repository content. The proxy sends the GitHub request credential to this URL.

cli-help-lib-minimal-openai-responses-proxy = Minimal OpenAI responses proxy

cli-help-lib-https-origin-of-the-http-3-proxy = HTTPS origin of the HTTP/3 proxy

cli-help-lib-file-containing-one-exact-approved-https-proxy-origin = File containing one exact approved HTTPS proxy origin per line

cli-help-lib-connect-target-authority-including-a-nonzero-port = CONNECT target authority, including a nonzero port

cli-help-lib-read-the-initial-bearer-from-stdin = Read the initial bearer from stdin

cli-help-lib-read-replacement-bearers-from-stdin-and-stop-when = Read replacement bearers from stdin and stop when the controlling pipe closes

cli-help-lib-read-a-json-list-of-extension-header-name = Read a JSON list of extension-header name/value pairs before the first bearer

cli-help-lib-emit-credential-safe-json-diagnostics-on-stderr-instead = Emit credential-safe JSON diagnostics on stderr instead of human-readable errors

cli-print-help = Print help

cli-print-version = Print version

cli-print-help-long-hint = Print help (see more with '--help')

cli-print-help-short-hint = Print help (see a summary with '-h')

cli-help-command = Print this message or the help of the given subcommand(s)

cli-help-subcommands = Print help for the subcommand(s)

cli-commands-heading = Commands

cli-arguments-heading = Arguments

cli-options-heading = Options

cli-local-server-heading = Local server options (without --remote)

cli-queue-config-help = Override a startup configuration value (key=value). Queued messages use the thread's model.

cli-possible-values = Possible values:

cli-default-value = Default: { $value }

cli-environment = Environment variable: { $value }

cli-usage-heading = Usage:

cli-error-heading = error:

cli-error-repeated-argument = The argument '{ $arg }' cannot be used multiple times.

cli-error-conflict = '{ $arg }' cannot be used with: { $prior }

cli-error-equals = Use an equal sign when assigning a value to '{ $arg }'.

cli-error-missing-value = A value is required for '{ $arg }'.

cli-error-invalid-value = Invalid value '{ $value }' for '{ $arg }'.

cli-error-invalid-subcommand = Unrecognized subcommand '{ $subcommand }'.

cli-error-missing-required = Required arguments were not provided: { $arg }

cli-error-missing-subcommand = '{ $subcommand }' requires a subcommand.

cli-error-invalid-utf8 = An argument contains invalid UTF-8.

cli-error-too-many-values = Unexpected extra value '{ $value }' for '{ $arg }'.

cli-error-too-few-values = '{ $arg }' requires at least { $minimum } values; received { $actual }.

cli-error-wrong-number-values = '{ $arg }' requires { $expected } values; received { $actual }.

cli-error-unknown-argument = Unexpected argument '{ $arg }'.

cli-error-other = Command-line processing failed.

cli-error-valid-values = Possible values: { $value }

cli-error-valid-subcommands = Subcommands: { $value }

cli-error-suggested-subcommand = Did you mean this subcommand? { $value }

cli-error-suggested-argument = Did you mean this argument? { $value }

cli-error-suggested-value = Did you mean this value? { $value }

cli-error-context = Details: { $value }

cli-error-try-help = For more information, try '--help'.

cli-feature-enabled = Enabled feature `{ $feature }` in config.toml.

cli-feature-disabled = Disabled feature `{ $feature }` in config.toml.

cli-error-unknown-feature = Unknown feature flag: { $feature }

cli-feature-under-development-warning = Under-development features enabled: { $feature }. Under-development features are incomplete and may behave unpredictably. To suppress this warning, set `suppress_unstable_features_warning = true` in { $path }.

cli-aliases = Aliases: { $value }
