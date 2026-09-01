<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    ArrowUpRight,
    Check,
    LogIn,
    LogOut,
    MessageSquare,
    RefreshCw,
    Sparkles,
  } from "@lucide/svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { openUrlInDefaultBrowser } from "$lib/utils/browserActions";
  import {
    beginChatGptOAuth,
    chatGptOAuthReady,
    completeChatGptOAuth,
    disconnectChatGptOAuth,
    getChatGptOAuthConnection,
    type BeginChatGptOAuthResponse,
    type ChatGptOAuthConnection,
  } from "$lib/utils/chatgptOAuth";

  interface Props {
    enabled?: boolean;
  }

  let { enabled = true }: Props = $props();

  let actionBusy = $state<string | null>(null);
  let actionNote = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let connection = $state<ChatGptOAuthConnection | null>(null);
  let loading = $state(false);
  let loaded = $state(false);
  let login = $state<BeginChatGptOAuthResponse | null>(null);
  let loginCancelled = false;

  const ready = $derived(chatGptOAuthReady(connection));

  $effect(() => {
    if (!enabled) {
      loginCancelled = true;
      loaded = false;
      connection = null;
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
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
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
          login = null;
          return "Connected. ChatGPT subscription models are ready to select below.";
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
      login = null;
      return "ChatGPT disconnected from the Medousa Agent.";
    });
  }

  async function reopenLogin() {
    if (!login) return;
    await openUrlInDefaultBrowser(login.verification_url);
  }

  function openChat() {
    if (layout.isMobile) {
      layout.setMobileTab("chat", { bump: true });
      return;
    }
    layout.navigateDesktop("chat");
  }
</script>

<div class="chatgpt-provider">
  <div class="chatgpt-provider-head">
    <span class="chatgpt-provider-icon" aria-hidden="true">
      <Sparkles size={16} strokeWidth={1.85} />
    </span>
    <span class="chatgpt-provider-copy">
      <span class="chatgpt-provider-title">OpenAI · ChatGPT account</span>
      <span class="chatgpt-provider-meta">Subscription models for the Medousa Agent</span>
    </span>
    <span
      class="chatgpt-provider-status"
      class:chatgpt-provider-status--in={ready}
      class:chatgpt-provider-status--out={
        connection?.status === "signed_out" || connection?.status === "reauth_required"
      }
      class:chatgpt-provider-status--wait={loading || actionBusy === "login"}
    >
      {#if !enabled}
        On workshop host
      {:else if loading}
        Checking…
      {:else if actionBusy === "login"}
        Waiting…
      {:else if ready}
        <Check size={12} strokeWidth={2.5} /> Connected
      {:else if connection?.status === "reauth_required"}
        Reconnect
      {:else}
        Not connected
      {/if}
    </span>
  </div>

  {#if !enabled}
    <p class="chatgpt-provider-note workshop-faint">
      ChatGPT account access is managed by the device running this workshop.
    </p>
  {:else if login}
    <div class="chatgpt-device-auth">
      <p class="chatgpt-provider-note workshop-faint">
        Enter this code on the ChatGPT sign-in page:
      </p>
      <code class="chatgpt-device-code font-mono">{login.user_code}</code>
    </div>
  {:else if ready}
    <p class="chatgpt-provider-note workshop-faint">
      Ready for Instant, General, and Coder. Pick this provider when choosing a model.
    </p>
  {:else if connection?.status === "reauth_required"}
    <p class="chatgpt-provider-note workshop-faint">
      The saved session can no longer refresh. Sign in again to restore it.
    </p>
  {:else}
    <p class="chatgpt-provider-note workshop-faint">
      Use your ChatGPT subscription while Medousa keeps its own agent loop and tools.
    </p>
  {/if}

  {#if enabled}
    <div class="chatgpt-provider-actions">
      {#if login}
        <button type="button" class="btn btn-sm variant-filled-primary" onclick={() => void reopenLogin()}>
          <ArrowUpRight size={13} strokeWidth={2} /> Open sign-in
        </button>
      {:else if ready}
        <button type="button" class="btn btn-sm variant-filled-primary" onclick={openChat}>
          <MessageSquare size={13} strokeWidth={2} /> Open Chat
        </button>
        <button
          type="button"
          class="btn btn-sm variant-soft-surface"
          disabled={actionBusy != null}
          onclick={() => void signOut()}
        >
          <LogOut size={13} strokeWidth={2} /> Disconnect
        </button>
      {:else}
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={actionBusy != null || loading}
          onclick={() => void signIn()}
        >
          <LogIn size={13} strokeWidth={2} />
          {connection?.status === "reauth_required" ? "Reconnect" : "Sign in with ChatGPT"}
        </button>
      {/if}
      <button
        type="button"
        class="chatgpt-refresh"
        title="Refresh ChatGPT status"
        aria-label="Refresh ChatGPT status"
        disabled={loading || actionBusy != null}
        onclick={() => void refreshConnection()}
      >
        <RefreshCw size={14} strokeWidth={1.9} />
      </button>
    </div>
  {/if}

  {#if actionNote}
    <p class="chatgpt-feedback text-content-success" role="status">{actionNote}</p>
  {/if}
  {#if actionError}
    <p class="chatgpt-feedback text-content-warning" role="alert">{actionError}</p>
  {/if}
</div>

<style>
  .chatgpt-provider {
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
    margin-bottom: 0.75rem;
    padding: 0.75rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    border-radius: 0.7rem;
    background: rgb(var(--color-surface-900) / 0.28);
  }

  .chatgpt-provider-head {
    display: flex;
    align-items: center;
    gap: 0.55rem;
  }

  .chatgpt-provider-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.9rem;
    height: 1.9rem;
    flex-shrink: 0;
    border-radius: 0.55rem;
    background: rgb(var(--color-primary-500) / 0.12);
    color: rgb(var(--theme-link));
  }

  .chatgpt-provider-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.08rem;
  }

  .chatgpt-provider-title {
    font-size: 0.82rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .chatgpt-provider-meta,
  .chatgpt-provider-note,
  .chatgpt-feedback {
    font-size: 0.7rem;
    line-height: 1.35;
  }

  .chatgpt-provider-status {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    flex-shrink: 0;
    border-radius: 999px;
    padding: 0.15rem 0.5rem;
    background: rgb(var(--color-surface-500) / 0.22);
    color: rgb(var(--theme-text-secondary));
    font-size: 0.66rem;
    font-weight: 600;
  }

  .chatgpt-provider-status--in {
    background: rgb(var(--color-success-500) / 0.18);
    color: rgb(var(--theme-success));
  }

  .chatgpt-provider-status--out {
    background: rgb(var(--color-warning-500) / 0.16);
    color: rgb(var(--theme-warning));
  }

  .chatgpt-provider-status--wait {
    background: rgb(var(--color-primary-500) / 0.16);
    color: rgb(var(--theme-link));
  }

  .chatgpt-provider-note,
  .chatgpt-feedback {
    margin: 0;
  }

  .chatgpt-device-auth {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding: 0.6rem 0.7rem;
    border: 1px solid rgb(var(--color-surface-600) / 0.35);
    border-radius: 0.55rem;
    background: rgb(var(--color-surface-800) / 0.42);
  }

  .chatgpt-device-code {
    color: rgb(var(--color-surface-100));
    font-size: 1rem;
    font-weight: 650;
    letter-spacing: 0.12em;
  }

  .chatgpt-provider-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }

  .chatgpt-refresh {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.9rem;
    height: 1.9rem;
    border: 0;
    border-radius: 0.5rem;
    background: transparent;
    color: rgb(var(--theme-text-quiet));
    cursor: pointer;
  }

  .chatgpt-refresh:hover:not(:disabled) {
    background: rgb(var(--color-surface-500) / 0.16);
    color: rgb(var(--color-surface-100));
  }

  .chatgpt-refresh:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

  @media (max-width: 420px) {
    .chatgpt-provider-head {
      align-items: flex-start;
      flex-wrap: wrap;
    }

    .chatgpt-provider-status {
      margin-left: 2.45rem;
    }
  }
</style>
