# Medousa for VS Code

The first host adapter for Medousa Anywhere.

## Current slice

- Persistent Medousa chat in the activity-bar sidebar
- `Medousa: Ask About This` command
- active editor, selection, language, workspace, and diagnostics context
- local/remote daemon endpoint setting
- bearer token in VS Code SecretStorage
- streaming assistant messages with cancel
- shared Liquid Markdown rendering for cards, charts, reports, tabs, slides,
  actions, and the rest of the portable embed catalog
- persistent session per workspace
- safe Markdown and code-block actions
- compact tool progress and approval prompts
- restored session history, connection state, and removable context chips
- searchable conversations with rename/delete and Copy/Share/Library reply actions
- per-conversation drafts, contextual prompts, deliberate loading/feedback states,
  and return-to-latest transcript navigation

This adapter intentionally does not implement inline edits, Forge custody,
vault browsing, or Home-level settings yet. Those land after the core
connection/context/session loop is proven.

The adapter advertises `supports_liquid_markdown` but not
`supports_ui_artifacts`. Assistant replies are parsed with the shared
`@medousa/liquid-markdown` contract and hydrated inside the extension webview;
action prompts, links, and clipboard requests cross the VS Code message bridge.
Home-only chart-editing controls and live feed loading are not available here.

## Development

Install dependencies after the shared client has been built:

```bash
npm install
npm run build
npm run package
```

The generated `medousa-vscode-0.2.0.vsix` is written to this directory.

Open this folder in VS Code and press `F5` to launch an Extension Development
Host.
