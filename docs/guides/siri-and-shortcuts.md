# Siri and Shortcuts

On iPhone, Siri can send a request to Medousa and continue the selected chat
without bringing Medousa to the foreground. The turn uses that chat's current
workshop, model, reasoning, identity, browser, and world settings.

## Ask Medousa

1. Open Medousa once and select the chat and workshop you want to continue.
2. Say **“Siri, talk to Medousa.”**
3. When Siri asks what you want to ask, dictate the request.

Medousa wakes if necessary and adds the request to the selected chat. When a
short answer finishes within about 20 seconds, the answer returns in a Siri card
and Medousa reads it with the installed iOS system voice. Longer
work continues in Medousa instead.

Siri may require you to unlock the phone before it runs Medousa or reveals a
response. Personal receives a scoped, OS-managed background execution lease for
the turn. Medousa stores the selected workshop and session context in its shared
app container and limits the text returned to Siri.

## Shortcuts and the Action button

The **Ask Medousa** action also appears in Shortcuts. You can put it in a
personal shortcut or assign that shortcut to the Action button. Leave the
workshop parameter unset to use the active workshop, or choose a specific one.

If Medousa cannot safely start the turn, it leaves the request in the composer
for review instead of silently discarding it.
