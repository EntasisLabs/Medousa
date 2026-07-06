<script lang="ts">
  import {
    listTrustedWorkshops,
    shareArtifactToPeer,
    shareNoteToPeer,
    type ShareConflictStrategy,
    type TrustedWorkshopSummary,
  } from "$lib/utils/lanShareApi";
  import { onMount } from "svelte";

  interface Props {
    open: boolean;
    artifactId?: string | null;
    vaultPath?: string | null;
    label?: string | null;
    onClose?: () => void;
    onShared?: (message: string) => void;
    onError?: (message: string) => void;
  }

  let {
    open,
    artifactId = null,
    vaultPath = null,
    label = null,
    onClose,
    onShared,
    onError,
  }: Props = $props();

  let trusted = $state<TrustedWorkshopSummary[]>([]);
  let workshopId = $state("");
  let conflictStrategy = $state<ShareConflictStrategy>("rename");
  let busy = $state(false);
  let loading = $state(false);

  const itemLabel = $derived(
    label?.trim() ||
      (artifactId ? `Artifact ${artifactId.slice(0, 12)}…` : null) ||
      vaultPath ||
      "Item",
  );

  async function loadTrusted() {
    loading = true;
    try {
      trusted = await listTrustedWorkshops();
      const sendable = trusted.filter((entry) => entry.hasSessionToken);
      if (!workshopId && sendable.length > 0) {
        workshopId = sendable[0]!.workshopId;
      }
    } catch (err) {
      onError?.(err instanceof Error ? err.message : String(err));
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (open) {
      void loadTrusted();
    }
  });

  onMount(() => {
    if (open) void loadTrusted();
  });

  async function submit() {
    if (!workshopId) {
      onError?.("Choose a trusted workshop.");
      return;
    }
    busy = true;
    try {
      if (artifactId) {
        await shareArtifactToPeer(workshopId, artifactId, conflictStrategy);
      } else if (vaultPath) {
        await shareNoteToPeer(workshopId, vaultPath, conflictStrategy);
      } else {
        throw new Error("Nothing to share.");
      }
      const peer = trusted.find((entry) => entry.workshopId === workshopId);
      onShared?.(`Shared “${itemLabel}” with ${peer?.label ?? "peer"}.`);
      onClose?.();
    } catch (err) {
      onError?.(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }
</script>

{#if open}
  <div
    class="share-peer-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose?.();
    }}
  >
    <div class="share-peer-sheet" role="dialog" aria-modal="true" aria-label="Share to peer">
      <header class="share-peer-header">
        <h3>Share to peer</h3>
        <button type="button" class="btn btn-sm btn-ghost" onclick={() => onClose?.()}>Close</button>
      </header>
      <p class="share-peer-lead">Send <strong>{itemLabel}</strong> to a trusted workshop.</p>

      {#if loading}
        <p class="share-peer-muted">Loading trusted workshops…</p>
      {:else if trusted.length === 0}
        <p class="share-peer-muted">
          Trust a workshop in Settings → Nearby before sharing.
        </p>
      {:else}
        <label class="share-peer-field">
          <span>Workshop</span>
          <select bind:value={workshopId} disabled={busy}>
            {#each trusted as workshop (workshop.workshopId)}
              <option value={workshop.workshopId} disabled={!workshop.hasSessionToken}>
                {workshop.label}{workshop.hasSessionToken ? "" : " (needs re-trust)"}
              </option>
            {/each}
          </select>
        </label>
        <label class="share-peer-field">
          <span>If it already exists</span>
          <select bind:value={conflictStrategy} disabled={busy}>
            <option value="rename">Rename</option>
            <option value="skip">Skip</option>
            <option value="overwrite">Overwrite</option>
          </select>
        </label>
        <div class="share-peer-actions">
          <button
            type="button"
            class="btn btn-sm btn-primary"
            disabled={busy || !workshopId}
            onclick={() => void submit()}
          >
            {busy ? "Sharing…" : "Share"}
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .share-peer-backdrop {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    background: rgb(0 0 0 / 0.45);
    padding: 1rem;
  }

  .share-peer-sheet {
    width: min(28rem, 100%);
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: rgb(var(--color-surface-900));
    padding: 1rem;
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.4);
  }

  .share-peer-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .share-peer-header h3 {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 600;
    color: rgb(var(--color-surface-50));
  }

  .share-peer-lead,
  .share-peer-muted {
    margin: 0.55rem 0 0.85rem;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .share-peer-lead strong {
    color: rgb(var(--color-surface-200));
  }

  .share-peer-field {
    display: grid;
    gap: 0.25rem;
    margin-bottom: 0.65rem;
    font-size: 0.75rem;
  }

  .share-peer-field span {
    color: rgb(var(--color-surface-400));
  }

  .share-peer-field select {
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 50%, transparent);
    padding: 0.35rem 0.5rem;
    color: rgb(var(--color-surface-100));
  }

  .share-peer-actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
