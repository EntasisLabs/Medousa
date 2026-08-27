<script lang="ts">
  import { untrack } from "svelte";
  import { vaultVersions } from "$lib/stores/vaultVersions.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import { ChevronDown } from "@lucide/svelte";

  interface Props {
    /** Omit page chrome when nested under Runtime Controls. */
    embedded?: boolean;
  }

  let { embedded = false }: Props = $props();
  const versionsOn = $derived(workshopDefaults.draft.vaultGitEnabled ?? false);

  // Only react to loaded/on — untrack store reads/writes so refresh can't loop.
  $effect(() => {
    if (!workshopDefaults.loaded) return;
    const on = versionsOn;
    untrack(() => {
      if (!on) {
        vaultVersions.markDisabledLocally();
        return;
      }
      void vaultVersions.refresh({ force: true });
    });
  });

  function setVersions(enabled: boolean) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      vaultGitEnabled: enabled,
    };
  }

  async function applyEnable(enabled: boolean) {
    setVersions(enabled);
    try {
      await vaultVersions.setEnabled(enabled, true);
      workshopDefaults.acknowledgeClean();
    } catch {
      /* error surfaced on store */
    }
  }

  const statusMeta = $derived.by(() => {
    if (vaultVersions.unsupported) {
      return "Not available on this workshop yet";
    }
    if (!versionsOn) return "Off — normal note conflict checks only";
    if (!vaultVersions.detect?.available) return "On — Git not available yet";
    if (!vaultVersions.status?.isRepo) return "On — ready to start versioning";
    const branch = vaultVersions.status.branch ?? "detached";
    const dirty =
      vaultVersions.status.dirtyCount === 0
        ? "clean"
        : `${vaultVersions.status.dirtyCount} changed`;
    return `On · ${branch} · ${dirty}`;
  });
</script>

<div class="versions-band" class:versions-band-spaced={embedded}>
  {#if embedded}
    <div class="versions-band-head">
      <h3 class="settings-subsection-heading">Versions</h3>
      <p class="settings-subsection-lead">
        Optional vault history — named snapshots on this machine.
      </p>
    </div>
  {:else}
    <header class="settings-section-header mb-4">
      <h2 class="text-base font-semibold text-surface-50">Versions</h2>
      <p class="workshop-faint mt-1 text-sm">Optional vault history. Off by default.</p>
    </header>
  {/if}
  {@render body()}
</div>

{#snippet body()}
  <div class="versions-stack">
    <label class="versions-tile">
      <span class="versions-tile-copy">
        <span class="versions-tile-title">Vault versioning</span>
        <span class="versions-tile-meta">{statusMeta}</span>
      </span>
      <input
        type="checkbox"
        class="versions-switch"
        checked={versionsOn}
        disabled={vaultVersions.busy || vaultVersions.unsupported}
        aria-label="Enable vault versioning"
        onchange={(event) =>
          void applyEnable((event.currentTarget as HTMLInputElement).checked)}
      />
    </label>

    {#if versionsOn && !vaultVersions.unsupported}
      <details class="versions-more">
        <summary class="versions-more-summary">
          <span>Git on this device</span>
          <ChevronDown size={14} strokeWidth={2} class="versions-more-chevron" aria-hidden="true" />
        </summary>
        <div class="versions-more-body">
          {#if vaultVersions.detect}
            <p class="versions-footnote">
              {#if vaultVersions.detect.available}
                Found {vaultVersions.detect.version ?? "Git"}
                {#if vaultVersions.detect.path}
                  <span class="block truncate font-mono text-xs">
                    {vaultVersions.detect.path}
                  </span>
                {/if}
              {:else}
                Git is not available yet.
                <span class="mt-1 block">{vaultVersions.detect.platformHint}</span>
              {/if}
            </p>
          {/if}

          {#if vaultVersions.status}
            <p class="versions-footnote mt-2">
              {#if !vaultVersions.status.available}
                Install Git to start versioning.
              {:else if !vaultVersions.status.isRepo}
                Vault is ready — start versioning to create the first snapshot store.
              {:else}
                Branch
                <span class="font-medium text-surface-100">
                  {vaultVersions.status.branch ?? "detached"}
                </span>
                ·
                {vaultVersions.status.dirtyCount === 0
                  ? "clean"
                  : `${vaultVersions.status.dirtyCount} changed`}
              {/if}
            </p>
          {/if}

          <div class="versions-actions mt-3">
            {#if vaultVersions.detect && !vaultVersions.detect.available}
              <button
                type="button"
                class="btn btn-sm variant-soft-surface"
                disabled={vaultVersions.busy}
                onclick={() => void vaultVersions.installGit()}
              >
                Install / locate Git
              </button>
            {/if}
            {#if vaultVersions.status?.available && !vaultVersions.status.isRepo}
              <button
                type="button"
                class="btn btn-sm variant-filled-primary"
                disabled={vaultVersions.busy}
                onclick={() => void vaultVersions.startVersioning()}
              >
                Start versioning
              </button>
            {/if}
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface"
              disabled={vaultVersions.busy}
              onclick={() => void vaultVersions.refresh()}
            >
              Refresh status
            </button>
          </div>
        </div>
      </details>
    {/if}

    {#if vaultVersions.error}
      <p class="versions-footnote versions-footnote-warn" role="alert">
        {vaultVersions.error}
      </p>
    {/if}

    <p class="versions-footnote">
      On and Off apply immediately — separate from Runtime Save.
    </p>
  </div>
{/snippet}

<style>
  .versions-band-spaced {
    margin-top: 1.25rem;
  }

  .versions-band-head .settings-subsection-heading {
    margin-bottom: 0.15rem;
  }

  .versions-band-head .settings-subsection-lead {
    margin-bottom: 0.6rem;
  }

  .versions-stack {
    display: grid;
    gap: 0.5rem;
  }

  .versions-tile {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-height: 3.25rem;
    padding: 0.55rem 0.75rem;
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    background: rgb(var(--color-surface-900) / 0.28);
  }

  .versions-tile-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .versions-tile-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .versions-tile-meta {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--theme-text-quiet));
  }

  .versions-switch {
    position: relative;
    flex-shrink: 0;
    width: 2.35rem;
    height: 1.3rem;
    margin: 0;
    appearance: none;
    border: 0;
    border-radius: 999px;
    background: rgb(var(--color-surface-600) / 0.55);
    cursor: pointer;
    transition: background 140ms ease;
  }

  .versions-switch::after {
    content: "";
    position: absolute;
    top: 0.15rem;
    left: 0.15rem;
    width: 1rem;
    height: 1rem;
    border-radius: 999px;
    background: rgb(var(--color-surface-100));
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
    transition: transform 140ms ease;
  }

  .versions-switch:checked {
    background: rgb(var(--color-primary-500) / 0.85);
  }

  .versions-switch:checked::after {
    transform: translateX(1.05rem);
  }

  .versions-switch:focus-visible {
    outline: 2px solid rgb(var(--color-primary-400) / 0.7);
    outline-offset: 2px;
  }

  .versions-switch:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .versions-more {
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.28);
    background: rgb(var(--color-surface-950) / 0.35);
  }

  .versions-more-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.55rem 0.75rem;
    font-size: 0.75rem;
    font-weight: 550;
    color: rgb(var(--color-surface-200));
    cursor: pointer;
    list-style: none;
  }

  .versions-more-summary::-webkit-details-marker {
    display: none;
  }

  :global(.versions-more-chevron) {
    flex-shrink: 0;
    color: rgb(var(--theme-text-quiet));
    transition: transform 140ms ease;
  }

  .versions-more[open] :global(.versions-more-chevron) {
    transform: rotate(180deg);
  }

  .versions-more-body {
    padding: 0 0.75rem 0.75rem;
  }

  .versions-footnote {
    margin: 0;
    font-size: 0.7rem;
    line-height: 1.4;
    color: rgb(var(--theme-text-quiet));
  }

  .versions-footnote-warn {
    color: rgb(var(--theme-warning) / 0.95);
  }

  .versions-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
</style>
