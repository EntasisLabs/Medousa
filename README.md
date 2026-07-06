
<div align="center">

<img src="assets/medousa-blk.png" alt="Medousa Logo" width="300">

</div>

<h1 style="text-align: center;">Medousa</h1>

<p align="center"><strong>Turn chaotic life into stone.</strong></p>

Medousa is a permanent AI workspace that lives on your devices. It remembers everything you tell it, verifies what it tells you, and keeps working even when you close the window.


You talk to it from the App,Discord, Telegram, Slack, or WhatsApp if you want them. It runs in the background, processes your requests, and surfaces answers that carry proof — not just words.



<p align="center"><em>One brain, your PC, your phone, your scheduled work, your memory — always here, always yours, always private.</em></p>
<p align="center"><strong>No cloud, no subscription, no hallucinated answers that you cannot trace.</strong></p>

---


## What you can do with it

| You need to… | So you… |
|---|---|
| Remember where you left off | Ask Medousa. It keeps your history — and who you are — across days. |
| Stop re-introducing yourself | Tell it once. Your preferences, your people, your rhythms. It builds a picture of you that gets sharper over time. |
| Wear a different hat on command | Switch to a **specialty** — morning brief, research deep-dive, or a voice you imported from Cursor, Hermes, or OpenClaw. No rewrite required. |
| Get answers you can trust | For live facts it goes out, gathers sources, and shows you the trail. No flat strings. |
| Automate what actually matters | Schedule a check-in, a report, or a full working session. Medousa runs it while you sleep. The answer finds you. |
| Hand off the heavy lifting | Send the big job to the background. One clear answer comes back — not a pile of half-finished threads. |
| Plug in what you already use | Connect the tools and services you rely on. Medousa learns what they can do — and asks before it acts. |
| Reach it from anywhere | Discord, Telegram, Slack, WhatsApp. Text `/brief` on Telegram and your morning summary starts. |
| Pin your own pages | Ask Medousa to build **custom views** — braindumps, studios, live dashboards — in the sidebar (**Settings → Canvas**). |
| Get checked in on | Turn on proactive nudges. Medousa reaches out on a rhythm you choose — with reasoning, not noise. |
| Run it fully offline | **Private brain** on your device — Gemma, local. Your hardware. Your rules. |
| Start fresh on a new device | Open Medousa, run the welcome flow. About sixty seconds. |


### Chat
<img width="1552" height="1012" alt="Chat" src="https://github.com/user-attachments/assets/00b35647-6f6c-4aa3-8d95-77db8e2df5e0" />

### Let Medousa do meaningful work
<img width="1552" height="1012" alt="Work" src="https://github.com/user-attachments/assets/3f4c490f-231e-453c-affb-d1c0eb030cdd" />
<img width="1552" height="1012" alt="Cron" src="https://github.com/user-attachments/assets/6fbd3745-d5ed-4d7b-980a-e5b05698ca99" />

### Review your life in one place
<img width="1552" height="1012" alt="Vault" src="https://github.com/user-attachments/assets/63563221-f28c-437e-9cd3-7f9c62da97b1" />

### Reflect about your journey and visualize your context
<img width="1552" height="1012" alt="Threads" src="https://github.com/user-attachments/assets/871a5952-2382-4b73-a164-39b8066a10ce" />
<img width="1552" height="1012" alt="Maps" src="https://github.com/user-attachments/assets/ac82139d-86db-417d-af96-06c2e9961fad" />

### Special work demands special skills, add what fits you
<img width="1552" height="1012" alt="Skills" src="https://github.com/user-attachments/assets/8b05ac5d-7e28-4bd4-bea9-72ee55a232be" />

### Check in from anywhere 
<img width="1552" height="1012" alt="Messaging" src="https://github.com/user-attachments/assets/b586c2d1-a117-42bf-9ccc-6f71729c19a2" />

### Connect your mobile app with a simple QR Code
<img width="1552" height="1012" alt="Settings   Tunnel" src="https://github.com/user-attachments/assets/5d828508-1b24-4c55-a225-7aa3f4288eb7" />

---


## What makes it reliable

You are not watching Medousa when it works. That is the point.

When you send a message or schedule a check-in, Medousa turns it into work that cannot be lost. If your laptop goes to sleep, if the network drops, if the engine restarts — that work waits. It retries. It picks up where it left off.

You never have to wonder whether something finished. If Medousa accepted it, it ran.

---

## What makes it safe

When Medousa runs a script — processing a spreadsheet, fetching a page, transforming a file — it runs inside a sealed environment. That script cannot touch your documents, your passwords, or your other applications unless you explicitly say it can.

When it reaches outside — sending a message, calling an external service — it can ask you first.

You do not have to trust the script. You only have to trust the seal.

---

## What makes it remember

Most assistants amnesia every time you open them. Medousa doesn't.

It remembers **what happened** — the texture of your weeks, compressed and searchable.

It remembers **who you are** — how you take your coffee, who Mario is, what you care about this quarter. The essentials surface at the start of every turn. Ask for more when you need it.

You can export that picture as markdown. Edit it. Hand it back. Or teach Medousa one fact at a time when you want to.

This is not a gimmick. It is the entire point.

---

## Specialties

Some jobs deserve their own voice.

A **manuscript** is a specialty pack — morning brief, research deep-dive, memory ritual — with its own tone, its own boundaries, its own schedule if you want one. Install one. Name it. Run it from Medousa, delegate it to the background, or schedule it to land in Telegram every morning.

Shipped with the repo: **morning-brief**. One command on Telegram. Your day, summarized.

### Bring your skills with you

You already did the work. Medousa doesn't make you start over.

If you're coming from **Hermes**, **OpenClaw**, or **Cursor**, you already have skills — folders with a `SKILL.md` file and optional scripts, references, and templates. Same format. Same muscle memory. Medousa speaks it natively.

One command imports a skill (or your whole library) as a Medousa specialty. Your `SKILL.md` stays the source of truth — scripts and reference files come along for the ride. Medousa wraps it with scheduling, delivery, memory, and tool policy when you want more than markdown alone.

Import commands and manuscript paths: **[docs/cookbook/skills-and-specialties.md](docs/cookbook/skills-and-specialties.md)**.

Native YAML manuscripts and imported skills coexist. Write new specialties in YAML when you want schedules, delivery, and tool boundaries baked in. Keep authoring `SKILL.md` when that's how you think. Medousa handles both — one runtime, one catalog, zero re-explaining yourself.

---

## Medousa Engine (developers)

The app is a client. The engine is the product underneath — durable jobs, HTTP API, local inference, MCP, channel ingest. Run it headless. Call it from your stack. Same runtime the app uses on every platform.

**Power users:** terminal workspace and CLI live in the **[developer docs](docs/README.md)** — not required for the app welcome flow.

**[Developer docs →](docs/README.md)**

---

### Chaos is not a personality trait. It is a failure of tools.

Every piece of software that forgets who you are, loses your work, or answers without proof is not your fault. It is a broken tool.

Medousa is built to be the opposite.

It remembers. It verifies. It finishes what it starts. It runs where you live — on your machine, in your chat, across your rooms. It does not guess. It does not forget. It does not leave you wondering whether something worked.

Chaotic life turns to stone when the tools around you stop adding to the noise.

That is what Medousa is for.


## Get Medousa

**[Download →](https://github.com/EntasisLabs/Medousa/releases)**  

### Mac · Windows · Linux · iOS · Android 

Open it. Pick how your brain thinks in the welcome flow. If you chose private mode, the model downloads to your device. You land in chat. About ninety seconds. No terminal.

---

## Powered by Stasis, Locus, and Resonantia

The reliability is not hand-waved. Medousa is built on the same infrastructure family as **[Resonantia](https://resonantia.me)** — not a chat wrapper pretending to be durable.

**[Stasis](https://github.com/EntasisLabs/stasis)** — the engine underneath. When you send something, schedule something, or ask for something later, Stasis makes it real work — saved, retried, finished, delivered back to you. Your laptop sleeps. The network drops. The app restarts. The work does not vanish.

**[Locus](https://github.com/EntasisLabs/locus)** — the memory layer. What matters becomes structured, timestamped, retrievable memory — not a scrollback dump. Medousa writes it, recalls it across turns, and stops treating every conversation like day one.

**[Resonantia](https://resonantia.me)** — the sibling on the same protocol. Where Medousa is the workspace that runs and remembers on your machine, Resonantia is the terrain you navigate — your mind, made visible. Same foundation. Different surface. If you want to *see* what this memory looks like when it becomes a map, start there.

Medousa is the brain. Stasis makes work finish. Locus makes memory stick. Resonantia shows what that memory can become.

---
