# Models and agent sources

The model name in the chat composer is also the entry point for choosing who
runs the conversation. The closed control stays quiet and shows only the active
model. Select it to see the current provider or connected account.

## Choose a source

The expanded model picker offers three sources:

- **Medousa** uses Medousa's native agent loop with a configured model provider
  or local model. Choose the provider first, then its model.
- **ChatGPT** currently uses the connected OpenAI account through the Codex
  runtime, which owns that agent loop. The available models come from that
  connected session.
- **Cursor** uses the connected Cursor account and the models advertised by its
  session.

If ChatGPT or Cursor is not ready, its source remains visible and opens
**Settings → Connections** for installation or sign-in. Configure API-key and
local providers under **Settings → Models**.

ChatGPT subscriptions and OpenAI API usage are separate. The OpenAI provider
will offer two explicit connections under **Medousa**: **API key** uses public
OpenAI API billing, while **ChatGPT account** uses subscription-backed Codex
model access and keeps Medousa's agent loop. This native ChatGPT-account
connection is not available yet. Until it lands, choosing OpenAI under Medousa
uses an API key; choosing ChatGPT uses the connected account through the Codex
runtime. Medousa never silently moves credentials between those routes.

## Choose General or Coder

Source and mode are independent:

- **General** is for everyday conversation, planning, and research. It does not
  require a project.
- **Coder** is repository-aware and requires a governed Forge project. Choose or
  create that project from the project control above chat.

General/Coder remains available with Medousa, ChatGPT/Codex, and Cursor. When an
external source runs in Coder mode, Medousa launches it inside the governed
project worktree rather than an arbitrary folder.
