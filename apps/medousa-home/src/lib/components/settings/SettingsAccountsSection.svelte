<script lang="ts">
  import { onDestroy, tick, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { Check, Copy, GitPullRequest, LogOut, ExternalLink } from "@lucide/svelte";
  import { daemonUnary } from "$lib/daemon/contractClient";
  import { hostedReviews } from "$lib/code/hostedReviews.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { isTauri } from "$lib/window";
  import type { IntegrationConnection } from "$lib/integrationsClient";
  import type { ReviewProviderAuthResponse } from "$lib/types/generated/daemon_api";

  type Challenge = Extract<ReviewProviderAuthResponse, {status:"pending"}>;
  type AuthAction = {action:"begin";provider_id:string;host:string} | {action:"poll" | "open" | "cancel";flow_id:string};
  let accounts = $state<IntegrationConnection[]>([]);
  let loading = $state(true);
  let busy = $state<string | null>(null);
  let pending = $state<Challenge | null>(null);
  let paused = $state(false);
  let copied = $state(false);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let target: {providerId:string;host:string} | null = null;
  let generation = 0;
  let polling = false;
  const providers = $derived(hostedReviews.providers.filter((p) => p.operations.includes("device_auth")));
  const available = $derived(accounts.filter((a) => hostedReviews.providers.some((p) => p.id === a.kind)));
  function auth(request: AuthAction) { return invoke<ReviewProviderAuthResponse>("review_provider_auth", {request}); }
  function release(flowId: string) { void auth({action:"cancel",flow_id:flowId}).catch(() => {}); }
  $effect(() => {
    const scope = chat.workshopScopeId;
    untrack(() => {
      if (pending) release(pending.flow_id);
      generation++; pending = null; target = null; paused = false; copied = false; polling = false;
      accounts = []; error = null; notice = null; busy = null; loading = true;
      hostedReviews.reset(scope);
      void refresh(scope, generation);
    });
  });
  $effect(() => {
    const challenge = pending;
    if (!challenge || paused) return;
    const run = generation;
    const timer = setTimeout(() => void poll(challenge.flow_id, run), Math.max(5, challenge.interval) * 1000);
    return () => clearTimeout(timer);
  });
  onDestroy(() => { generation++; if (pending) release(pending.flow_id); });
  function message(cause: unknown): string {
    const raw = cause instanceof Error ? cause.message : String(cause);
    const start = raw.indexOf("{");
    if (start >= 0) {
      try { const envelope = JSON.parse(raw.slice(start)); if (typeof envelope.message === "string") return envelope.message; } catch { /* Plain transport error. */ }
    }
    return raw;
  }
  async function refresh(scope: string, run: number) {
    try {
      const [result] = await Promise.all([
        daemonUnary<{connections: IntegrationConnection[]}>("integrations.get"), hostedReviews.discover(),
      ]);
      if (scope !== chat.workshopScopeId || run !== generation) return;
      accounts = result.connections;
      if (hostedReviews.error) error = hostedReviews.error;
      if (hostedReviews.connectionId && !accounts.some((a) => a.connection_id === hostedReviews.connectionId)) hostedReviews.selectAccount(null);
    } catch (cause) { if (run === generation) error = message(cause); }
    finally { if (run === generation) loading = false; }
  }
  function select(account: IntegrationConnection) {
    let host: string | null = null;
    try { if (account.base_url) host = new URL(account.base_url).hostname; } catch { /* Invalid metadata is rejected by the workshop. */ }
    hostedReviews.selectAccount(account.connection_id, account.kind, host);
  }
  async function signIn(providerId: string, host: string) {
    if (busy) return;
    const run = ++generation; const scope = chat.workshopScopeId;
    busy = providerId; target = {providerId, host}; error = null; notice = null; paused = false; copied = false;
    try {
      const response = await auth({action:"begin",provider_id:providerId,host});
      if (run !== generation || scope !== chat.workshopScopeId) {
        if (response.status === "pending") release(response.flow_id);
        return;
      }
      if (response.status !== "pending") throw new Error("Sign-in did not return an authorization code. Try again.");
      pending = response;
      await tick();
      if (run === generation) await openSignIn(response.flow_id, run);
    } catch (cause) { if (run === generation) error = message(cause); }
    finally { if (run === generation && !pending) { busy = null; target = null; } }
  }
  async function openSignIn(flowId: string, run = generation) {
    try { await auth({action:"open",flow_id:flowId}); }
    catch (cause) { if (run === generation) error = message(cause); }
  }
  async function poll(flowId: string, run = generation) {
    if (polling || !pending || pending.flow_id !== flowId) return;
    const scope = chat.workshopScopeId;
    const accountTarget = target;
    polling = true; paused = false;
    try {
      const response = await auth({action:"poll",flow_id:flowId});
      if (run !== generation || scope !== chat.workshopScopeId || !accountTarget) return;
      if (response.status === "pending") { pending = response; error = null; }
      else if (response.status === "connected") {
        pending = null; busy = null; target = null; error = null;
        hostedReviews.selectAccount(response.connection_id, accountTarget.providerId, accountTarget.host);
        release(flowId);
        await refresh(scope, run);
        if (run === generation) notice = `Connected as ${response.account}.`;
      } else { pending = null; busy = null; target = null; release(flowId); }
    } catch (cause) { if (run === generation) { error = message(cause); paused = true; } }
    finally { if (run === generation) polling = false; }
  }
  function cancelSignIn() {
    const flow = pending;
    generation++; pending = null; target = null; busy = null; paused = false; polling = false; copied = false; error = null;
    if (flow) release(flow.flow_id);
  }
  async function copyCode() {
    const flow = pending;
    if (!flow) return;
    try { await navigator.clipboard.writeText(flow.user_code); if (pending?.flow_id === flow.flow_id) copied = true; }
    catch { error = "Could not copy the code. Select it and copy it manually."; }
  }
  async function disconnect(account: IntegrationConnection) {
    if (busy) return;
    const run = generation; const scope = chat.workshopScopeId;
    busy = account.connection_id; error = null; notice = null;
    try {
      await daemonUnary("integrations.by_connection_id.delete", {connection_id:account.connection_id});
      if (run !== generation || scope !== chat.workshopScopeId) return;
      if (hostedReviews.connectionId === account.connection_id) hostedReviews.selectAccount(null);
      await refresh(scope, run);
    } catch (cause) { if (run === generation) error = message(cause); }
    finally { if (run === generation) busy = null; }
  }
</script>

<section class="settings-section accounts-section">
  <header class="section-heading"><h2>Accounts</h2><p>Connect the services you use with this workshop.</p></header>
  {#if loading}<p class="quiet">Loading accounts…</p>
  {:else}
    {#each providers as provider (provider.id)}
      <section class="service" aria-label={provider.label}>
        <header class="service-heading">
          <div class="service-title"><GitPullRequest size={22} /><div><h3>{provider.label}</h3><p>Browse and share reviews in Medousa.</p></div></div>
          {#if isTauri()}
            <button type="button" class="sign-in" disabled={!!busy} onclick={() => void signIn(provider.id, provider.hosts[0] ?? "")}>
              <ExternalLink size={14} />{busy === provider.id ? "Waiting for browser…" : `Sign in with ${provider.label}`}
            </button>
          {/if}
        </header>
        {#each available.filter((a) => a.kind === provider.id) as account (account.connection_id)}
          <div class="account">
            <div class="account-copy"><strong>{account.label || provider.label}</strong><span>{account.base_url?.replace(/^https:\/\//, "")}</span></div>
            {#if hostedReviews.connectionId === account.connection_id}<span class="selected"><Check size={14} />Used for reviews</span>
            {:else}<button type="button" disabled={!!busy} onclick={() => select(account)}>Use for reviews</button>{/if}
            <button type="button" class="disconnect" aria-label={`Disconnect ${account.label || provider.label}`} title="Disconnect account" disabled={!!busy} onclick={() => void disconnect(account)}><LogOut size={15} /></button>
          </div>
        {/each}
        {#if pending && busy === provider.id}
          <div class="authorization">
            <p>Enter this code on {provider.label} to connect your account.</p>
            <div class="code-row"><strong class="user-code">{pending.user_code}</strong><button type="button" aria-label="Copy authorization code" onclick={() => void copyCode()}>{#if copied}<Check size={14} />Copied{:else}<Copy size={14} />Copy code{/if}</button></div>
            <div class="auth-actions"><button type="button" onclick={() => void openSignIn(pending!.flow_id)}><ExternalLink size={14} />Open sign-in page</button><button type="button" onclick={cancelSignIn}>Cancel</button>
              {#if paused}<button type="button" onclick={() => void poll(pending!.flow_id)}>Retry connection</button>{/if}
            </div>
            {#if !paused}<p class="waiting" role="status">Waiting for approval… This page updates automatically.</p>{/if}
          </div>
        {:else if busy === provider.id}<p class="help" role="status">Starting sign-in…</p>
        {:else if !isTauri()}<p class="help">Open the Medousa app to connect an account.</p>{/if}
      </section>
    {:else}<p class="quiet">Install a review provider in Settings → Packages to connect an account.</p>{/each}
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if notice}<p class="notice" role="status">{notice}</p>{/if}
</section>

<style>
  .accounts-section { max-width:52rem; }
  .section-heading { margin-bottom:1.5rem; }
  h2 { font-size:1.15rem; font-weight:600; margin:0 0 .4rem; }
  h3 { font-size:.875rem; font-weight:600; margin:0; }
  p { margin:0; line-height:1.6; }
  .section-heading p, .service-title p, .quiet, .help { color:rgb(var(--theme-text-secondary)); font-size:.8rem; }
  .service { border:1px solid rgb(var(--theme-border)/.6); border-radius:.65rem; margin-bottom:1rem; overflow:hidden; }
  .service-heading { display:flex; flex-wrap:wrap; align-items:center; justify-content:space-between; gap:1rem; padding:1.15rem; }
  .service-title { display:flex; align-items:center; gap:.85rem; }
  .service-title p { margin-top:.2rem; }
  button { display:inline-flex; align-items:center; justify-content:center; gap:.4rem; border:1px solid rgb(var(--theme-border)/.6); border-radius:.4rem; background:transparent; color:rgb(var(--theme-text)); padding:.45rem .7rem; font-size:.75rem; cursor:pointer; }
  button:hover { background:rgb(var(--theme-text)/.06); }
  button:disabled { opacity:.5; cursor:default; }
  .sign-in { background:rgb(var(--theme-text)/.06); }
  .account { display:flex; align-items:center; gap:.8rem; padding:.9rem 1.15rem; border-top:1px solid rgb(var(--theme-border)/.4); }
  .account-copy { min-width:0; flex:1; display:flex; flex-direction:column; gap:.2rem; }
  .account-copy strong { font-size:.8rem; font-weight:500; overflow-wrap:anywhere; }
  .account-copy span, .selected { color:rgb(var(--theme-text-secondary)); font-size:.7rem; }
  .selected { display:flex; align-items:center; gap:.3rem; }
  .disconnect { border:0; padding:.4rem; color:rgb(var(--theme-text-secondary)); }
  .help { padding:0 1.15rem 1rem; }
  .authorization { display:flex; flex-direction:column; gap:.8rem; border-top:1px solid rgb(var(--theme-border)/.4); padding:1.15rem; font-size:.8rem; color:rgb(var(--theme-text-secondary)); }
  .code-row { display:flex; flex-wrap:wrap; align-items:center; gap:1rem; }
  .user-code { user-select:text; font:600 1.4rem ui-monospace, monospace; letter-spacing:.12em; color:rgb(var(--theme-text)); }
  .auth-actions { display:flex; flex-wrap:wrap; gap:.5rem; }
  .waiting { font-size:.75rem; }
  .error { color:rgb(var(--color-amber-400)); font-size:.8rem; margin-top:.8rem; }
  .notice { color:rgb(var(--theme-text-secondary)); font-size:.8rem; margin-top:.8rem; }
</style>
