<script lang="ts">
  /**
   * Settings → Connections — ChatGPT (Codex) + Cursor account sign-in.
   * Credentials stay in the vendor CLI stores; Medousa only orchestrates
   * `codex login` / `cursor agent login` and reads sign-in state.
   */
  import { onMount } from "svelte";
  import {
    ArrowUpRight,
    Check,
    CircleAlert,
    Download,
    LogIn,
    LogOut,
    MessageSquare,
    RefreshCw,
    Sparkles,
    MousePointer2,
  } from "@lucide/svelte";
  import { accountConnections } from "$lib/stores/accountConnections.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import {
    accountConnectionsSupported,
    authStatusLabel,
    beginChatgptDeviceLogin,
    beginTerminalLogin,
    accountSignOut,
    installAccountCli,
  } from "$lib/utils/accountConnections";
  import { openGuide } from "$lib/guide/openGuide";

  let actionBusy = $state<string | null>(null);
  let actionNote = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let waitingFor = $state<"chatgpt" | "cursor" | null>(null);

  onMount(() => {
    void accountConnections.refresh(true);
  });

  const USE_HINT =
    "In Chat, open the runtime menu next to the composer (Medousa / Cursor / ChatGPT), pick the agent, then send a message.";

  async function withAction(key: string, fn: () => Promise<string | null>) {
    if (actionBusy) return;
    actionBusy = key;
    actionNote = null;
    actionError = null;
    try {
      const note = await fn();
      actionNote = note;
      await accountConnections.refresh(true);
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      actionBusy = null;
      waitingFor = null;
    }
  }

  async function waitForSignIn(account: "chatgpt" | "cursor"): Promise<string> {
    waitingFor = account;
    const status = await accountConnections.awaitSignedIn(account, {
      timeoutMs: 3 * 60_000,
      intervalMs: 2_500,
    });
    waitingFor = null;
    if (status === "signed_in") {
      return `Signed in. ${USE_HINT}`;
    }
    return "Still waiting for sign-in — finish in the browser or terminal, then tap Refresh here.";
  }

  async function signInChatGpt() {
    await withAction("chatgpt-login", async () => {
      try {
        const start = await beginChatgptDeviceLogin();
        actionNote = start.code
          ? `Approve in the browser — enter code ${start.code} if asked. Waiting for Codex to finish…`
          : (start.detail ?? "Approve in the browser that opened. Waiting for Codex to finish…");
        return await waitForSignIn("chatgpt");
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        if (message.toLowerCase().includes("already signed in")) {
          return `Already signed in. ${USE_HINT}`;
        }
        // Device auth unavailable (older codex / headless) — fall back to terminal login.
        actionNote = await beginTerminalLogin("chatgpt");
        return await waitForSignIn("chatgpt");
      }
    });
  }

  async function signInCursor() {
    await withAction("cursor-login", async () => {
      actionNote = await beginTerminalLogin("cursor");
      return await waitForSignIn("cursor");
    });
  }

  async function signOut(account: "chatgpt" | "cursor") {
    await withAction(`${account}-logout`, () => accountSignOut(account));
  }

  async function installCli(account: "chatgpt" | "cursor") {
    await withAction(`${account}-install`, async () => {
      const result = await installAccountCli(account);
      return result.detail;
    });
  }

  function openChat() {
    layout.navigateDesktop("chat");
  }

  const supported = accountConnectionsSupported();
  const anySignedIn = $derived(
    accountConnections.isSignedIn("chatgpt") || accountConnections.isSignedIn("cursor"),
  );
</script>

<section class="settings-section connections">
  <header class="settings-section-header">
    <div class="min-w-0 flex-1">
      <h2 class="text-base font-semibold text-surface-50">Connections</h2>
      <p class="workshop-faint mt-1 text-sm">
        Sign in with ChatGPT or Cursor, then pick that runtime in Chat to run
        their coding agent. Credentials stay with the vendor CLIs — Medousa never
        sees your tokens.
      </p>
    </div>
    <button
      type="button"
      class="workshop-rail-btn shrink-0"
      title="Refresh status"
      aria-label="Refresh status"
      disabled={accountConnections.loading || actionBusy != null}
      onclick={() => void accountConnections.refresh(true)}
    >
      <RefreshCw size={15} strokeWidth={1.85} />
    </button>
  </header>

  {#if !supported}
    <p class="workshop-faint mt-2 text-sm">
      Connections are available in the Medousa desktop app.
    </p>
  {:else}
    {#if accountConnections.error}
      <p class="mt-2 flex items-center gap-1.5 text-sm text-content-warning">
        <CircleAlert size={14} strokeWidth={2} />
        {accountConnections.error}
      </p>
    {/if}

    <ol class="connections-steps workshop-faint mt-3">
      <li><strong>Install</strong> the CLI if needed (button on the card).</li>
      <li><strong>Sign in</strong> — browser or terminal opens; finish there.</li>
      <li>
        <strong>In Chat</strong>, open the runtime menu next to the composer and
        choose <em>ChatGPT / Codex</em> or <em>Cursor</em>, then send a message.
      </li>
    </ol>

    <div class="connections-cards mt-3">
      {#each [
        { info: accountConnections.connections?.chatgpt, account: "chatgpt" as const, icon: "chatgpt" },
        { info: accountConnections.connections?.cursor, account: "cursor" as const, icon: "cursor" },
      ] as card (card.account)}
        {@const info = card.info}
        <div class="connections-card" data-account={card.account}>
          <div class="connections-card-head">
            <span class="connections-card-icon" aria-hidden="true">
              {#if card.icon === "chatgpt"}
                <Sparkles size={16} strokeWidth={1.85} />
              {:else}
                <MousePointer2 size={16} strokeWidth={1.85} />
              {/if}
            </span>
            <div class="min-w-0 flex-1">
              <p class="connections-card-title">
                {card.account === "chatgpt" ? "ChatGPT" : "Cursor"}
              </p>
              <p class="connections-card-sub workshop-faint">
                {card.account === "chatgpt"
                  ? "Subscription chat + Codex coding agent"
                  : "Cursor coding agent"}
              </p>
            </div>
            <span
              class="connections-status"
              class:connections-status--in={info?.authStatus === "signed_in"}
              class:connections-status--out={info?.authStatus === "signed_out"}
              class:connections-status--wait={waitingFor === card.account}
            >
              {#if waitingFor === card.account}
                Waiting…
              {:else if info?.authStatus === "signed_in"}
                <Check size={12} strokeWidth={2.5} />
                {authStatusLabel(info.authStatus)}
              {:else}
                {authStatusLabel(info?.authStatus ?? "unknown")}
              {/if}
            </span>
          </div>

          {#if info && !info.binaryPresent}
            <p class="connections-note workshop-faint">
              {card.account === "chatgpt" ? "Codex CLI" : "Cursor Agent CLI"} isn’t
              installed yet — Medousa can install it for you with the vendor’s
              official installer.
            </p>
          {:else if waitingFor === card.account}
            <p class="connections-note workshop-faint">
              Finish sign-in in the browser or terminal. This card updates when
              the vendor CLI reports you’re signed in.
            </p>
          {:else if info?.authStatus === "signed_in"}
            <p class="connections-note workshop-faint">
              Ready. In Chat, pick
              {card.account === "chatgpt" ? "ChatGPT / Codex" : "Cursor"}
              from the runtime menu next to the composer.
            </p>
          {:else if info?.authStatus === "signed_out"}
            <p class="connections-note workshop-faint">
              {info.detail ?? "Not signed in."}
            </p>
          {/if}

          <div class="connections-card-actions">
            {#if info && !info.binaryPresent}
              <button
                type="button"
                class="btn btn-sm variant-filled-primary"
                disabled={actionBusy != null}
                onclick={() => void installCli(card.account)}
              >
                <Download size={13} strokeWidth={2} />
                {actionBusy === `${card.account}-install` ? "Installing…" : "Install"}
              </button>
            {:else if info?.authStatus === "signed_in"}
              <button
                type="button"
                class="btn btn-sm variant-filled-primary"
                onclick={openChat}
              >
                <MessageSquare size={13} strokeWidth={2} />
                Open Chat
              </button>
              <button
                type="button"
                class="btn btn-sm variant-soft-surface"
                disabled={actionBusy != null}
                onclick={() => void signOut(card.account)}
              >
                <LogOut size={13} strokeWidth={2} />
                Sign out
              </button>
            {:else}
              <button
                type="button"
                class="btn btn-sm variant-filled-primary"
                disabled={actionBusy != null}
                onclick={() =>
                  void (card.account === "chatgpt" ? signInChatGpt() : signInCursor())}
              >
                <LogIn size={13} strokeWidth={2} />
                {waitingFor === card.account
                  ? "Waiting…"
                  : actionBusy === `${card.account}-login` || actionBusy === "chatgpt-login"
                    ? "Signing in…"
                    : "Sign in"}
              </button>
            {/if}
          </div>
        </div>
      {/each}
    </div>

    {#if anySignedIn && !waitingFor && !actionNote}
      <p class="connections-feedback mt-2 text-sm text-surface-200">{USE_HINT}</p>
    {/if}
    {#if actionNote}
      <p class="connections-feedback mt-2 text-sm text-content-success">{actionNote}</p>
    {/if}
    {#if actionError}
      <p class="connections-feedback mt-2 text-sm text-content-warning">{actionError}</p>
    {/if}

    <p class="workshop-faint mt-3 text-xs">
      <button
        type="button"
        class="connections-guide-link"
        onclick={() => void openGuide("acp-external-agents")}
      >
        How coding agents work <ArrowUpRight size={11} strokeWidth={2} />
      </button>
    </p>
  {/if}
</section>

<style>
  .connections-steps {
    margin: 0;
    padding-left: 1.15rem;
    font-size: 0.78rem;
    line-height: 1.45;
    display: grid;
    gap: 0.2rem;
  }

  .connections-steps strong {
    color: rgb(var(--color-surface-100));
    font-weight: 600;
  }

  .connections-steps em {
    font-style: normal;
    color: rgb(var(--theme-link));
  }

  .connections-cards {
    display: grid;
    gap: 0.75rem;
    grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
  }

  .connections-card {
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
    border-radius: 0.75rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.3);
    background: rgb(var(--color-surface-900) / 0.5);
    padding: 0.8rem 0.9rem;
  }

  .connections-card-head {
    display: flex;
    align-items: center;
    gap: 0.55rem;
  }

  .connections-card-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.9rem;
    height: 1.9rem;
    border-radius: 0.55rem;
    background: rgb(var(--color-surface-500) / 0.18);
    color: rgb(var(--color-surface-200));
    flex-shrink: 0;
  }

  .connections-card-title {
    margin: 0;
    font-size: 0.875rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .connections-card-sub {
    margin: 0.1rem 0 0;
    font-size: 0.72rem;
  }

  .connections-status {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    flex-shrink: 0;
    border-radius: 999px;
    padding: 0.15rem 0.55rem;
    font-size: 0.68rem;
    font-weight: 600;
    background: rgb(var(--color-surface-500) / 0.22);
    color: rgb(var(--theme-text-secondary));
  }

  .connections-status--in {
    background: rgb(var(--color-success-500) / 0.18);
    color: rgb(var(--theme-success));
  }

  .connections-status--out {
    background: rgb(var(--color-warning-500) / 0.16);
    color: rgb(var(--theme-warning));
  }

  .connections-status--wait {
    background: rgb(var(--color-primary-500) / 0.16);
    color: rgb(var(--theme-link));
  }

  .connections-note {
    margin: 0;
    font-size: 0.75rem;
    line-height: 1.35;
  }

  .connections-card-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .connections-guide-link {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    color: rgb(var(--theme-link));
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-size: inherit;
  }

  .connections-guide-link:hover {
    text-decoration: underline;
  }
</style>
