<script lang="ts">
  import { onMount } from "svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import {
    downloadShareBundle,
    exportShareBundle,
    importShareBundle,
    listTrustedWorkshops,
    pushShareBundleToWorkshop,
    revokeTrustedWorkshop,
    type ShareConflictStrategy,
    type ShareImportResult,
    type TrustedWorkshopSummary,
  } from "$lib/utils/lanShareApi";
  import { isTauri } from "$lib/window";
  import { Share2, Upload, Users } from "@lucide/svelte";

  let trusted = $state<TrustedWorkshopSummary[]>([]);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let success = $state<string | null>(null);
  let includeEnvironment = $state(true);
  let conflictStrategy = $state<ShareConflictStrategy>("rename");
  let lastBundle = $state<Record<string, unknown> | null>(null);
  let pushWorkshopId = $state("");
  let importInput: HTMLInputElement | undefined = $state();

  function openPeers() {
    layout.navigateDesktop("peers", { bump: true });
  }

  async function refreshTrusted() {
    if (!isTauri()) return;
    try {
      trusted = await listTrustedWorkshops();
      if (!pushWorkshopId && trusted.length > 0) {
        pushWorkshopId = trusted[0]!.workshopId;
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  onMount(() => {
    void refreshTrusted();
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
      success = "Share bundle exported.";
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

  async function handlePush() {
    if (!lastBundle) {
      error = "Export a bundle first, then push it.";
      return;
    }
    if (!pushWorkshopId) {
      error = "Choose a trusted workshop.";
      return;
    }
    busy = true;
    error = null;
    success = null;
    try {
      const result = await pushShareBundleToWorkshop({
        workshopId: pushWorkshopId,
        bundle: lastBundle,
        conflictStrategy,
      });
      success = `Pushed to peer — ${formatImportResult(result)}`;
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
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Nearby &amp; sharing</h2>
    <p class="workshop-faint mt-1 text-sm">
      Connect and message peers from the Peers rail. Use this page for canvas bundle export and
      advanced trust management.
    </p>
  </header>

  <div class="lan-share-cta">
    <div>
      <p class="lan-share-cta-title">Peers</p>
      <p class="lan-share-cta-body">
        Show your invite QR, one-tap connect on the same Wi‑Fi, and your inbox.
      </p>
    </div>
    <button type="button" class="btn btn-sm btn-primary" onclick={openPeers}>
      <Users size={14} />
      Open Peers
    </button>
  </div>

  <div class="lan-share-block">
    <h3 class="lan-share-heading">Trusted workshops</h3>
    {#if trusted.length === 0}
      <p class="lan-share-empty">No trusted peers yet — connect from Peers.</p>
    {:else}
      <ul class="lan-share-list">
        {#each trusted as workshop (workshop.workshopId)}
          <li class="lan-share-row">
            <div class="lan-share-row-copy">
              <p class="lan-share-row-title">{workshop.label}</p>
              <p class="lan-share-row-meta">{workshop.daemonUrl}</p>
            </div>
            <button
              type="button"
              class="btn btn-sm btn-ghost"
              disabled={busy}
              onclick={() => void revokeTrust(workshop.workshopId)}
            >
              Revoke
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="lan-share-block">
    <h3 class="lan-share-heading">Share bundle</h3>
    <p class="lan-share-lead">
      Export custom canvas views as a file, or push a full bundle to a trusted peer.
    </p>
    <label class="lan-share-checkbox">
      <input type="checkbox" bind:checked={includeEnvironment} disabled={busy} />
      Include custom canvas views and widgets
    </label>
    <label class="lan-share-field">
      <span>Import conflicts</span>
      <select bind:value={conflictStrategy} disabled={busy}>
        <option value="rename">Rename duplicates</option>
        <option value="skip">Skip duplicates</option>
        <option value="overwrite">Overwrite duplicates</option>
      </select>
    </label>
    <div class="lan-share-actions">
      <button type="button" class="btn btn-sm btn-primary" disabled={busy} onclick={() => void handleExport()}>
        <Share2 size={14} />
        Export bundle
      </button>
      <button type="button" class="btn btn-sm btn-ghost" disabled={busy} onclick={() => importInput?.click()}>
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
    </div>
    {#if trusted.length > 0}
      <div class="lan-share-push">
        <label class="lan-share-field">
          <span>Push to trusted workshop</span>
          <select bind:value={pushWorkshopId} disabled={busy}>
            {#each trusted as workshop (workshop.workshopId)}
              <option value={workshop.workshopId}>{workshop.label}</option>
            {/each}
          </select>
        </label>
        <button
          type="button"
          class="btn btn-sm btn-primary"
          disabled={busy || !lastBundle}
          onclick={() => void handlePush()}
        >
          Push bundle
        </button>
      </div>
    {/if}
  </div>

  {#if error}
    <p class="lan-share-error">{error}</p>
  {/if}
  {#if success}
    <p class="lan-share-success">{success}</p>
  {/if}
</section>

<style>
  .lan-share-cta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    margin-top: 1rem;
    padding: 0.85rem 0.9rem;
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-primary-500) 30%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 10%, transparent);
  }

  .lan-share-cta-title {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .lan-share-cta-body,
  .lan-share-lead,
  .lan-share-empty {
    margin: 0.25rem 0 0;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .lan-share-block {
    margin-top: 1.25rem;
  }

  .lan-share-heading {
    margin: 0 0 0.45rem;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .lan-share-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.4rem;
  }

  .lan-share-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.55rem 0.65rem;
    border-radius: 0.55rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-700) 45%, transparent);
  }

  .lan-share-row-title {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    color: rgb(var(--color-surface-100));
  }

  .lan-share-row-meta {
    margin: 0.1rem 0 0;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-500));
    overflow-wrap: anywhere;
  }

  .lan-share-checkbox {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    margin-bottom: 0.65rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-300));
  }

  .lan-share-field {
    display: grid;
    gap: 0.25rem;
    margin-bottom: 0.65rem;
    font-size: 0.75rem;
  }

  .lan-share-field span {
    color: rgb(var(--color-surface-400));
  }

  .lan-share-field select {
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 60%, transparent);
    padding: 0.35rem 0.5rem;
    color: rgb(var(--color-surface-100));
  }

  .lan-share-actions,
  .lan-share-push {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 0.45rem;
  }

  .lan-share-push {
    margin-top: 0.75rem;
  }

  .lan-share-error {
    margin: 0.75rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }

  .lan-share-success {
    margin: 0.75rem 0 0;
    font-size: 0.75rem;
    color: rgb(var(--color-success-300));
  }
</style>
