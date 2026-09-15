# Siri and Shortcuts

On iPhone, Siri can send a request to Medousa and continue the chat you already
have selected. The turn uses that chat's current workshop, model, reasoning,
identity, browser, and world settings.

## Ask Medousa

1. Open Medousa once and select the chat and workshop you want to continue.
2. Say **“Siri, talk to Medousa.”**
3. When Siri asks what you want to ask, dictate the request.

Medousa wakes if necessary and adds the request to the selected chat. When a
short answer finishes within about 20 seconds, the answer returns to Siri;
whether it is spoken or shown depends on your Siri response settings. Longer
work continues in Medousa instead.

Siri may require you to unlock the phone before it opens Medousa or reveals a
response. Request text is not placed in the app-opening URL. Medousa transfers
it through a short-lived, one-time receipt and limits the text returned to Siri.

## Shortcuts and the Action button

The **Ask Medousa** action also appears in Shortcuts. You can put it in a
personal shortcut or assign that shortcut to the Action button. Leave the
workshop parameter unset to use the active workshop, or choose a specific one.

If Medousa cannot safely start the turn, it leaves the request in the composer
for review instead of silently discarding it.
