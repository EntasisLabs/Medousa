# Choose a coding runtime

Open **Settings → Medousa Agent → Coding runtime** on the connected workshop.
Choose **Medousa Coder**, **Codex**, **Cursor**, or **Hermes** as your preferred
runtime. Add optional fallbacks and move them into the order you want, then
save the runtime settings.

Medousa Coder is the default until you choose. External runtimes need their
package installed and account connected on the workshop that executes the work.
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
