# Models, providers, and runtimes

The runtime control under the composer chooses who owns the agent loop:
**Medousa**, **Codex**, **Cursor**, or **Hermes**. The model control inside the
composer stays quiet and shows only the active model.

## Choose a runtime

Use the runtime control under the composer:

- **Medousa** uses Medousa's native agent loop with a configured model provider
  or local model.
- **Codex** uses the connected ChatGPT account through the Codex runtime, which
  owns that agent loop.
- **Cursor** uses the connected Cursor account and the models advertised by its
  session.
- **Hermes** uses the configured Hermes Agent CLI through ACP and the providers
  advertised by that runtime.

If Codex, Cursor, or Hermes is not ready, its runtime option opens **Settings →
Connections** for installation or sign-in. Hermes can also be prepared from the
terminal with `hermes acp --setup`.

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
workshop daemon—including Embedded Personal on phone—and remains available
through that workshop's secure credential store. It is separate from the
desktop-only **Codex runtime** card and can be disconnected independently.

While connected, the picker refreshes from the ChatGPT account's Codex model
catalog. The list therefore follows that account's current entitlements; if the
catalog cannot be reached, Medousa keeps its curated fallback choice available.
The adapter carries a separately versioned Codex-backend compatibility identity;
Medousa's own app version is never sent as the Codex protocol version.

Completed Medousa replies show a small model receipt below the answer. This is
the successful provider/model route observed by the daemon after fallback, so
it can differ from the model that was initially requested.

When Codex, Cursor, or Hermes owns the loop, the model picker contains only the
choices advertised by that runtime. Runtime selection stays under the composer,
so it is not duplicated inside the model picker.

## Run a private model on iPhone or iPad

On iOS, **Medousa Local** runs MLX models inside the app. The prompt, model
weights, and generated tokens stay on the device; the Embedded Personal daemon
still owns conversation history, tools, and the agent loop.

Choose **Medousa Local** in the model picker, then manage downloads under
**Settings → Connection → Private brain**. Medousa recommends a small model for
the device and also offers other text and vision checkpoints. You can paste a
full Hugging Face MLX repository ID when you want a compatible model that is not
in the curated list. Downloaded models remain available offline and can be
unloaded to release memory or removed to reclaim storage.

Desktop keeps using the existing optional local-engine package. Selecting
Medousa Local never silently changes a remote workshop: the model runs wherever
the selected workshop has inference authority.

## Choose General or Coder

Source and mode are independent:

- **General** is for everyday conversation, planning, and research. It does not
  require a project.
- **Coder** is repository-aware and requires a governed Forge project. Choose or
  create that project from the project control above chat.

General/Coder remains available with Medousa, Codex, Cursor, and Hermes. When
an external source runs in Coder mode, Medousa launches it inside the governed
project worktree rather than an arbitrary folder.
