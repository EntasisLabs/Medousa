# ADR-021 — Agent messages and results return to the addressed session

**Status:** Accepted on 2026-11-20. Implemented.

## Context

A human asks agent A to do something. A sends work to B and C. Today, the way A launches a worker determines how A can reach it while it runs and what happens when it finishes. For example, the ordinary inbox rejects a delegated Coder session, and the local handoff handler constructs a new owner turn after it already has the worker's result. This allowed a completion to switch A's model, introduce reviewer instructions, and spend tokens acknowledging an event the runtime had already recorded.

## Decision

**Agents communicate by addressing a session.** A subagent and a standalone agent use the same messaging and execution rules. Spawning determines which session runs the work. A request records the originating workshop and session, the destination workshop and session, and a correlation ID so its result can find the sender even after a restart or a switch to another chat.

| Event | Runtime behavior |
|---|---|
| Human messages A | A handles the message in its session with its selected model, mode, context, and tools. |
| A sends work to B or C | The send tool returns acceptance and the request ID once the runtime has accepted the work, or reports the failure. This tool result is the acceptance acknowledgment. It costs zero additional model calls. |
| A messages B while B is working | B receives the message in its existing session at the next safe point in its tool loop. This applies to Coder too. B keeps its current work. A can clarify an instruction without opening another assignment. |
| B or C finishes, fails, or is cancelled | The runtime returns the outcome and result to A, correlated to that worker's request. Returning the finished turn is automatic. A retry, duplicate delivery, or restart produces one observable terminal result per request. |
| A receives a terminal result | If A is running, its loop consumes the result. If A yielded, the runtime resumes A's existing continuation. B and C may finish in either order. Each result remains attributable to its own request. |

**Delivery costs zero inference.** A can spend a normal model round deciding what to do with a result. That round continues the original session with its model, reasoning settings, mode, conversation context, and tool access. A changed picker default cannot redirect pending work. The result is an event in that conversation. Receipt alone must never create a replacement owner session, inject a callback prompt, replay the assignment as a fresh human request, narrow A's tools, or launch an independent reviewer. A separate review is work that the human requested or the agreed workflow already includes.

### Across daemons

Local and remote sends have the same acceptance, follow-up, and completion semantics. The destination daemon authenticates the pairing and enforces tool permissions at execution time. Sending a message conveys the message and its provenance; it grants no additional permissions.

| Pairing role | Tools available to the remote agent |
|---|---|
| Worker | The tools the destination daemon has configured for the worker role. |
| Portal | The tools the destination daemon has configured for the portal role. |
| Peer | The granular grants configured for that specific pairing, within the destination daemon's policy. |

Revocation takes effect when the agent next tries to use the revoked authority. Forge continues to enforce project and edit permissions independently of how a message reaches a Coder.

## Consequences for implementation

Use Stasis agent envelopes, correlated waits, and terminal results as the common delivery mechanism. Medousa owns session addressing, pairing policy, and the connection to the session's execution loop. Route ordinary messages, in-flight worker guidance, and Coder follow-ups through that same session address. The handoff completion handler should settle the request and deliver its result to A. Reconstructing a fresh Assistant turn from saved settings is an interim workaround and does not satisfy this decision.

## Acceptance scenario

1. A starts B locally and C on a paired daemon. Both sends return accepted request IDs. Count **zero model calls** to acknowledge acceptance.
2. While B is working as Coder, A sends a clarification to B's session. B receives it before finishing and keeps the same assignment.
3. Change the default model and open a different chat. Let C finish before B. A receives both terminal results in its original session and continues with its original model and tools. Exercise both cases: A is running when a result arrives, and A has yielded.
4. Repeat with a worker failure, a cancellation, and a duplicate terminal event across a restart. Each accepted request has one observable terminal outcome. Count **zero replacement owner sessions, zero delivery-only inference calls, and zero unsolicited reviewer launches**.
5. Run the remote case under worker, portal, and peer pairings. Verify an allowed and a denied tool call against each destination policy, including a grant revoked during the work.

## Verification

Run the [two-daemon A/B/C test](../../../scripts/ci/test-agent-session-messaging.py) on macOS with a built daemon:

```sh
python3 scripts/ci/test-agent-session-messaging.py --daemon /path/to/medousa_daemon
```

The test uses isolated data directories, loopback model endpoints, and real pairing, Forge, and Stasis execution. The worker connection negotiates portal authority. The peer case uses a separate native peer pairing and changes its destination grant during a Coder assignment. The restart case replays an already-recorded signed terminal result and compares transcript entry IDs, digests, and inference counts.

## Code anchors

- [Stasis delegation and wait correlation](../../../src/delegation.rs)
- [Native session messaging and Coder addressing](../../../src/agent_messaging.rs)
- [Worker guidance](../../../src/agent_runtime/turn_worker_tools.rs)
- [Local handoff completion](../../../src/daemon/coordination/handoff_wake.rs) and [owner result intake](../../../src/daemon/coordination/owner_intake.rs)
- [Paired-workshop result delivery](../../engine/coordination.md#returning-a-paired-workshop-result)
