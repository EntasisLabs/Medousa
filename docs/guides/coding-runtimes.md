# Choose a coding runtime

Open **Settings → Medousa Agent → Coding runtime** on the connected workshop.
Choose **Medousa Coder**, **Codex**, **Cursor**, **Hermes**, **Claude Code**,
**Devin**, **Pi**, **Muse Code**, or **Hades** as your preferred runtime. Add optional fallbacks and move them into the order you want, then
save the runtime settings.

Medousa Coder is the default until you choose. External runtimes need their
official CLI installed and configured on the workshop that executes the work.
Your coding runtime choice is separate from the model used by Medousa Coder.

Future coding assignments use your saved preference. If it is unavailable,
Medousa tries the fallbacks you selected before preparing the assignment. With
no available choice, it reports the blocker. Once work is assigned, it keeps
that runtime. You can still ask for a specific runtime in a particular request
without changing your saved preference.

You can ask from your current chat to work on an existing project. The runtime
checks project ownership and uses a separate execution session; you do not need
to open the project's chat just to assign a coder. Assignment approvals still
apply. A saved runtime preference does not authorize additional work.

## Connect an external runtime

Open **Settings → External Agents** to inspect the connected workshop's runtime
availability and authentication status. Codex, Cursor, and Hermes retain their
existing local install and sign-in actions. For Claude Code, Devin, Pi, Muse Code,
and Hades, install and configure the official CLI on the workshop host, then
refresh. Medousa reuses that agent's own account and settings; it does not copy
credentials from another device or replace them with a Medousa model key.

Claude Code and Pi use pinned ACP bridges; their native CLIs and bridge prerequisites
must be available on the executing workshop. Hades currently targets Linux
workshops. Unsupported platforms or missing bridge dependencies appear as a
runtime blocker. Authentication that cannot be verified is shown as **Unknown**,
not as a successful sign-in.

From an iPad or a remote connection, you can inspect and use the host's configured
runtimes. Local installation and terminal login are available only when Medousa
and the selected workshop share the same machine. Configure remote credentials
on that workshop host. **Muse Code** is the executable coding agent; **Muse** in
Connected agents remains the independent WhatsApp conversation connection.

External coding-agent chat currently accepts text. Remove attachments before
sending; Medousa reports unsupported attachments instead of silently dropping them.
Permissions show the choices supplied by the agent. Choose **Cancel** to dismiss
a pending request without selecting a provider option. Older workshops retain
**Allow** and **Deny**.

## Follow assigned work

The assignment card in your chat shows the coder's execution state, its current
action when available, the previous action's result, and the latest activity
time. Medousa Coder runs in its own execution session, so your chat can be idle
while the coder continues working. External runtimes report their execution
state and activity time; detailed actions depend on the runtime's observations.

A failed action can be followed by a repair attempt. The card reports the whole
assignment as completed, failed, cancelled, or interrupted only after Medousa
receives its verified final result. The selected card keeps that result visible
after the assignment leaves the pending inbox.

If updates are delayed or the runtime cannot observe the execution, the card
shows that uncertainty and the last known activity. It does not restart the
work. Older workshops may show that live progress is unavailable until updated.
