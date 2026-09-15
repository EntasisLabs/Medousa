# Siri and Shortcuts

On iPhone, Siri can send a request to Medousa and continue the selected chat
without bringing Medousa to the foreground. The turn uses that chat's current
workshop, model, reasoning, and identity settings.

## Ask Medousa

1. Open Medousa once and select the chat and workshop you want to continue.
2. Say **“Siri, talk to Medousa.”**
3. When Siri asks what you want to ask, dictate the request.

Medousa wakes if necessary and adds the request to the selected chat. When a
short answer finishes within about 12 seconds, the answer returns in a Siri card
and Medousa reads it with the installed iOS system voice. Longer work returns a
continuing card while Medousa uses the remainder of its bounded iOS background
window to finish tools and post a completion notification.

On iPhone, open **Settings → Preferences → Siri** to choose when Medousa speaks,
limit spoken-answer length, pin the current chat, or select a faster model for
requests sent to a Shared workshop. **Auto** speaks through the phone while
leaving voice-only routes such as headphones to Siri, avoiding duplicate audio.

Siri may require you to unlock the phone before it runs Medousa or reveals a
response. Personal receives a scoped, OS-managed background execution lease for
the turn. Medousa stores the selected workshop and session context in its shared
app container and limits the text returned to Siri.

Siri turns can use tools that run without an on-screen interface. Tools that
need a browser host, approval, secret, or other interactive UI pause safely;
Siri asks you to open Medousa, where the selected chat keeps the request and
can finish the interaction. While that chat already has an active turn, another
Siri request reports that Medousa is still working instead of starting a
conflicting turn. Personal tool work also has a device-safe execution deadline:
if iOS suspends it before completion, Medousa releases the Siri-owned turn so
the chat cannot remain stuck. Open the app to continue work that needs longer,
or use a Shared workshop for work that must run independently of the phone.

## Shortcuts and the Action button

The **Ask Medousa** action also appears in Shortcuts. You can put it in a
personal shortcut or assign that shortcut to the Action button. Leave the
workshop parameter unset to use the active workshop, or choose a specific one.

If Medousa cannot safely start the turn, it leaves the request in the composer
for review instead of silently discarding it.

## Focused actions

Medousa also provides Siri and Shortcuts actions for checking what needs your
attention, summarizing active work, checking running work, and capturing an
entry in your journal. They use the same selected or pinned chat and background
execution rules as **Ask Medousa**.

If a Personal response takes longer than Siri's result window, Medousa reports
that it is continuing and records a durable completion watch. Medousa posts a
local completion notification when the background process finishes, or
reconciles the watch on the next OS wake if iOS terminated the process. Shared
workshop hosts can continue work independently of the phone's process lifetime.
