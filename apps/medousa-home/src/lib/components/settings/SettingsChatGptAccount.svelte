<script lang="ts">
  import { onDestroy } from "svelte";
  import { X } from "@lucide/svelte";
  import SettingsListRow from "$lib/components/settings/SettingsListRow.svelte";
  import { openUrlInDefaultBrowser } from "$lib/utils/browserActions";
  import {
    signInChatGptOAuth, selectChatGptAccount, CHATGPT_USAGE_URL,
    chatGptOAuthReady,
    disconnectChatGptOAuth,
    getChatGptOAuthConnection,
    listChatGptOAuthModels,
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
  let welcome = $state(false);
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
    if (connection?.status === "plan_usage_disabled") return "Plan usage disabled";
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

  async function signIn(newAccount = false) {
    await withAction("login", async () => {
      loginCancelled = false;
      const result = await signInChatGptOAuth(newAccount ? undefined : connection?.client_id ?? undefined,
        connection?.status === "plan_usage_disabled");
      if (loginCancelled) return null;
      connection = result.connection;
      welcome = result.first_connection;
      if (chatGptOAuthReady(connection)) await refreshAccountModels();
      else accountModels = [];
      return connection.plan_usage_enabled ? "ChatGPT plan usage is enabled." : "Signed in. Enable ChatGPT plan usage to use account models.";
    });
  }

  async function selectAccount(clientId: string) {
    await withAction("select", async () => {
      accountModels = [];
      connection = await selectChatGptAccount(clientId);
      if (chatGptOAuthReady(connection)) await refreshAccountModels();
      return null;
    });
  }

  async function signOut() {
    await withAction("logout", async () => {
      loginCancelled = true;
      const result = await disconnectChatGptOAuth();
      connection = await getChatGptOAuthConnection();
      accountModels = [];
      return result.revoked ? "Signed out from ChatGPT on this workshop." :
        "Signed out locally. Remote revocation was not confirmed; disconnect Medousa in ChatGPT Settings.";
    });
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

        {#if welcome}
          <div role="status" class="chatgpt-account-copy">
            <strong>You're using your ChatGPT plan</strong>
            <p>Medousa requests will count toward your ChatGPT plan and any credits you allow in ChatGPT Settings.</p>
            <button type="button" class="btn variant-filled-primary btn-sm" onclick={() => (welcome = false)}>Got it</button>
          </div>
        {/if}

        {#if connection?.profiles?.length}
          <label class="chatgpt-account-copy">
            ChatGPT account
            <select value={connection.client_id ?? ""} disabled={actionBusy != null}
              onchange={(event) => void selectAccount(event.currentTarget.value)}>
              {#each connection.profiles as profile (profile.client_id)}
                <option value={profile.client_id}>{profile.email ?? profile.account_id ?? "Registration"} · {profile.client_id.slice(-8)}</option>
              {/each}
            </select>
          </label>
          <button type="button" class="btn variant-ghost-surface btn-sm" disabled={actionBusy != null} onclick={() => void signIn(true)}>Add account</button>
        {/if}

        {#if actionBusy === "login"}
          <p class="chatgpt-account-copy">Finish signing in in your browser, then return to Medousa.</p>
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
            {connection?.status === "plan_usage_disabled" ? "Signed in. Enable ChatGPT plan usage to use account models." : connection?.status === "reauth_required"
              ? "The saved session can no longer refresh. Sign in again to restore account models."
              : "Sign in to make your ChatGPT subscription models available under Model roles."}
          </p>
          <button
            type="button"
            class="model-catalog-manual-btn chatgpt-account-primary"
            disabled={actionBusy != null || loading}
            onclick={() => void signIn()}
          >
            <img src="/brand/external-agents/openai-blossom-white.svg" alt="" width="18" height="18" />
            {connection?.status === "plan_usage_disabled" ? "Enable ChatGPT plan usage" : "Continue with ChatGPT"}
          </button>
        {/if}

        {#if connection?.connected && !ready}
          <button type="button" class="btn variant-ghost-surface btn-sm" disabled={actionBusy != null} onclick={() => void signOut()}>Sign out</button>
        {/if}
        {#if ready}
          <p class="chatgpt-account-copy">Using ChatGPT plan</p>
        {/if}
        <button type="button" class="btn variant-ghost-surface btn-sm" onclick={() => void openUrlInDefaultBrowser(CHATGPT_USAGE_URL)}>Manage usage</button>

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
  .chatgpt-account-feedback {
    margin: 0;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.75rem;
    line-height: 1.5;
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
