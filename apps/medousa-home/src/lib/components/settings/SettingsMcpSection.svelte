<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import McpServersPanel from "$lib/components/skills/McpServersPanel.svelte";
  import {
    fetchPackagesCatalog,
    formatPackageBytes,
    installPackage,
    listenPackageProgress,
    removePackage,
    type HomePackageRow,
    type PackageProgressEvent,
  } from "$lib/utils/packagesApi";
  import { isTauriDesktop } from "$lib/platform";
  import { onThisHostPhrase } from "$lib/platformCopy";

  let gatewayPackage = $state<HomePackageRow | null>(null);
  let loadingPackage = $state(true);
  let packageError = $state<string | null>(null);
  let busy = $state(false);
  let progress = $state<PackageProgressEvent | null>(null);
  let unlisten: (() => void) | null = null;

  const desktop = $derived(isTauriDesktop());

  async function refreshGatewayPackage() {
    if (!desktop) {
      loadingPackage = false;
      return;
    }
    loadingPackage = true;
    packageError = null;
    try {
      const catalog = await fetchPackagesCatalog();
      gatewayPackage = catalog?.packages.find((row) => row.id === "mcp-gateway") ?? null;
    } catch (err) {
      packageError = err instanceof Error ? err.message : String(err);
    } finally {
      loadingPackage = false;
    }
  }

  onMount(() => {
    void refreshGatewayPackage();
    if (!desktop) return;
    void listenPackageProgress((event) => {
      if (event.packageId === "mcp-gateway") progress = event;
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

  async function onInstall() {
    if (!gatewayPackage || busy) return;
    busy = true;
    packageError = null;
    progress = {
      packageId: gatewayPackage.id,
      displayName: gatewayPackage.displayName,
      phase: "downloading",
      phaseLabel: "Downloading",
      percent: 0,
      message: "Starting…",
    };
    try {
      await installPackage(gatewayPackage.id);
      await refreshGatewayPackage();
    } catch (err) {
      packageError = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
      progress = null;
    }
  }

  async function onRemove() {
    if (!gatewayPackage?.optional || !gatewayPackage.installed || busy) return;
    busy = true;
    packageError = null;
    try {
      await removePackage(gatewayPackage.id);
      await refreshGatewayPackage();
    } catch (err) {
      packageError = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
      progress = null;
    }
  }
</script>

<section class="settings-section prefs mcp">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">MCP</h2>
    <p class="workshop-faint mt-1 text-sm">
      Gateway and external tool servers for specialists and chat.
    </p>
  </header>

  {#if !desktop}
    <p class="workshop-faint mt-4 text-sm">
      Connect MCP servers from the Medousa desktop app.
    </p>
  {:else}
    {#if gatewayPackage}
      <div class="prefs-band">
        <div class="prefs-band-head">
          <h3 class="settings-subsection-heading">Gateway</h3>
          <p class="settings-subsection-lead">
            Binary that hosts MCP servers {onThisHostPhrase()}.
          </p>
        </div>
        <div class="prefs-stack">
          <div class="prefs-tile">
            <span class="prefs-tile-copy">
              <span class="prefs-tile-title">{gatewayPackage.displayName}</span>
              <span class="prefs-tile-meta">
                {gatewayPackage.hint}
                {#if formatPackageBytes(gatewayPackage.sizeBytes)}
                  · ~{formatPackageBytes(gatewayPackage.sizeBytes)}
                {/if}
                {#if gatewayPackage.installed && gatewayPackage.installedVersion}
                  · v{gatewayPackage.installedVersion}
                {:else if gatewayPackage.availableVersion}
                  · v{gatewayPackage.availableVersion}
                {/if}
              </span>
              {#if busy && progress}
                <span class="prefs-tile-meta mt-1 block">
                  {progress.phaseLabel} · {progress.message}
                </span>
              {/if}
            </span>
            {#if gatewayPackage.installed && !gatewayPackage.updateAvailable}
              <span class="prefs-tile-cta prefs-tile-cta-static">Installed</span>
              {#if gatewayPackage.optional}
                <button
                  type="button"
                  class="prefs-tile-cta"
                  disabled={busy}
                  onclick={() => void onRemove()}
                >
                  Remove
                </button>
              {/if}
            {:else}
              <button
                type="button"
                class="prefs-tile-cta"
                disabled={busy || loadingPackage}
                onclick={() => void onInstall()}
              >
                {busy ? "…" : actionLabel(gatewayPackage)}
              </button>
            {/if}
          </div>
        </div>
      </div>
    {:else if loadingPackage}
      <p class="workshop-faint mt-4 text-xs">Loading gateway package…</p>
    {/if}

    {#if packageError}
      <p class="mt-2 text-xs text-content-warning">{packageError}</p>
    {/if}

    <div class="prefs-band">
      <McpServersPanel />
    </div>
  {/if}
</section>

<style>
  .prefs {
    --prefs-gap: 0.5rem;
    --prefs-tile-radius: 0.65rem;
    --prefs-tile-pad: 0.55rem 0.75rem;
    --prefs-tile-min-h: 3.25rem;
    --prefs-tile-border: rgb(var(--color-surface-500) / 0.32);
    --prefs-tile-bg: rgb(var(--color-surface-900) / 0.28);
  }

  .prefs-band {
    margin-top: 1.25rem;
  }

  .prefs-band-head .settings-subsection-heading {
    margin-bottom: 0.15rem;
  }

  .prefs-band-head .settings-subsection-lead {
    margin-bottom: 0.6rem;
  }

  .prefs-stack {
    display: grid;
    gap: var(--prefs-gap);
  }

  .prefs-tile {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
  }

  .prefs-tile-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .prefs-tile-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .prefs-tile-meta {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--theme-text-quiet));
  }

  .prefs-tile-cta {
    flex-shrink: 0;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--theme-text-tertiary));
    cursor: pointer;
  }

  .prefs-tile-cta:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .prefs-tile-cta-static {
    cursor: default;
    color: rgb(var(--theme-success));
  }
</style>
