# Tool loop: interim assistant text — fix

## Symptom

During an interactive agent turn (TUI, Telegram ingest, daemon interactive), the model sometimes:

1. Streams a short status update (“Let me check that…”)
2. Then attempts tool calls on the **next** model round

The turn ended after step 1 with no tools run. If the model only called tools and replied at the end, behavior was fine.

## Root cause

`MedousaToolLoopPipeline` in `src/medousa_tool_loop.rs` treated **any** model response with assistant text and **zero** `tool_calls` as a final answer:

```text
tool_calls.is_empty() && maybe_text.is_some() → return Ok (terminate)
```

That is correct **after** tools have run (`invocations` non-empty). It is wrong **before** any tools when the model is still working (status preamble).

A non-streaming retry existed for stream+text+no tools on the **first** round only; it did not cover “text then tools on round 2+” and still finalized if the retry returned text without tools.

## Fix

`should_finalize_on_text_only_response()`:

| Condition | Finalize? |
|-----------|-----------|
| `has_selected_tool` (single-tool legacy path) | No — legacy fallback handles |
| `invocations.len() > 0` (tools already ran) | Yes — final synthesis |
| `rounds_executed >= max_tool_rounds` | Yes — budget exhausted |
| Else (agent mode, no tools yet, rounds remain) | No — append `ChatMessage::assistant(text)` and **continue** the loop |

## Related: recurring agent turns

Scheduled recurring jobs default to `workflow.stasis.prompt` (one LLM call, no Medousa tool registry).

Use `execution_mode: "agent_turn"` on `POST /v1/recurring/prompt` (or register tools with `job_type` / payload) to run `workflow.medousa.recurring_agent_turn`, which calls `run_agent_turn` and the same tool loop per tick.

See [recurring-delivery-roadmap.md](recurring-delivery-roadmap.md) Phase 3.
