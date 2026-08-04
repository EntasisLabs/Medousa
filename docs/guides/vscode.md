# Medousa for VS Code

Medousa for VS Code keeps a focused workshop chat beside your editor. It uses
the same Medousa Engine, sessions, identity, tools, and vault as the Medousa
app; advanced workflows still open in Medousa.

## Install a development build

From the Medousa repository:

```bash
cd integrations/vscode
npm install
npm run package
code --install-extension medousa-vscode-0.1.0.vsix --force
```

Reload VS Code after installing, then select the Medousa icon in the activity
bar.

## Connect a workshop

The extension starts with `http://127.0.0.1:7419`, the local Medousa Engine.
Use **Medousa: Configure Connection** from the Command Palette for another
workshop URL. Paired workshop tokens are stored in VS Code SecretStorage and
never written to settings or logs.

The chat header shows checking, connected, reconnecting, unavailable, and
authorization-required states.

## Chat with editor context

The composer includes removable chips for the current workspace, active file,
selection, and diagnostics. Remove a chip when that context should not be sent
with the next turn.

Editor context is attached to the turn as structured metadata. It remains
available for follow-up questions without appearing as text in the transcript.

The controls above the composer show the conversation's Medousa mode and
bound Forge undertaking. These are daemon-owned: opening the same conversation
in Medousa preserves the same General/Coder selection and project binding.
Choose **Coder** at any time. If the conversation has no project yet, the
project control lets you continue ready work, create a blank codebase, or let
Medousa choose or create the project from your message. New codebases are
initialized and provisioned on the connected workshop, then VS Code offers to
open the governed worktree locally. On a remote workshop, the daemon remains
filesystem authority and VS Code sends editor context only as a bounded
observation.

Mode suggestions appear inline with **Switch** and **Not now** actions. Their
expiry and auto-accept behavior use the policy configured in Medousa.

- **Enter** sends.
- **Shift+Enter** adds a line.
- **Ctrl/Cmd+Enter** also sends.
- **Stop** stops the active turn without discarding the conversation.
- **+** starts a new conversation.
- **↗** hands advanced work to Medousa.

Answers support Markdown and fenced code. Code blocks can be copied or inserted
at the active selection after confirmation. Tool activity stays collapsed and
approval requests appear only when Medousa needs attention.

Settled replies include **Copy**, **Share**, and **Library** actions. In VS Code,
Share copies the reply to the system clipboard so it can be pasted into any
destination. Library saves the user/assistant turn into the active workshop
vault's inbox with the same `chat-turn` metadata used by Medousa.

## Session continuity

Each VS Code workspace remembers its active Medousa session and restores its
transcript when the sidebar opens. Select the conversation title in the chat
header to open searchable workshop history. From there you can switch sessions,
rename them, or delete a session and its associated memory after confirmation.

Starting a new conversation creates a new daemon-owned session; it does not
delete earlier sessions or their memory. Untitled conversations receive the
daemon's transcript-derived title and can be renamed at any time.

Composer drafts are kept separately for each conversation. Switching away and
back restores the unfinished thought where you left it. When a response is in
progress, switching is held until you stop or finish it; starting a fresh chat
asks before cancelling active work.

## Troubleshooting

- **Workshop unavailable:** confirm Medousa Engine is running and the endpoint
  is correct.
- **Authorization required:** configure a current portal/pairing token.
- **Old UI after reinstall:** install the VSIX with `--force`, then reload VS
  Code.
- **Need engine telemetry:** enable `medousa.showEngineDetails` in VS Code
  settings. It is off by default so chat remains operator-facing.
