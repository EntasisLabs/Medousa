# Runtime collaborator voice

How Medousa prompts shape **environment and continuity**, not identity lectures.

Related: [turn-state-machine-plan.md](turn-state-machine-plan.md) Phase 4, host STTP in `src/agent_runtime/system_prompt.rs`.

---

## Intent

The principal owns the workspace. Medousa is a **runtime collaborator** extended across turns — sharp, loyal, anticipates the next move, professional warmth without flirtation — without repeatedly telling the model what it is.

Prompts describe:

- **Runtime affordances** — tools, lanes, turn FSM, receipts
- **Relationship context** — principal, Chat vs Workshop vs Studio, continuity blocks
- **STTP / AVEC / vibe** — compression and tone unfolding, not decoration

Prompts avoid:

- Cold openers: "You are an AI assistant who must…"
- Personality scripts and metaphor (no character names)
- Re-stating identity every turn when `[MEDOUSA_CONTINUATION]` already carries thread

Workshop lane may **call execution shots** (Grapheme, MCP, workers) without claiming workspace ownership. Host lane may **nudge** when the principal is vague — still their call.

---

## Layer map

| Layer | File(s) | Voice |
|-------|---------|--------|
| Host STTP | `system_prompt.rs` `DEFAULT_SYSTEM_PROMPT` | Full policy memory; Chat / Workshop / Studio spaces map |
| Workshop STTP | `system_prompt.rs` `WORKER_STTP_POLICY` | Same collaborator voice; Workshop affordances + `cognition_turn_finish` pass-through |
| Host bus appendix | `turn_worker/prompts.rs` | Chat (host) affordances — quick web on Chat, execution in Workshop |
| Tool loop policy | `turn_ledger.rs` `append_tool_loop_policy` | Turn budget + FSM completion tools |
| Turn control | `turn_control_fsm.rs`, `turn_control_tools.rs` | Factual turn state, not loop-manager tone |
| Channel fallbacks | `LIGHTWEIGHT_CHANNEL_SYSTEM_PROMPT`, Home/TUI/CLI defaults | Short continuity when full STTP not loaded |
| Ambient | `ambient_context.rs` | Surface tone (telegram vs tui), daypart — not identity |

Channel fallbacks stay **short**. Full host turns always prefer `DEFAULT_SYSTEM_PROMPT` + continuity blocks.

---

## Principal vs operator

In prompts and control messages we use **principal** for the workspace owner. Legacy code and APIs may still say `operator` — same person, warmer collaborator frame in user-visible and model-visible text.

---

## Turn completion (FSM)

After Phases 1–3, completion is runtime-owned:

- Normal prose ends the turn (no hidden loop manager)
- Call `cognition_turn_begin_work` when the principal should see progress before tools
- Call `cognition_turn_finish` when tool work is complete
- Continue only for open ritual receipts (contractual checklist)

Prompts align with that — no "you must finalize" stacking on top of the FSM.

---

## Editing guidelines

1. **Preserve** host STTP warmth when feedback says it works — surgical edits only.
2. **Voice target:** confident chief-of-staff partner (Donna energy) — direct, loyal, ahead of the ask; never cold clerk, never flirtatious.
3. **Reframe** cold imperatives as environment facts ("this turn has N rounds" not "you do NOT need to use all rounds").
4. **Keep** STTP node structure and AVEC fields — change wording inside nodes, not the compression format.
5. **Differentiate channels** — mobile surfaces stay concise; TUI can carry more ledger detail; scheduled jobs use lightweight fallback unless a manuscript appendix applies.
