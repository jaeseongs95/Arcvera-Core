# Application-owned chat controls and exit guidance.
controls-back = Back
controls-goal-edit = Edit goal
controls-goal-edit-help = Type a goal objective and press Enter
controls-goal-resume-title = Resume paused goal?
controls-goal-resume = Resume goal
controls-goal-resume-help = Mark it active and continue when idle
controls-goal-leave-paused = Leave paused
controls-goal-leave-paused-help = Keep it paused; use /goal resume later
controls-goal = Goal
controls-goal-status = Status:{ " " }
controls-goal-objective = Objective:{ " " }
controls-goal-time = Time used:{ " " }
controls-goal-tokens = Tokens used:{ " " }
controls-goal-budget = Token budget:{ " " }
controls-goal-active-commands = Commands: /goal edit, /goal pause, /goal clear
controls-goal-paused-commands = Commands: /goal edit, /goal resume, /goal clear
controls-goal-finished-commands = Commands: /goal edit, /goal clear
controls-goal-active = active
controls-goal-paused = paused
controls-goal-stalled = stalled
controls-goal-usage-limited = usage limited
controls-goal-budget-limited = limited by budget
controls-goal-complete = complete
controls-voice-settings = Voice settings
controls-voice-devices = Set sound devices
controls-voice-choose = Choose a voice
controls-voice-select = Select voice
controls-voice-next = Applies to your next voice conversation.
controls-voice-input-device = Input device
controls-voice-output-device = Output device
controls-voice-system-default = System default
controls-voice-sound-devices = Sound devices
controls-voice-ambiguous = Identical device names; use System default.
controls-voice-input-channels = Input channels
controls-voice-all-channels = All channels (mixed)
controls-voice-channel-help = Select the input channels to mix. Select at least one.
controls-voice-channel-required = Select at least one input channel.
controls-ide-off = IDE context is off.
controls-ide-on = IDE context is on.
controls-ide-usage = Usage: /ide [on|off|status]
controls-ide-skipped = IDE context was skipped for this message.
controls-ide-future = Future messages will include your current IDE selection and open tabs.
controls-ide-connected = Connected to your IDE.
controls-ide-enable-failed = IDE context could not be enabled.
controls-clear-disabled = Ctrl+L is disabled while a task is in progress.
controls-copy-no-response = No agent response to copy
controls-rename-thread = Rename thread
controls-name-thread = Name thread
controls-name-help = Type a name and press Enter
controls-name-empty = Thread name cannot be empty.
controls-name-generating = Generating a title suggestion…
controls-name-suggested = Suggested from this conversation
controls-exit-running = Task is still running
controls-exit-choose = Choose what happens to the current task.
controls-exit-cancel = Cancel task
controls-exit-cancel-help = Stop the current task and stay in Codex
controls-exit-background = Run in background
controls-exit-background-help = Exit Codex and leave the task running
controls-exit = Exit
controls-exit-help = Stop the current task and exit Codex
controls-exit-stop-shortcut = use the configured stop shortcut
controls-exit-disconnected = Disconnected from this task. Any running work continues.
controls-exit-disconnected-fatal = Disconnected from this task. Work may still be running.
controls-exit-disconnected-stopped = Disconnected from this task. The current turn was stopped.
controls-exit-reconnect = To reconnect, run:
controls-exit-continue = To continue this session, run:
controls-goal-value = Goal: { $value }
controls-copy-failed = Copy failed: { $value }
controls-voice-microphone = Microphone: { $value }
controls-voice-input = Input { $value }
controls-voice-inputs = Inputs { $value }
controls-voice-saved = Voice set to { $value }. Applies to your next voice conversation.
controls-exit-press-key = press { $value }
controls-exit-stop-turn = Stop the current turn: run { $command }, select this task, and { $hint }.
controls-exit-archived = Session archived: { $value }
controls-exit-select = Or run { $command } and select { $value }.
controls-exit-session-id = Session ID: { $value }
controls-editor-missing = Cannot open external editor: set $VISUAL or $EDITOR before starting Codex.
controls-editor-failed = Failed to open editor: { $value }
controls-redraw-failed = Failed to redraw transcript: { $value }
controls-switch-failed = Failed to switch side conversation: { $value }
controls-clear-failed = Failed to clear terminal UI: { $value }
controls-review-steer-unavailable = Steer messages aren't supported during /review. Press Ctrl+C now to cancel the review.
controls-tokens = Token usage: total={ $total } input={ $input }{ $cached } output={ $output }{ $reasoning }
controls-tokens-so-far = Token usage so far: total={ $total } input={ $input }{ $cached } output={ $output }{ $reasoning }
controls-tokens-cached = { " " }(+ { $value } cached)
controls-tokens-reasoning = { " " }(reasoning { $value })
controls-ide-open-project = Open this project in VS Code or Cursor with the Codex extension active.
controls-ide-no-context-retry = The IDE extension did not provide context. Try /ide again.
controls-ide-too-large-retry = The selected IDE context is too large. Clear any large selection in your IDE and try /ide again.
controls-ide-request-retry = Codex could not request IDE context. Try /ide again.
controls-ide-read-retry = Codex could not read IDE context. Try /ide again.
controls-ide-too-large = The selected IDE context is too large. Clear any large selection in your IDE.
controls-ide-timeout = Codex timed out waiting for IDE context. It will keep trying on future messages.
controls-ide-changed = The IDE connection changed while Codex was requesting context. Codex will keep trying on future messages.
controls-ide-extension-timeout = The IDE extension did not answer in time. Codex will keep trying on future messages.
controls-ide-incompatible = The connected IDE extension is not compatible with this IDE context request.
controls-ide-unsupported-client = The connected IDE client does not support IDE context requests.
controls-ide-lost-connection = Codex lost the IDE connection while requesting context. Codex will keep trying on future messages.
controls-ide-unexpected-response = Codex received an unexpected IDE context response. Codex will keep trying on future messages.
controls-ide-no-context-future = The IDE extension did not provide context. Codex will keep trying on future messages.
controls-ide-read-future = Codex could not read IDE context. Codex will keep trying on future messages.
controls-ide-platform-unsupported = IDE context is not supported on this platform
controls-copy-confirmed = Copied { $value } to clipboard
controls-copy-unconfirmed = Copy unconfirmed; /export saves chat
controls-copy-pending = Copying { $value }…
controls-copy-busy = Copy already in progress


cleanup-unsupported = The selected server does not support the subagent cleanup API. No conversations were deleted. Specify a supporting server with --remote, or switch to a supporting server before retrying.
cleanup-selection-preserved = The selected item is preserved because its terminal state could not be confirmed: { $id }
cleanup-confirmation-required = Actual cleanup requires terminal confirmation or --apply --confirm.
cleanup-confirmation-token = cleanup
cleanup-confirmation-prompt = Type '{ $token }' to permanently delete the { $count } conversations selected in the preview above:
cleanup-partial-failure = Some items were preserved or their cleanup could not be confirmed. Check the per-item results above.

controls-compact-pending = Compaction will be attempted once after this turn completes. It has not started yet; busy conversations or pending messages prevent admission.

controls-compact-started = Compaction started. Completion is reported by the compaction events.

controls-compact-busy = Compaction did not start because the conversation is busy or has pending messages.

controls-compact-not-started = Compaction did not start:

controls-compact-failed = Compaction request failed:

controls-compact-timeout = Compaction result is unknown after timeout. Do not resend automatically.

controls-memory-disabled = Memories are disabled or unavailable. Ask to enable memories explicitly; saved reading preferences alone do not enable the feature. Environment or managed restrictions may prevent activation.
controls-memory-consolidation-failed = Memory consolidation request failed: { $error }
controls-memory-consolidation-admitted = This is the consolidation request status. accepted is not completion. Read memory_view for persisted job state; job completion does not prove that a particular note was included.
controls-memory-note-scope-unsupported = This server has not declared support for saving a memory note to a different scope. The note was not saved.
controls-memory-note-scope-unconfirmed = The actual scope of the saved memory note could not be verified. Check the result and do not resend automatically.
language-save-legacy-cleanup-failed = The client language preference was saved, but removing the old shared configuration key failed: { $error }. Do not resend automatically.
daemon-auto-version-fallback = The shared background server version { $server } differs from this CLI { $client } or could not be verified. Running this CLI's backend without changing or restarting the shared server.

controls-memory-overridden = Memory settings were saved but overridden. Current runtime activation is unconfirmed; do not resend automatically.

controls-memory-runtime-differs = Memory settings were saved, but the current runtime differs. Explicit session, managed, or environment restrictions may prevent activation. Do not resend automatically.

controls-memory-confirmed = Memory settings and current runtime were confirmed. Reading applies to future context, generation to new work; running work may finish. No summary generation or model turn was started.

diagnostics-status = Startup mode and saved settings are shown separately. General logging alone does not enable cache observation.

diagnostics-saved-next-launch = Saved for the next launch. The current process keeps its startup mode. General logging alone does not enable cache observation.

diagnostics-home-failed = Could not find the local diagnostics settings directory: { $error }

diagnostics-operation-failed = The diagnostics operation failed: { $error }. Query status before retrying any change.

diagnostics-settings-busy = Diagnostics settings are busy. Query status before retrying the change.

diagnostics-operation-unconfirmed = The diagnostics operation could not be confirmed: { $error }. Query status before retrying any change.

diagnostics-cache-wire-requires-response = Wire cache observation requires response observation. Select both explicitly.

controls-memory-command-hint = /memory enable on|off · /memory edit <correction> · /memory read on|off · /memory generate on|off
controls-memory-usage = Usage: /memory view | enable on|off | edit <correction> | read on|off | generate on|off | scope project|global
daemon-auto-selected-mismatch = The selected shared server executable { $managed_path } differs from this CLI { $cli_path }. This session will use this CLI's bundled server; the shared installation was preserved. To explicitly change the selection, run `codex app-server daemon update --from-cli --yes --no-force`.
daemon-implicit-version-mismatch = The shared server at { $endpoint } reports version { $server }, which does not match this CLI { $client } or could not be verified. The connection was refused. Current CLI executable: { $cli_path }; its bundled backend is available with --no-daemon. The shared server was not changed or restarted. The socket does not identify the running server executable path. To explicitly select this CLI's server, run `codex app-server daemon update --from-cli --yes --no-force`.
daemon-version-manage-hint = Use /daemon to manage the local background server.
daemon-version-update-warning = Updating may interrupt active or queued work.
daemon-running-embedded = Running this CLI's bundled backend without the shared background server: { $reason }.
