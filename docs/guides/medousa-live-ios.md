# Medousa Live on iPhone

Medousa Live is the in-app voice path for a continuous conversation with the
selected Medousa chat. It complements Siri: use Siri for a quick request or to
launch Medousa, and use Live when you want a back-and-forth conversation.

> Medousa Live is currently a preview. Foreground conversation is the first
> supported milestone. Background and locked-screen continuity still require
> device qualification on each supported iOS release.

## Before you start

- Pair the iPhone with a reachable workshop.
- Configure an OpenAI API key on that workshop. The key stays with the workshop
  and is never returned to the iPhone.
- Allow microphone access when iOS asks.
- Open the chat that should receive the Live conversation.

## Start a conversation

1. Open **Chat** in Medousa on iPhone.
2. Tap **Talk live** below the top bar.
3. Wait for the control to say **Listening**.
4. Speak normally. The control changes between listening, thinking, and
   speaking as the conversation progresses.

Use the microphone button to mute or unmute. Use the red phone button to end
the Live session and release the microphone.

## What is different from Siri?

Siri owns a short invocation window and cannot be resumed later by an app after
long background work. Medousa Live owns its audio session while the user has
explicitly started a conversation, so it can provide barge-in, continuing
speech, and visible Live Activity state without pretending a completed
background job can reopen Siri.

Requests that need tools or durable work will be delegated to the selected
Medousa session as the trusted sideband path is completed. Siri and Shortcuts
remain available as a production fallback.

## Troubleshooting

- **The control says Live is unavailable:** use the installed iOS app rather
  than a desktop or browser build.
- **Microphone access is disabled:** enable Medousa in **Settings → Privacy &
  Security → Microphone**.
- **The session cannot start:** confirm the selected workshop is reachable and
  has an OpenAI API key configured.
- **Audio stops after backgrounding:** return to Medousa. Background WebRTC is
  still being qualified; foreground voice should continue to work.
