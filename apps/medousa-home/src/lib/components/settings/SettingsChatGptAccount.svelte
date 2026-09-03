<script lang="ts">
  import { onDestroy } from "svelte";
  import { ArrowUpRight, X } from "@lucide/svelte";
  import SettingsListRow from "$lib/components/settings/SettingsListRow.svelte";
  import { openUrlInDefaultBrowser } from "$lib/utils/browserActions";
  import {
    beginChatGptOAuth,
    chatGptOAuthReady,
    completeChatGptOAuth,
    disconnectChatGptOAuth,
    getChatGptOAuthConnection,
    listChatGptOAuthModels,
    type BeginChatGptOAuthResponse,
    type ChatGptOAuthConnection,
  } from "$lib/utils/chatgptOAuth";

  interface Props {
    enabled?: boolean;
    disabled?: boolean;
  }

  let { enabled = true, disabled = false }: Props = $props();

  let actionBusy = $state<string | null>(null);
  let actionNote = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let connection = $state<ChatGptOAuthConnection | null>(null);
  let loading = $state(false);
  let loaded = $state(false);
  let login = $state<BeginChatGptOAuthResponse | null>(null);
  let accountModels = $state<string[]>([]);
  let modelsLoading = $state(false);
  let sheetOpen = $state(false);
  let loginCancelled = false;

  const ready = $derived(chatGptOAuthReady(connection));
  const rowValue = $derived.by(() => {
    if (!enabled) return "On workshop host";
    if (actionBusy === "login") return "Waiting…";
    if (loading && !connection) return "Checking…";
    if (ready) return "Connected";
    if (connection?.status === "reauth_required") return "Reconnect";
    return "Not connected";
  });
  const sheetStatus = $derived.by(() => {
    if (actionBusy === "login") return "Waiting for approval";
    if (loading && !connection) return "Checking";
    if (ready) return "Signed in";
    if (connection?.status === "reauth_required") return "Reconnect needed";
    return "Not signed in";
  });

  $effect(() => {
    if (!enabled) {
      loginCancelled = true;
      loaded = false;
      connection = null;
      accountModels = [];
      sheetOpen = false;
      return;
    }
    if (loaded) return;
    loaded = true;
    void refreshConnection();
  });

  onDestroy(() => {
    loginCancelled = true;
  });

  async function refreshConnection() {
    if (!enabled || loading) return;
    loading = true;
    actionError = null;
    try {
      connection = await getChatGptOAuthConnection();
      if (chatGptOAuthReady(connection)) {
        await refreshAccountModels();
      } else {
        accountModels = [];
      }
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function refreshAccountModels() {
    if (modelsLoading) return;
    modelsLoading = true;
    try {
      const response = await listChatGptOAuthModels();
      accountModels = response.models;
    } catch {
      accountModels = [];
    } finally {
      modelsLoading = false;
    }
  }

  async function withAction(key: string, fn: () => Promise<string | null>) {
    if (actionBusy) return;
    actionBusy = key;
    actionNote = null;
    actionError = null;
    try {
      actionNote = await fn();
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      actionBusy = null;
    }
  }

  async function signIn() {
    await withAction("login", async () => {
      loginCancelled = false;
      login = await beginChatGptOAuth();
      await openUrlInDefaultBrowser(login.verification_url);
      actionNote = `Enter code ${login.user_code} in the browser. Waiting for approval…`;

      while (!loginCancelled && Date.now() < Date.parse(login.expires_at_utc)) {
        const result = await completeChatGptOAuth(login.login_id);
        if (result.status === "connected") {
          connection = result.connection ?? (await getChatGptOAuthConnection());
          await refreshAccountModels();
          login = null;
          return "Connected. Subscription models are now available under Model roles.";
        }
        const delaySeconds = Math.max(
          1,
          result.retry_after_seconds ?? login.poll_interval_seconds,
        );
        await new Promise((resolve) => setTimeout(resolve, delaySeconds * 1000));
      }

      login = null;
      if (loginCancelled) return null;
      throw new Error("ChatGPT sign-in expired. Start it again to receive a new code.");
    });
  }

  async function signOut() {
    await withAction("logout", async () => {
      loginCancelled = true;
      await disconnectChatGptOAuth();
      connection = await getChatGptOAuthConnection();
      accountModels = [];
      login = null;
      return "Signed out from ChatGPT on this workshop.";
    });
  }

  async function reopenLogin() {
    if (!login) return;
    await openUrlInDefaultBrowser(login.verification_url);
  }

  async function refreshAll() {
    await refreshConnection();
  }

  function openSheet() {
    if (!enabled || disabled) return;
    sheetOpen = true;
    void refreshConnection();
  }
</script>

<SettingsListRow
  label="ChatGPT account"
  value={rowValue}
  hint="OpenAI subscription models"
  disabled={disabled || !enabled}
  onclick={openSheet}
/>

{#if sheetOpen}
  <div
    class="model-catalog-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) sheetOpen = false;
    }}
  >
    <div
      class="model-catalog-sheet model-catalog-sheet-narrow"
      role="dialog"
      aria-modal="true"
      aria-label="ChatGPT account"
    >
      <header class="model-catalog-sheet-header">
        <div class="min-w-0 flex-1">
          <h3 class="model-catalog-sheet-title">ChatGPT account</h3>
          <p class="model-catalog-sheet-subtitle">
            OpenAI subscription access for this workshop.
          </p>
        </div>
        <button
          type="button"
          class="model-catalog-sheet-close"
          aria-label="Close"
          onclick={() => (sheetOpen = false)}
        >
          <X size={18} />
        </button>
      </header>

      <div class="model-catalog-custom-form chatgpt-account-sheet">
        <div class="chatgpt-account-status">
          <span class="chatgpt-account-status-label">Status</span>
          <span class:chatgpt-account-status-ready={ready}>{sheetStatus}</span>
        </div>

        {#if login}
          <div class="chatgpt-device-auth">
            <p>Enter this code on the ChatGPT sign-in page:</p>
            <code class="chatgpt-device-code font-mono">{login.user_code}</code>
            <p>Medousa will finish connecting after approval.</p>
          </div>
          <button
            type="button"
            class="model-catalog-manual-btn chatgpt-account-primary"
            onclick={() => void reopenLogin()}
          >
            Open sign-in page <ArrowUpRight size={13} strokeWidth={2} />
          </button>
        {:else if ready}
          <p class="chatgpt-account-copy">
            {#if modelsLoading}
              Reading the models available to this account…
            {:else if accountModels.length > 0}
              {accountModels.length} subscription model{accountModels.length === 1 ? " is" : "s are"}
              available under Model roles. Compatible models can also accept image input.
            {:else}
              Subscription models are available under Model roles. Compatible models can also
              accept image input.
            {/if}
          </p>
          <div class="chatgpt-account-actions">
            <button
              type="button"
              class="btn variant-ghost-surface btn-sm"
              disabled={loading || modelsLoading || actionBusy != null}
              onclick={() => void refreshAll()}
            >
              Refresh status
            </button>
            <button
              type="button"
              class="btn variant-ghost-surface btn-sm"
              disabled={actionBusy != null}
              onclick={() => void signOut()}
            >
              {actionBusy === "logout" ? "Signing out…" : "Sign out"}
            </button>
          </div>
        {:else}
          <p class="chatgpt-account-copy">
            {connection?.status === "reauth_required"
              ? "The saved session can no longer refresh. Sign in again to restore account models."
              : "Sign in to make your ChatGPT subscription models available under Model roles."}
          </p>
          <button
            type="button"
            class="model-catalog-manual-btn chatgpt-account-primary"
            disabled={actionBusy != null || loading}
            onclick={() => void signIn()}
          >
            {connection?.status === "reauth_required" ? "Reconnect" : "Sign in with ChatGPT"}
          </button>
        {/if}

        {#if actionNote}
          <p class="chatgpt-account-feedback text-content-success" role="status">{actionNote}</p>
        {/if}
        {#if actionError}
          <p class="chatgpt-account-feedback text-content-warning" role="alert">{actionError}</p>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .chatgpt-account-sheet {
    gap: 0.85rem;
  }

  .chatgpt-account-status {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.25);
    padding-bottom: 0.75rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.75rem;
  }

  .chatgpt-account-status-label {
    color: rgb(var(--theme-text-quiet));
    font-size: 0.68rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .chatgpt-account-status-ready {
    color: rgb(var(--theme-success));
  }

  .chatgpt-account-copy,
  .chatgpt-account-feedback,
  .chatgpt-device-auth p {
    margin: 0;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.75rem;
    line-height: 1.5;
  }

  .chatgpt-device-auth {
    display: grid;
    gap: 0.45rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.3);
    border-radius: 0.55rem;
    background: rgb(var(--color-surface-900) / 0.28);
    padding: 0.75rem;
  }

  .chatgpt-device-code {
    color: rgb(var(--color-surface-100));
    font-size: 1rem;
    font-weight: 650;
    letter-spacing: 0.12em;
  }

  .chatgpt-account-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem;
  }

  .chatgpt-account-primary {
    display: inline-flex;
    width: fit-content;
    align-items: center;
    gap: 0.35rem;
  }
</style>
