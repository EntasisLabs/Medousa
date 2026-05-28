# Medousa Architecture And Flow Guide

## Executive Summary

Medousa is a policy-guided cognitive runtime product built for high-accountability operations.

It combines:

- deterministic orchestration behavior from Stasis
- tool-first evidence acquisition
- verification and confidence signaling
- operator-controlled routing and runtime policy

The result is a platform that remains useful even when tool execution degrades, because policy adherence and reasoning behavior still remain structured and observable.

## 1) System Topology

```mermaid
flowchart LR
    subgraph Channels["Operator Channels"]
        TUI["medousa_tui"]
        CLI["medousa_cli"]
        API["medousa_daemon API"]
    end

    subgraph Product["Medousa Product Runtime"]
        Router["Stage Routing Matrix\n(role -> provider/model/policy)"]
        ToolLoop["Tool Loop Pipeline\n(stream + tool invocations)"]
        Verifier["Verification + Context Pack"]
    end

    subgraph Stasis["Stasis Orchestration Backbone"]
        Jobs["Job Lifecycle\n(enqueue/attempt/outcome)"]
        Scheduler["Recurring Materialization"]
        Outbox["Outbox + Delivery"]
    end

    subgraph Data["Persistence + Evidence"]
        Hist["Session History"]
        Art["Artifact Store + Chunk Refs"]
        Verify["Verification Store"]
    end

    TUI --> Router
    CLI --> Router
    API --> Router

    Router --> ToolLoop
    ToolLoop --> Verifier
    ToolLoop --> Jobs
    Verifier --> Verify

    Jobs --> Scheduler
    Jobs --> Outbox

    ToolLoop --> Hist
    ToolLoop --> Art
    Verifier --> Hist
    Verifier --> Art

    Hist --> TUI
    Art --> TUI
    Verify --> TUI
```

## 2) Prompt-To-Answer Runtime Flow

```mermaid
sequenceDiagram
    autonumber
    participant U as Operator
    participant UI as Medousa TUI
    participant AR as Agent Runtime
    participant TP as Tool Loop Pipeline
    participant LLM as Routed LLM Target
    participant TO as Tool Registry
    participant EV as Verifier/Context Pack
    participant ST as Stores (History/Artifacts/Verification)

    U->>UI: Submit prompt
    UI->>AR: start_prompt_run(prompt)
    AR->>EV: Resolve context pack + verifier policy
    EV-->>AR: Prompt augmentation + verification_state
    AR->>TP: execute_with_stream_prior_messages_max_rounds
    TP->>LLM: Stream completion request

    loop During generation
        LLM-->>TP: Content chunk / Reasoning chunk
        TP-->>AR: StreamDelta
        AR-->>UI: AgentChunk / AgentReasoningChunk
        UI->>UI: Coalesce thinking buffer
    end

    opt Model requests tools
        TP->>TO: invoke_tool(name,input)
        TO-->>TP: tool_output
        TP-->>AR: tool_invocations
        AR-->>UI: ToolPayload + receipts
        UI->>ST: Persist artifacts + chunk refs
    end

    opt Continuation synthesis trigger
        AR->>TP: second synthesis pass
        TP->>LLM: continuation request
        LLM-->>TP: refined final response
    end

    TP-->>AR: final response
    AR-->>UI: AgentResponse
    UI->>UI: merge streamed + final body
    UI->>ST: Append turn + verification metadata
    UI-->>U: Render answer_state badge + response
```

## 3) Answer Trust State Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Drafting: Prompt submitted
    Drafting --> Tooling: Tool call requested
    Tooling --> Drafting: Tool output received
    Drafting --> Continuation: Large evidence detected
    Continuation --> Drafting: Refined synthesis

    Drafting --> Finalizing: AgentResponse received
    Finalizing --> Verified: answer_state=verified
    Finalizing --> Provisional: answer_state=provisional

    Verified --> [*]
    Provisional --> [*]

    Drafting --> Failed: runtime/model error
    Tooling --> Failed: tool failure
    Continuation --> Failed: continuation failure
    Failed --> [*]
```

## 4) Operator Control Plane

Primary control points:

- global provider/model/base-url
- per-stage route target and policy profile
- verifier thresholds
- response depth mode
- tool-call strictness and round limits

Design implications:

- operators can tune speed vs quality by stage
- trust posture can be tightened without rewriting workflows
- fallback behavior remains explicit and reviewable

## 5) Evidence And Governance Model

Governance posture in Medousa centers on inspectability:

- tool invocation payloads are receipted
- large payloads are artifacted and chunk-referenced
- verification reports are persisted and queryable
- answer state is represented as metadata (`verified` or `provisional`)

This gives teams a practical audit trail from user question to produced answer.

## 6) Resilience And Degraded-Mode Behavior

Medousa supports graceful degradation:

- context and prompt budgets cap request size
- continuation synthesis can recompose large evidence sets
- typed reasoning stream handling preserves model thought flow when available
- fallback paths keep the assistant useful when tools fail

In degraded conditions, the expected behavior is a clear uncertainty statement plus actionable recovery guidance, not a silent failure.

## 7) Small-Model Guidance (3B-4B Class)

For constrained models, reliability improves when prompts remain explicit and bounded.

Recommended settings:

- strict or analytical policy on verifier stage
- moderate max tool rounds (avoid deep recursion)
- concise response depth for low-latency first pass
- stage-specific routing where synthesis uses a stronger model if available

Prompting guidance:

- keep task objective and output format concrete
- separate facts from hypotheses explicitly
- ask for structured takeaways with uncertainty notes
- force tool-grounding language for real-world claims

## 8) Enterprise Rollout Checklist

1. Define stage routing baseline for each environment.
2. Set verifier thresholds aligned to risk tolerance.
3. Validate artifact and verification retention policies.
4. Exercise failure scenarios: missing keys, tool parse errors, partial outages.
5. Establish operator runbooks for routing sync and degraded-mode response.
6. Track quality metrics: verified ratio, tool failure rate, continuation hit rate.

## 9) Key Implementation Anchors

- `medousa/src/bin/medousa_tui/agent_runtime.rs`
- `medousa/src/bin/medousa_tui/event_reducer.rs`
- `medousa/src/bin/medousa_tui/settings_ui.rs`
- `medousa/src/tools.rs`
- `medousa/src/stage_routing.rs`
- `medousa/src/verification_store.rs`
- `medousa/src/context_pack.rs`