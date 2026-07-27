<script lang="ts">
  import { onMount } from "svelte";
  import { Download, RefreshCw } from "@lucide/svelte";
  import { appUpdate } from "$lib/stores/appUpdate.svelte";
  import { openGuide } from "$lib/guide/openGuide";
  import { openAppUpdateDownload } from "$lib/utils/appUpdate";
  import { isTauri } from "$lib/window";
  import { toast } from "$lib/stores/toast.svelte";

  let downloadBusy = $state(false);
  let downloadError = $state<string | null>(null);

  const status = $derived(appUpdate.status);
  const checking = $derived(appUpdate.checking);
  const currentLabel = $derived(status?.currentVersion ?? "…");
  const channelLabel = $derived(status?.channel ?? "stable");

  const statusLine = $derived.by(() => {
    if (checking && !status) return "Checking for updates…";
    if (status?.updateAvailable && status.latestVersion) {
      return `Update available · ${status.latestVersion}`;
    }
    if (status?.latestVersion) return "You’re up to date";
    if (status?.error || appUpdate.lastError) return "Couldn’t reach the release channel";
    return "Check the channel for a newer build";
  });

  onMount(() => {
    if (!isTauri()) return;
    if (!appUpdate.status) {
      void appUpdate.check({ quiet: true });
    }
  });

  async function onCheck() {
    downloadError = null;
    const next = await appUpdate.check();
    if (!next) {
      toast.show("Could not reach the release channel.", { durationMs: 1800 });
      return;
    }
    if (next.error && !next.latestVersion) {
      toast.show("Update check failed.", { durationMs: 1800 });
      return;
    }
    if (next.updateAvailable) {
      toast.show(`Update available · ${next.latestVersion}`, { durationMs: 1800 });
    } else {
      toast.show("You’re up to date.", { durationMs: 1400 });
    }
  }

  async function onDownload() {
    if (!isTauri()) return;
    downloadBusy = true;
    downloadError = null;
    try {
      await openAppUpdateDownload();
      toast.show("Opening installer download…", { durationMs: 1400 });
    } catch (err) {
      downloadError = err instanceof Error ? err.message : String(err);
    } finally {
      downloadBusy = false;
    }
  }
</script>

{#if isTauri()}
  <div class="app-update-head">
    <h3 class="settings-subsection-heading">App</h3>
    <p class="settings-subsection-lead">
      Desktop shell on this machine · {channelLabel}
    </p>
    <button
      type="button"
      class="settings-learn-more"
      onclick={() => void openGuide("workshops-connections")}
    >
      Learn more
    </button>
  </div>

  <div class="app-update-tile">
    <div class="app-update-copy">
      <span class="app-update-version">{currentLabel}</span>
      <span class="app-update-status">{statusLine}</span>
    </div>

    {#if status?.updateAvailable && status.downloadUrl}
      <button
        type="button"
        class="app-update-action"
        disabled={downloadBusy || checking}
        onclick={() => void onDownload()}
      >
        <Download size={14} strokeWidth={1.75} />
        <span>{downloadBusy ? "Opening…" : "Download"}</span>
      </button>
    {:else}
      <button
        type="button"
        class="app-update-action"
        disabled={checking}
        onclick={() => void onCheck()}
      >
        <RefreshCw size={14} strokeWidth={1.75} class={checking ? "animate-spin" : ""} />
        <span>{checking ? "Checking…" : "Check"}</span>
      </button>
    {/if}
  </div>

  {#if status?.updateAvailable && status.downloadUrl}
    <button
      type="button"
      class="app-update-secondary"
      disabled={checking}
      onclick={() => void onCheck()}
    >
      Recheck for updates
    </button>
  {/if}

  {#if downloadError || appUpdate.lastError}
    <p class="app-update-error">{downloadError ?? appUpdate.lastError}</p>
  {/if}
{/if}

<style>
  .app-update-head :global(.settings-subsection-heading) {
    margin-bottom: 0.15rem;
  }

  .app-update-head :global(.settings-subsection-lead) {
    margin-bottom: 0.6rem;
  }

  .app-update-tile {
    display: flex;
    align-items: center;
    gap: 0.85rem;
    min-height: 3rem;
    padding: 0.7rem 0.85rem;
    border-radius: 0.75rem;
    border: 1px solid rgb(var(--color-surface-600) / 0.35);
    background: rgb(var(--color-surface-900) / 0.45);
  }

  .app-update-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.15rem;
  }

  .app-update-version {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .app-update-status {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--color-surface-500));
  }

  .app-update-action {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.35rem;
    border: 0;
    background: transparent;
    padding: 0.2rem 0;
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--color-surface-400));
    cursor: pointer;
  }

  .app-update-action:hover:not(:disabled) {
    color: rgb(var(--color-surface-200));
  }

  .app-update-action:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .app-update-secondary {
    display: inline-block;
    margin-top: 0.55rem;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.68rem;
    color: rgb(var(--color-surface-500));
    cursor: pointer;
  }

  .app-update-secondary:hover:not(:disabled) {
    color: rgb(var(--color-surface-300));
  }

  .app-update-secondary:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .app-update-error {
    margin: 0.45rem 0 0;
    font-size: 0.68rem;
    color: rgb(var(--color-error-400));
  }
</style>
