# Activity and background tasks

Open Spotlight with **⌘K** or **Ctrl+K**, then choose **Activity** to follow tasks, agents, code runs, schedules, and recent note activity across the connected workshop.

Related: [Chat](guide:chat) · [Permissions, budgets, and tool safety](guide:permissions-budgets) · [Navigation and surfaces](guide:navigation-surfaces)

## Follow progress

Expand conversations, delegated turns, and agents to see their children. Select a row to preview progress and retained output. **Open chat**, **Open project**, **Open review**, and **Open note** return to the relevant context. **Task details** opens results, timeline, linked notes, and task controls over your current screen. Close it or press Escape to return.

Use **All**, **Running**, **Needs you**, or **Finished** to focus the hierarchy. Search keeps the ancestors of matching activity visible. Needs you, Finished, and search include older tasks still retained by the daemon.

Needs you offers clear-failed and retry/hide-stuck actions. Finished offers archive-settled and clear-failed/stopped actions. They act on up to 24 matching tasks at a time; repeat for another batch. Individual cancel, retry, and budget actions are in task details.

```callout
tone: tip
title: Keep your place
body: Task details and New ask open over your current screen. Close them to return to the same tab.
```

## Start a background ask

- Choose **New ask** in Activity or **Ask** in the titlebar's **+** menu.
- Use `/ask …` in Chat or Spotlight's **Background task** command.
- Send a vault note as an ask from its note actions.

You can continue using the app while the task runs. The composer opens over your current screen. Accepted standalone asks open their task details. Completion dialogs are available across desktop and mobile and queue when several asks finish together. They offer **View task**, journal and delivery actions, and **Later**.

## Asks in Chat

When a conversation has background asks, its **background asks** button opens their transcripts. From there you can save an answer to the vault, turn tool history into a flow, or **Move to chat** when an ask has settled. This transcript list belongs to the conversation you opened it from.

## Retention and diagnostics

**Settings → Preferences → Task activity** controls how long completed tasks remain in the default activity list and when archives clear. Status filters and search can find older tasks while the daemon retains them.

Activity is the workshop overview. **Settings → Engine status** provides queue, delivery, routing, and failure diagnostics. Schedule configuration and history remain in **Automations**; project execution and review remain in **Code**.

Next: [Troubleshooting](guide:troubleshooting) if a task stays blocked or running too long.
