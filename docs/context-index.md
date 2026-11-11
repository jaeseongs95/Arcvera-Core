# Arcvera-Core indexed context and incremental compaction

[English](context-index.md) · [한국어](context-index.ko.md)

R41 introduces a host-owned, vendor-independent index for the current conversation. It records source identities, task lifetimes, dependencies, and exclusion history without copying conversation bodies into the index. This document describes the development candidate; final executable behavior, performance, and release acceptance remain unverified until recorded in the [R41 release note](releases/R41.md).

## Data and authority

| Record | Meaning |
| --- | --- |
| Source | A native response-item ID, or a call/result locator for a legacy tool item; kind, turn, ordinal, and optional task owner |
| Task | Explicit ID, label, `active` or `completed` state, and dependency IDs |
| Revision | Monotonically increasing policy revision used for compare-and-swap updates |
| Audit event | Accepted operations and the source IDs excluded or reactivated by that revision |

Original content remains in the existing conversation store. Index metadata and retrieved historical content do not grant permissions, replace current user instructions, or establish that a request was authorized. External-memory retrieval remains disabled when disabled by the host. This tool does not query external memory, arbitrary files, or other conversations. An unavailable lookup is not evidence that relevant information does not exist.

Source registration stores a SHA-256 equality digest of the serialized original, without retaining its body in the index. Reusing a locator for a different body, or restoring an older descriptor without equality evidence, conservatively reactivates that source as unknown. Every distinct original remains preserved, including when compaction retains a same-ID live suffix. This can increase model input; it does not establish semantic equivalence or authorize discarding either original. Hashing costs scale with original size and do not impose a total memory bound.

Original identity and prepared-view identity are separate. `body_sha256` identifies the serialized original; an optional `projection_sha256` identifies the host-prepared resident view, which may contain a truncated tool result. A checkpoint may retain the known source's classification when that view and its locator, kind, call identity and host-compaction scope match. Raw ingestion always checks original identity: a new raw item equal to an older prepared view does not become the older original. An absent original digest or a conflicting original remains unknown and active. Anonymous checkpoint items are reconciled by occurrence count; identical repeated items cannot all claim one descriptor.

## Active projection

Before starting work, use `create_task` if necessary and `set_focus` to select the current task. Model API requests retain that task and its transitive dependency closure; unrelated tasks' classified details are excluded even when those tasks remain active. Completed dependencies stay available while the focused task references them. Completing the focused task does not select another task automatically: explicitly set the next focus. The original history and authorization-review evidence are separate from this model projection.

User messages, system/developer controls, unknown kinds, unclassified items, and legacy items without a stable locator remain active. This preserves the latest user corrections without inferring instruction expiry. Without a selected focus, the index conservatively retains the active-task closure and unclassified content. This legacy mode does not prove that every retained item is relevant to the current task.

New assistant/tool details are associated with the captured focus when known. Newly host-scoped reasoning/compaction items follow that focus; legacy opaque content remains conservative. Tool calls and results form a group: incomplete groups remain active, and binding one member binds the group to one task. A source cannot silently move to another task. Only a clearly successful native `context_index` apply's completed call/result group is omitted from model replay so its mutation arguments cannot repeatedly expose unrelated task metadata. Failed, pending, read, list and audit groups retain their normal preservation rules; generic CodeMode execution is not treated as a successful native apply by inference. Raw history, audit and authorization evidence remain preserved.

Sampling, prewarm and local/remote compaction are intended to share this projection. An index-policy revision change invalidates a cached WebSocket `previous_response_id` continuation so old server-side context cannot restore excluded details; unchanged policy retains the existing continuation optimization when its other prerequisites hold. Opening a new transport clears its connection and continuation state while retaining the bound policy revision and authentication owner generation. A reconnect must not turn an unchanged policy into another policy change. Foreground/background compaction adoption must reject a result captured under a stale policy. Current user/control/unclassified roots are preserved independently of summaries; their accumulation limits achievable input reduction. Routine exclusion is an index update, without a full-conversation summarization request. Existing summary compaction remains available when active input still needs compression. R41 does not provide a new semantic summary format, automatic instruction-expiry inference, or a total process-memory bound.

The implementation caches active membership, task reachability and resident source positions. Projection selects active resident positions rather than copying every retained body and then filtering it. Policy changes still clone body-free index state and rebuild derived sets, and the index/audit grow with the conversation. Bounded request-only navigation metadata identifies reachable tasks and active sources outside the resident window; retrieve required originals with `read` before relying on them. Automatic rehydration of every archived source and physical memory reclamation are not implied. Complete-path latency and memory improvements require measurement.

## Native tool contract

`context_index` is a native conversation tool, not a PowerShell or slash command. Its requests are closed JSON objects; unsupported fields and invalid values are errors.

| Action | Inputs and result |
| --- | --- |
| `list` | Optional nonnegative `cursor`, `limit` from 1 to 100 (default 25), and `include_unrelated` (default false); returns revision, focus, source/task metadata, active reasons, total/visible counts, cache metadata, recovery status, and `next_cursor` |
| `read` | Exact `source_id` from this conversation, optional byte `offset` and `max_bytes` from 1 to 16384 (default 8192); returns a UTF-8-safe slice of serialized `ResponseItem` JSON and `next_offset` |
| `apply` | Required `expected_revision` and 1 to 128 `operations`; atomically validates and accepts the batch or reports an error |
| `audit` | Optional nonnegative `cursor` and `limit` from 1 to 100 (default 25); returns event summaries/counts and `next_cursor`. With an exact event `revision`, returns its original JSON in bounded UTF-8 chunks using `offset` and `max_bytes` (default 8192, maximum 16384) |

With a focus selected, `list` normally shows active sources and reachable tasks. `include_unrelated: true` explicitly expands metadata navigation to archived/unrelated entries; it does not change focus or automatically retrieve their bodies. Sources and tasks page independently at the same cursor in index-key order. `audit` pages accepted event summaries in order; byte offsets require an event revision. Continue with the returned cursor/offset. If the index revision changes during multi-page inspection, obtain a current view before deciding. Requests and serialized responses are limited to 65536 bytes; reduce the page size if necessary.

| Operation | Required fields |
| --- | --- |
| `create_task` | `task_id`, `label` |
| `bind_sources` | `task_id`, `source_ids` (1 to 1024 exact known IDs) |
| `set_task_state` | `task_id`, `state` (`active` or `completed`) |
| `add_dependency` | `task_id`, `dependency_id` (both existing tasks) |
| `remove_dependency` | `task_id`, `dependency_id` (existing edge) |
| `set_focus` | `task_id` (existing task selected as the request-context root) |

Task IDs contain 1 to 256 UTF-8 bytes and no control characters; labels allow at most 1024 bytes. Dependencies are directed: if focused task A depends on B, B and B's dependencies stay reachable regardless of B's completion state. Traversal terminates for cycles. An unrelated cycle does not make itself reachable; without focus, active tasks provide the conservative roots.

For a new index, an example batch is:

```json
{"action":"apply","expected_revision":0,"operations":[{"op":"create_task","task_id":"investigation","label":"Investigate the reported failure"},{"op":"create_task","task_id":"repair","label":"Repair and verify"},{"op":"add_dependency","task_id":"repair","dependency_id":"investigation"},{"op":"set_focus","task_id":"repair"}]}
```

Use the actual revision returned by `list`, and bind only source IDs returned for this conversation. Marking `investigation` completed does not exclude its eligible sources while focused `repair` depends on it. Unrelated tasks' classified details are excluded even while active. User/control sources remain protected regardless of completion. This example does not establish permission to execute any repair.

`read` uses only the current live conversation's store, including inheritance the store authorizes for a fork. In an ephemeral conversation, only retained in-memory history is available and may already be truncated; the result identifies that storage boundary. Unknown IDs, missing originals, unstable legacy locators, invalid UTF-8 offsets, or storage errors are explicit failures. Identical repeats of one source remain readable. If the same locator identifies different original bodies, lookup returns an explicit ambiguity error and preserves every original. Do not substitute a summary for the original or interpret failure as absence.

Persisted original lookup must work independently of full-history and display-page APIs. Paginated fork reads must honor frozen ancestor byte and ordinal cutoffs, including archived compressed ancestors, and scan past compaction records without treating replacement views as originals. Corrupt records, unavailable files and cancellation must remain explicit failures. The local scan has a linear I/O cost in authorized history bytes and retains the selected original plus the largest decoded record; compressed input can require an anonymous temporary decoded file. The response chunk limit is not a total-process memory bound. These boundaries require final native verification before release acceptance.

## Persistence, migration, and recovery

Arcvera-Core maintains its own context and storage contracts. Compatibility with the official Codex CLI is outside support scope. OpenAI server communication, including the required server API and authentication paths, remains supported. Source provenance or preservation of stored originals does not imply official CLI interchangeability.

Policy changes share the persistence ordering boundary of conversation checkpoints/settings. An accepted transaction owns its lifetime even if the tool waiter is cancelled. An event is appended before its revision is published to the live index. An uncertain append or a publication error is an explicit recovery error: new sampling, policy mutation and compaction must not continue until cold resume resolves stored state. A failed response-item append marks this recovery state before releasing persistence serialization, preventing later policy changes from binding an unpersisted source. Original history is retained because partial durability is uncertain. Do not automatically resend a request with an unknown outcome. After recovery, inspect `list` and `audit` before deciding what to do next.

Compaction resume metadata can hold the body-free index snapshot, and subsequent policy events carry expected/new revisions. Restore validates references and reconstructs derived sets. A mismatched revision or malformed reference is an error, not permission to drop metadata silently. Destructive history replacement rebuilds from the replacement history rather than reviving abandoned task bindings. Actual resume, fork, rollback, concurrent compaction, and failure-recovery boundaries require execution tests on the final candidate.

Core checkpoint restore and local-store migration share the same descriptor reconciliation. A general snapshot that omits a resident native/call source reconciles it from the prior original descriptor; it does not treat truncated resident bytes as a new original or infer ownership from the incoming focus. An unknown resident stays active. An actual rollback or destructive history replacement starts a new index for the surviving history and clears the abandoned tasks, focus, archives and identity evidence. This reset intentionally differs from restoring a compaction checkpoint.

For an older conversation without index metadata, the safe starting view keeps registered sources active. No migration should guess completed tasks or delete originals. Existing compacted conversations cannot reconstruct originals already absent from their authorized store. R41 does not introduce a raw fallback, forced reset, automatic authentication, or cross-conversation recovery lookup.

Before an authorized installation or shared-server switch, retain the complete previous package and preserve conversation/authentication storage. New context-index rollout records and checkpoint fields have not been shown to be readable by older executables; reverting an EXE is not a validated downgrade of conversations modified by R41. Preserve the new records and stop on an incompatible-reader error. Test recovery with isolated copies and a compatible reader; do not edit operational storage or delete records to force an older version to open it. Installation and operational switching are separate actions from publication.

## Cache observations and retention

Native `list` reports the latest completed sampling response's cache observation. A present, nonnegative integer `input_tokens_details.cached_tokens` gives response-level `hit` when positive and `miss` when zero. Missing/null data is `unknown`; malformed containers, negative/noninteger counters, a malformed supplied input count, or a cached count exceeding the input count are `invalid`. Missing metrics are not converted to zero. Aggregate cached tokens never identify which source, task, instruction or body hit the cache.

`observed_at` is the host's sampling-completed observation time, not a server cache-creation time. `request_context_index_revision` is captured for the request; `current_context_index_revision` and `revision_status` distinguish current, stale and unknown observations. A late result is not relabeled with a newer policy revision. Coverage currently describes sampling only; compaction and prewarm are `not_observed`.

Retention duration and expiry remain `unknown` with `not_provided` provenance and null values. No adapter-confirmed TTL/expiry is implemented; arbitrary usage fields or guessed provider defaults are not promoted to server guarantees. Any later configured or estimated values must remain distinct from server-confirmed evidence. Cache state never controls relevance, permission or deletion of originals/required context. Observation timestamps are returned on explicit metadata lookup; they are not automatically injected into the request navigation manifest.

## Verification obligations

The release must connect each claim to tests of the actual sampling/compaction/native-tool and persistence paths. Required boundaries include focused-task filtering of serialized HTTP/WebSocket payloads, completed-but-referenced sources, unrelated active tasks and labels, transitive/cyclic dependencies, latest user corrections and permission evidence, incomplete call/result groups, successful native-apply metadata replay, stale revisions and atomic failed batches, bounded audit/read output, unavailable originals, UTF-8 reads, durable append uncertainty and cancelled waiters, stale foreground/background compaction adoption, policy-change WebSocket reset with unchanged input, stable-policy continuation, and resume/fork recovery. Cache checks must distinguish absent/malformed/zero/positive counters, late revision-bound observations, unknown TTL/expiry, and lack of per-source attribution. Performance comparisons must identify the same workload, binaries, active/total source counts, elapsed time, and memory observations. Missing measurements are unverified.

[R41 release status](releases/R41.md) · [R42 deferred scope](roadmap/R42.md) · [Validation](TESTING.md)
