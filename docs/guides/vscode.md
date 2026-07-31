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
code --install-extension medousa-vscode-0.2.0.vsix --force
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

- **Enter** sends.
- **Shift+Enter** adds a line.
- **Cancel** stops the active turn.
- **+** starts a new conversation.
- **↗** hands advanced work to Medousa.

Answers support Markdown and fenced code. Code blocks can be copied or inserted
at the active selection after confirmation. Tool activity stays collapsed and
approval requests appear only when Medousa needs attention.

## Session continuity

Each VS Code workspace keeps one Medousa session and restores its transcript
when the sidebar opens. Starting a new conversation creates a new daemon-owned
session; it does not delete the earlier session or its memory.

## Troubleshooting

- **Workshop unavailable:** confirm Medousa Engine is running and the endpoint
  is correct.
- **Authorization required:** configure a current portal/pairing token.
- **Old UI after reinstall:** install the VSIX with `--force`, then reload VS
  Code.
- **Need engine telemetry:** enable `medousa.showEngineDetails` in VS Code
  settings. It is off by default so chat remains operator-facing.
