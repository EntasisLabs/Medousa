# Models, providers, and runtimes

The runtime control under the composer chooses who owns the agent loop:
**Medousa**, **Codex**, or **Cursor**. The model control inside the composer stays
quiet and shows only the active model.

## Choose a runtime

Use the runtime control under the composer:

- **Medousa** uses Medousa's native agent loop with a configured model provider
  or local model.
- **Codex** uses the connected ChatGPT account through the Codex runtime, which
  owns that agent loop.
- **Cursor** uses the connected Cursor account and the models advertised by its
  session.

If Codex or Cursor is not ready, its runtime option opens **Settings →
Connections** for installation or sign-in.

## Choose a provider and model

When Medousa owns the loop, open the model picker and choose a provider first,
then a model from that provider. Configure API-key and local providers under
**Settings → Models**.

ChatGPT subscriptions and OpenAI API usage are separate. The OpenAI provider
offers two explicit routes under **Medousa**: **OpenAI · API key** uses public
OpenAI API billing, while **OpenAI · ChatGPT account** uses subscription-backed
model access and keeps Medousa's agent loop. The ChatGPT-account route appears
as ready only when the workshop daemon has a connected native account. Medousa
never silently moves credentials between these routes.

To connect the native route, open **Settings → Connections**, find **ChatGPT
account — Medousa runtime**, and choose **Connect ChatGPT**. Medousa opens the
verification page and displays the device code to enter. The card updates when
authorization completes. This connection is stored and refreshed by the
workshop daemon; it is separate from the **Codex runtime** card and can be
disconnected independently.

While connected, the picker refreshes from the ChatGPT account's Codex model
catalog. The list therefore follows that account's current entitlements; if the
catalog cannot be reached, Medousa keeps its curated fallback choice available.
The adapter carries a separately versioned Codex-backend compatibility identity;
Medousa's own app version is never sent as the Codex protocol version.

Completed Medousa replies show a small model receipt below the answer. This is
the successful provider/model route observed by the daemon after fallback, so
it can differ from the model that was initially requested.

When Codex or Cursor owns the loop, the model picker contains only the choices
advertised by that runtime. Runtime selection stays under the composer, so it
is not duplicated inside the model picker.

## Choose General or Coder

Source and mode are independent:

- **General** is for everyday conversation, planning, and research. It does not
  require a project.
- **Coder** is repository-aware and requires a governed Forge project. Choose or
  create that project from the project control above chat.

General/Coder remains available with Medousa, Codex, and Cursor. When an
external source runs in Coder mode, Medousa launches it inside the governed
project worktree rather than an arbitrary folder.
