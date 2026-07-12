<script lang="ts">
  import { onMount } from "svelte";
  import PeerAvatar from "$lib/components/peers/PeerAvatar.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import {
    downloadShareBundle,
    exportShareBundle,
    getLanPairingStatus,
    importShareBundle,
    listTrustedWorkshops,
    pushShareBundleToWorkshop,
    revokeTrustedWorkshop,
    setLanPairingEnabled,
    type LanPairingStatus,
    type ShareConflictStrategy,
    type ShareImportResult,
    type TrustedWorkshopSummary,
  } from "$lib/utils/lanShareApi";
  import { isTauri } from "$lib/window";
  import { Share2, Upload, Users } from "@lucide/svelte";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  let trusted = $state<TrustedWorkshopSummary[]>([]);
  let lanPairing = $state<LanPairingStatus | null>(null);
  let lanBusy = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);
  let includeEnvironment = $state(true);
  let conflictStrategy = $state<ShareConflictStrategy>("rename");
  let lastBundle = $state<Record<string, unknown> | null>(null);
  let importInput: HTMLInputElement | undefined = $state();

  const CONFLICT_OPTIONS: {
    id: ShareConflictStrategy;
    label: string;
    hint: string;
  }[] = [
    { id: "rename", label: "Rename", hint: "Keep both — duplicates get a new name" },
    { id: "skip", label: "Skip", hint: "Leave existing views alone" },
    { id: "overwrite", label: "Overwrite", hint: "Replace matching views" },
  ];

  function openPeers() {
    if (mobile || layout.isMobile) {
      layout.openMore("peers");
      return;
    }
    layout.navigateDesktop("peers", { bump: true });
  }

  async function refreshTrusted() {
    if (!isTauri()) return;
    try {
      trusted = await listTrustedWorkshops();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function refreshLanPairing() {
    if (!isTauri()) return;
    try {
      lanPairing = await getLanPairingStatus();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function toggleLanPairing(enabled: boolean) {
    lanBusy = true;
    error = null;
    success = null;
    try {
      lanPairing = await setLanPairingEnabled(enabled);
      success = lanPairing.message;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      await refreshLanPairing();
    } finally {
      lanBusy = false;
    }
  }

  onMount(() => {
    void refreshTrusted();
    void refreshLanPairing();
  });

  async function handleExport() {
    busy = true;
    error = null;
    success = null;
    try {
      const customSurfaces = (environment.spec?.surfaces ?? [])
        .filter((surface) => surface.kind === "custom")
        .map((surface) => surface.id);
      const bundle = await exportShareBundle({
        includeEnvironment,
        surfaceIds: includeEnvironment ? customSurfaces : [],
      });
      lastBundle = bundle;
      downloadShareBundle(bundle);
      success = "Bundle ready — downloaded, and ready to send to a peer.";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function handleImportFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    busy = true;
    error = null;
    success = null;
    try {
      const text = await file.text();
      const bundle = JSON.parse(text) as Record<string, unknown>;
      const result = await importShareBundle({ bundle, conflictStrategy });
      await environment.load();
      success = formatImportResult(result);
      lastBundle = bundle;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function handlePush(workshopId: string) {
    if (!lastBundle) {
      error = "Export a bundle first, then send it.";
      return;
    }
    busy = true;
    error = null;
    success = null;
    try {
      const result = await pushShareBundleToWorkshop({
        workshopId,
        bundle: lastBundle,
        conflictStrategy,
      });
      success = `Sent to peer — ${formatImportResult(result)}`;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function revokeTrust(workshopId: string) {
    busy = true;
    error = null;
    try {
      await revokeTrustedWorkshop(workshopId);
      await workshops.load();
      await refreshTrusted();
      success = "Trust revoked.";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  function formatImportResult(result: ShareImportResult): string {
    return `Imported ${result.surfacesImported} views, ${result.componentsImported} widgets, ${result.vaultNotesImported} notes, ${result.artifactsImported} artifacts.`;
  }

  function peerStatus(workshop: TrustedWorkshopSummary): "ready" | "reconnect" {
    return workshop.hasSessionToken ? "ready" : "reconnect";
  }

  function peerMeta(workshop: TrustedWorkshopSummary): string {
    if (!workshop.hasSessionToken) return "Needs reconnect";
    if (workshop.inbound) return "Connected to you";
    return "Trusted peer";
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Nearby</h2>
    <p class="workshop-faint mt-1 text-sm">
      Peers rail for inbox and connect — this page for pairing, trust, and canvas backups.
    </p>
  </header>

  <div class="nearby-callout mt-5">
    <span class="nearby-callout-icon" aria-hidden="true">
      <Users size={16} strokeWidth={1.75} />
    </span>
    <div class="min-w-0 flex-1">
      <p class="nearby-callout-title">Peers</p>
      <p class="nearby-callout-body">
        Inbox and share only — peers never appear in the workshop switcher.
      </p>
    </div>
    <button type="button" class="btn btn-sm variant-filled-primary shrink-0" onclick={openPeers}>
      Open Peers
    </button>
  </div>

  <div class="mt-6">
    <h3 class="settings-subsection-heading">Pairing window</h3>
    <p class="settings-subsection-lead">
      Open briefly on Wi‑Fi to pair phones and peers. The engine restarts, then returns to
      loopback-only — already-paired clients keep working.
    </p>
    <div class="settings-toggle-list">
      <label class="settings-toggle-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Allow LAN pairing</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            {#if !isTauri()}
              Connect to the workshop host to change pairing.
            {:else if lanPairing?.enabled}
              On — engine is listening on the network
            {:else}
              Off — engine is loopback-only
            {/if}
          </span>
        </span>
        <input
          type="checkbox"
          class="checkbox shrink-0"
          checked={lanPairing?.enabled ?? false}
          disabled={lanBusy || !isTauri()}
          onchange={(event) =>
            void toggleLanPairing((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>
    </div>
    {#if lanPairing?.bind}
      <p class="nearby-footnote">
        {lanPairing.message}
        <span class="nearby-footnote-mono">{lanPairing.bind}</span>
      </p>
    {/if}
  </div>

  <div class="mt-6">
    <h3 class="settings-subsection-heading">Trusted peers</h3>
    <p class="settings-subsection-lead">
      Revoke removes trust. Reconnect from Peers if a session goes stale.
    </p>
    {#if trusted.length === 0}
      <p class="workshop-faint text-sm">No peers yet — open Peers to connect.</p>
    {:else}
      <div class="settings-toggle-list">
        {#each trusted as workshop (workshop.workshopId)}
          <div class="settings-toggle-row settings-metric-row nearby-peer-row">
            <PeerAvatar label={workshop.label} status={peerStatus(workshop)} />
            <span class="min-w-0 flex-1">
              <span class="block text-sm font-medium text-surface-100">{workshop.label}</span>
              <span class="workshop-faint mt-0.5 block text-xs">{peerMeta(workshop)}</span>
            </span>
            <button
              type="button"
              class="btn btn-sm btn-ghost shrink-0 text-error-400"
              disabled={busy}
              onclick={() => void revokeTrust(workshop.workshopId)}
            >
              Revoke
            </button>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <div class="mt-6">
    <h3 class="settings-subsection-heading">Canvas backup</h3>
    <p class="settings-subsection-lead">
      Export custom views as a file, import one, or send the last export to a trusted peer.
    </p>

    <div class="settings-toggle-list">
      <label class="settings-toggle-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Include views</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Custom canvas rooms and widgets in the bundle
          </span>
        </span>
        <input
          type="checkbox"
          class="checkbox shrink-0"
          bind:checked={includeEnvironment}
          disabled={busy}
        />
      </label>
    </div>

    <p class="settings-subsection-lead mt-4 mb-2">If names collide</p>
    <div class="grid gap-2 sm:grid-cols-3">
      {#each CONFLICT_OPTIONS as option (option.id)}
        <button
          type="button"
          class="settings-depth-card {conflictStrategy === option.id
            ? 'settings-depth-card-active'
            : ''}"
          disabled={busy}
          aria-pressed={conflictStrategy === option.id}
          onclick={() => (conflictStrategy = option.id)}
        >
          <span class="block text-sm font-medium text-surface-100">{option.label}</span>
          <span class="workshop-faint mt-1 block text-xs leading-snug">{option.hint}</span>
        </button>
      {/each}
    </div>

    <div class="nearby-actions mt-4">
      <button
        type="button"
        class="btn btn-sm variant-filled-primary"
        disabled={busy}
        onclick={() => void handleExport()}
      >
        <Share2 size={14} />
        Export
      </button>
      <button
        type="button"
        class="btn btn-sm variant-soft"
        disabled={busy}
        onclick={() => importInput?.click()}
      >
        <Upload size={14} />
        Import file
      </button>
      <input
        bind:this={importInput}
        type="file"
        accept="application/json,.json"
        class="hidden"
        onchange={handleImportFile}
      />
      {#if lastBundle}
        <span class="nearby-ready-pill">Ready to send</span>
      {/if}
    </div>
  </div>

  {#if trusted.length > 0}
    <div class="mt-6">
      <h3 class="settings-subsection-heading">Send to peer</h3>
      <p class="settings-subsection-lead">
        {#if lastBundle}
          Sends the last exported or imported bundle.
        {:else}
          Export a bundle first, then pick a peer below.
        {/if}
      </p>
      <div class="settings-toggle-list">
        {#each trusted as workshop (workshop.workshopId)}
          <div class="settings-toggle-row settings-metric-row nearby-peer-row">
            <PeerAvatar label={workshop.label} status={peerStatus(workshop)} />
            <span class="min-w-0 flex-1">
              <span class="block text-sm font-medium text-surface-100">{workshop.label}</span>
              <span class="workshop-faint mt-0.5 block text-xs">
                {#if !workshop.hasSessionToken}
                  Needs reconnect before send
                {:else}
                  {peerMeta(workshop)}
                {/if}
              </span>
            </span>
            <button
              type="button"
              class="btn btn-sm variant-filled-primary shrink-0"
              disabled={busy || !lastBundle || !workshop.hasSessionToken}
              onclick={() => void handlePush(workshop.workshopId)}
            >
              Send
            </button>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  {#if error}
    <p class="nearby-feedback nearby-feedback-error">{error}</p>
  {/if}
  {#if success}
    <p class="nearby-feedback nearby-feedback-ok">{success}</p>
  {/if}
</section>

<style>
  .nearby-callout {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.85rem 0.9rem;
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.4);
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.35);
  }

  .nearby-callout-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2.25rem;
    height: 2.25rem;
    flex-shrink: 0;
    border-radius: 0.5rem;
    color: rgb(var(--color-primary-200));
    background: rgb(var(--color-primary-500) / 0.12);
  }

  .nearby-callout-title {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }

  .nearby-callout-body {
    margin: 0.2rem 0 0;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
  }

  .nearby-footnote {
    margin: 0.55rem 0 0;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
  }

  .nearby-footnote-mono {
    display: block;
    margin-top: 0.15rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.6875rem;
    color: rgb(var(--shell-muted, var(--color-surface-500)));
  }

  .nearby-peer-row {
    cursor: default;
  }

  .nearby-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
  }

  .nearby-ready-pill {
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--color-success-300));
    padding: 0.2rem 0.5rem;
    border-radius: 999px;
    background: rgb(var(--color-success-500) / 0.12);
  }

  .nearby-feedback {
    margin: 0.85rem 0 0;
    font-size: 0.75rem;
  }

  .nearby-feedback-error {
    color: rgb(var(--color-error-300));
  }

  .nearby-feedback-ok {
    color: rgb(var(--color-success-300));
  }
</style>
