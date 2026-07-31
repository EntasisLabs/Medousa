# Medousa for VS Code

The first host adapter for Medousa Anywhere.

## Current slice

- Persistent Medousa chat in the activity-bar sidebar
- `Medousa: Ask About This` command
- active editor, selection, language, workspace, and diagnostics context
- local/remote daemon endpoint setting
- bearer token in VS Code SecretStorage
- streaming assistant messages with cancel
- persistent session per workspace

This adapter intentionally does not implement inline edits, Forge custody,
vault actions, or Home-level settings yet. Those land after the core
connection/context/session loop is proven.

## Development

Install dependencies after the shared client has been built:

```bash
npm install
npm run build
npm run package
```

The generated `medousa-vscode-0.1.0.vsix` is written to this directory.

Open this folder in VS Code and press `F5` to launch an Extension Development
Host.
