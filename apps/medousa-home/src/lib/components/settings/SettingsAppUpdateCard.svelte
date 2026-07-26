<script lang="ts">
  import { onMount } from "svelte";
  import { Download, RefreshCw } from "@lucide/svelte";
  import { appUpdate } from "$lib/stores/appUpdate.svelte";
  import { openAppUpdateDownload } from "$lib/utils/appUpdate";
  import { isTauri } from "$lib/window";
  import { toast } from "$lib/stores/toast.svelte";

  let downloadBusy = $state(false);
  let downloadError = $state<string | null>(null);

  const status = $derived(appUpdate.status);
  const checking = $derived(appUpdate.checking);
  const currentLabel = $derived(status?.currentVersion ?? "…");
  const channelLabel = $derived(status?.channel ?? "stable");

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
  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Medousa app</h3>
      <p class="settings-subsection-lead">
        Desktop shell version on this machine · {channelLabel} channel.
      </p>
    </div>

    <div class="prefs-stack">
      <div class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Medousa {currentLabel}</span>
          <span class="prefs-tile-meta">
            {#if checking && !status}
              Checking for updates…
            {:else if status?.updateAvailable && status.latestVersion}
              Update available · {status.latestVersion}
            {:else if status?.latestVersion}
              You’re up to date
            {:else if status?.error || appUpdate.lastError}
              Couldn’t reach the release channel
            {:else}
              Check the channel for a newer build
            {/if}
          </span>
        </span>
        <button
          type="button"
          class="prefs-tile-cta"
          disabled={checking}
          onclick={() => void onCheck()}
        >
          <RefreshCw size={14} strokeWidth={1.75} class={checking ? "animate-spin" : ""} />
          {checking ? "Checking…" : "Check"}
        </button>
      </div>

      {#if status?.updateAvailable && status.downloadUrl}
        <div class="prefs-tile">
          <span class="prefs-tile-copy">
            <span class="prefs-tile-title">Install update</span>
            <span class="prefs-tile-meta">
              Opens the {status.latestVersion} installer download for this machine.
            </span>
          </span>
          <button
            type="button"
            class="prefs-tile-cta"
            disabled={downloadBusy}
            onclick={() => void onDownload()}
          >
            <Download size={14} strokeWidth={1.75} />
            {downloadBusy ? "Opening…" : "Download"}
          </button>
        </div>
      {/if}

      {#if downloadError || appUpdate.lastError}
        <p class="workshop-faint text-xs text-error-400">
          {downloadError ?? appUpdate.lastError}
        </p>
      {/if}
    </div>
  </div>
{/if}
