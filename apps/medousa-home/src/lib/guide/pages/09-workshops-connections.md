# Workshops and connections

A **workshop** is a named connection to an engine — local on this Mac, or a paired portal elsewhere. Home always has one active workshop; switching changes vault, sessions, and tools under your feet.

## Your workshops

In Settings → Workshop you will see cards for each known workshop:

- **This device** — local engine (often `127.0.0.1`)
- **Paired portals** — other machines you've trusted

**Active** marks the current link. **Switch** moves Home onto another workshop. Edit when the address or label needs a correction.

## Status

Below workshops, **Status** shows live connection health and engine summary (tools, age, profile). If chat misbehaves, look here first.

## App updates

The **App** band shows the desktop shell version on this machine and whether a newer build is on the channel. **Check** probes the release manifest; **Download** opens the installer when an update is available. Updates are opt-in downloads for v1 — not a silent background replace.

## This Mac

Login-start and phone/LAN reachability toggles live here. Starting Medousa at login can bring the engine up without opening the full UI — useful when phone companions expect the workshop to be awake. Details for phone/LAN also appear under Sharing — see [Sharing and phone](guide:sharing-phone).

## Private brain

Optional local models (for example offline Gemma) are separate from cloud chat models. Idle state and selected local model show on that band when configured.

## Common connection failures

| Symptom | Check |
|---------|-------|
| Chat won't send | Status offline? Wrong workshop? |
| Vault empty / wrong notes | Active workshop identity |
| Tools missing | Engine summary / restart engine |
| Update check fails | Network to the release channel |

```callout
tone: note
title: One active connection
body: Switching workshops is cheap; running half-configured duplicates is not. Keep the list tidy.
```

Related: [Getting started](guide:getting-started), [Sharing and phone](guide:sharing-phone).
