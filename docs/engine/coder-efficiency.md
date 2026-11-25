# Coder usage attribution and batch edits

## Usage records

Native tool-loop inference appends `kind: "inference"` records to the existing
session `turn_ledger` JSONL store in the daemon's data directory. Session deletion
removes these records with the rest of that ledger. Host and worker loops use
the same runtime implementation.

Daemon records carry `execution_id` (the actual host turn or worker ID) and, for
workers, `parent_turn_id`. `stream_turn_id` is a UI counter and may repeat across
turns and peers; it is not a unique execution identifier. Records also carry
`rounds_executed`. Their `inference`
object contains:

- `schema_version: 2` (the report also accepts version 1) and `outcome`: `completed`, `provider_error`,
  `malformed_tool_json`, or `interrupted`;
- elapsed milliseconds, the provider model and response ID when available;
- provider-reported input, cache-read, cache-write, output, and reasoning tokens;
- generated assistant text, serialized tool arguments, and code edit payload
  sizes in **characters**, plus the number and names of emitted tool calls;
- unescaped top-level intent characters, counted once per emitted call, and
  requested child operations for `cognition_coder_read_batch`;
- SHA-256 fingerprints and byte sizes for tools, the top-level system field,
  and ordered messages, with comparisons against the previous loop request.

Source, prompts, arguments, results, and reasoning bodies are not copied into
the inference record. Other existing ledger records retain their existing
scratch and receipt behavior.

Missing provider counts remain `null`. Output already includes reasoning where
the adapter normalizes it into completion usage: do not add reasoning again.
The pinned GenAI adapter does not expose OpenAI's `cache_write_tokens` field;
its cache-creation count is recorded only when supplied. Some adapters also
normalize reported zero detail counters to absent. Do not interpret either
case as zero usage or calculate an exact invoice from incomplete counts.

Fingerprints represent the GenAI request before provider rendering. Message
prefix lengths are not token counts or provider cache-breakpoint measurements.
Tool/schema changes and history rewrites identify candidates for investigation,
not proven cache misses. Comparison restarts at each new loop execution.

One record represents one logical model request from the tool loop, including
its explicit malformed-JSON retry rounds. Internal provider-adapter fallbacks,
non-tool-loop inference, and provider billing after an interrupted stream are
not separately observable here. An error/interruption has unknown usage, not a
zero-cost receipt. Recording does not change model, reasoning, cache policy,
completion, or verification behavior.

Summarize a completed session ledger snapshot with:

```sh
python3 scripts/analyze-coder-usage.py /path/to/session-ledger.jsonl
```

The JSON report groups by execution ID, labels legacy stream-only attribution
`legacy_stream_ambiguous`, includes counter coverage and partial reported
sums, and leaves incomplete totals and cache ratios `null`. Use it to compare
tool rounds and generated edit payloads across equivalent tasks. Provider usage
reports remain the billing authority.

## Context selection and replaceable projections

Context uses a hybrid policy. At turn start, whole older chat exchanges may be
selected out toward a soft working target of 280,000 estimated tokens, bounded
by the route's available input budget. The tool loop reuses the selected baseline. If a later request still
exceeds the hard budget after maintenance, it tries selection once more from
the stored source prefix with the current tool results. A new turn or checkpoint
resume can also select again. Stickies, work projections and the HUD remain replaceable tails.
Crossing the working target requests maintenance; it is not an admission failure.
The separate hard budget follows the route's input/shared limits, output reserve,
safety and observation headroom. Required input above the soft target can still
run when it fits the hard budget.

Observation lane budgets are fixed for the turn rather than shrinking after each
inference. Under pressure, the runtime can archive complete older successful tool
batches, including their arguments and model-visible results. The four newest
results, failures, user guidance and governing state stay resident. Rollover needs
a durable evidence retrieval receipt and a successfully saved checkpoint before
the loop continues with a compact archive pointer. Storage or checkpoint failure
preserves the resident history. Full invocation receipts remain unchanged.
If the hard budget still cannot fit a growing loop, an available checkpoint sink
saves a recoverable continuation and the principal receives a checkpoint outcome,
not a completion claim. Initial irreducible input or missing checkpoint storage
still produces an explicit overflow rather than silent truncation.

ChatGPT sign-in (`openai-codex`) retains the account catalog's `context_window`
(or `max_context_window` when absent), bounded by `max_context_window`, and applies
`effective_context_window_percent` (95% when absent). These account-route limits
are independent of API-key catalogs. Older cached GPT-5/GPT-6 account entries
without limits use an explicit estimated 272,000-token shared window until the
account catalog refreshes. Other unknown routes retain the conservative 8,000
shared-window assumption. Neither estimate is advertised as provider metadata.
The sign-in bridge does not send an output cap; it reserves an advertised output
maximum when available, otherwise an estimated 4,096 tokens.

Recognized GPT-4o/4.1, GPT-5/6 and o1/o3/o4 routes on OpenAI, Codex and Azure use
an offline `o200k_base` tokenizer proxy plus 25% text headroom. In particular,
GPT-6's exact encoding is not assumed known. Other routes retain an explicit
conservative UTF-8 byte estimate. Both add protocol framing and media allowances;
neither claims exact provider-rendered tokenization. Extra dispatch payload uses
the same route accounting. Layer-only character/4 telemetry is a different
measure; the inference report's total and hard `input_budget` govern dispatch.
For ChatGPT sign-in, a replay signature contributes once per message even when
multiple tool calls carry it. The full proxy records its share in
`estimate.replay_signature_tokens`. With a positive input measurement from the
same turn and model and matching tools and system instructions, Medousa can use
the previous measured input plus the estimated cost of each new or replaced
message. Removed messages receive no subtraction from measured usage. Selection
and dispatch use the lower of that bound and the full estimate. When the
measured bound wins, `estimate.previous_input_tokens` records its basis.
A locally rejected follow-up appears in the ledger as `window_rejected`, with
the candidate footprint, estimate and input budget. Provider usage summaries
exclude these local decisions from dispatched request counts.
Deferred session history remains stored and retrievable.

The inference window report includes typed `deferred_pointers` with source scope,
provenance, selection reasons, and native retrieval routes. Chat-history routes
retain their exact session IDs; engineering routes retain their undertaking and
call/event IDs. Context-pointer follow checks the admitted principal's visibility
and reads the requested native source independently of ambient top-N ranking.
Deferral or archival in the digest is not deletion and grants no new permissions.
Ranking separates relevance, recency and activity from source confidence. A failed
call is activity, not an unresolved obligation; a successful call is not work
completion. Without an authoritative resolution relationship, resolution is
`unknown`.

The model receives a replaceable `[MEDOUSA_CONTEXT_HUD]` system tail on the
inference request copy, not an appended conversation turn. It carries available
work focus/lifecycle, meaningful changes, pressure and deferred-history retrieval.
Its bounded footprint is accounted before dispatch. If it cannot fit, the HUD is
omitted rather than cutting policy, current input, protected stickies or tool
pairs. Stable request prefixes are unchanged.

Existing `context_usage` stream events and `ContextUsageReport` now have an
optional `hud` projection. Older clients may ignore it. It contains available
`work` facts (`work_id`, authoritative `phase`, safe progress/blocker/decision
accounting), a `pressure` band, a `deferred_items` count and the accounting label.
Work facts are absent when no authoritative work projection is available. Private
prompt/evidence/review prose is not copied into the projection. A lifecycle phase
or executor result is not proof that the user's goal is verified or accepted.

Daemon accounting snapshots are replaced on every inference, including resumed
and growing tool loops. User-facing events coalesce work/selection changes;
changes to estimated token counts or pressure alone do not emit another update.
The existing operator summary renders the safe projection, with accounting detail
available in the report. Totals remain conservative estimates including protocol
and media allowances; per-layer estimates are text-only, and provider-reported
usage remains a separate authority. These views do not introduce another work
store, change retrieval admission, or expand tool permissions.

## Batch replacements through `code.write`

`cognition_store_write` accepts `action: "code.write"` with an `edits` array.
Fetch the typed `code.write` contract through `cognition_schema` as usual.

```json
{
  "intent": "Update both method call sites",
  "action": "code.write",
  "path": "src/service.rs",
  "expected_sha256": "sha256:<digest returned by code.read>",
  "edits": [
    {"find": "client.fetch(first)", "replace": "client.load(first)"},
    {"find": "client.fetch(second)", "replace": "client.load(second)"}
  ]
}
```

The batch contains 1–128 replacements in **one existing UTF-8 file**. Every
`find` must be nonempty and occur exactly once in the original revision;
include surrounding context to disambiguate. Matches cannot overlap. Array
order does not change addressing: replacement text is never searched by a
subsequent edit. Empty replacement text deletes a match. The batch cannot be
combined with `content`, `find`, or `replace` at the top level. Existing full-file
and single-replacement calls remain supported.

All edits, the combined payload size, and the resulting file size are validated
before publication. The existing adapter file-size limits apply. A missing,
ambiguous, overlapping, or stale match leaves the target unchanged. Success
returns `mode: "batch"`, `applied_edits`, final byte size, and the new digest;
it does not echo source content.

The daemon retains authority over the path, lease, policy, and claims. Native
batch writes stage a complete file beside the target, preserve permissions,
recheck the original contents, and publish by rename. Governed `/workspace`
writes retain their execution fences and recheck SHA-256 before rename; their
environment must provide `sha256sum` or `shasum`. Native batch commits are
serialized. Independent editors and shell writers do not share that lock;
optimistic checks cannot prevent an external write in the final check/rename
interval. This is not a multi-file transaction or a filesystem lock for humans.

If a write is interrupted or its transport outcome is uncertain, inspect the
current file before retrying. A replay using the old digest fails after a
successful content-changing batch. Batch editing introduces no automatic
mutation retries.


## Shared intent for independent workspace observations

Coder intent remains required and bounded to 320 characters. The prompt and
schema recommend an outcome-oriented phrase, usually 4–8 words, such as
`Find affected callers`. This is guidance, not another hard length restriction.
Edits, commands, delegation, and completion keep their own intent.

Use `cognition_coder_read_batch` when 1–4 independent `code.read`/`code.search`
operations share a purpose and their arguments are already known:

```json
{
  "intent": "Inspect cache configuration and callers",
  "operations": [
    {"action": "code.read", "path": "src/config.rs", "line_start": 20, "line_end": 80},
    {"action": "code.search", "query": "cache_key", "max_results": 12}
  ]
}
```

The model supplies intent once. The runtime passes it to each child's normal
Coder admission and activity lifecycle, preserving individual call IDs and
receipts where the registry provides them. Local roots and portable `/workspace`
binding, authority checks, and claims follow the existing single-read path.
Portable tasks must admit both the batch tool and `cognition_store_read`; the
batch cannot expand the destination's admitted tools. Revocation is checked at
each child boundary, including when later items are still queued.

Only the two typed operations are accepted. Child intents, roots, arbitrary
tools, nested batches, and writes are rejected. The entire typed request is
validated before any child starts: 1–4 items, at most 64 KiB of serialized
operation input, nonempty paths/queries, ordered line or byte ranges (not both),
and search limits of 1–100. Path authority is checked individually during child
admission, so one denied item does not hide successful sibling observations.

Concurrency respects the configured parallel-tool setting and is capped at
four; disabling parallel tools makes children sequential. The outer tool loop
executes a read batch separately from other calls even if mutating parallelism
is enabled, avoiding multiplication of the inner concurrency limit. Children
run as futures in the caller's task; cancellation drops active and queued
futures without spawning detached read tasks. Underlying I/O follows its
existing cancellation semantics.

Results are returned in input order with a zero-based `index`, `ok`, and either
`result` or an explicit error. Each serialized child result is limited to
24 KiB, and the combined response stays below 128 KiB. Oversized results are
omitted with `code: "batch_result_too_large"` and their byte count; narrow the
range/search limit or use a single read. Source is never silently truncated
inside a successful item. Normal tool-loop observation budgets still apply.
Prefer bounded ranges when reading several files. These independent reads are
not a snapshot transaction; concurrent writers can change files between reads.

A result-dependent search, edit, or validation starts a new decision and call.
Do not pack a read/edit/test chain into a batch. Multiple ordinary calls in one
model response remain supported; batching primarily adds shared intent and a
bounded grouped result. It only saves inference rounds when those observations
would otherwise have been requested in separate model responses.

Version 2 usage records add `generated.intent_chars` and
`generated.requested_batch_operations`. These are `null` when a response was not
observed. Child counts describe model requests, not proof of execution or
successful reads. Intent counts exclude the JSON key/escaping overhead and are
a subset of tool argument size. The report includes `generated_coverage` (the
number of completed requests supplying each generated counter) and
`tool_calls_per_completed_request`. Generated sums are partial when coverage
is below request count; legacy records have no measurements of the new fields.
Use the existing token counts to measure whole-request costs, and compare
representative tasks for missed evidence, unnecessary edits, verification
quality, and recovery from failed reads before claiming behavioral equivalence
or a savings percentage.


## Task ownership and peer execution

The latest user request owns a new host turn's goal. Earlier goals and gaps are
labeled as context, not carried forward as active tool obligations. Exact in-turn
resume still restores its checkpoint. A peer receives the host handoff but starts
its own goal, step counter, and tool obligations.

In Coder, own the full requested outcome. The smallest coherent change means
controlled scope, not stopping after one local fix: trace affected callers,
contracts, and tests, then keep implementing and validating until the outcome
is satisfied or a concrete blocker prevents further authorized progress. Follow
existing architecture as well as repository conventions. Use existing Coder
memory at milestones for complex or long work; do not record every tool call.

Use your own tools directly to work and `turn.update_user` for nonterminal
progress. `turn.checkpoint` deliberately ends the current agent turn and waits
for the principal, so use it only when their input is needed or work must pause.
`workshop.spawn` explicitly creates a separate concurrent peer. Assign a bounded
task and expected result, perform complementary work, and integrate its findings.
The ambiguous `turn.begin_work` action is hidden from Coder advertisements and
schema discovery; old calls still map to spawning a peer for compatibility.

Coder's `intent` is the short purpose of the call. The separate `worker_profile`
selects the peer environment, defaulting to `coder`; explicitly select `research`
or `general` only for those assignments. Fetched action schemas include required
Coder intent metadata. Existing profile-valued intents remain accepted for old
calls. Coder peers retain Forge admission, leases, and workspace placement rules;
spawning does not grant permission for concurrent writers in an attached checkout.

Native Coder file operations carry the selected Forge checkout root through the
admitted tool invocation. This works for isolated worktrees and explicitly
attached current checkouts outside Medousa's data directory. The grant is local
to that invocation; knowing the filesystem path does not authorize an unbound
tool. Existing path containment, symlink checks, mutation policy, and live Coder
lease checks still apply.

The shell host keeps scripts and managed worktrees on its existing root path.
For a tracked agent/task shell in an external attached checkout, creation
requires a matching open Forge project and revalidates its repository, branch,
HEAD, index, and requested cwd. Human project shells use the separate native-only
workspace-shell route: they validate the current folder's repository identity
and containment without renewing a Forge execution lease or baseline. Branch
changes are context for that human shell and remain fenced for tracked work.
Newly attached projects are available without restarting the shell host; closed
projects reject new project shells. Project roots never enter the host's global
allowlist. This admission check does not turn the PTY into an OS filesystem sandbox.

The daemon and `medousa-session` must be upgraded together for this behavior.
Shell-host API revision 6 accepts `--forge-root` and reports that store path in
`/health`; the daemon rejects a host using an older protocol or a different Forge
store. Source builds must include both `engine` and `shell-session` components.

Worker status and cancellation return compact `record`/`records` summaries:
identity, profile, status, bounded task/result/error text, termination reason, and
update time. Result text is paginated in 1,200-character slices: call
`workshop.status` with `work_id` and the returned `result_next_offset` as
`result_offset` until it is null. Pages reassemble the full result without
replaying internal transcripts. Full worker transcripts remain in workspace
history. Session lookup
uses the active execution context before the legacy host bus. Status is queried
with `workshop.status`; cancellation uses `workshop.cancel` with the worker ID,
not scheduler job cancellation or capability discovery.

After a native Coder peer finishes or is cancelled, the runtime flushes its
memory queue, releases its turn-owned PTYs with confirmed destruction (the host
retains and retries unconfirmed cleanup), and releases its checkout authority
before publishing the terminal result and resuming the host. The host acquires
its own authority; peer edits remain in the attached checkout.

Parallel results are grouped by session and parent turn correlation ID. Legacy
records without that ID use the stream turn ID and do not join modern cohorts.
An exclusive in-flight claim prevents simultaneous intake of the same cohort;
results are marked delivered only after the resumed host produces a terminal
response, input request, checkpoint, or further delegation. Failed or cancelled
intake releases the claim and leaves the results pending. Durable worker jobs
retry pending intake under their existing three-attempt policy without rerunning
completed peer work. Waiting for siblings or an intake already in flight does
not consume retries. Startup reconciliation can also recover pending results.
A process interruption between host delivery and its persisted acknowledgment
can cause redelivery; this is not an exactly-once delivery guarantee.

## Accumulated tool observations

When retained tool-response text exceeds 256 KiB, the runtime compacts eligible
older results toward 128 KiB. The four newest results, errors, tool-call arguments,
assistant messages, and user guidance stay intact. Compaction preserves each
provider call/response pair and runs in batches to avoid rewriting history on
every round. These are observation thresholds, not a hard total-context cap.

Coder stores old observations as evidence before replacing them. Receipts remain
readable with the indicated evidence tool. If storage is unavailable, a result
with no existing reference and no safe query path remains intact. A fresh read
returns current state; it is not equivalent to replaying historical evidence.
Mutations must never be rerun merely to retrieve their output. Authoritative tool
receipts remain unchanged. Coder peers use the same evidence storage path.

Failure guidance addresses missing metadata, stale pointers, unavailable shell
paths, missing worker scope, and unavailable backends without recommending
unrelated delegation. Repeated failures are grouped by the existing perception
governor. Progress guidance calls for new findings, blockers, or changed plans,
so acknowledgments are not required on every tool round. Identical consecutive
progress messages are delivered once within a loop; comparison uses the complete
text so an update with new findings after the same opening is still delivered.


## Runtime snapshots and delta reads

Each native Coder user turn starts with a bounded shared-engineering snapshot
containing an activity `epoch` and `revision`, alongside its repository and memory
entry context. Native Coder peers use the same protocol. During the tool loop,
one small system pointer replaces its predecessor at the transcript tail immediately
before inference, including resumed and text-only rounds. It
contains the latest and oldest retained revisions, latest peer/blocked revisions,
and bounded overlap targets. It never acknowledges events or adds another full
repository/activity frame. Tool receipts and the initial snapshot stay intact.

Use `cognition_coder_context_read` only when announced changes matter:

```json
{
  "intent": "Inspect peer changes before integration",
  "mode": "delta",
  "epoch": "<epoch from snapshot>",
  "since_revision": 42,
  "limit": 16
}
```

Deltas read oldest-first. Continue from `next_since_revision`, preserving the
returned `through_revision` and any `agent_id`, `target` (substring), or `kind`
filters. Events arriving after that boundary belong to a later read. Cursors are
explicit and replayable; reads and other agents' snapshots do not mutate them.
A filtered cursor describes only that filter, not all activity. Pages contain at
most 16 events and 24 KiB of event payload; oversized individual events carry an
engineering pointer for focused retrieval. An empty matching page still advances
through the scanned range.

The store retains 400 events per undertaking. A retention gap, changed epoch, or
revision discontinuity returns `snapshot_required`; it never silently skips the
missing range. Call the same tool with only `mode: "snapshot"` and `intent` to
establish a fresh bounded snapshot and repository observation. The next user turn
also gets a new entry snapshot. Within a turn, ordinary receipts already describe
the agent's own work: routine polling and pointer acknowledgments are unnecessary.
Checkpoint resumes preserve the transcript marker and explicit cursors.

Conflict notices remain in tool admission receipts and the compact pointer;
lease/workspace checks and cancellation gates still enforce authority independently
of model reads. The activity revision tracks governed engineering events, not
arbitrary out-of-band filesystem changes. Request a snapshot or focused file read
when those changes matter; digest preconditions still protect mutations.

## Agent shell output

On Unix, agent-created workshop sessions run a quiet POSIX shell with terminal
echo and canonical line buffering disabled. Pagers and automatic color are
disabled for both one-shot and sustained agent commands. Sessions initialize
the configured Bash/Zsh login environment once to preserve tool and version-manager
PATH entries, then switch to the quiet command shell. Other Unix shells use the
daemon's inherited environment. Home's human-created Terminal sessions still
use the configured login shell.

One-shot commands execute in a child shell with explicit start/completion
boundaries, so multiline scripts, quoted text, syntax errors, and `exit` cannot
corrupt the persistent command shell. Coder can call the command tool directly;
a readiness tool call is not required. The first readiness check can open a shell.
Repeated checks reuse the current turn’s preferred idle shell, including when
multiple checks arrive together. Startup observes retained output from
sequence zero and waits for the shell's readiness marker. Slow login initialization
does not kill the session; cancellation remains owned by the turn.

`cognition_coder_shell_run.wait_ms` is an observation window (default and maximum
15 seconds), not a command lifetime. When it expires, the command keeps running
and returns `ok: true`, `status: "running"`, `completed: false`, and `session_id`.
Call the same tool with that `session_id`, `poll: true`, and no `command` to observe
more output and the eventual exit code. Output cursors and the completion parser
stay in the existing turn-owned shell state, including across connection loss.
Polling never resubmits the command. An uncertain transport result reports
`status: "unknown"`; reconnect by polling before deciding whether to retry work.

An independent command uses another session while the preferred one is busy.
Explicitly targeting a busy session with another command is rejected. Session
tools can send raw `input` or poll an existing command; use
`cognition_shell_session_interrupt` for an explicit interrupt. Turn cancellation
and turn cleanup still interrupt owned sessions before releasing Forge authority.
Nonzero exit codes and recoverable tool failures remain observations for the
agent to act on; three failed batches do not automatically terminate its turn.

The model receives plain text with terminal styling, titles and other control
sequences removed. PTY responses retain at most 32 KiB of text (head and tail,
plus an omission label), set `output_truncated` when text was omitted, and keep
draining to the completion boundary or wait deadline. Reaching the text cap
does not interrupt the command. Exit status is detected independently of the
text cap, and the replay cursor advances through consumed frames. Raw PTY bytes
remain in the existing session stream; this adds no receipt retrieval protocol.
Portable execution uses the same plain-text projection within its host output
limit and preserves that host's truncation flag.

Prefer typed, ranged source reads and digest-fenced `code.write` edits, batching
independent reads where useful. Shell remains available for builds, tests and
commands. A truncated output is incomplete evidence: narrow subsequent reads
instead of replaying the same large output.

## Memory schema compatibility

Persistent Locus startup applies targeted object-field repairs even when the
large graph initialization/backfill is skipped. In particular, older SurrealDB
schemas can retain a strict `semantic_links.*` array-item definition after its
parent is made flexible. Startup explicitly repairs both definitions so typed
link fields such as `confidence` persist with their relation and target. The
surrounding memory table remains schemafull. An existing empty graph is detected
from table metadata, rather than treating a successful empty read as a table.
Restart an updated daemon to apply the repair; forcing a full graph backfill is
not required for this compatibility issue. Pending memory promotions retain
their normal retry path.
