<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import {
    fetchPackagesCatalog,
    formatPackageBytes,
    installPackage,
    listenPackageProgress,
    openPackageInstaller,
    removePackage,
    type HomePackageRow,
    type HomePackagesCatalog,
    type PackageProgressEvent,
  } from "$lib/utils/packagesApi";
  import McpServersPanel from "$lib/components/skills/McpServersPanel.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { isTauri } from "$lib/window";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  let catalog = $state<HomePackagesCatalog | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let busyId = $state<string | null>(null);
  let progress = $state<PackageProgressEvent | null>(null);
  let unlisten: (() => void) | null = null;

  const desktop = $derived(isTauri() && !mobile);

  async function refresh() {
    loading = true;
    error = null;
    try {
      catalog = await fetchPackagesCatalog();
      if (!catalog) {
        error = "Couldn’t load packages.";
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    if (!desktop) {
      loading = false;
      return;
    }
    void refresh();
    void listenPackageProgress((event) => {
      progress = event;
    }).then((fn) => {
      unlisten = fn;
    });
  });

  onDestroy(() => {
    unlisten?.();
  });

  function actionLabel(row: HomePackageRow): string {
    if (row.updateAvailable) return "Update";
    if (row.installed) return "Installed";
    return "Install";
  }

  async function onInstall(row: HomePackageRow) {
    if (busyId) return;
    busyId = row.id;
    error = null;
    progress = {
      packageId: row.id,
      displayName: row.displayName,
      phase: "downloading",
      phaseLabel: "Downloading",
      percent: 0,
      message: "Starting…",
    };
    try {
      await installPackage(row.id);
      await refresh();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busyId = null;
      progress = null;
    }
  }

  async function onRemove(row: HomePackageRow) {
    if (busyId || !row.optional || !row.installed) return;
    busyId = row.id;
    error = null;
    try {
      await removePackage(row.id);
      await refresh();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busyId = null;
      progress = null;
    }
  }

  function openOfflineModels() {
    settingsNav.openSection("basement");
    layout.navigateDesktop("settings", { bump: true });
  }
</script>

{#if !desktop}
  <div>
    <h2 class="text-sm font-semibold text-surface-100">Packages</h2>
    <p class="workshop-faint mt-1 text-xs">
      Optional binaries install on the desktop app. Use this Mac’s Medousa Home to add offline
      brain, adapters, or tools.
    </p>
  </div>
{:else}
  <div>
    <h2 class="text-sm font-semibold text-surface-100">Packages</h2>
    <p class="workshop-faint mt-1 text-xs">
      Home already has the engine. Add what you need here — offline brain, channel adapters, CLI,
      and MCP servers.
    </p>

    {#if loading}
      <p class="workshop-faint mt-6 text-xs">Loading catalog…</p>
    {:else if catalog}
      <div class="settings-toggle-list mt-6">
        {#each catalog.packages as row (row.id)}
          {@const active = busyId === row.id}
          {@const sizeLabel = formatPackageBytes(row.sizeBytes)}
          <div class="settings-toggle-row items-start gap-3">
            <span class="min-w-0 flex-1">
              <span class="block text-sm font-medium text-surface-100">{row.displayName}</span>
              <span class="workshop-faint mt-0.5 block text-xs">
                {row.hint}
                {#if sizeLabel}
                  · ~{sizeLabel}
                {/if}
                {#if row.installed && row.installedVersion}
                  · v{row.installedVersion}
                {:else if row.availableVersion}
                  · v{row.availableVersion}
                {/if}
              </span>
              {#if active && progress && progress.packageId === row.id}
                <div class="mt-2">
                  <div class="h-1 overflow-hidden rounded-full bg-surface-700/80">
                    <div
                      class="h-full bg-primary-500 transition-[width] duration-200"
                      style={`width: ${Math.max(2, Math.min(100, progress.percent))}%`}
                    ></div>
                  </div>
                  <p class="workshop-faint mt-1 text-[11px]">
                    {progress.phaseLabel} · {progress.message}
                  </p>
                </div>
              {/if}
            </span>
            <div class="flex shrink-0 flex-col items-end gap-1">
              {#if row.installed && !row.updateAvailable}
                <span class="text-xs text-success-400">Installed</span>
                {#if row.optional}
                  <button
                    type="button"
                    class="workshop-text-action text-xs"
                    disabled={Boolean(busyId)}
                    onclick={() => void onRemove(row)}
                  >
                    Remove
                  </button>
                {/if}
              {:else}
                <button
                  type="button"
                  class="btn preset-filled-primary-500 h-7 px-3 text-xs"
                  disabled={Boolean(busyId)}
                  onclick={() => void onInstall(row)}
                >
                  {active ? "Working…" : actionLabel(row)}
                </button>
              {/if}
            </div>
          </div>
        {/each}
      </div>

      <div class="mt-8">
        <h3 class="settings-subsection-heading">Offline models</h3>
        <p class="settings-subsection-lead">
          Gemma weights download separately once the offline brain binary is installed.
        </p>
        <button
          type="button"
          class="settings-toggle-row mt-2 w-full text-left"
          onclick={openOfflineModels}
        >
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-medium text-surface-100">Open Connection → Extras</span>
            <span class="workshop-faint mt-0.5 block text-xs">
              Private brain panel for model download and load
            </span>
          </span>
          <span class="workshop-text-action shrink-0 text-xs">Open…</span>
        </button>
      </div>

      {#if catalog.installerAvailable}
        <div class="mt-8 border-t border-surface-500/30 pt-6">
          <p class="workshop-faint text-xs">
            Advanced repair and full workloads still live in Medousa Installer.
          </p>
          <button
            type="button"
            class="workshop-text-action mt-2 text-xs"
            disabled={Boolean(busyId)}
            onclick={() => void openPackageInstaller()}
          >
            Open Medousa Installer…
          </button>
        </div>
      {/if}
    {/if}

    <div class="mt-10 border-t border-surface-500/30 pt-6">
      <h3 class="settings-subsection-heading">MCP servers</h3>
      <p class="settings-subsection-lead">
        External tools connected through the MCP gateway — what’s live right now.
      </p>
      <div class="mt-4">
        <McpServersPanel />
      </div>
    </div>

    {#if error}
      <p class="mt-4 text-xs text-warning-400">{error}</p>
    {/if}
  </div>
{/if}
