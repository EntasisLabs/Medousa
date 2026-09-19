# Medousa Live in CarPlay (development preview)

The first CarPlay slice is a native companion for the existing iPhone Live
session, not a second assistant. It shows fixed listening, working, and muted
states and offers Start Live, Mute / Unmute, and End Live controls. Starting
uses the current chat in the selected workshop. Tool handoffs and spoken
results continue through the same Live session.

No transcript, chat answer, tool output, or model-generated imagery appears
on the car screen. Disconnecting CarPlay leaves the phone conversation running.
The native audio owner uses a non-mixing play-and-record session in default
mode while CarPlay is connected, without forcing the phone speaker.

## Current limits

This is not yet a qualified driving feature. Set up the phone while parked.
The phone webview still owns the WebRTC transport and tool-result observer.
CarPlay does not bootstrap that runtime independently or guarantee that it
will keep running when iOS suspends the webview. Control availability requires
a recent heartbeat from the phone runtime. Stale controls are disabled and
show “Finish setup on iPhone while parked”; there is no claim that a queued
tap has successfully muted audio or ended a session before the runtime acts.

Controls are bound to the current workshop/chat identity. Delayed taps are
discarded after a workshop/chat change, while the runtime is busy, or when
the heartbeat has expired. A control tap does not grant new tool permissions.

## Developer setup

Xcode 27 includes the required CarPlay framework in its iOS SDK. Apple's
[additional Xcode tools](https://developer.apple.com/download/all/) include
the separate CarPlay Simulator app. The installed iOS Simulator runtime alone
does not imply that this separate app is installed.

Enable scene registration for an experimental build:

```bash
cd apps/medousa-home
MEDOUSA_CARPLAY=1 npm run ios:prepare
```

Keep `MEDOUSA_CARPLAY=1` set for the subsequent iOS build, which runs preparation
again. Normal preparation removes the experimental scene configuration.
The source scene configuration is `src-tauri/ios-carplay/scene.json` and the
native delegate is linked through the existing Swift archive, not compiled
twice in the Xcode app target.

Scene registration alone is insufficient: Apple must approve the voice-based
conversational CarPlay entitlement and it must be present in the app's signed
entitlements and provisioning profile. This slice does not alter the normal
phone entitlements or request approval on your behalf. Follow the current
[CarPlay Developer Guide and entitlement request](https://developer.apple.com/carplay/).

## Qualification still required

- CarPlay template rendering and button interaction in the simulator.
- Existing phone Live session attachment, mute/unmute, and ending voice.
- Cold CarPlay launch with no phone scene, permission onboarding, and unlock.
- Vehicle microphone/speaker routing, calls, Siri interruptions, and reconnection.
- Background/suspension behavior and tool-result voice continuation.

Independent cold-start and dependable background control need further native
transport/lifecycle work before this can be advertised as production support.

The native archive now includes an opt-in primary WebSocket transport,
scene-independent lifecycle state, and a native audio graph that converts the
current microphone route to mono 24 kHz PCM16 and plays returned PCM without the
webview. Local mute gates capture immediately and also sends the matching Live
mute/unmute command. It follows the
[GPT-Live WebSocket contract](https://developers.openai.com/api/docs/guides/voice-websockets?api=live):
confirmed startup before PCM input, bounded ordered sends, immediate local input
mute, and explicit finalization. It does not change the existing phone WebRTC
path by default or enable independent CarPlay startup yet. Tool-result
continuation remains a prerequisite before that native audio graph can replace
the qualified phone WebRTC path.

The trusted bootstrap is now implemented as an opt-in native command for the
Personal workshop. Rust reads the iPhone's Keychain-backed OpenAI credential,
builds bounded identity/history instructions from the selected daemon session,
and passes both directly to Swift memory through the native bridge. The secret is
not returned to JavaScript, CarPlay, app-group defaults, Live Activity state, or
logs. A developer-only **Native Live transport** preference can opt Talk Live
into this path. It defaults off and falls back to the qualified WebRTC path when
native startup fails; tool handoffs remain on WebRTC during qualification.
No new key store or managed agent backend is introduced.

The native preview can now drain bounded delegation/transcript events into the
same foreground Medousa coordinator used by WebRTC and return verified progress
or commentary through a narrow allowlist. This qualifies conversation continuity
without creating a second tool runtime. It still depends on the app webview being
active for tool dispatch; background-independent daemon sideband ownership is not
yet claimed.

Run the scene-independent native lifecycle tests on macOS with Xcode installed:

```bash
bash scripts/ci/test-ios-live-lifecycle.sh
```
