<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    ArrowUpRight,
    Brain,
    ChevronRight,
    LoaderCircle,
    ShieldCheck,
    Sparkles,
  } from "@lucide/svelte";
  import ProviderPicker from "$lib/components/settings/ProviderPicker.svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import type { ProviderCatalogEntry } from "$lib/types/providers";
  import { openUrlInDefaultBrowser } from "$lib/utils/browserActions";
  import {
    beginChatGptOAuth,
    chatGptOAuthReady,
    completeChatGptOAuth,
    getChatGptOAuthConnection,
    listChatGptOAuthModels,
    type BeginChatGptOAuthResponse,
    type ChatGptOAuthConnection,
  } from "$lib/utils/chatgptOAuth";
  import { validateProviderKey } from "$lib/utils/providersApi";

  const CHATGPT_DEFAULT_MODEL = "gpt-5.6-sol";

  let provider = $state("openai");
  let model = $state("gpt-5.4-mini");
  let apiKey = $state("");
  let baseUrl = $state("");
  let needsApiKey = $state(true);
  let validating = $state(false);
  let chatGptBusy = $state(false);
  let chatGptLogin = $state<BeginChatGptOAuthResponse | null>(null);
  let statusMessage = $state<string | null>(null);
  let chatGptLoginCancelled = false;

  onDestroy(() => {
    chatGptLoginCancelled = true;
  });

  function onProviderChange(id: string, entry: ProviderCatalogEntry) {
    provider = id;
    needsApiKey = entry.needsApiKey;
    baseUrl = entry.defaultBaseUrl ?? "";
    statusMessage = null;
  }

  function onPickerStatus(message: string | null, ok?: boolean) {
    if (message) {
      statusMessage = message;
    } else if (ok !== false) {
      statusMessage = null;
    }
  }

  async function finishWithChatGpt() {
    let selectedModel = CHATGPT_DEFAULT_MODEL;
    try {
      const result = await listChatGptOAuthModels();
      const available = result.models.map((entry) => entry.trim()).filter(Boolean);
      if (available.length > 0 && !available.includes(selectedModel)) {
        selectedModel = available[0];
      }
    } catch {
      // The runtime default remains valid if model discovery is temporarily unavailable.
    }

    await wizard.applyScreen1Setup({
      path: "managed",
      provider: "openai-codex",
      model: selectedModel,
      baseUrl: null,
      apiKey: null,
      startCore: false,
    });
  }

  async function continueWithChatGpt() {
    if (wizard.busy || validating || chatGptBusy) return;

    chatGptBusy = true;
    chatGptLoginCancelled = false;
    wizard.error = null;
    statusMessage = null;

    try {
      let connection: ChatGptOAuthConnection = await getChatGptOAuthConnection();
      if (!chatGptOAuthReady(connection)) {
        chatGptLogin = await beginChatGptOAuth();
        await openUrlInDefaultBrowser(chatGptLogin.verification_url);

        while (
          !chatGptLoginCancelled &&
          Date.now() < Date.parse(chatGptLogin.expires_at_utc)
        ) {
          const result = await completeChatGptOAuth(chatGptLogin.login_id);
          if (result.status === "connected") {
            connection = result.connection ?? (await getChatGptOAuthConnection());
            break;
          }
          const delaySeconds = Math.max(
            1,
            result.retry_after_seconds ?? chatGptLogin.poll_interval_seconds,
          );
          await new Promise((resolve) => setTimeout(resolve, delaySeconds * 1000));
        }
      }

      if (chatGptLoginCancelled) return;
      if (!chatGptOAuthReady(connection)) {
        throw new Error("ChatGPT sign-in expired. Try again to receive a new code.");
      }

      chatGptLogin = null;
      await finishWithChatGpt();
    } catch (err) {
      statusMessage = err instanceof Error ? err.message : String(err);
    } finally {
      chatGptBusy = false;
    }
  }

  async function reopenChatGptLogin() {
    if (!chatGptLogin) return;
    try {
      await openUrlInDefaultBrowser(chatGptLogin.verification_url);
    } catch (err) {
      statusMessage = err instanceof Error ? err.message : String(err);
    }
  }

  async function continueSetup() {
    if (!model.trim()) {
      statusMessage = "Choose a model before continuing.";
      return;
    }
    if (needsApiKey && !apiKey.trim()) {
      statusMessage = "Add your provider API key before continuing.";
      return;
    }

    validating = true;
    wizard.error = null;
    statusMessage = null;
    try {
      const validation = await validateProviderKey({
        provider,
        apiKey: needsApiKey ? apiKey.trim() : "",
        baseUrl: baseUrl.trim() || null,
      });
      if (!validation.ok) {
        statusMessage = validation.message;
        return;
      }

      await wizard.applyScreen1Setup({
        path: "byok",
        provider,
        model: model.trim() || validation.suggestedModel || "gpt-5.4-mini",
        baseUrl: baseUrl.trim() || null,
        apiKey: needsApiKey ? apiKey.trim() : null,
        // Personal already runs in-process; never try to spawn a desktop sidecar.
        startCore: false,
      });
    } catch (err) {
      statusMessage = err instanceof Error ? err.message : String(err);
    } finally {
      validating = false;
    }
  }

  async function skipSetup() {
    validating = true;
    wizard.error = null;
    statusMessage = null;
    try {
      await wizard.skipBrain();
    } catch (err) {
      statusMessage = err instanceof Error ? err.message : String(err);
    } finally {
      validating = false;
    }
  }

  const canContinue = $derived(
    !wizard.busy &&
      !validating &&
      !chatGptBusy &&
      model.trim().length > 0 &&
      (!needsApiKey || apiKey.trim().length > 0),
  );
</script>

<div class="wizard-step">
  <button
    type="button"
    class="workshop-text-action self-start text-sm"
    disabled={wizard.busy || validating || chatGptBusy}
    onclick={() => void wizard.back()}
  >
    ← Back
  </button>

  <div class="wizard-stagger mt-4">
    <div class="wizard-beat flex h-12 w-12 items-center justify-center rounded-2xl bg-primary-500/12 text-content-link">
      <Brain class="h-6 w-6" strokeWidth={1.7} aria-hidden="true" />
    </div>
    <div class="wizard-beat mt-5">
      <p class="text-[11px] font-semibold uppercase tracking-[0.16em] text-content-link/90">
        Medousa Agent
      </p>
      <h1 id="product-wizard-title" class="mt-2 text-2xl font-semibold tracking-tight text-surface-50">
        Choose how Medousa thinks
      </h1>
      <p class="mt-2 text-sm leading-relaxed text-content-tertiary">
        Connect the model provider you already use. Personal stays on this device; only AI requests
        go to the provider you choose.
      </p>
    </div>

    <div class="wizard-beat mt-6 rounded-xl border border-primary-500/25 bg-primary-500/10 p-4">
      <div class="flex items-start gap-3">
        <span
          class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-primary-500/12 text-content-link"
          aria-hidden="true"
        >
          <Sparkles class="h-5 w-5" strokeWidth={1.8} />
        </span>
        <div class="min-w-0">
          <h2 class="text-base font-semibold text-surface-50">Continue with ChatGPT</h2>
          <p class="mt-1 text-sm leading-relaxed text-content-tertiary">
            Use your ChatGPT subscription. No API key required.
          </p>
        </div>
      </div>

      <button
        type="button"
        class="btn variant-filled-primary mt-4 inline-flex min-h-11 w-full items-center justify-center gap-2"
        disabled={wizard.busy || validating || chatGptBusy}
        onclick={() => void continueWithChatGpt()}
      >
        {#if chatGptBusy}
          <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
          {chatGptLogin ? "Waiting for ChatGPT…" : "Connecting…"}
        {:else}
          <Sparkles class="h-4 w-4" aria-hidden="true" />
          Sign in with ChatGPT
        {/if}
      </button>

      {#if chatGptLogin}
        <div class="mt-3 rounded-lg border border-surface-500/30 bg-surface-950/30 p-3">
          <p class="text-xs text-content-quiet">Enter this code on the ChatGPT sign-in page:</p>
          <div class="mt-2 flex items-center justify-between gap-3">
            <code class="font-mono text-lg font-semibold tracking-[0.12em] text-surface-50">
              {chatGptLogin.user_code}
            </code>
            <button
              type="button"
              class="workshop-text-action inline-flex min-h-9 items-center gap-1.5 text-xs"
              onclick={() => void reopenChatGptLogin()}
            >
              Open again
              <ArrowUpRight class="h-3.5 w-3.5" aria-hidden="true" />
            </button>
          </div>
          <p class="mt-2 text-xs leading-relaxed text-content-quiet">
            Finish in the browser, then return here. Medousa will continue automatically.
          </p>
        </div>
      {/if}
    </div>

    <div class="wizard-beat my-5 flex items-center gap-3">
      <span class="h-px flex-1 bg-surface-500/25"></span>
      <span class="text-[10px] font-semibold uppercase tracking-[0.15em] text-content-quiet">
        Or choose another provider
      </span>
      <span class="h-px flex-1 bg-surface-500/25"></span>
    </div>

    <div class="wizard-beat rounded-xl border border-surface-500/35 bg-surface-950/35 p-4">
      <ProviderPicker
        providerId={provider}
        {model}
        {apiKey}
        {baseUrl}
        disabled={wizard.busy || validating || chatGptBusy}
        compact
        progressive
        showValidate={false}
        excludeProviderIds={["ollama", "openai-codex", "bedrock"]}
        onProviderChange={onProviderChange}
        onModelChange={(value) => (model = value)}
        onApiKeyChange={(value) => (apiKey = value)}
        onBaseUrlChange={(value) => (baseUrl = value)}
        onStatus={onPickerStatus}
      />
    </div>

    <p class="wizard-beat mt-4 flex items-start gap-2 text-xs leading-relaxed text-content-quiet">
      <ShieldCheck class="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
      {provider === "medousa-local"
        ? "Model weights and inference stay on this device. Download or switch models anytime in Settings."
        : "Your credential is stored securely on this device. You can change providers anytime from the model picker or Settings."}
    </p>

    {#if statusMessage}
      <p class="wizard-beat mt-4 text-sm text-content-warning" role="status" aria-live="polite">
        {statusMessage}
      </p>
    {/if}
  </div>

  <div class="mt-auto flex items-center justify-between gap-3 pt-8">
    <button
      type="button"
      class="btn variant-ghost min-h-11"
      disabled={wizard.busy || validating || chatGptBusy}
      onclick={() => void skipSetup()}
    >
      Set up later
    </button>
    <button
      type="button"
      class="btn variant-filled-primary inline-flex min-h-11 items-center gap-2 px-6"
      disabled={!canContinue}
      onclick={() => void continueSetup()}
    >
      {#if validating || wizard.busy}
        <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
        Connecting…
      {:else}
        Continue
        <ChevronRight class="h-4 w-4" aria-hidden="true" />
      {/if}
    </button>
  </div>
</div>
