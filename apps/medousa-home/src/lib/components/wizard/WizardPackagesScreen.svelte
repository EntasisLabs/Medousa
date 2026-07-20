<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Check, LoaderCircle } from "@lucide/svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import { isTauri } from "$lib/window";
  import {
    fetchPackagesCatalog,
    formatPackageBytes,
    installPackage,
    listenPackageProgress,
    type HomePackageRow,
    type PackageProgressEvent,
  } from "$lib/utils/packagesApi";
  import { WIZARD_PACKAGE_OFFER_FALLBACK } from "$lib/utils/wizardPackageOffers";

  let rows = $state<HomePackageRow[]>([]);
  let loading = $state(true);
  let installing = $state(false);
  let selected = $state<Set<string>>(new Set());
  let progressById = $state<Record<string, PackageProgressEvent>>({});
  let statusLine = $state<string | null>(null);
  let unlisten: (() => void) | null = null;

  const offerRows = $derived.by(() => {
    if (rows.length === 0) {
      return WIZARD_PACKAGE_OFFER_FALLBACK.map(
        (entry) =>
          ({
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
          }) satisfies HomePackageRow,
      );
    }
    const byId = new Map(rows.map((row) => [row.id, row]));
    return WIZARD_PACKAGE_OFFER_FALLBACK.map((entry) => {
      const fromCatalog = byId.get(entry.id);
      if (fromCatalog) {
        return {
          ...fromCatalog,
          // Keep wizard copy human — catalog hints are Settings-speak.
          hint: entry.hint,
        };
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

  const selectedPending = $derived(
    offerRows.filter((row) => selected.has(row.id) && !row.installed),
  );

  onMount(() => {
    void refresh();
    if (!isTauri()) {
      loading = false;
      return;
    }
    void listenPackageProgress((event) => {
      progressById = { ...progressById, [event.packageId]: event };
    }).then((fn) => {
      unlisten = fn;
    });
  });

  onDestroy(() => {
    unlisten?.();
  });

  async function refresh() {
    loading = true;
    try {
      const catalog = await fetchPackagesCatalog();
      rows = catalog?.packages ?? [];
    } catch {
      rows = [];
    } finally {
      loading = false;
    }
  }

  function toggle(row: HomePackageRow) {
    if (row.installed || installing) return;
    const next = new Set(selected);
    if (next.has(row.id)) next.delete(row.id);
    else next.add(row.id);
    selected = next;
  }

  function skip() {
    if (installing) return;
    wizard.skipExtras();
  }

  async function continuePackages() {
    if (installing) return;
    const toInstall = selectedPending.map((row) => row.id);
    if (toInstall.length === 0 || !isTauri()) {
      wizard.continueExtras();
      return;
    }

    installing = true;
    statusLine = null;
    const failures: string[] = [];

    for (const id of toInstall) {
      const label =
        offerRows.find((row) => row.id === id)?.displayName ?? id;
      progressById = {
        ...progressById,
        [id]: {
          packageId: id,
          displayName: label,
          phase: "downloading",
          phaseLabel: "Downloading",
          percent: 0,
          message: "Starting…",
        },
      };
      try {
        await installPackage(id);
      } catch {
        failures.push(label);
      }
    }

    await refresh();
    installing = false;

    if (failures.length > 0) {
      statusLine = `Couldn't install ${failures.join(", ")} — try Settings → Packages.`;
      // Still advance so onboarding never traps.
      window.setTimeout(() => wizard.continueExtras(), 900);
      return;
    }

    wizard.continueExtras();
  }

  const continueLabel = $derived.by(() => {
    if (installing) return "Installing…";
    if (selectedPending.length === 0) return "Open your desk";
    if (selectedPending.length === 1) return `Add ${selectedPending[0]!.displayName}`;
    return `Add ${selectedPending.length} connections`;
  });
</script>

<div class="wizard-step">
  <div class="wizard-stagger">
    <h1 id="product-wizard-title" class="wizard-beat text-2xl font-semibold tracking-tight text-surface-50">
      Who else should reach you?
    </h1>
    <p class="wizard-beat mt-2 text-sm leading-relaxed text-surface-400">
      Optional. Pick what you want now — or skip and open your desk.
    </p>

    {#if loading}
      <p class="wizard-beat mt-8 text-sm text-surface-500">Loading…</p>
    {:else}
      <ul class="wizard-beat mt-7 space-y-3">
        {#each offerRows as row (row.id)}
          {@const isSelected = selected.has(row.id)}
          {@const progress = progressById[row.id]}
          {@const sizeLabel = formatPackageBytes(row.sizeBytes)}
          <li>
            <button
              type="button"
              class="wizard-path-card flex w-full items-start gap-3 text-left {isSelected &&
              !row.installed
                ? 'wizard-path-card-active'
                : ''} {row.installed ? 'opacity-80' : ''}"
              disabled={wizard.busy || installing || row.installed}
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
                <span class="mt-0.5 block text-xs leading-relaxed text-surface-400">
                  {row.hint}
                  {#if sizeLabel}
                    · ~{sizeLabel}
                  {/if}
                </span>
                {#if installing && progress && progress.packageId === row.id}
                  <span class="mt-2 block">
                    <span class="block h-1 overflow-hidden rounded-full bg-surface-700/80">
                      <span
                        class="block h-full bg-primary-500 transition-[width] duration-200"
                        style={`width: ${Math.max(2, Math.min(100, progress.percent))}%`}
                      ></span>
                    </span>
                    <span class="mt-1 block text-[11px] text-surface-500">
                      {progress.phaseLabel}
                    </span>
                  </span>
                {/if}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    {#if statusLine}
      <p class="wizard-beat mt-4 text-sm text-warning-200">{statusLine}</p>
    {/if}

    {#if !isTauri()}
      <p class="wizard-beat mt-4 text-xs text-surface-500">
        Installs run in the desktop app — skip for now.
      </p>
    {/if}
  </div>

  <div class="mt-auto flex flex-wrap items-center justify-between gap-3 pt-8">
    <button
      type="button"
      class="btn variant-ghost min-h-11"
      disabled={wizard.busy || installing}
      onclick={skip}
    >
      Not now
    </button>
    <button
      type="button"
      class="btn variant-filled-primary wizard-cta inline-flex min-h-12 items-center gap-2 px-10"
      disabled={wizard.busy || installing || loading}
      onclick={() => void continuePackages()}
    >
      {#if installing}
        <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
      {/if}
      {continueLabel}
    </button>
  </div>
</div>
