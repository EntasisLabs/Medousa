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
External Agents** for installation or sign-in. Hermes can also be prepared from the
terminal with `hermes acp --setup`.

## Choose a provider and model

When Medousa owns the loop, open the model picker and choose a provider first,
then a model from that provider. Under **Settings → Medousa Agent**, connect
provider access first and then assign model roles:

- **Conversation model** handles ordinary turns and is also tried first for
  image input when that model supports vision.
- **Image backup** is optional. Set it only when you want a different model to
  handle images that the conversation model cannot accept.
- **Dictation model** transcribes microphone input; it is a separate speech
  route rather than a property of the conversation model.

Accounts, API keys, and custom endpoints live together under **Providers**.
Each access method opens the same quiet detail sheet; model assignment stays
under **Model roles**. Stage routing remains an advanced control for specialized
steps inside a turn.

ChatGPT subscriptions and OpenAI API usage are separate. The OpenAI provider
offers two explicit routes under **Medousa**: **OpenAI · API key** uses public
OpenAI API billing, while **OpenAI · ChatGPT account** uses subscription-backed
model access and keeps Medousa's agent loop. The ChatGPT-account route appears
as ready only when the workshop daemon has a connected native account. Medousa
never silently moves credentials between these routes.

To connect the native route, open **Settings → Medousa Agent → Providers**, select
**ChatGPT account**, and choose **Sign in with ChatGPT**. Medousa opens the
verification page and displays the device code to enter. The provider row updates when
authorization completes. This connection is stored and refreshed by the
workshop daemon—including Embedded Personal on phone—and remains available
through that workshop's secure credential store. It is separate from the
desktop-only **Codex runtime** card and can be disconnected independently.

While connected, the picker refreshes from the ChatGPT account's Codex model
catalog. The list therefore follows that account's current entitlements; if the
catalog cannot be reached, Medousa keeps its curated fallback choice available.
Compatible account models can accept both text and image input through
Medousa's native loop, while continuing to use Medousa modes and tools. The
Codex account transport currently produces text responses; dedicated image
generation, speech generation, and transcription routes remain separate rather
than being falsely advertised as account-model capabilities.
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

Manage downloads under **Settings → Connection → Private brain**, then choose a
verified on-device model under **Medousa Local** in the model picker. Keeping
storage management in one place makes incomplete downloads visible and
removable instead of presenting them as usable models. Medousa recommends a
small model for the device and also offers other text and vision checkpoints.
You can paste a full Hugging Face MLX repository ID when you want a compatible
model that is not in the curated list. Downloaded models remain available
offline and can be unloaded to release memory or removed to reclaim storage.

The curated LFM choices separate their jobs: **LFM2.5 2.6B** is the stronger
text, reasoning, and tool-use model, while **LFM2.5 VL 1.6B** remains available
when a conversation needs image understanding.

Desktop keeps using the existing optional local-engine package. Selecting
Medousa Local never silently changes a remote workshop: the model runs wherever
the selected workshop has inference authority.

## Choose General, Teacher, Instant, or Coder

Source and mode are independent:

- **General** is for everyday conversation, planning, and research. It does not
  require a project.
- **Teacher** is an evidence-first mentor. It connects an unfamiliar idea to
  concepts you already understand, names the relationship between them, and
  helps you predict or apply the idea instead of only supplying an answer.
  Pattern matches are treated as hypotheses, not proof: Teacher distinguishes
  facts, inferences, assumptions, and uncertainty; challenges false premises;
  and verifies current, contested, niche, or high-stakes claims with sources.
  It will give a direct answer when you are stuck, ask for one, or safety
  matters, then reconnect that answer to the underlying model.
- **Instant** keeps General's behavior and agent loop but loads only recent
  conversation context and a compact everyday tool set. MCP capabilities stay
  available through lazy search and invocation instead of preloading every MCP
  tool schema. It is a good fit for quick replies and private models running on
  a phone.
- **Coder** is repository-aware and requires a governed Forge project. Choose or
  create that project from the project control above chat.

General, Teacher, and Instant are policies for Medousa's native loop. Teacher
changes its teaching and evidence policy, while Instant changes only the context
loaded for the turn; neither changes generation settings. Codex, Cursor, and
Hermes own their own agent policies when selected. Coder's governed project
boundary still applies to those external runtimes, so Medousa launches them
inside the project worktree rather than an arbitrary folder.

## Narrate replies

**Narrate** is independent from the selected mode. Turn it on under the composer
to read each completed assistant reply with the device's system voice, whether
you are using General, Teacher, Instant, or Coder. Turn it off without changing
the conversation mode, or use **Read aloud** on a completed reply to replay only
that message.

Narration is a local presentation preference: the written reply remains the
canonical transcript, code blocks are summarized rather than spoken character
by character, and speech is not sent back into the agent loop. Availability and
voice quality follow the system speech engine on the current device.
