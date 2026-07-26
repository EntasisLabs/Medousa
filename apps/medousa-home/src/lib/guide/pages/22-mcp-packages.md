# Packages and MCP

**Advanced.** Optional installs on the **desktop app** — Offline brain, helpers, and external tool servers (MCP). The phone usually asks you to manage these on the computer.

Related: [Permissions, budgets, and tool safety](guide:permissions-budgets) · [Grapheme and automations](guide:grapheme-automations) · [Troubleshooting](guide:troubleshooting#mcp-unavailable)

## Packages

**Settings → Packages** — optional binaries (Offline brain, channel helpers, etc.). The MCP gateway is **not** listed here; it lives under MCP.

| Action | Notes |
|--------|--------|
| **Install** / **Update** | Progress on the row; may open **Medousa Installer** |
| **Installed** | Ready |
| **Remove** | When the package is optional |

Disk use varies by package — Offline brain is the large one. Non-desktop shells cannot install binaries locally.

## MCP

**Settings → MCP**

1. Install/update the **Gateway** package (`mcp-gateway`) if prompted — Install / Update / Remove when optional.
2. **Add server** with transport:
   - **Local command**
   - **Remote HTTP**
   - **Remote SSE**
   - **Mock** (dev)
3. Fill **Server id**, **Title**, URL/token or Command/Arguments.
4. Enable/disable servers under **Your servers**; advanced gateway config path when shown.

Tools from MCP appear for chat, specialists, and Flow **External tool** steps — still subject to Runtime Controls module allowlists.

```callout
tone: warning
title: Trust the server
body: An MCP server is code with tools. Prefer local commands you control; treat remote URLs and tokens like API keys.
```

Next: [Specialist agents](guide:specialist-agents) · [Runtime telemetry](guide:runtime-telemetry).
