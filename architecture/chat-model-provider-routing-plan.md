# Chat model, provider, runtime, and account routing

Status: accepted for implementation on `codex/provider-oauth-routing`

## Product contract

Chat exposes two independent choices:

1. **Model source** — selected inside the model picker. This includes the model
   provider, account/credential route, and the runtime that can legally use it.
2. **Agent mode** — General or Coder. Coder requires a governed Forge project;
   General does not.

The closed composer shows only the selected model. It does not spend permanent
space on provider, credential, or runtime labels. Opening the model picker shows
that routing information before the model list.

General/Coder is available for Medousa, Codex, and Cursor. The mode changes the
work contract, not the provider. When Coder is selected, the project selector
above chat supplies the governed worktree for external ACP sessions as well as
native Medousa turns.

## Route identity

A model id is not a unique route. The same visible model can be reachable through
OpenAI directly, OpenRouter, or an account-backed agent runtime. Persist and
compare the complete route:

```text
runtime / provider / credential-route / model

medousa / openai / api-key / gpt-5.6-sol
medousa / openai-codex / chatgpt-oauth / gpt-5.6-sol
medousa / openrouter / api-key / openai/gpt-5.6-sol
codex / openai / chatgpt-account / gpt-5.6-sol
cursor / cursor / cursor-account / <advertised-model>
```

The current implementation already persists the Medousa provider/model and the
per-chat external runtime. ACP sessions advertise their available model and
reasoning controls after session creation. The UI composes those existing sources
instead of inventing a second model registry.

## Supported execution routes

| Runtime | Connection | Model discovery | Loop owner | Billing/entitlement | Status |
| --- | --- | --- | --- | --- | --- |
| Medousa | Provider API key or local endpoint | Medousa provider catalog | Medousa | Provider account | Implemented |
| Medousa | ChatGPT OAuth through the Codex Responses transport | Account-entitled Codex model catalog | Medousa | ChatGPT/Codex subscription quota | Planned in this branch |
| Codex | ChatGPT account | ACP session config | Codex | ChatGPT/Codex entitlement | Implemented |
| Cursor | Cursor account | ACP session config | Cursor | Cursor entitlement | Implemented |

Switching source is atomic from the user's perspective. Medousa cancels an old
external ACP session, changes the persisted runtime, creates the new session when
needed, and then exposes the newly advertised model choices.

## ChatGPT account boundary

ChatGPT subscription authentication and OpenAI API authentication are separate
credential and billing systems. A ChatGPT OAuth access token is **not** stored or
sent as an `api.openai.com` API key.

ChatGPT OAuth can nevertheless back a native Medousa turn. In that route,
Medousa sends Responses-shaped requests to the dedicated Codex Responses
transport at `https://chatgpt.com/backend-api/codex/responses`, authenticated by
the ChatGPT OAuth bearer token and account identity required by that transport.
The response may contain tool calls, but Medousa executes those tools and owns
the continuation loop. Selecting this route does not launch an ACP session and
does not transfer loop ownership to Codex.

The Codex route remains account-backed and forces ChatGPT authentication while
removing `OPENAI_API_KEY` and `CODEX_API_KEY` from the spawned ACP process. This
prevents a source switch from silently changing billing routes.

`Medousa + ChatGPT account` is therefore a first-class provider route to
implement, distinct from both the public OpenAI API and the Codex ACP runtime.
Its adapter must own:

- OAuth authorization and refresh-token rotation;
- secure daemon-side credential storage (never localStorage or the webview);
- model/entitlement discovery;
- Responses request/stream adaptation without Codex owning the loop;
- revocation, expiry, and account-switch behavior;
- explicit user-facing billing/entitlement copy.

An OAuth flow that merely creates an API key is still an API-billed convenience
route, not this subscription-backed transport, and must be labeled as such.

Current ecosystem implementations validate this separation. OpenClaw documents
native agent-model access with Codex subscription OAuth separately from its
optional Codex app-server harness, while Pi exposes an `openai-codex` provider
using ChatGPT subscription authentication. Hermes documents the other valid
shape: delegating the loop to Codex app-server. These are transport choices, not
evidence that ChatGPT OAuth must imply Codex owns the loop.

References:

- [OpenClaw OpenAI provider](https://github.com/openclaw/openclaw/blob/main/docs/providers/openai.md)
- [Pi releases](https://github.com/badlogic/pi-mono/releases)
- [Hermes Codex app-server runtime](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/codex-app-server-runtime.md)

## Fit with the current `genai` stack

Medousa already uses `genai` 0.6 through Stasis' `GenaiChatClient`. The underlying
`genai` crate has most of the HTTP/SSE wire adapter we need:

- the `openai_resp` adapter serializes Responses requests, tools, tool outputs,
  reasoning options, and parses both full and SSE responses;
- `ServiceTargetResolver` can replace the normal OpenAI endpoint;
- `AuthResolver` can supply route-specific authentication; and
- `AuthData::RequestOverride` can replace the final request URL and headers for
  an unorthodox authentication scheme.

This is not yet proof that `genai` is the complete production transport. Its
`openai_resp` streaming implementation currently posts an HTTP request and reads
an SSE event stream. Current OpenClaw/Pi implementations describe the Codex
Responses route as WebSocket-first with SSE fallback. Medousa must verify the SSE
path with an entitled account and every supported model. If the live contract
requires WebSocket, keep `genai` for shared Responses types/conversion where
practical and add a WebSocket implementation behind the same provider transport
interface. Do not bend the Medousa agent loop around a transport detail.

The current Stasis wrapper is narrower: it accepts a model and optional base URL,
then resolves credentials from process environment variables. That is suitable
for API keys but not for refreshable, account-scoped OAuth. The native ChatGPT
route therefore needs a small Medousa/Stasis adapter seam that can construct a
`genai::Client` with route-specific resolvers and inject it behind the existing
`AiChatClient` port. Do not place the OAuth token in `OPENAI_API_KEY`,
`STASIS_LLM_API_KEY`, or any other process-global variable.

The preferred boundary is:

```text
Medousa turn orchestrator
  -> AiChatClient
  -> OpenAiCodexChatClient
       -> ChatGPT credential broker (refresh + account id)
       -> CodexResponsesTransport
            -> genai HTTP/SSE adapter, when accepted
            -> native WebSocket adapter, when required
  <- tool calls / text / reasoning / usage
```

Keep OAuth acquisition and durable refresh-token storage outside `genai`.
`genai` is the inference wire adapter; the daemon-side credential broker is the
identity and lifecycle authority.

## UI behavior

The model picker has progressive disclosure:

1. Closed trigger: selected model only.
2. Open panel: current source/connection and source switcher.
3. Model list: models from the selected source.
4. Secondary controls: reasoning/depth remain adjacent to the model control.

Source labels:

- **Medousa** — secondary line names the active provider, such as
  `OpenAI · API key`, `OpenAI · ChatGPT account`, or `Ollama · Local`.
- **Codex** — secondary line is `ChatGPT account · Codex runtime`.
- **Cursor** — secondary line is `Cursor account · Cursor runtime`.

Locked account sources remain visible and lead to Settings → Connections.
Provider configuration remains in Settings → Models/Providers; the picker selects
among configured routes and can link there when setup is required.

## State and lifecycle rules

- Runtime choice remains per chat.
- Native provider/model remains the workshop default until per-chat native model
  overrides are introduced separately.
- ACP model choice remains session-scoped and is restored from advertised config.
- Changing runtime clears stale ACP session/config handles.
- Changing Coder project restarts the external session in the daemon-resolved
  governed worktree.
- Changing General/Coder is independent of runtime.
- A missing Coder project opens the project chooser rather than starting in an
  arbitrary cwd.

## Phased delivery plan

Each phase must leave existing API-key, local, Codex ACP, and Cursor ACP routes
working. A later phase may depend on an earlier phase's types and ports, but no
phase may temporarily route ChatGPT OAuth through `api.openai.com` or process
environment variables.

### Phase 0 — selector and route UX foundation

Status: implemented on this branch; validation complete.

Deliverables:

- Move Medousa/Codex/Cursor source switching into the model picker.
- Keep the closed trigger model-only.
- Show native providers in the expanded picker and filter models progressively.
- Use ACP-advertised model options for Codex/Cursor.
- Keep only non-model ACP controls in the adjacent session controls.
- Make General/Coder visible for every runtime.
- Assign external Coder sessions to the daemon-resolved governed project.

Exit gate:

- Frontend type checking, focused selector tests, production build, and ACP
  routing tests pass.

### Phase 1 — explicit inference route and credential requirements

Status: implemented; validation complete.

Deliverables:

- Add the canonical provider id `openai-codex` for native ChatGPT-account
  inference; do not overload `openai` or the `codex` runtime id.
- Replace the binary `provider_needs_api_key` decision with an explicit
  credential requirement: `none`, `api_key`, or `chatgpt_oauth`.
- Make inference eligibility and fallback telemetry distinguish
  `missing_api_key` from `missing_chatgpt_oauth`.
- Preserve the full provider/model route through main, delegated worker, and
  stage-routing paths.
- Add provider catalog metadata without presenting the route as usable before
  OAuth is connected.

Tests:

- Credential-requirement unit tests cover local, API-key, and ChatGPT OAuth
  providers.
- Eligibility tests prove `openai-codex` never falls back to API-key detection.
- Route tests prove `openai` and `openai-codex` remain distinct for identical
  model ids.
- Existing inference-router and model-route tests remain green.

Exit gate:

- The runtime can represent and reject an unconnected `openai-codex` target with
  the precise `missing_chatgpt_oauth` reason; no OAuth or network calls exist yet.

### Phase 2 — daemon-owned ChatGPT OAuth broker

Status: implemented; daemon broker, remote-safe HTTP contract, and lifecycle
validation complete.

Deliverables:

- Implement daemon-owned PKCE/device authorization suitable for local and remote
  workshops.
- Store access token, refresh token, expiry, and account identity in secure
  daemon-side storage; expose status, begin, complete, refresh, and disconnect
  commands without returning secrets to the webview.
- Deduplicate concurrent refreshes and retry one inference request after an
  authentication-expiry response.
- Keep the existing Codex CLI-owned login intact for the Codex ACP runtime; do
  not parse or copy another application's credential files.

Tests:

- OAuth state/PKCE validation, expiry boundaries, refresh rotation, concurrent
  refresh deduplication, revocation, redaction, and remote-workshop flows.

Exit gate:

- A workshop can connect/disconnect a ChatGPT account and report entitlement
  status without invoking an inference transport or exposing a token to Home.

### Phase 3 — native Codex Responses HTTP/SSE adapter

Status: pending Phase 2.

Deliverables:

- Add `OpenAiCodexChatClient` behind the existing `AiChatClient` port.
- Add a narrow `CodexResponsesTransport` boundary.
- Use `genai`'s `openai_resp` conversion and SSE parser while overriding the
  exact URL and complete required header set from the resolved OAuth credential.
- Preserve text, reasoning summaries/signatures, tool calls, tool outputs,
  usage, cancellation, and error bodies across the adapter.
- Add account-backed model discovery without using the public OpenAI models API.

Tests:

- Golden request tests reject public-API-only or unsupported fields.
- Recorded SSE fixtures cover text, reasoning, multiple tool calls, tool output
  continuation, usage, and structured error responses.
- A live opt-in probe validates the transport against an entitled test account.

Exit gate:

- General and Coder turns complete through Medousa's loop using ChatGPT OAuth,
  or the live probe demonstrates that WebSocket is required and Phase 4 becomes
  mandatory before enabling the route.

### Phase 4 — WebSocket transport when required

Status: conditional on the Phase 3 live contract probe.

Deliverables:

- Implement Codex Responses WebSocket connection setup, `response.create`
  framing, event decoding, cancellation, reconnect boundaries, and keepalive.
- Reuse the Phase 3 response normalization so the Medousa loop cannot distinguish
  HTTP/SSE from WebSocket.
- Prefer `auto` transport selection only after deterministic fallback and error
  classification exist.

Tests:

- Recorded WebSocket event fixtures, disconnect/reconnect boundaries,
  cancellation, authentication expiry, and SSE fallback behavior.

Exit gate:

- Supported account/model combinations select a verified transport without
  changing loop ownership or silently switching billing routes.

### Phase 5 — connection choice and model discovery UX

Status: pending a working native transport.

Deliverables:

- Under Medousa → OpenAI, expose explicit **API key** and **ChatGPT account**
  connections before model selection.
- Rename the external ChatGPT source to **Codex** so runtime ownership is clear;
  keep its secondary label `ChatGPT account · Codex runtime`.
- Show connection, expiry, entitlement, model-unavailable, and reconnect states
  without exposing transport jargon in the closed composer.
- Keep General/Coder and governed-project selection independent of the
  connection route.

Tests:

- Route-selection state tests, locked/reconnect states, accessibility, mobile
  behavior, and no cross-chat/runtime credential leakage.

Exit gate:

- A user can deliberately choose either OpenAI billing or ChatGPT subscription
  quota and can always tell who owns the agent loop before sending.

### Phase 6 — hardening and release

Status: pending Phase 5.

Deliverables:

- Entitlement-aware model refresh, rate-limit reporting, account switching,
  redacted diagnostics, migration from any development credential format, and
  rollback behavior.
- Operator and end-user docs updated from planned to implemented behavior.
- Never scrape the ChatGPT web UI, read another application's credential files,
  or treat the OAuth token as an API key for `api.openai.com`.

Exit gate:

- CI parity passes, release builds pass, secrets do not appear in logs or durable
  turn state, and a manual matrix covers local/remote workshops plus
  General/Coder and API-key/ChatGPT/Codex/Cursor routes.

## Acceptance criteria

- The composer trigger contains the selected model name and no runtime/provider
  badge.
- Opening it identifies the current runtime, provider/account route, and model.
- A user can switch Medousa/Codex/Cursor from that panel.
- A Medousa user can switch configured providers and then select a model.
- Under OpenAI, a Medousa user can choose API key or ChatGPT account without
  changing runtime ownership.
- Codex/Cursor model choices come from ACP session config.
- General/Coder remains available after every source switch.
- Coder cannot send without a governed project.
- Runtime switching cannot leak an OpenAI API key into the ChatGPT/Codex route.
- Native ChatGPT OAuth refresh is daemon-owned, concurrency-safe, and invisible
  to the webview and model transcript.
- Existing native and external turns continue using their current daemon APIs.
