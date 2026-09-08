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

In Coder, use your own tools directly to work and `turn.update_user` for progress.
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
