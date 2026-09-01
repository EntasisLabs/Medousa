<script lang="ts">
  /**
   * Settings → Connections — daemon-owned ChatGPT OAuth plus Codex/Cursor/Hermes
   * vendor runtime sign-in. These credential routes intentionally stay separate.
   */
  import { onDestroy, onMount } from "svelte";
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
    Bot,
  } from "@lucide/svelte";
  import { accountConnections } from "$lib/stores/accountConnections.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { isTauriDesktop } from "$lib/platform";
  import {
    authStatusLabel,
    beginChatgptDeviceLogin,
    beginTerminalLogin,
    accountSignOut,
    installAccountCli,
    type AccountId,
  } from "$lib/utils/accountConnections";
  import { openGuide } from "$lib/guide/openGuide";
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
    chatGptAccountAuth?: boolean;
  }

  let { chatGptAccountAuth = true }: Props = $props();

  let actionBusy = $state<string | null>(null);
  let actionNote = $state<string | null>(null);
  let actionError = $state<string | null>(null);
  let waitingFor = $state<AccountId | null>(null);
  let nativeChatGpt = $state<ChatGptOAuthConnection | null>(null);
  let nativeChatGptLoading = $state(false);
  let nativeChatGptLoaded = $state(false);
  let nativeLogin = $state<BeginChatGptOAuthResponse | null>(null);
  let nativeLoginCancelled = false;

  onMount(() => {
    if (isTauriDesktop()) void accountConnections.refresh(true);
  });

  $effect(() => {
    if (!chatGptAccountAuth) {
      nativeChatGptLoaded = false;
      nativeChatGpt = null;
      return;
    }
    if (nativeChatGptLoaded) return;
    nativeChatGptLoaded = true;
    void refreshNativeChatGpt();
  });

  onDestroy(() => {
    nativeLoginCancelled = true;
  });

  const USE_HINT =
    "In Chat, choose the runtime under the composer, then choose its provider and model inside the composer.";

  const nativeChatGptReady = $derived(chatGptOAuthReady(nativeChatGpt));

  async function refreshNativeChatGpt() {
    if (!chatGptAccountAuth) return;
    nativeChatGptLoading = true;
    try {
      nativeChatGpt = await getChatGptOAuthConnection();
    } catch (err) {
      actionError = err instanceof Error ? err.message : String(err);
    } finally {
      nativeChatGptLoading = false;
    }
  }

  async function refreshAllConnections() {
    await Promise.all([
      isTauriDesktop() ? accountConnections.refresh(true) : Promise.resolve(),
      chatGptAccountAuth ? refreshNativeChatGpt() : Promise.resolve(),
    ]);
  }

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

  async function waitForSignIn(account: AccountId): Promise<string> {
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

  async function signInCodex() {
    await withAction("codex-login", async () => {
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

  async function signInNativeChatGpt() {
    await withAction("native-chatgpt-login", async () => {
      nativeLoginCancelled = false;
      nativeLogin = await beginChatGptOAuth();
      await openUrlInDefaultBrowser(nativeLogin.verification_url);
      actionNote = `Enter code ${nativeLogin.user_code} in the browser. Waiting for approval…`;

      while (!nativeLoginCancelled && Date.now() < Date.parse(nativeLogin.expires_at_utc)) {
        const result = await completeChatGptOAuth(nativeLogin.login_id);
        if (result.status === "connected") {
          nativeChatGpt = result.connection ?? (await getChatGptOAuthConnection());
          nativeLogin = null;
          return "ChatGPT is connected to the Medousa runtime. Choose OpenAI · ChatGPT account in Chat.";
        }
        const delaySeconds = Math.max(
          1,
          result.retry_after_seconds ?? nativeLogin.poll_interval_seconds,
        );
        await new Promise((resolve) => setTimeout(resolve, delaySeconds * 1000));
      }
      nativeLogin = null;
      if (nativeLoginCancelled) return null;
      throw new Error("ChatGPT sign-in expired. Start it again to receive a new code.");
    });
  }

  async function signOutNativeChatGpt() {
    await withAction("native-chatgpt-logout", async () => {
      nativeLoginCancelled = true;
      await disconnectChatGptOAuth();
      nativeChatGpt = await getChatGptOAuthConnection();
      nativeLogin = null;
      return "ChatGPT disconnected from the Medousa runtime.";
    });
  }

  async function reopenNativeLogin() {
    if (!nativeLogin) return;
    await openUrlInDefaultBrowser(nativeLogin.verification_url);
  }

  async function signInCursor() {
    await withAction("cursor-login", async () => {
      actionNote = await beginTerminalLogin("cursor");
      return await waitForSignIn("cursor");
    });
  }

  async function signInHermes() {
    await withAction("hermes-login", async () => {
      actionNote = await beginTerminalLogin("hermes");
      return await waitForSignIn("hermes");
    });
  }

  async function signOut(account: AccountId) {
    await withAction(`${account}-logout`, () => accountSignOut(account));
  }

  async function installCli(account: AccountId) {
    await withAction(`${account}-install`, async () => {
      const result = await installAccountCli(account);
      return result.detail;
    });
  }

  function openChat() {
    if (layout.isMobile) {
      layout.setMobileTab("chat", { bump: true });
      return;
    }
    layout.navigateDesktop("chat");
  }

  const desktopCli = isTauriDesktop();
  const supported = $derived(chatGptAccountAuth || desktopCli);
  const anySignedIn = $derived(
    nativeChatGptReady ||
      accountConnections.isSignedIn("chatgpt") ||
      accountConnections.isSignedIn("cursor") ||
      accountConnections.isSignedIn("hermes"),
  );
</script>

<section class="settings-section connections">
  <header class="settings-section-header">
    <div class="min-w-0 flex-1">
      <h2 class="text-base font-semibold text-surface-50">Connections</h2>
      <p class="workshop-faint mt-1 text-sm">
        Connect accounts to the runtimes available on this host. Each connection
        stays isolated to the runtime that owns it.
      </p>
    </div>
    <button
      type="button"
      class="workshop-rail-btn shrink-0"
      title="Refresh status"
      aria-label="Refresh status"
      disabled={!supported || accountConnections.loading || nativeChatGptLoading || actionBusy != null}
      onclick={() => void refreshAllConnections()}
    >
      <RefreshCw size={15} strokeWidth={1.85} />
    </button>
  </header>

  {#if !supported}
    <p class="workshop-faint mt-2 text-sm">
      Embedded Personal uses provider credentials under Medousa Agent. Native account adapters
      are not installed on this host.
    </p>
  {:else}
    {#if accountConnections.error}
      <p class="mt-2 flex items-center gap-1.5 text-sm text-content-warning">
        <CircleAlert size={14} strokeWidth={2} />
        {accountConnections.error}
      </p>
    {/if}

    <ol class="connections-steps workshop-faint mt-3">
      <li><strong>Choose ownership</strong> — Medousa{desktopCli ? ", Codex, Cursor, or Hermes" : ""}.</li>
      <li><strong>Sign in</strong> to that connection; credentials are not shared.</li>
      <li>
        <strong>In Chat</strong>, choose the runtime under the composer. For
        Medousa, choose the provider inside the model picker.
      </li>
    </ol>

    <div class="connections-cards mt-3">
      {#if chatGptAccountAuth}
        <div class="connections-card" data-account="native-chatgpt">
        <div class="connections-card-head">
          <span class="connections-card-icon" aria-hidden="true">
            <Sparkles size={16} strokeWidth={1.85} />
          </span>
          <div class="min-w-0 flex-1">
            <p class="connections-card-title">ChatGPT account</p>
            <p class="connections-card-sub workshop-faint">Medousa runtime · subscription usage</p>
          </div>
          <span
            class="connections-status"
            class:connections-status--in={nativeChatGptReady}
            class:connections-status--out={nativeChatGpt?.status === "signed_out" || nativeChatGpt?.status === "reauth_required"}
            class:connections-status--wait={nativeChatGptLoading || actionBusy === "native-chatgpt-login"}
          >
            {#if nativeChatGptLoading}
              Checking…
            {:else if actionBusy === "native-chatgpt-login"}
              Waiting…
            {:else if nativeChatGptReady}
              <Check size={12} strokeWidth={2.5} /> Connected
            {:else if nativeChatGpt?.status === "reauth_required"}
              Reconnect
            {:else}
              Not connected
            {/if}
          </span>
        </div>

        {#if nativeLogin}
          <div class="connections-device-auth">
            <p class="connections-note workshop-faint">Enter this code in the ChatGPT sign-in page:</p>
            <code class="connections-device-code font-mono">{nativeLogin.user_code}</code>
          </div>
        {:else if nativeChatGptReady}
          <p class="connections-note workshop-faint">
            Ready for Medousa’s Instant, General, and Coder agents. In Chat,
            keep the Medousa runtime and choose OpenAI · ChatGPT account.
          </p>
        {:else if nativeChatGpt?.status === "reauth_required"}
          <p class="connections-note workshop-faint">
            The saved session can no longer refresh. Sign in again to restore it.
          </p>
        {:else}
          <p class="connections-note workshop-faint">
            Uses your ChatGPT subscription while Medousa keeps ownership of the
            agent loop and tools. This is separate from the Codex runtime below.
          </p>
        {/if}

        <div class="connections-card-actions">
          {#if nativeLogin}
            <button
              type="button"
              class="btn btn-sm variant-filled-primary"
              onclick={() => void reopenNativeLogin()}
            >
              <ArrowUpRight size={13} strokeWidth={2} /> Open sign-in
            </button>
          {:else if nativeChatGptReady}
            <button type="button" class="btn btn-sm variant-filled-primary" onclick={openChat}>
              <MessageSquare size={13} strokeWidth={2} /> Open Chat
            </button>
            <button
              type="button"
              class="btn btn-sm variant-soft-surface"
              disabled={actionBusy != null}
              onclick={() => void signOutNativeChatGpt()}
            >
              <LogOut size={13} strokeWidth={2} /> Disconnect
            </button>
          {:else}
            <button
              type="button"
              class="btn btn-sm variant-filled-primary"
              disabled={actionBusy != null || nativeChatGptLoading}
              onclick={() => void signInNativeChatGpt()}
            >
              <LogIn size={13} strokeWidth={2} />
              {nativeChatGpt?.status === "reauth_required" ? "Reconnect" : "Connect ChatGPT"}
            </button>
          {/if}
        </div>
        </div>
      {/if}

      {#if desktopCli}
        {#each [
          { info: accountConnections.connections?.chatgpt, account: "chatgpt" as const, icon: "chatgpt", title: "Codex", sub: "Codex runtime · ChatGPT account", cli: "Codex CLI", runtimeLabel: "Codex" },
          { info: accountConnections.connections?.cursor, account: "cursor" as const, icon: "cursor", title: "Cursor", sub: "Cursor coding agent", cli: "Cursor Agent CLI", runtimeLabel: "Cursor" },
          { info: accountConnections.connections?.hermes, account: "hermes" as const, icon: "hermes", title: "Hermes", sub: "Hermes Agent · ACP", cli: "Hermes Agent CLI", runtimeLabel: "Hermes" },
        ] as card (card.account)}
        {@const info = card.info}
        <div class="connections-card" data-account={card.account}>
          <div class="connections-card-head">
            <span class="connections-card-icon" aria-hidden="true">
              {#if card.icon === "chatgpt"}
                <Sparkles size={16} strokeWidth={1.85} />
              {:else if card.icon === "hermes"}
                <Bot size={16} strokeWidth={1.85} />
              {:else}
                <MousePointer2 size={16} strokeWidth={1.85} />
              {/if}
            </span>
            <div class="min-w-0 flex-1">
              <p class="connections-card-title">{card.title}</p>
              <p class="connections-card-sub workshop-faint">{card.sub}</p>
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
              {card.cli} isn’t installed yet — Medousa can install it for you with the vendor’s
              official installer.
            </p>
          {:else if waitingFor === card.account}
            <p class="connections-note workshop-faint">
              Finish sign-in in the browser or terminal. This card updates when
              the vendor CLI reports you’re signed in.
            </p>
          {:else if info?.authStatus === "signed_in"}
            <p class="connections-note workshop-faint">
              Ready. In Chat, pick {card.runtimeLabel} from the runtime control under the composer.
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
                {card.account === "hermes" ? "Reconfigure" : "Sign out"}
              </button>
            {:else}
              <button
                type="button"
                class="btn btn-sm variant-filled-primary"
                disabled={actionBusy != null}
                onclick={() =>
                  void (card.account === "chatgpt"
                    ? signInCodex()
                    : card.account === "hermes"
                      ? signInHermes()
                      : signInCursor())}
              >
                <LogIn size={13} strokeWidth={2} />
                {waitingFor === card.account
                  ? "Waiting…"
                  : actionBusy === `${card.account}-login` || actionBusy === "codex-login" || actionBusy === "hermes-login"
                    ? "Signing in…"
                    : "Sign in"}
              </button>
            {/if}
          </div>
        </div>
        {/each}
      {/if}
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

  .connections-device-auth {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    border-radius: 0.55rem;
    border: 1px solid rgb(var(--color-surface-600) / 0.35);
    background: rgb(var(--color-surface-800) / 0.42);
    padding: 0.6rem 0.7rem;
  }

  .connections-device-code {
    color: rgb(var(--color-surface-100));
    font-size: 1rem;
    font-weight: 650;
    letter-spacing: 0.12em;
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
