<script lang="ts">
  import {
    getStorageStatus,
    runStorageMaintenance,
    updateStorageSettings,
    type StorageMaintenanceReport,
    type StorageUsageReport,
  } from "$lib/daemon";
  import { isTauri } from "$lib/window";
  import { onMount } from "svelte";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();
  const gib = 1024 ** 3;

  let status = $state<StorageUsageReport | null>(null);
  let enabled = $state(true);
  let repositoryCapGb = $state(10);
  let globalCapGb = $state(30);
  let freeFloorGb = $state(10);
  let inactiveHours = $state(24);
  let loading = $state(false);
  let saving = $state(false);
  let maintaining = $state(false);
  let error = $state<string | null>(null);
  let feedback = $state<string | null>(null);
  let preview = $state<StorageMaintenanceReport | null>(null);

  function bytesToGb(bytes: number): number {
    return Math.round((bytes / gib) * 10) / 10;
  }

  function formatBytes(bytes: number | null | undefined): string {
    if (bytes == null) return "—";
    if (bytes >= gib) return `${(bytes / gib).toFixed(1)} GB`;
    if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(0)} MB`;
    if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    return `${bytes} B`;
  }

  function applyStatus(next: StorageUsageReport) {
    status = next;
    enabled = next.settings.enabled;
    repositoryCapGb = bytesToGb(next.settings.repository_cache_max_bytes);
    globalCapGb = bytesToGb(next.settings.global_cache_max_bytes);
    freeFloorGb = bytesToGb(next.settings.free_disk_floor_bytes);
    inactiveHours = next.settings.min_inactive_age_hours;
  }

  async function refresh() {
    if (!isTauri()) return;
    loading = true;
    error = null;
    try {
      applyStatus(await getStorageStatus());
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function save() {
    if (!isTauri() || saving) return;
    saving = true;
    error = null;
    feedback = null;
    preview = null;
    try {
      applyStatus(
        await updateStorageSettings({
          enabled,
          repository_cache_max_bytes: Math.max(0, Math.round(repositoryCapGb * gib)),
          global_cache_max_bytes: Math.max(0, Math.round(globalCapGb * gib)),
          free_disk_floor_bytes: Math.max(0, Math.round(freeFloorGb * gib)),
          min_inactive_age_hours: Math.max(0, Math.round(inactiveHours)),
        }),
      );
      feedback = "Storage policy saved.";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  async function maintain(dryRun: boolean) {
    if (!isTauri() || maintaining) return;
    maintaining = true;
    error = null;
    feedback = null;
    try {
      const report = await runStorageMaintenance(dryRun);
      preview = dryRun ? report : null;
      applyStatus(report.after);
      feedback = dryRun
        ? report.actions.length > 0
          ? `${report.actions.length} inactive cache${report.actions.length === 1 ? "" : "s"} would free ${formatBytes(report.selected_bytes)}.`
          : "No inactive cache needs cleanup."
        : `Freed ${formatBytes(report.reclaimed_bytes)} from regenerable caches.`;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      maintaining = false;
    }
  }

  onMount(() => {
    void refresh();
  });
</script>

<section class="settings-subsection mt-8">
  <h3 class="settings-subsection-heading">Workshop storage</h3>
  <p class="settings-subsection-lead">
    Measure every managed category; clean only regenerable Forge build caches.
  </p>

  {#if !isTauri()}
    <p class="workshop-faint text-sm">Connect to a workshop to inspect its storage.</p>
  {:else if loading && !status}
    <p class="workshop-faint text-sm">Measuring workshop storage…</p>
  {:else}
    {#if status}
      <div class="storage-summary">
        <span>Build caches <strong>{formatBytes(status.build_caches.physical_bytes)}</strong></span>
        <span>Worktrees <strong>{formatBytes(status.forge_worktrees.physical_bytes)}</strong></span>
        <span>Detamu <strong>{formatBytes(status.detamu.physical_bytes)}</strong></span>
        <span>Artifacts <strong>{formatBytes(status.artifacts.physical_bytes)}</strong></span>
        <span>Coder evidence <strong>{formatBytes(status.coder_evidence.physical_bytes)}</strong></span>
        <span>Free disk <strong>{formatBytes(status.available_disk_bytes)}</strong></span>
      </div>
    {/if}

    <div class="settings-toggle-list mt-4">
      <label class="settings-toggle-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Automatic cache cleanup</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Every six hours; active and reviewable undertakings stay protected
          </span>
        </span>
        <input type="checkbox" class="checkbox shrink-0" bind:checked={enabled} disabled={saving} />
      </label>

      {#each [
        { label: "Per repository", hint: "0 disables this cap", value: "repository" },
        { label: "All build caches", hint: "0 disables this cap", value: "global" },
        { label: "Keep disk free", hint: "Pressure floor; 0 disables", value: "floor" },
      ] as row}
        <label class="settings-toggle-row settings-metric-row">
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-medium text-surface-100">{row.label}</span>
            <span class="workshop-faint mt-0.5 block text-xs">{row.hint}</span>
          </span>
          <span class="settings-metric-value">
            <input
              type="number"
              class="settings-metric-input settings-metric-input-wide"
              min="0"
              step="0.5"
              inputmode="decimal"
              value={row.value === "repository" ? repositoryCapGb : row.value === "global" ? globalCapGb : freeFloorGb}
              oninput={(event) => {
                const value = Number(event.currentTarget.value);
                if (row.value === "repository") repositoryCapGb = value;
                else if (row.value === "global") globalCapGb = value;
                else freeFloorGb = value;
              }}
              disabled={saving}
              aria-label={`${row.label} gigabytes`}
            />
            <span class="settings-metric-unit" aria-hidden="true">GB</span>
          </span>
        </label>
      {/each}

      <label class="settings-toggle-row settings-metric-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Inactive for</span>
          <span class="workshop-faint mt-0.5 block text-xs">Minimum age before cap-based cleanup</span>
        </span>
        <span class="settings-metric-value">
          <input
            type="number"
            class="settings-metric-input settings-metric-input-wide"
            min="0"
            inputmode="numeric"
            bind:value={inactiveHours}
            disabled={saving}
            aria-label="Inactive cache age hours"
          />
          <span class="settings-metric-unit" aria-hidden="true">hours</span>
        </span>
      </label>
    </div>

    {#if error}<p class="mt-3 text-sm text-red-400">{error}</p>{/if}
    {#if feedback}<p class="mt-3 text-sm text-emerald-400/90">{feedback}</p>{/if}
    {#if status?.scan_warnings.length}
      <p class="mt-3 text-xs text-amber-300">Some paths could not be measured ({status.scan_warnings.length}).</p>
    {/if}

    <div class="mt-4 flex flex-wrap gap-2">
      <button type="button" class="btn btn-sm variant-filled-primary" disabled={saving || maintaining || mobile} onclick={() => void save()}>
        {saving ? "Saving…" : "Save policy"}
      </button>
      <button type="button" class="btn btn-sm variant-soft" disabled={saving || maintaining} onclick={() => void maintain(true)}>
        {maintaining ? "Measuring…" : "Preview cleanup"}
      </button>
      <button
        type="button"
        class="btn btn-sm variant-soft"
        disabled={saving || maintaining || !preview || preview.actions.length === 0 || mobile}
        onclick={() => void maintain(false)}
      >
        Clean previewed caches
      </button>
    </div>
  {/if}
</section>

<style>
  .storage-summary {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(8.5rem, 1fr));
    gap: 0.5rem;
    margin-top: 0.75rem;
    font-size: 0.75rem;
    color: rgb(var(--shell-muted, var(--color-surface-400)));
  }

  .storage-summary span {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.5rem 0.625rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.25);
    border-radius: 0.5rem;
  }

  .storage-summary strong {
    color: rgb(var(--shell-label, var(--color-surface-200)));
    font-weight: 600;
  }
</style>
