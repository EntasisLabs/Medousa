<script lang="ts">
  import { onMount } from "svelte";
  import { Plus, Terminal, RefreshCw, Trash2 } from "@lucide/svelte";
  import { sshTargets, sshAction, sshSetAccess, sshRemove, type SshTarget, type SshConfig, type HostInspection } from "$lib/utils/sshApi";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { isTauriDesktop } from "$lib/platform";

  let targets = $state<SshTarget[]>([]);
  let adding = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let note = $state<string | null>(null);
  let name = $state("");
  let host = $state("");
  let port = $state(22);
  let username = $state("");
  let identityFile = $state("");
  let agentAccess = $state(true);
  let inspection = $state<HostInspection | null>(null);
  let inspectedEndpoint = $state("");
  let trusted = $state(false);
  const endpoint = $derived(JSON.stringify([host.trim(), port]));
  const inspected = $derived(inspection !== null && inspectedEndpoint === endpoint);
  const config = $derived<SshConfig>({ name: name.trim(), host: host.trim(), port,
    username: username.trim(), identity_file: identityFile.trim() || null, agent_access: agentAccess });

  onMount(() => { void action(refresh); });
  async function refresh() { targets = await sshTargets(); }
  async function action(work: () => Promise<void>) {
    if (busy) return;
    busy = true; error = null; note = null;
    try { await work(); } catch (e) { error = e instanceof Error ? e.message : String(e); }
    finally { busy = false; }
  }
  async function inspect() {
    const checked = endpoint;
    const result = await sshAction<HostInspection>("inspect", config);
    inspection = result; inspectedEndpoint = checked; trusted = false;
  }
  async function save() {
    if (!inspected || !trusted || !inspection) return;
    await sshAction("save", { config, host_keys: inspection.host_keys });
    adding = false; inspection = null; trusted = false;
    name = ""; host = ""; username = ""; identityFile = ""; port = 22;
    await refresh(); note = "SSH target saved. Test the connection to check authentication.";
  }
  async function open(target: SshTarget) {
    const receipt = await sshAction<{ session_id: string | null; error: string | null }>("terminal", {
      target_id: target.target_id, request_key: crypto.randomUUID(),
    });
    if (!receipt.session_id) throw new Error(receipt.error || "SSH terminal could not start");
    layout.navigateDesktop("chat");
    shellTabs.openTerminal(receipt.session_id, { title: `SSH · ${target.name}`, executionRuntimeId: null });
  }
</script>

<div class="prefs-band">
  <div class="prefs-band-head">
    <div>
      <h3 class="settings-subsection-heading">SSH targets</h3>
      <p class="settings-subsection-lead">Servers this workshop can reach. No Medousa installation needed on the server.</p>
    </div>
    <button type="button" class="btn btn-sm variant-ghost-surface" disabled={busy} onclick={() => { adding = !adding; error = null; }}>
      <Plus size={15} /> Add server
    </button>
  </div>
  {#if adding}
    <div class="ssh-form">
      <label>Name <input class="input" bind:value={name} maxlength="80" placeholder="Homelab" disabled={busy} /></label>
      <label>Host <input class="input" bind:value={host} placeholder="server.local or IP address" disabled={busy} /></label>
      <div class="ssh-fields">
        <label>Username <input class="input" bind:value={username} placeholder="medousa" disabled={busy} /></label>
        <label>Port <input class="input" type="number" min="1" max="65535" bind:value={port} disabled={busy} /></label>
      </div>
      <label>SSH key path on the workshop <input class="input" bind:value={identityFile} placeholder="Leave blank to use the workshop’s SSH agent" disabled={busy} /></label>
      <label class="ssh-check"><input type="checkbox" bind:checked={agentAccess} disabled={busy} /> Allow Medousa agents to use this server</label>
      <button type="button" class="btn btn-sm variant-soft-surface" disabled={busy || !host.trim() || !username.trim() || !name.trim()} onclick={() => void action(inspect)}>
        {busy ? "Checking…" : "Check server identity"}
      </button>
      {#if inspected && inspection}
        <p class="workshop-faint text-xs">Compare these fingerprints with the server’s host keys before trusting this connection.</p>
        <div class="ssh-fingerprints">{#each inspection.fingerprints as fingerprint}<code>{fingerprint}</code>{/each}</div>
        <label class="ssh-check"><input type="checkbox" bind:checked={trusted} disabled={busy} /> Trust this server identity</label>
        <button type="button" class="btn btn-sm variant-filled-primary" disabled={busy || !trusted} onclick={() => void action(save)}>Save connection</button>
      {/if}
    </div>
  {/if}
  <div class="prefs-stack">
    {#each targets as target (target.target_id)}
      <div class="ssh-target prefs-tile">
        <div class="ssh-target-info"><strong>{target.name}</strong><span class="workshop-faint text-xs">{target.username}@{target.host}:{target.port}</span></div>
        <label class="ssh-check text-xs"><input type="checkbox" checked={target.agent_access} disabled={busy}
          onchange={(event) => { const enabled = event.currentTarget.checked; event.currentTarget.checked = target.agent_access; void action(async () => { await sshSetAccess(target.target_id, enabled); await refresh(); }); }} /> Agent access</label>
        <div class="ssh-actions">
          <button type="button" class="btn btn-sm variant-ghost-surface" disabled={busy} onclick={() => void action(async () => { await sshAction("test", { target_id: target.target_id }); note = `Connected to ${target.name}.`; })}><RefreshCw size={14} /> Test</button>
          {#if isTauriDesktop()}<button type="button" class="btn btn-sm variant-soft-surface" disabled={busy} onclick={() => void action(() => open(target))}><Terminal size={14} /> Terminal</button>{/if}
          <button type="button" class="btn btn-sm variant-ghost-surface" title={`Remove ${target.name}`} aria-label={`Remove ${target.name}`} disabled={busy} onclick={() => void action(async () => { await sshRemove(target.target_id); await refresh(); })}><Trash2 size={14} /></button>
        </div>
      </div>
    {/each}
  </div>
  {#if !targets.length && !adding}<p class="workshop-faint text-sm">Add a server, then ask Medousa to work with it by name.</p>{/if}
  {#if note}<p class="text-sm text-content-success" role="status">{note}</p>{/if}
  {#if error}<p class="text-sm text-content-warning" role="alert">{error}</p>{/if}
</div>
<style>
  .ssh-form { display: grid; gap: .7rem; margin: .75rem 0; max-width: 38rem; }
  .ssh-form label { display: grid; gap: .3rem; font-size: .8rem; }
  .ssh-fields { display: grid; grid-template-columns: 1fr 6rem; gap: .7rem; }
  .ssh-form .ssh-check, .ssh-check { display: flex; align-items: center; gap: .5rem; }
  .ssh-fingerprints { display: grid; gap: .25rem; overflow: auto; font-size: .7rem; }
  .ssh-target { display: flex; gap: .7rem; align-items: center; flex-wrap: wrap; }
  .ssh-target-info { flex: 1; min-width: 8rem; display: grid; gap: .2rem; font-size: .85rem; }
  .ssh-actions { display: flex; gap: .25rem; }
</style>
