<script lang="ts">
  import {
    listIntroWorkshops,
    meshAcceptIntro,
    meshDeclineIntro,
    meshListIntroCandidates,
    meshListIntros,
    meshRequestIntro,
    oppositeEndpoints,
    type IntroWorkshopSummary,
    type MeshIntroCandidate,
    type MeshIntroRecord,
  } from "$lib/utils/meshIntroApi";
  import { onMount } from "svelte";

  interface Props {
    open: boolean;
    onClose?: () => void;
    onStatus?: (message: string) => void;
    onError?: (message: string) => void;
  }

  let { open, onClose, onStatus, onError }: Props = $props();

  let workshops = $state<IntroWorkshopSummary[]>([]);
  let workshopId = $state("");
  let candidates = $state<MeshIntroCandidate[]>([]);
  let intros = $state<MeshIntroRecord[]>([]);
  let targetDeviceId = $state("");
  let note = $state("");
  let loading = $state(false);
  let busy = $state(false);
  let acceptedHints = $state<MeshIntroRecord | null>(null);

  const pendingForMe = $derived(
    intros.filter((intro) => intro.status === "pending"),
  );
  const sendableWorkshops = $derived(workshops.filter((w) => w.hasSessionToken));

  async function refresh() {
    if (!workshopId) {
      candidates = [];
      intros = [];
      return;
    }
    loading = true;
    try {
      const [nextCandidates, nextIntros] = await Promise.all([
        meshListIntroCandidates(workshopId),
        meshListIntros(workshopId, "all"),
      ]);
      candidates = nextCandidates;
      intros = nextIntros;
      if (!targetDeviceId && nextCandidates.length > 0) {
        targetDeviceId = nextCandidates[0]!.deviceId;
      }
    } catch (err) {
      onError?.(err instanceof Error ? err.message : String(err));
    } finally {
      loading = false;
    }
  }

  async function loadWorkshops() {
    loading = true;
    try {
      workshops = await listIntroWorkshops();
      const sendable = workshops.filter((entry) => entry.hasSessionToken);
      if (!workshopId && sendable.length > 0) {
        workshopId = sendable[0]!.workshopId;
      }
      await refresh();
    } catch (err) {
      onError?.(err instanceof Error ? err.message : String(err));
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (open) {
      acceptedHints = null;
      void loadWorkshops();
    }
  });

  onMount(() => {
    if (open) void loadWorkshops();
  });

  async function requestIntro() {
    if (!workshopId || !targetDeviceId) {
      onError?.("Choose a workshop and a person to meet.");
      return;
    }
    busy = true;
    try {
      await meshRequestIntro(workshopId, targetDeviceId, note.trim() || null);
      note = "";
      onStatus?.("Intro request sent — waiting for them to accept.");
      await refresh();
    } catch (err) {
      onError?.(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }

  async function accept(intro: MeshIntroRecord) {
    busy = true;
    try {
      const accepted = await meshAcceptIntro(workshopId, intro.id);
      acceptedHints = accepted;
      onStatus?.(`Intro accepted with ${accepted.requesterDisplayName}.`);
      await refresh();
    } catch (err) {
      onError?.(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }

  async function decline(intro: MeshIntroRecord) {
    busy = true;
    try {
      await meshDeclineIntro(workshopId, intro.id);
      onStatus?.("Intro declined.");
      await refresh();
    } catch (err) {
      onError?.(err instanceof Error ? err.message : String(err));
    } finally {
      busy = false;
    }
  }

  function hintLine(intro: MeshIntroRecord): string {
    const endpoints = oppositeEndpoints(intro);
    if (!endpoints) return "No dial hints yet.";
    const parts = [
      endpoints.lanBaseUrl ? `LAN ${endpoints.lanBaseUrl}` : null,
      endpoints.irohTicket ? "Iroh ticket ready" : null,
      endpoints.irohEndpointId ? `Endpoint ${endpoints.irohEndpointId.slice(0, 10)}…` : null,
    ].filter(Boolean);
    return parts.join(" · ") || "No dial hints yet.";
  }
</script>

{#if open}
  <div
    class="intro-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose?.();
    }}
  >
    <div class="intro-sheet" role="dialog" aria-modal="true" aria-label="Meet via workshop">
      <header class="intro-header">
        <h3>Meet via workshop</h3>
        <button type="button" class="btn btn-sm btn-ghost" onclick={() => onClose?.()}>Close</button>
      </header>
      <p class="intro-lead">
        Ask this workshop to introduce you to another paired client. Endpoints stay private until
        they accept — then you can dial them directly.
      </p>

      {#if sendableWorkshops.length === 0}
        <p class="intro-muted">
          Connect a portal or peer door to a workshop first. The host must also grant
          <code>client.rendezvous</code> in Settings → Sharing.
        </p>
      {:else}
        <label class="intro-field">
          <span>Workshop door</span>
          <select
            bind:value={workshopId}
            disabled={busy || loading}
            onchange={() => void refresh()}
          >
            {#each sendableWorkshops as workshop (workshop.workshopId)}
              <option value={workshop.workshopId}>{workshop.label}</option>
            {/each}
          </select>
        </label>

        {#if loading}
          <p class="intro-muted">Loading…</p>
        {:else}
          {#if pendingForMe.length > 0}
            <section class="intro-section">
              <h4>Pending</h4>
              {#each pendingForMe as intro (intro.id)}
                <div class="intro-card">
                  <p class="intro-card-title">
                    {intro.requesterDisplayName}
                    <span aria-hidden="true">→</span>
                    {intro.targetDisplayName}
                  </p>
                  {#if intro.note}
                    <p class="intro-card-note">{intro.note}</p>
                  {/if}
                  <div class="intro-card-actions">
                    {#if intro.youAre === "target"}
                      <button
                        type="button"
                        class="btn btn-sm btn-primary"
                        disabled={busy}
                        onclick={() => void accept(intro)}
                      >
                        Accept
                      </button>
                      <button
                        type="button"
                        class="btn btn-sm btn-ghost"
                        disabled={busy}
                        onclick={() => void decline(intro)}
                      >
                        Decline
                      </button>
                    {:else}
                      <span class="intro-waiting">Waiting for them…</span>
                    {/if}
                  </div>
                </div>
              {/each}
            </section>
          {/if}

          <section class="intro-section">
            <h4>Request intro</h4>
            {#if candidates.length === 0}
              <p class="intro-muted">
                No other clients with rendezvous on this workshop yet.
              </p>
            {:else}
              <label class="intro-field">
                <span>Meet</span>
                <select bind:value={targetDeviceId} disabled={busy}>
                  {#each candidates as candidate (candidate.deviceId)}
                    <option value={candidate.deviceId}>
                      {candidate.displayName} ({candidate.role})
                    </option>
                  {/each}
                </select>
              </label>
              <label class="intro-field">
                <span>Note (optional)</span>
                <input
                  type="text"
                  bind:value={note}
                  disabled={busy}
                  placeholder="Want to compare notes?"
                />
              </label>
              <div class="intro-actions">
                <button
                  type="button"
                  class="btn btn-sm btn-primary"
                  disabled={busy || !targetDeviceId}
                  onclick={() => void requestIntro()}
                >
                  {busy ? "Sending…" : "Introduce me"}
                </button>
              </div>
            {/if}
          </section>

          {#if acceptedHints && acceptedHints.status === "accepted"}
            <section class="intro-section">
              <h4>Dial hints</h4>
              <p class="intro-hints">{hintLine(acceptedHints)}</p>
            </section>
          {/if}
        {/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .intro-backdrop {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    background: rgb(0 0 0 / 0.45);
    padding: 1rem;
  }

  .intro-sheet {
    width: min(28rem, 100%);
    max-height: min(85vh, 40rem);
    overflow: auto;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: rgb(var(--color-surface-900));
    padding: 1rem;
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.4);
  }

  .intro-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .intro-header h3 {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 600;
    color: rgb(var(--color-surface-50));
  }

  .intro-lead,
  .intro-muted,
  .intro-hints {
    margin: 0.55rem 0 0.85rem;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--theme-text-tertiary));
  }

  .intro-muted code {
    font-size: 0.7rem;
    color: rgb(var(--color-surface-200));
  }

  .intro-field {
    display: grid;
    gap: 0.25rem;
    margin-bottom: 0.65rem;
    font-size: 0.75rem;
  }

  .intro-field span {
    color: rgb(var(--theme-text-tertiary));
  }

  .intro-field select,
  .intro-field input {
    border-radius: 0.45rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 50%, transparent);
    padding: 0.35rem 0.5rem;
    color: rgb(var(--color-surface-100));
  }

  .intro-section {
    margin-top: 0.85rem;
    padding-top: 0.75rem;
    border-top: 1px solid color-mix(in srgb, var(--color-surface-600) 40%, transparent);
  }

  .intro-section h4 {
    margin: 0 0 0.55rem;
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: rgb(var(--theme-text-secondary));
  }

  .intro-card {
    margin-bottom: 0.55rem;
    padding: 0.55rem 0.65rem;
    border-radius: 0.55rem;
    background: color-mix(in srgb, var(--color-surface-800) 80%, transparent);
  }

  .intro-card-title {
    margin: 0;
    font-size: 0.8125rem;
    color: rgb(var(--color-surface-100));
  }

  .intro-card-note {
    margin: 0.25rem 0 0;
    font-size: 0.7rem;
    color: rgb(var(--theme-text-tertiary));
  }

  .intro-card-actions,
  .intro-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.4rem;
    margin-top: 0.45rem;
  }

  .intro-hints {
    color: rgb(var(--color-success-300, 134 239 172));
  }

  .intro-waiting {
    font-size: 0.7rem;
    color: rgb(var(--theme-text-tertiary));
  }
</style>
