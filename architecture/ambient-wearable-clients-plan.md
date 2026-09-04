# Ambient wearable clients — watchOS & Meta Glasses

> **Status:** Exploration / living plan  
> **Date:** 2026-08-23  
> **Owner:** Medousa platform  
> **Related:** [ADR-012 Anywhere surfaces](../docs/architecture/decisions/adr-012-medousa-anywhere-surfaces.md), [ADR-021 ambient sensor surfaces](../docs/architecture/decisions/adr-021-ambient-sensor-surfaces.md), [medousa-anywhere-plan.md](medousa-anywhere-plan.md), [media-and-attachments-plan.md](media-and-attachments-plan.md), [iroh-p2p-pairing-plan.md](iroh-p2p-pairing-plan.md), [desktop-companion-plan.md](desktop-companion-plan.md)

## Product intent

Watch and glasses are **not** second brains and **not** thin copies of Home.
They are **ambient sensor + glance surfaces**: capture what the user is seeing
or saying, ship that context to an already-paired remote workshop daemon, and
return a short answer the device can speak or show.

Target interactions:

| Utterance / gesture | Device | What Medousa receives |
|---------------------|--------|------------------------|
| “Medousa, what am I looking at?” | Meta glasses | One camera frame + prompt |
| “Hey, what is this?” | Glasses (tap / voice) | Frame + short prompt |
| Raise wrist → speak → hear reply | Apple Watch | STT text (and optional glance status) |
| Complication / Live Activity style glance | Watch | Session/turn status only |

All inference, vault, packages, tools, and session authority stay on the
**remote workshop daemon**. The wearable (or the phone that brokers it) only
authenticates, uploads media, starts a turn, and presents a bounded reply.

This matches the Home-first / phone-portal model: the device is a window and a
sensor, never a co-located filesystem authority (`workshop.kind !== "local"`).

## Taxonomy (proposed)

ADR-012 today has two Anywhere families:

1. **Native host surfaces** — VS Code, Neovim, Obsidian  
2. **External-agent adapters** — Notion, Slack, …

Wearables fit neither cleanly. Propose a third family:

3. **Ambient sensor surfaces** — watchOS, Meta Glasses (phone-mediated), future
   similar devices

| Concern | Ambient sensor surface |
|---------|------------------------|
| Daemon | Always remote (paired workshop) |
| UI | Host-native glance / voice / short text — not ChatPanel |
| Context | Camera frame, mic utterance, coarse location/time — not vault/fs |
| Tools | No Packages, no Forge, no browser host; optional later client tools |
| Handoff | “Open in Medousa” → Home with same session |

See [ADR-021](../docs/architecture/decisions/adr-021-ambient-sensor-surfaces.md).

## What already exists (reuse first)

The daemon path for the hero features is largely already there:

```text
capture bytes
  → POST /v1/media/upload?session_id=…
  → POST /v1/interactive/turn  { prompt, media_refs, surface, host_context? }
  → SSE  /v1/interactive/turn/{id}/stream
```

| Need | Existing foundation | Gap for wearables |
|------|---------------------|-------------------|
| Vision turn | `media_refs` + `media_vision.rs` (images → multimodal) | Device capture + upload client |
| Voice → text | `POST /v1/stt/transcribe` (Home dictation) | Watch/glasses mic capture → same endpoint |
| Surface tagging | `TurnSurfaceContext.channel_surface` (`home-ios`, `browser`, …) | Add `home-watchos`, `meta-glasses` |
| Host provenance | `HostTurnContext.source` (editor/note/browser shaped) | Document wearable `source` values; optional sensor fields later |
| Pairing / trust | Phone QR + Iroh (`portal` / `paired`) | Watch credential share from phone; glasses via phone DAT |
| Thin TS client | `@medousa/client` | Watch is Swift; glasses path is native on phone — need Swift HTTP helper or thin Rust/FFI, not the TS package |
| Glance UX precedent | iOS Live Activity / home widget | Watch complications; glasses audio/display (DAT) |
| Capability degrade | Mobile read-only charter, no Packages | Formal “ambient” capability profile |

**Non-goals for v1**

- Running `medousa_daemon` on watch or glasses  
- Full chat history UI, Library, Forge, Packages, canvas, browser host  
- True multimodal **audio** in `media_refs` (MIME allowlist is image/doc today; keep STT-first)  
- Continuous always-on camera streaming into the model (capture-on-ask only)  
- Independent glasses networking that bypasses the phone (Meta DAT is phone-brokered)

## Platform realities

### Meta Glasses (Ray-Ban Meta / Oakley Meta / Display)

Meta’s **Wearables Device Access Toolkit (DAT)** integrates into an **iOS or
Android mobile app**, not into a standalone glasses process:

- App starts a `DeviceSession`; user confirms permissions in Meta AI.
- **Camera:** stream or `capturePhoto` → JPEG/frame bytes land in the phone app.
- **Mic / speaker:** Bluetooth HFP; audio is ~8 kHz mono into the phone app.
- Only one DAT session at a time; wear/hinge events pause/stop the session.
- Publishing to the public store is gated (preview → limited GA); treat early
  work as prototype / TestFlight-class.

**Implication for Medousa:** Meta Glasses are a **sensor peripheral of Home
iOS (later Android)**, not a third workshop connection target. The phone already
holds pairing credentials to the workshop daemon; glasses only feed capture into
that portal.

Recommended placement:

```text
apps/medousa-home (iOS)
  └─ native DAT bridge (Swift/Kotlin plugin or Tauri plugin)
       capture photo / short utterance
         → existing media_upload + interactive turn
         → TTS or short on-phone reply; Display glasses later
```

Tauri iOS already hosts the mobile shell; DAT is native SDK work beside (or
under) the Tauri layer — same pattern as Live Activity / WidgetKit bridges.

### watchOS

Apple Watch constraints that drive design:

- Prefer **URLSession** HTTP(S). Low-level sockets / unrestricted WebSockets are
  blocked except special audio/VoIP cases ([TN3135](https://developer.apple.com/documentation/technotes/tn3135-low-level-networking-on-watchos)).
- SSE over URLSession streaming is the right streaming shape if we stream at all;
  alternatively **POST turn → poll session history / short blocking ask** for
  wrist-sized latency budgets.
- Background work is aggressive; use background `URLSession` so uploads finish
  when the wrist drops.
- Independent watch apps can talk to servers directly; **Watch Connectivity**
  remains useful to mirror pairing secrets from Medousa iOS, not as the workshop
  transport.
- Mic: short recording → STT (reuse daemon) → text prompt. System dictation is
  an acceptable v0 if raw capture is painful.

**Implication:** ship a **native watchOS companion target** next to Home iOS
(not a Svelte/Tauri port). UI = raise-to-speak, one-line reply, complication
status, “Open on iPhone”.

## End-to-end shapes

### A. Glasses “what am I looking at?”

```text
User: double-tap / voice wake on glasses
  → Home iOS DAT session active (or started)
  → capturePhoto → JPEG
  → POST /v1/media/upload (workshop bearer from phone pairing)
  → POST /v1/interactive/turn
       prompt: "What am I looking at?"
       media_refs: [{ media_id, kind: "image", mime }]
       surface: { channel_surface: "meta-glasses", supports_*: false }
       host_context: { source: "meta-glasses", resource_kind: "camera_frame", … }
  → stream or await final text
  → speak reply via glasses HFP / show on Display if available
  → same session remains open in Home for follow-up
```

Requires a **vision-capable inference profile** on the workshop (already a
Home Settings concern).

### B. Watch quick ask

```text
User raises wrist → records audio
  → POST /v1/stt/transcribe  OR  on-device dictation
  → POST /v1/interactive/turn { prompt, surface: home-watchos }
  → short reply on watch + optional haptic
  → complication shows “thinking / done”
```

Optional later: attach a phone-camera or glasses frame when both devices are
available (Watch Connectivity / shared session id) — not required for v1.

## Work breakdown

### Phase 0 — Contract & product lock (small, no device SDKs)

1. Accept ADR-021 ambient sensor surfaces.  
2. Reserve `channel_surface` values: `home-watchos`, `meta-glasses`.  
3. Document wearable `host_context.source` conventions (`meta-glasses`,
   `watchos`) without expanding the type yet.  
4. Define ambient capability profile: no UI artifacts, no liquid markdown, no
   browser host, no Packages/charter writes; handoff deeplink `medousa://…`.  
5. Spike matrix: vision model required for glasses; STT profile required for
   watch voice.

### Phase 1 — Phone-mediated glasses MVP (highest leverage)

Depends on Medousa Home **iOS** (already first-class).

1. Native DAT integration spike in Home iOS (mock device first).  
2. “Capture for Medousa” action: photo → `media_upload` → turn with fixed or
   spoken prompt.  
3. Settings: enable glasses session, permission deep-link to Meta AI, workshop
   must already be paired.  
4. Reply path: on-phone first; HFP speaker second.  
5. Privacy copy: frames uploaded to **user’s workshop**, retention = media store
   policy; no continuous recording in v1.

### Phase 2 — watchOS companion MVP

1. Xcode watchOS target alongside Home iOS (SwiftUI).  
2. Credential bootstrap from iOS via Watch Connectivity / App Group / Keychain
   sync — **do not** invent a second pairing QR on the watch.  
3. HTTP client for health, STT, interactive turn; prefer short turns + history
   poll if SSE is flaky on wrist.  
4. UI: dictate → reply → open-on-phone.  
5. Complication / WidgetKit status from existing workspace card patterns
   (`liveActivity.ts` / `homeWidget.ts` as behavioral precedent).

### Phase 3 — Polish & shared ambient kit

1. Shared **Swift** workshop client (or thin generated OpenAPI Swift) used by
   watch + any native iOS sensor bridges — keep `@medousa/client` for JS hosts.  
2. Optional ambient client-tool registration (e.g. `glasses_capture_frame`) so
   the *model* can request a fresh frame mid-turn while a DAT session is live —
   mirrors browser `registerClient` / `external_read` tools.  
3. Android DAT parity once Home Android is first-class.  
4. Display-glasses web/HTML glance only if product wants on-lens chrome.

## Daemon / SDK changes (expected)

| Change | Size | Notes |
|--------|------|-------|
| `channel_surface` docs + allowlists / presentation | Small | Stringly today; document new tags |
| Ambient capability defaults when surface is watch/glasses | Small | Force `supports_*` false server-side if desired |
| `HostTurnContext` wearable fields | Optional | Prefer reuse (`source`, `resource_kind=camera_frame`) before new structs |
| Media MIME / size for glasses JPEG | Likely none | Already image allowlist |
| Audio in `media_refs` | Out of scope v1 | Stay on STT |
| Pairing protocol | None for glasses; watch reuses phone creds | |
| Push / complication feed | Later | Could reuse Live Activity card projection |

Most of the work is **client and UX**, not a new daemon product surface.

## Trust, privacy, and product copy

- Workshop remains the authority and the place data lands (`medousa/media/`).  
- Glasses/watch never become `kind: local`.  
- Explicit capture gestures only; no silent always-on vision.  
- Settings → Connection still owns which workshop is active; wearables follow
  the phone’s active workshop.  
- User-facing name stays **Medousa** (not “Medousa Watch”).

## Open questions

1. **Wake phrase:** system Meta AI vs in-app DAT session only — product/legal
   constraint; may require “open Medousa session on phone first”.  
2. **Session continuity:** always append to the phone’s active Home session vs
   dedicated `ambient` session per device. Recommendation: **same active session**
   so “what am I looking at?” continues in Home.  
3. **Offline workshop:** if phone loses Iroh/LAN, wearables fail closed with a
   clear glance error (same as phone portal).  
4. **Android Home maturity:** glasses Android DAT waits on Home Android; iOS
   first is consistent with current platform matrix.  
5. **Meta publishing gate:** keep Phase 1 behind TestFlight / org testers until
   Meta GA allows public DAT apps.

## Suggested first spike (engineering)

1. From a paired Home iOS build (or curl with phone bearer): upload a JPEG and
   run “What am I looking at?” through interactive turn + vision profile —
   proves daemon path with zero DAT.  
2. DAT mock device → same upload path.  
3. watchOS Hello: URLSession health + one turn with hardcoded bearer from
   Watch Connectivity.

Stop after spike notes; do not land production UI until ADR-021 is accepted.

## Code anchors

| Path | Role |
|------|------|
| `apps/medousa-home/` | Phone portal + future DAT / watch companion host |
| `apps/medousa-home/src/lib/liveActivity.ts` | Glance pattern |
| `apps/medousa-home/src/lib/utils/chatMediaUpload.ts` | Media attach → upload |
| `apps/medousa-home/src/lib/utils/composerStt.ts` | STT turn prep |
| `packages/medousa-client/` | TS thin client (JS hosts only) |
| `src/media_handlers.rs` / `media_vision.rs` / `stt_handlers.rs` | Upload, vision, STT |
| `crates/medousa-types/src/daemon_api.rs` | `InteractiveTurnRequest`, `TurnSurfaceContext` |
| `crates/medousa-types/src/turn.rs` | `HostTurnContext` |
| `docs/guides/phone-pairing.md` | Pairing trust model to extend |
