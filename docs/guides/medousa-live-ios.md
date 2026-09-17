# Medousa Live on iPhone

Medousa Live is the in-app voice path for a continuous conversation with the
selected Medousa chat. It complements Siri: use Siri for a quick request or to
launch Medousa, and use Live when you want a back-and-forth conversation.

> Medousa Live is currently a preview. Foreground conversation is the first
> supported milestone. Background and locked-screen continuity still require
> device qualification on each supported iOS release.

## Before you start

- Personal runs through the embedded workshop on your iPhone; pairing another
  computer is optional.
- Configure an OpenAI API key on the iPhone. Live's current transport uses this
  local credential, independently of ChatGPT/Codex authentication for chat.
  The key is not exposed to JavaScript or the voice model.
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

In Personal, Live starts with bounded recent saved conversation and identity
context from the embedded daemon, with a voice-specific Medousa prompt. Full
execution policy and tools remain in the daemon. This is not unrestricted
access to your vault or all memories.

Requests needing tools can be handed to the existing Medousa chat. Return to
the chat where Live started before requesting work if you have changed chats
or workshops. Live now trials GPT-Live (`gpt-live-1`) with client delegation:
the existing chat executes tools and returns verified results for spoken
presentation. GPT-Live access must be enabled for the OpenAI project. If startup
fails, **Try legacy voice** explicitly retries the previous Realtime transport;
there is no silent model fallback.

Captions are timestamped fragments, grouped independently for each speaker.
These display groups are not authoritative completed turns. In Personal,
caption snapshots are saved when Live ends, without generating another response.
The legacy transport still saves finalized messages as they arrive. Replayed IDs do
not create duplicate entries. The visible timeline refreshes when no normal chat
turn is streaming; otherwise reopening the conversation shows saved messages.
Live now observes the exact tool-work turn and sends its terminal answer back for
spoken summarization while the original Live connection remains open. Failures
and cancellations are reported as such. After ten minutes without a terminal
result, Live reports pending work rather than success; the work is not cancelled.
Ending Live detaches the voice observer, leaving the normal chat work intact.
This return path still needs on-device qualification, especially in background.
The GPT-Live trial uses a bounded transcript-collection window for delegation;
late transcripts, corrections during running work, and detailed spoken results
still need qualification. Spoken backend output is a short excerpt; full output
stays in chat. Ending Live requests graceful session finalization before media
cleanup, with a five-second timeout if final usage cannot be confirmed.

Tool requests appear as ordinary spoken user messages, not internal routing
envelopes. The Live dock shows working, returning-result, and API-accepted states.
Completion is checked against the daemon's turn record, even after chat removes
the turn from its active map. API acceptance is not confirmation that audio was
heard. A rejected or unacknowledged result is shown explicitly in the dock;
the verified full answer remains in chat. Previously saved routing envelopes
are not automatically removed from conversation history.

Short acknowledgments such as “okay, no worries” do not start another daemon
turn or cancel running work. Genuine delegated follow-up requests are serialized
instead of failing with an active-turn conflict. This is not approval handling
or spoken cancellation: use the existing approval/cancellation controls.
While Live remains connected, **Hear result** re-presents the latest completed
work result without executing tools again. It also works when API acceptance
occurred but the spoken presentation was interrupted or not heard. Acceptance
and audible delivery remain separate; speech interruption recovery still needs
device testing.

Live captures terminal answers from the existing chat stream in a bounded,
workshop/conversation/turn-scoped completion receipt before active-turn cleanup.
This lets result delivery survive history reloads that replace streaming message
IDs with saved transcript-entry IDs. Daemon turn-record polling remains a fallback;
the newest unrelated assistant message is never used as the tool result.
Siri and Shortcuts remain available.

Start Live with **Talk live** on the right of the mobile composer's context row.
During a call, a compact bottom dock shows status, the latest caption, mute,
end, and return-to-conversation controls across mobile tabs.
The call keeps its originating workshop label and conversation. Changing chats
within that workshop does not redirect pending tool results; new tool requests
require returning to the original conversation. Changing workshops ends Live
rather than moving an active call to another daemon.

## Troubleshooting

Live hands questions such as “what have I been up to this week?” or “what did
we decide last time?” to the workshop for a permitted memory/history lookup.
The voice session's recent context is not treated as a complete activity log.
Simply telling Medousa about your week does not require a lookup.

When you end a Live session, its speech is saved as an expandable **Live
transcript** below Medousa's message actions, rather than individual chat bubbles.
Tool-backed speech attaches to that work's answer; voice-only sessions create
one Medousa turn. Captions remain in the Live dock while connected. If the
answer is outside the loaded history page, the transcript stays available as
a standalone transcript until history is reloaded with that answer. This does
not rewrite or remove transcripts saved by earlier versions.

Successful turns do not send a separate completion notification while their
conversation is visible in Medousa. Background completions still notify, and
failed turns still notify even when the conversation is visible. Settled chat
timelines show identical result text and repeated tool-run identities once;
the stored transcript is preserved.

- **The control says Live is unavailable:** use the installed iOS app rather
  than a desktop or browser build.
- **Microphone access is disabled:** enable Medousa in **Settings → Privacy &
  Security → Microphone**.
- **The session cannot start:** confirm the iPhone has an OpenAI API key
  configured and can reach OpenAI. A paired workshop must also be reachable
  for work delegated to that workshop.
- **Audio stops after backgrounding:** return to Medousa. Background WebRTC is
  still being qualified; foreground voice should continue to work.
