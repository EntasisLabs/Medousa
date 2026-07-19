<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Check, LoaderCircle, X } from "@lucide/svelte";
  import { isTauri } from "$lib/window";
  import {
    dismissConnectionsInvite,
    shouldShowConnectionsInvite,
  } from "$lib/utils/connectionsInvite";
  import {
    fetchPackagesCatalog,
    formatPackageBytes,
    installPackage,
    listenPackageProgress,
    type HomePackageRow,
    type PackageProgressEvent,
  } from "$lib/utils/packagesApi";
  import { WIZARD_PACKAGE_OFFER_FALLBACK } from "$lib/utils/wizardPackageOffers";

  let open = $state(false);
  let rows = $state<HomePackageRow[]>([]);
  let selected = $state<Set<string>>(new Set());
  let installing = $state(false);
  let statusLine = $state<string | null>(null);
  let progressById = $state<Record<string, PackageProgressEvent>>({});
  let unlisten: (() => void) | null = null;

  const offerRows = $derived.by(() => {
    const byId = new Map(rows.map((row) => [row.id, row]));
    return WIZARD_PACKAGE_OFFER_FALLBACK.map((entry) => {
      const fromCatalog = byId.get(entry.id);
      if (fromCatalog) {
        return { ...fromCatalog, hint: entry.hint };
      }
      return {
        id: entry.id,
        displayName: entry.displayName,
        hint: entry.hint,
        categoryLabel: "",
        installed: false,
        installedVersion: null,
        availableVersion: null,
        updateAvailable: false,
        sizeBytes: 0,
        optional: true,
      } satisfies HomePackageRow;
    });
  });

  const pending = $derived(
    offerRows.filter((row) => selected.has(row.id) && !row.installed),
  );

  onMount(() => {
    if (!shouldShowConnectionsInvite()) return;
    open = true;
    void refresh();
    if (isTauri()) {
      void listenPackageProgress((event) => {
        progressById = { ...progressById, [event.packageId]: event };
      }).then((fn) => {
        unlisten = fn;
      });
    }
  });

  onDestroy(() => {
    unlisten?.();
  });

  async function refresh() {
    try {
      const catalog = await fetchPackagesCatalog();
      rows = catalog?.packages ?? [];
    } catch {
      rows = [];
    }
  }

  function toggle(row: HomePackageRow) {
    if (row.installed || installing) return;
    const next = new Set(selected);
    if (next.has(row.id)) next.delete(row.id);
    else next.add(row.id);
    selected = next;
  }

  function close() {
    dismissConnectionsInvite();
    open = false;
  }

  async function installSelected() {
    if (!isTauri() || pending.length === 0) {
      close();
      return;
    }
    installing = true;
    statusLine = null;
    const failures: string[] = [];
    for (const row of pending) {
      try {
        await installPackage(row.id);
      } catch {
        failures.push(row.displayName);
      }
    }
    await refresh();
    installing = false;
    if (failures.length > 0) {
      statusLine = `Couldn't install ${failures.join(", ")} — try Settings → Packages.`;
      return;
    }
    close();
  }
</script>

{#if open}
  <div
    class="fixed inset-0 z-[80] flex items-end justify-center bg-surface-950/55 p-4 backdrop-blur-sm sm:items-center"
    role="presentation"
  >
    <div
      class="flex w-full max-w-lg flex-col overflow-hidden rounded-2xl border border-surface-500/40 bg-surface-900 shadow-2xl"
      role="dialog"
      aria-modal="true"
      aria-labelledby="connections-invite-title"
    >
      <div class="flex items-start justify-between gap-3 px-5 pt-5">
        <div>
          <h2 id="connections-invite-title" class="text-lg font-semibold text-surface-50">
            Reach you elsewhere?
          </h2>
          <p class="mt-1 text-sm text-surface-400">
            Optional — Discord, Telegram, WhatsApp, or MCP tools. Anytime in Settings → Packages.
          </p>
        </div>
        <button
          type="button"
          class="rounded-lg p-1.5 text-surface-400 hover:bg-surface-800 hover:text-surface-100"
          aria-label="Dismiss"
          disabled={installing}
          onclick={close}
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <ul class="mt-4 max-h-[50vh] space-y-2 overflow-y-auto px-5">
        {#each offerRows as row (row.id)}
          {@const isSelected = selected.has(row.id)}
          {@const progress = progressById[row.id]}
          {@const sizeLabel = formatPackageBytes(row.sizeBytes)}
          <li>
            <button
              type="button"
              class="flex w-full items-start gap-3 rounded-xl border px-3 py-3 text-left transition {isSelected &&
              !row.installed
                ? 'border-primary-500/55 bg-primary-500/10'
                : 'border-surface-500/35 bg-surface-950/40'} {row.installed ? 'opacity-80' : ''}"
              disabled={installing || row.installed}
              onclick={() => toggle(row)}
            >
              <span
                class="mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded border {row.installed ||
                isSelected
                  ? 'border-primary-500/60 bg-primary-500/20 text-primary-200'
                  : 'border-surface-500/50 text-transparent'}"
                aria-hidden="true"
              >
                {#if row.installed || isSelected}
                  <Check class="h-3.5 w-3.5" strokeWidth={2.5} />
                {/if}
              </span>
              <span class="min-w-0 flex-1">
                <span class="flex items-center gap-2">
                  <span class="text-sm font-semibold text-surface-50">{row.displayName}</span>
                  {#if row.installed}
                    <span class="text-[11px] text-success-400">Ready</span>
                  {/if}
                </span>
                <span class="mt-0.5 block text-xs text-surface-400">
                  {row.hint}
                  {#if sizeLabel}
                    · ~{sizeLabel}
                  {/if}
                </span>
                {#if installing && progress}
                  <span class="mt-2 block h-1 overflow-hidden rounded-full bg-surface-700/80">
                    <span
                      class="block h-full bg-primary-500"
                      style={`width: ${Math.max(2, Math.min(100, progress.percent))}%`}
                    ></span>
                  </span>
                {/if}
              </span>
            </button>
          </li>
        {/each}
      </ul>

      {#if statusLine}
        <p class="px-5 pt-3 text-sm text-warning-200">{statusLine}</p>
      {/if}

      <div class="flex flex-wrap items-center justify-between gap-3 px-5 py-5">
        <button
          type="button"
          class="btn variant-ghost min-h-11"
          disabled={installing}
          onclick={close}
        >
          Not now
        </button>
        <button
          type="button"
          class="btn variant-filled-primary inline-flex min-h-11 items-center gap-2 px-6"
          disabled={installing}
          onclick={() => void installSelected()}
        >
          {#if installing}
            <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
            Installing…
          {:else if pending.length === 0}
            Done
          {:else if pending.length === 1}
            Add {pending[0]!.displayName}
          {:else}
            Add {pending.length}
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}
