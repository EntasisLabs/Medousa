<script lang="ts">
  import { onMount } from "svelte";
  import { Plus, Terminal, RefreshCw, Trash2, Server, Ellipsis } from "@lucide/svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import { sshTargets, sshAction, sshSetAccess, sshRemove, type SshTarget, type SshConfig, type HostInspection } from "$lib/utils/sshApi";
  import { openSshTerminal } from "$lib/utils/terminalActions";
  import { isTauriDesktop } from "$lib/platform";

  let targets = $state<SshTarget[]>([]);
  let adding = $state(false);
  let busy = $state(false);
  let menuTarget = $state<string | null>(null);
  let menuTrigger: HTMLButtonElement | null = null;
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
    if (menuTarget) { menuTarget = null; menuTrigger?.focus(); }
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
      <div class="ssh-target">
        <span class="ssh-target-icon"><Server size={18} strokeWidth={1.6} /></span>
        <div class="ssh-target-info"><strong>{target.name}</strong><span>{target.username}@{target.host}:{target.port}</span></div>
        <div class="ssh-actions">
          <label class="ssh-access">Agent access <input class="ssh-switch" type="checkbox" role="switch" aria-label={`Agent access for ${target.name}`} checked={target.agent_access} disabled={busy}
            onchange={(event) => { const enabled = event.currentTarget.checked; event.currentTarget.checked = target.agent_access; void action(async () => { await sshSetAccess(target.target_id, enabled); await refresh(); }); }} /></label>
          {#if isTauriDesktop()}<button type="button" class="btn btn-sm variant-soft-surface" disabled={busy} onclick={() => void action(() => openSshTerminal(target))}><Terminal size={14} /> Terminal</button>{/if}
          <OverflowMenu
            open={menuTarget === target.target_id}
            onOpenChange={(open) => { menuTarget = open ? target.target_id : null; if (!open) menuTrigger?.focus(); }}
            mobileTitle={target.name}
            panelWidth={184}
          >
            {#snippet trigger({ open, toggle })}
              <button type="button" class="ssh-more" class:active={open} aria-label={`More actions for ${target.name}`} title="More actions" aria-haspopup="menu" aria-expanded={open} disabled={busy}
                onclick={(event) => { menuTrigger = event.currentTarget; toggle(); }}><Ellipsis size={17} /></button>
            {/snippet}
            <button type="button" role="menuitem" class="ssh-menu-item" disabled={busy} onclick={() => void action(async () => { await sshAction("test", { target_id: target.target_id }); note = `Connected to ${target.name}.`; })}><RefreshCw size={14} /> Test connection</button>
            <button type="button" role="menuitem" class="ssh-menu-item ssh-menu-remove" disabled={busy} onclick={() => void action(async () => { await sshRemove(target.target_id); await refresh(); })}><Trash2 size={14} /> Remove server</button>
          </OverflowMenu>
        </div>
      </div>
    {/each}
  </div>
  {#if !targets.length && !adding}<p class="workshop-faint text-sm">Add a server, then ask Medousa to work with it by name.</p>{/if}
  {#if note}<p class="text-sm text-content-success" role="status">{note}</p>{/if}
  {#if error}<p class="text-sm text-content-warning" role="alert">{error}</p>{/if}
</div>
<style>
  .prefs-band { margin-top: 1.25rem; }
  .prefs-band-head { display: flex; align-items: start; justify-content: space-between; gap: .75rem; margin-bottom: .6rem; }
  .prefs-band-head > button { flex-shrink: 0; }
  .settings-subsection-heading { margin-bottom: .15rem; }
  .settings-subsection-lead { margin-bottom: 0; }
  .prefs-stack { display: grid; gap: .5rem; }
  .ssh-form { display: grid; gap: .7rem; margin: .75rem 0; max-width: 38rem; }
  .ssh-form label { display: grid; gap: .3rem; font-size: .8rem; }
  .ssh-fields { display: grid; grid-template-columns: 1fr 6rem; gap: .7rem; }
  .ssh-form .ssh-check { display: flex; align-items: center; gap: .5rem; }
  .ssh-fingerprints { display: grid; gap: .25rem; overflow: auto; font-size: .7rem; }
  .ssh-target { display: flex; gap: .65rem; align-items: center; flex-wrap: wrap; min-height: 3.25rem; padding: .65rem .75rem; border: 1px solid rgb(var(--color-surface-500) / .32); border-radius: .65rem; background: rgb(var(--color-surface-900) / .28); }
  .ssh-target-icon { display: flex; flex-shrink: 0; color: rgb(var(--theme-text-quiet)); }
  .ssh-target-info { flex: 1; min-width: 8rem; display: grid; gap: .2rem; overflow-wrap: anywhere; }
  .ssh-target-info strong { font-size: .8rem; font-weight: 550; color: rgb(var(--color-surface-100)); }
  .ssh-target-info > span { font-size: .68rem; color: rgb(var(--theme-text-quiet)); }
  .ssh-actions { display: flex; align-items: center; gap: .5rem; }
  .ssh-access { display: flex; align-items: center; gap: .5rem; margin-right: .35rem; white-space: nowrap; font-size: .72rem; color: rgb(var(--theme-text-tertiary)); cursor: pointer; }
  .ssh-switch { appearance: none; width: 1.7rem; height: 1rem; padding: 0; margin: 0; border: 1px solid rgb(var(--color-surface-500) / .6); border-radius: 1rem; background: rgb(var(--color-surface-500) / .5); background-image: none; cursor: pointer; transition: background .15s; }
  .ssh-switch::after { content: ""; display: block; width: .7rem; height: .7rem; margin: .08rem; border-radius: 50%; background: rgb(var(--color-surface-100)); transition: transform .15s; }
  .ssh-switch:checked { background: rgb(var(--color-primary-500)); border-color: transparent; background-image: none; }
  .ssh-switch:checked::after { transform: translateX(.7rem); }
  .ssh-switch:focus-visible, .ssh-more:focus-visible { outline: 2px solid rgb(var(--color-primary-400)); outline-offset: 3px; }
  .ssh-switch:disabled, .ssh-more:disabled { opacity: .5; cursor: default; }
  .ssh-more { display: flex; align-items: center; justify-content: center; width: 1.8rem; height: 1.8rem; border: 0; border-radius: .4rem; background: transparent; color: rgb(var(--theme-text-tertiary)); cursor: pointer; }
  .ssh-more:hover, .ssh-more.active { background: rgb(var(--color-surface-500) / .2); color: rgb(var(--color-surface-100)); }
  .ssh-menu-item { display: flex; align-items: center; gap: .55rem; width: 100%; padding: .5rem .6rem; border: 0; border-radius: .3rem; background: transparent; color: rgb(var(--theme-text-secondary)); font-size: .75rem; text-align: left; cursor: pointer; }
  .ssh-menu-item:hover, .ssh-menu-item:focus-visible { background: rgb(var(--color-surface-500) / .2); outline: none; }
  .ssh-menu-remove { color: rgb(var(--color-error-400)); }
  @media (max-width: 640px) {
    .ssh-actions { flex-basis: 100%; justify-content: flex-end; padding-left: 1.8rem; }
    .ssh-access { margin-right: auto; }
  }
</style>
