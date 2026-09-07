<script lang="ts">
  import ComputerWatchSheet from "$lib/components/settings/ComputerWatchSheet.svelte";
  import {
    loadComputerDriverReadiness,
    type ComputerDriverReadiness,
    type ComputerPermissionKind,
    type ComputerPermissionReport,
    type ComputerWorldControlState,
  } from "$lib/daemon";
  import { executionTargets } from "$lib/stores/executionTargets.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { isTauri } from "$lib/window";
  import { Eye, RefreshCw } from "@lucide/svelte";

  const permissions: Array<{
    id: ComputerPermissionKind;
    label: string;
    readyHint: string;
  }> = [
    {
      id: "accessibility",
      label: "Controls & text",
      readyHint: "Semantic observation and native actions",
    },
    {
      id: "screen_capture",
      label: "Focused-window pixels",
      readyHint: "Bounded screenshots with secure regions redacted",
    },
    {
      id: "input_control",
      label: "Foreground input",
      readyHint: "Guarded click fallback when semantics are unavailable",
    },
  ];

  let readiness = $state<ComputerDriverReadiness[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let targetRuntimeId = $state<string | null>(null);
  let watching = $state<{
    runtimeId: string;
    runtimeLabel: string;
    readiness: ComputerDriverReadiness;
  } | null>(null);
  let requestGeneration = 0;

  const computerTargets = $derived(executionTargets.worldTargets("computer"));
  const workshopLabel = $derived(
    executionTargets.runtimeLabel(targetRuntimeId) ?? "Unavailable workshop",
  );
  const targetIsLocalWorkshop = $derived(
    targetRuntimeId === executionTargets.inventory?.parent_runtime_id &&
      workshops.activeWorkshop?.kind === "local",
  );

  async function refresh(
    workshopId = workshops.activeWorkshopId,
    options: { forceInventory?: boolean } = {},
  ) {
    const generation = ++requestGeneration;
    loading = true;
    error = null;
    try {
      await executionTargets.refresh({ force: options.forceInventory });
      if (generation !== requestGeneration || workshopId !== workshops.activeWorkshopId) return;
      const runtimeId = executionTargets.worldRuntimeId("computer");
      targetRuntimeId = runtimeId;
      if (!runtimeId) {
        readiness = [];
        return;
      }
      const next = await loadComputerDriverReadiness(
        executionTargets.transportRuntimeId(runtimeId),
      );
      if (generation !== requestGeneration || workshopId !== workshops.activeWorkshopId) return;
      readiness = next;
    } catch (cause) {
      if (generation !== requestGeneration || workshopId !== workshops.activeWorkshopId) return;
      readiness = [];
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (generation === requestGeneration && workshopId === workshops.activeWorkshopId) {
        loading = false;
      }
    }
  }

  $effect(() => {
    const workshopId = workshops.activeWorkshopId;
    watching = null;
    targetRuntimeId = null;
    if (!isTauri()) {
      loading = false;
      return;
    }
    executionTargets.activateWorkshopScope(workshopId);
    void refresh(workshopId, { forceInventory: true });
    return () => {
      requestGeneration += 1;
    };
  });

  function selectTarget(runtimeId: string) {
    const id = runtimeId.trim();
    if (!id || id === targetRuntimeId) return;
    executionTargets.setWorldRuntimeId("computer", id);
    targetRuntimeId = id;
    void refresh();
  }

  function permissionFor(
    row: ComputerDriverReadiness,
    permission: ComputerPermissionKind,
  ): ComputerPermissionReport | undefined {
    return row.preflight?.permissions.find((candidate) => candidate.permission === permission);
  }

  function permissionLabel(report: ComputerPermissionReport | undefined): string {
    switch (report?.status) {
      case "granted":
        return "Ready";
      case "denied":
      case "not_determined":
        return "Needed";
      case "restricted":
        return "Restricted";
      case "unsupported":
      default:
        return "Unavailable";
    }
  }

  function permissionHint(
    report: ComputerPermissionReport | undefined,
    readyHint: string,
  ): string {
    if (report?.status === "granted") return readyHint;
    return report?.guidance?.trim() || "This driver did not report the permission as available.";
  }

  function platformLabel(platform: string | undefined): string {
    switch (platform) {
      case "macos":
        return "macOS";
      case "windows":
        return "Windows";
      case "linux":
        return "Linux";
      default:
        return platform?.trim() || "Native desktop";
    }
  }

  function canWatch(row: ComputerDriverReadiness): boolean {
    return (
      permissionFor(row, "accessibility")?.status === "granted" &&
      permissionFor(row, "screen_capture")?.status === "granted"
    );
  }

  function controlLabel(row: ComputerDriverReadiness): string {
    if (row.control?.requester_has_control) return "You have control";
    if (row.control?.holder === "human") return "Another person has control";
    if (row.control && row.control.holder !== "available") return "Medousa is operating";
    return "Ready to watch";
  }

  function updateControl(control: ComputerWorldControlState) {
    if (!watching || watching.runtimeId !== targetRuntimeId) return;
    readiness = readiness.map((row) =>
      row.driver.driver_id === control.driver_id ? { ...row, control } : row,
    );
  }
</script>

<section class="computer-band" aria-labelledby="computer-control-title">
  <div class="computer-heading">
    <div>
      <h3 id="computer-control-title" class="settings-subsection-heading">Computer</h3>
      <p class="settings-subsection-lead">
        Choose which workshop owns the desktop. Checking never opens a system permission prompt.
      </p>
    </div>
    {#if isTauri()}
      <button
        type="button"
        class="computer-refresh"
        aria-label="Refresh computer readiness"
        title="Refresh computer readiness"
        disabled={loading}
        onclick={() => void refresh(workshops.activeWorkshopId, { forceInventory: true })}
      >
        <RefreshCw size={14} strokeWidth={1.8} class={loading ? "animate-spin" : ""} aria-hidden="true" />
      </button>
    {/if}
  </div>

  {#if isTauri()}
    <label class="computer-target-row">
      <span>
        <span class="computer-permission-title">Computer workshop</span>
        <span class="computer-permission-hint">New desktop views and actions stay on this runtime</span>
      </span>
      <select
        aria-label="Computer workshop"
        value={targetRuntimeId ?? ""}
        disabled={loading && computerTargets.length === 0}
        onchange={(event) => selectTarget(event.currentTarget.value)}
      >
        {#if targetRuntimeId && executionTargets.worldSelectionUnavailable("computer")}
          <option value={targetRuntimeId}>{workshopLabel} · unavailable</option>
        {/if}
        {#each computerTargets as target (target.runtime_id)}
          <option value={target.runtime_id}>{target.label}</option>
        {/each}
      </select>
    </label>
  {/if}

  {#if !isTauri()}
    <p class="workshop-faint text-xs">Connect through Medousa to inspect this workshop.</p>
  {:else if loading && readiness.length === 0}
    <p class="workshop-faint text-xs">Checking the workshop…</p>
  {:else if error}
    <p class="settings-danger-callout text-xs leading-relaxed" role="status">{error}</p>
  {:else if readiness.length === 0}
    <div class="computer-empty">
      <span class="text-sm font-medium text-surface-100">
        {targetRuntimeId ? `Computer control isn’t running on ${workshopLabel}` : "No authorized computer workshop"}
      </span>
      <span class="workshop-faint mt-0.5 block text-xs">
        {#if !targetRuntimeId}
          Grant Browser & computer worlds to a paired workshop, or install Computer control locally.
        {:else if targetIsLocalWorkshop}
          Install Computer control in Settings → Packages, then restart the workshop.
        {:else}
          Install Computer control on {workshopLabel}, then restart its daemon.
        {/if}
      </span>
    </div>
  {:else}
    <div class="computer-drivers">
      {#each readiness as row (row.driver.driver_id)}
        <article class="computer-driver">
          <header class="computer-driver-heading">
            <span class="min-w-0">
              <span class="block truncate text-sm font-medium text-surface-100">
                {row.driver.display_name || "Native computer"}
              </span>
              <span class="workshop-faint mt-0.5 block text-xs">
                {platformLabel(row.preflight?.platform)} · {row.driver.ownership} desktop
              </span>
            </span>
            <span class="computer-driver-state" class:computer-driver-state-ready={Boolean(row.preflight)}>
              {row.preflight ? "Connected" : "Unavailable"}
            </span>
          </header>

          {#if row.error}
            <p class="computer-driver-error" role="status">{row.error}</p>
          {:else}
            <div class="computer-permissions">
              {#each permissions as permission (permission.id)}
                {@const report = permissionFor(row, permission.id)}
                <div class="computer-permission">
                  <span class="computer-permission-copy">
                    <span class="computer-permission-title">{permission.label}</span>
                    <span class="computer-permission-hint">
                      {permissionHint(report, permission.readyHint)}
                    </span>
                  </span>
                  <span
                    class="computer-permission-status"
                    class:computer-permission-ready={report?.status === "granted"}
                    class:computer-permission-needed={report?.status === "denied" || report?.status === "not_determined"}
                  >
                    {permissionLabel(report)}
                  </span>
                </div>
              {/each}
            </div>
            {#if row.preflight}
              <div class="computer-watch-row">
                <span>
                  <span class="computer-permission-title">Live view</span>
                  <span class="computer-permission-hint">{controlLabel(row)}</span>
                </span>
                <button
                  type="button"
                  disabled={!canWatch(row)}
                  title={canWatch(row)
                    ? `Watch ${row.driver.display_name || "this computer"}`
                    : "Controls & text and Focused-window pixels must both be ready"}
                  onclick={() => {
                    if (!targetRuntimeId) return;
                    watching = {
                      runtimeId: targetRuntimeId,
                      runtimeLabel: workshopLabel,
                      readiness: row,
                    };
                  }}
                >
                  <Eye size={13} strokeWidth={1.9} aria-hidden="true" />
                  Watch
                </button>
              </div>
            {/if}
          {/if}
        </article>
      {/each}
    </div>
  {/if}
</section>

<ComputerWatchSheet
  open={Boolean(watching)}
  readiness={watching?.readiness ?? null}
  executionRuntimeId={watching
    ? executionTargets.transportRuntimeId(watching.runtimeId)
    : null}
  workshopLabel={watching?.runtimeLabel ?? "Workshop"}
  onClose={() => (watching = null)}
  onControlChange={updateControl}
/>

<style>
  .computer-band {
    margin-top: 1.25rem;
  }

  .computer-heading,
  .computer-driver-heading,
  .computer-permission,
  .computer-target-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .computer-target-row {
    margin: 0.7rem 0 0.6rem;
    align-items: center;
    border: 1px solid rgb(var(--color-surface-500) / 0.26);
    border-radius: 0.65rem;
    padding: 0.65rem 0.75rem;
    background: rgb(var(--color-surface-900) / 0.2);
  }

  .computer-target-row > span {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.12rem;
  }

  .computer-target-row select {
    max-width: min(46%, 14rem);
    min-height: 1.9rem;
    flex: 0 1 auto;
    border: 1px solid rgb(var(--color-surface-500) / 0.28);
    border-radius: 0.5rem;
    padding: 0.3rem 1.65rem 0.3rem 0.5rem;
    background-color: rgb(var(--color-surface-800) / 0.5);
    color: rgb(var(--theme-text-secondary));
    font-size: 0.72rem;
  }

  .computer-target-row select:focus-visible {
    outline: 2px solid rgb(var(--color-primary-400) / 0.58);
    outline-offset: 2px;
  }

  .computer-refresh {
    display: inline-flex;
    width: 1.8rem;
    height: 1.8rem;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: rgb(var(--theme-text-quiet));
  }

  .computer-refresh:hover:not(:disabled) {
    background: rgb(var(--color-surface-700) / 0.35);
    color: rgb(var(--color-surface-100));
  }

  .computer-refresh:disabled {
    opacity: 0.5;
  }

  .computer-refresh:focus-visible {
    outline: 2px solid rgb(var(--color-primary-400) / 0.65);
    outline-offset: 2px;
  }

  .computer-empty,
  .computer-driver {
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    border-radius: 0.65rem;
    background: rgb(var(--color-surface-900) / 0.28);
  }

  .computer-empty {
    padding: 0.8rem 0.9rem;
  }

  .computer-drivers {
    display: grid;
    gap: 0.5rem;
  }

  .computer-driver-heading {
    padding: 0.75rem 0.85rem;
  }

  .computer-driver-state,
  .computer-permission-status {
    flex: 0 0 auto;
    font-size: 0.65rem;
    font-weight: 600;
    color: rgb(var(--theme-text-quiet));
  }

  .computer-driver-state-ready,
  .computer-permission-ready {
    color: rgb(var(--color-success-400));
  }

  .computer-permission-needed {
    color: rgb(var(--color-warning-400));
  }

  .computer-driver-error {
    margin: 0;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.22);
    padding: 0.65rem 0.85rem;
    font-size: 0.7rem;
    line-height: 1.45;
    color: rgb(var(--color-warning-400));
  }

  .computer-permissions {
    border-top: 1px solid rgb(var(--color-surface-500) / 0.22);
  }

  .computer-permission {
    padding: 0.6rem 0.85rem;
  }

  .computer-permission + .computer-permission {
    border-top: 1px solid rgb(var(--color-surface-500) / 0.18);
  }

  @media (max-width: 520px) {
    .computer-target-row {
      align-items: stretch;
      flex-direction: column;
    }

    .computer-target-row select {
      width: 100%;
      max-width: none;
    }
  }

  .computer-watch-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.22);
    padding: 0.65rem 0.85rem;
  }

  .computer-watch-row > span {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 0.12rem;
  }

  .computer-watch-row button {
    display: inline-flex;
    min-height: 1.9rem;
    flex: none;
    align-items: center;
    gap: 0.3rem;
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-700) / 0.55);
    padding: 0.35rem 0.55rem;
    font-size: 0.68rem;
    font-weight: 580;
    color: rgb(var(--color-surface-100));
  }

  .computer-watch-row button:hover:not(:disabled) {
    background: rgb(var(--color-primary-500) / 0.16);
    color: rgb(var(--color-primary-200));
  }

  .computer-watch-row button:disabled {
    cursor: not-allowed;
    opacity: 0.42;
  }

  .computer-permission-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.12rem;
  }

  .computer-permission-title {
    font-size: 0.75rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .computer-permission-hint {
    font-size: 0.67rem;
    line-height: 1.35;
    color: rgb(var(--theme-text-quiet));
  }
</style>
