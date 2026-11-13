<!-- Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md. -->
<!-- Modified for OpenDIY Codex R33 (2026-11-02); see MODIFICATIONS.md. -->
# codex-app-server-daemon

> `codex-app-server-daemon` is experimental and its lifecycle contract may
> change while the remote-management flow is still being developed.

`codex-app-server-daemon` backs the machine-readable `arcvera app-server`
lifecycle commands used by remote clients such as the desktop and mobile apps.
It is intended for Arcvera Core instances launched over SSH, including fresh developer
machines that should expose app-server with `remote_control` enabled.

## Platform support

The daemon supports Linux, macOS, and Windows using platform-specific process
and file-locking primitives. Windows startup requires a non-elevated terminal
whose host permits detached child processes.

Windows automatic attachment requires the canonical socket address to fit the
108-byte AF_UNIX limit (including its terminator). A short junction alias whose
resolved address exceeds that limit falls back to the embedded server. Use a
shorter `ARCVERA_HOME` to share the daemon; discovery does not trust a mutable alias.

Shared clients use the environment inherited when the daemon started. Opening a
new terminal or clearing variables there does not clear the running daemon's
environment; per-client environment isolation is not provided.
An invocation that sets `CODEX_EXEC_SERVER_URL` skips implicit daemon attachment
so its executor selection is preserved. If an implicitly discovered daemon cannot
initialize the connection, the TUI starts an embedded server instead. Explicit
`--remote` endpoints remain authoritative and report connection failures.

## Commands

```sh
arcvera app-server daemon start
arcvera app-server daemon restart
arcvera app-server daemon update
arcvera app-server daemon update --from-cli --no-force
arcvera app-server daemon enable-remote-control
arcvera app-server daemon disable-remote-control
arcvera app-server daemon stop
arcvera app-server daemon version
arcvera app-server daemon status --json
arcvera app-server daemon bootstrap --remote-control
```

`update --from-cli --no-force` selects the invoking complete package only after
holding the operation lock and both existing package roots' installer locks,
and after the existing daemon finishes graceful shutdown. It never requests
updater shutdown: that path can force-kill installer descendants even when the
updater receives only a graceful request. A pre-existing legacy updater is not terminated by this graceful-only operation. The
new dedicated local pin prevents it from restarting the selected server; a
legacy updater may still finish an independent standalone-root package update.
This graceful-only operation never force-kills a managed process
or its installer descendants. Installer-lock failure, a failed request or elapsed
shutdown grace leaves `current` unchanged and does not start a replacement or
automatically restart the updater. A staged release may remain on disk.

Success retains the existing `status: "updated"` JSON. An incomplete non-forcing
stop prints the same output fields with `shutdownUnconfirmed` (request/ACK
failed), `shutdownTimedOut` (grace elapsed), or `shutdownOutcomeUnknown` (state
could not be reconciled), and exits unsuccessfully. Version fields are null when
not re-observed. The old process may still be draining: an ACK is not process
exit, an already-sent shutdown cannot be undone, and these outcomes must not be
automatically retried. `--no-force` requires `--from-cli`; omitting it preserves
the existing force-after-grace behavior.

On Unix, non-forcing daemon shutdown uses `SIGHUP`, which R28 and newer treat
as graceful-only even when already draining. On Windows it sends
`graceful:<pid>` on the existing managed `/daemon/shutdown` control connection
and requires the identical acknowledgment. This graceful-only extension needs
an updated server. R28/R32 reject it without a shutdown event, so a Windows
R28/R32-to-R33 non-forcing automatic transition is unsupported: it returns
`shutdownUnconfirmed`, preserves the selection, and never falls back to the
legacy forceable PID-only request. Repeating a no-force request cannot escalate
an existing graceful drain. Default shutdown requests keep their forceable policy.

`status --json` is a passive observation: `status` is `running`, `stopped`, or
`unknown`, with `managedCodexPath`, `managedCodexVersion`, `appServerVersion`,
`socketPath`, `cliVersion`, and `reason`. Missing/unobserved values are null.
Both legacy and dedicated PID namespaces are inspected without stale-record
cleanup. Starting reservations, changed PID generations, metadata errors, and
unreachable existing sockets are unknown, not stopped. The socket probe has a
two-second deadline and the selected binary version probe a five-second deadline.
No settings, authentication, log contents, or conversation payloads are returned.
An observation is not an atomic installation snapshot or a reservation against
new work, and `stopped` is not a safe-to-upgrade/quiescence gate.

On success, every command writes exactly one JSON object to stdout. Consumers
should parse that JSON rather than relying on human-readable text. Lifecycle
responses report the resolved backend, socket path, local CLI version, and
running app-server version when applicable.

Arcvera-Core 1.0.1 disables the inherited OpenAI installer and automatic updater.
Saved updater preferences do not enable it. `arcvera update`, daemon update
without `--from-cli`, and the updater loop fail before accessing the home directory
or downloading files. Download a complete Arcvera release package manually from
[Arcvera-Core releases](https://github.com/jaeseongs95/Arcvera-Core/releases).

To select an extracted complete package explicitly, run its executable with
`app-server daemon update --from-cli --yes`. This can stop the existing managed
server and interrupt work; coordinate the transition with its clients first.
Use `--no-force` for the graceful-only behavior described above.

For all managed app-server shutdowns, including explicit stop and restart and
updater-triggered restarts, `shutdownGraceSeconds` defaults to 60 and accepts
an integer from 0 through 300. Zero forces shutdown immediately after requesting
a graceful exit; the five-minute maximum bounds the wait even if a turn is still
running.

## Bootstrap flow

Version 1.0.1 is distributed as a Windows x86-64 package. Extract the complete
release ZIP, then use a non-elevated terminal that permits detached child processes:

```powershell
.\bin\arcvera.exe app-server daemon bootstrap --remote-control
```

The default product home is `~/.arcvera`; `ARCVERA_HOME` overrides it.
`CODEX_HOME` is ignored. Existing official Codex data is not imported automatically.
`bootstrap` copies the invoking complete package into
`ARCVERA_HOME/packages/app-server-daemon` when no managed package exists. A bare
executable cannot supply a new installation. An existing selection is reused;
it is not silently replaced. A new package must be selected explicitly using
`update --from-cli`.

Settings and process records live under `ARCVERA_HOME/app-server-daemon/`.
Managed Windows binaries use `packages/app-server-daemon/current/bin/arcvera.exe`.
The product does not start an automatic updater. Required upstream helper names
and machine-readable protocol fields are retained for compatibility.

## Lifecycle semantics

`start` is idempotent and returns after app-server is ready to answer the normal
JSON-RPC initialize handshake on the Unix control socket.

`restart` stops any managed daemon and starts it again.

`enable-remote-control` and `disable-remote-control` persist the launch setting
for future starts. If a managed app-server is already running, they restart it
so the new setting takes effect immediately.

Top-level `arcvera remote-control start` enables and persists remote control for
the managed daemon, overriding a saved disabled value. It starts or bootstraps
the daemon as needed. Plain `arcvera remote-control` uses the managed daemon when
eligible; `arcvera remote-control --no-daemon` runs a separate foreground server.
`arcvera remote-control stop` stops the managed daemon without clearing its saved
remote-control preference.
`daemon start` and `daemon restart` use that saved preference. `daemon bootstrap`
sets it according to `--remote-control` (disabled when omitted).

`stop` sends a graceful termination request first, then force-terminates the
process after the configured grace window if it is still alive.

All mutating lifecycle commands are serialized per `ARCVERA_HOME`, so a concurrent
`start`, `restart`, `enable-remote-control`, `disable-remote-control`, `stop`,
or `bootstrap` does not race another in-flight lifecycle operation.

## State

The daemon stores its local state under `ARCVERA_HOME/app-server-daemon/`:

- `settings.json` for remote-control launch settings and updater preferences
- `app-server.pid` for the app-server process record
- `app-server-updater.pid` for the pid-backed standalone updater loop
- `daemon.lock` for daemon-wide lifecycle serialization
