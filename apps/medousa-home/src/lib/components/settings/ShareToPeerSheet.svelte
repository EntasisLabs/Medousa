<script lang="ts">
  import {
    askArtifactForReview,
    askNoteForReview,
    bringArtifactHome,
    bringNoteHome,
    listTrustedWorkshops,
    shareArtifactToPeer,
    shareNoteToPeer,
    type PeerShareMode,
    type TrustedWorkshopSummary,
  } from "$lib/utils/lanShareApi";
  import { onMount } from "svelte";

  interface Props {
    open: boolean;
    mode?: PeerShareMode;
    artifactId?: string | null;
    vaultPath?: string | null;
    label?: string | null;
    onClose?: () => void;
    onShared?: (message: string) => void;
    onError?: (message: string) => void;
  }

  let {
    open,
    mode = "share",
    artifactId = null,
    vaultPath = null,
    label = null,
    onClose,
    onShared,
    onError,
  }: Props = $props();

  let trusted = $state<TrustedWorkshopSummary[]>([]);
  let workshopId = $state("");
  let busy = $state(false);
  let loading = $state(false);

  const isAsk = $derived(mode === "ask");
  const isBring = $derived(mode === "bring");
  const itemLabel = $derived(
    label?.trim() ||
      (artifactId ? `Artifact ${artifactId.slice(0, 12)}…` : null) ||
      vaultPath ||
      "Item",
  );
  const dialogLabel = $derived(
    isAsk ? "Ask for review" : isBring ? "Bring home" : "Share to peer",
  );
  const commitLabel = $derived(isAsk ? "Ask" : isBring ? "Bring home" : "Share");
  const busyLabel = $derived(isAsk ? "Asking…" : isBring ? "Sending…" : "Sharing…");
  const emptyVerb = $derived(isAsk ? "asking" : isBring ? "bringing home" : "sharing");

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
        if (isAsk) {
          await askArtifactForReview(workshopId, artifactId);
        } else if (isBring) {
          await bringArtifactHome(workshopId, artifactId);
        } else {
          await shareArtifactToPeer(workshopId, artifactId);
        }
      } else if (vaultPath) {
        if (isAsk) {
          await askNoteForReview(workshopId, vaultPath);
        } else if (isBring) {
          await bringNoteHome(workshopId, vaultPath);
        } else {
          await shareNoteToPeer(workshopId, vaultPath);
        }
      } else {
        throw new Error(
          isAsk
            ? "Nothing to ask about."
            : isBring
              ? "Nothing to bring home."
              : "Nothing to share.",
        );
      }
      const peer = trusted.find((entry) => entry.workshopId === workshopId);
      const peerLabel = peer?.label ?? "peer";
      onShared?.(
        isAsk
          ? `Asked ${peerLabel} to review “${itemLabel}” — it will show in their Peers inbox.`
          : isBring
            ? `Brought “${itemLabel}” home to ${peerLabel} — it will show in their Peers inbox.`
            : `Shared “${itemLabel}” with ${peerLabel} — it will show in their Peers inbox.`,
      );
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
    <div class="share-peer-sheet" role="dialog" aria-modal="true" aria-label={dialogLabel}>
      <header class="share-peer-header">
        <h3>{dialogLabel}</h3>
        <button type="button" class="btn btn-sm btn-ghost" onclick={() => onClose?.()}>Close</button>
      </header>
      <p class="share-peer-lead">
        {#if isAsk}
          Ask a trusted peer to review <strong>{itemLabel}</strong>. They’ll get a Peers
          inbox card; the note/artifact imports automatically.
        {:else if isBring}
          Send <strong>{itemLabel}</strong> back to a trusted peer’s workshop. They’ll get a
          Peers inbox card; it imports automatically.
        {:else}
          Send <strong>{itemLabel}</strong> to a trusted workshop. They’ll get a Peers
          inbox card and the note/artifact will import automatically.
        {/if}
      </p>

      {#if loading}
        <p class="share-peer-muted">Loading trusted workshops…</p>
      {:else if trusted.length === 0}
        <p class="share-peer-muted">
          Trust a workshop in Settings → Nearby before {emptyVerb}.
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
        <div class="share-peer-actions">
          <button
            type="button"
            class="btn btn-sm btn-primary"
            disabled={busy || !workshopId}
            onclick={() => void submit()}
          >
            {busy ? busyLabel : commitLabel}
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
