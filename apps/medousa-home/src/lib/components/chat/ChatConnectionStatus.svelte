<script lang="ts">
  import { LoaderCircle } from "@lucide/svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { isTauri } from "$lib/window";
  import {
    clearEngineStaleLock,
    diagnoseEngine,
    openEngineLog,
    type EngineDiagnosis,
  } from "$lib/utils/engineDiagnosticsApi";
  import { restartEngine, startEngine, waitForEngine } from "$lib/utils/providersApi";
  import { reconnectWorkshop } from "$lib/workshopConnection";
  import { workshops } from "$lib/stores/workshops.svelte";

  interface Props {
    onOpenConnection?: () => void;
  }

  let { onOpenConnection }: Props = $props();

  let diagnosis = $state<EngineDiagnosis | null>(null);
  let actionMessage = $state<string | null>(null);
  let busy = $state(false);
  let detailsOpen = $state(false);

  $effect(() => {
    if (!connection.offline) {
      detailsOpen = false;
      diagnosis = null;
      actionMessage = null;
    }
  });

  function toggleDetails() {
    detailsOpen = !detailsOpen;
    if (detailsOpen && workshops.activeWorkshop?.kind === "local") {
      void refreshDiagnosis();
    }
  }

  async function refreshDiagnosis() {
    try {
      diagnosis = await diagnoseEngine();
    } catch {
      diagnosis = null;
    }
  }

  async function recoverDesktop(mode: "start" | "restart" | "fix_lock") {
    if (connection.recovering || busy) return;
    connection.setRecovering(true);
    busy = true;
    actionMessage = null;
    try {
      if (mode === "fix_lock") {
        actionMessage = "Clearing leftover lock…";
        await clearEngineStaleLock();
      }
      actionMessage =
        mode === "restart" ? "Restarting Medousa…" : "Starting Medousa…";
      if (mode === "restart") {
        await restartEngine();
      } else {
        await startEngine();
      }
      const wait = await waitForEngine(45);
      const health = await reconnectWorkshop((next) => connection.setHealth(next));
      await refreshDiagnosis();
      actionMessage = health.ok
        ? "Connected — you can send a message now."
        : wait.message || health.message || diagnosis?.message || null;
    } catch (err) {
      actionMessage = err instanceof Error ? err.message : String(err);
      await refreshDiagnosis();
    } finally {
      connection.setRecovering(false);
      busy = false;
    }
  }

  function openConnectionSettings() {
    if (onOpenConnection) {
      onOpenConnection();
      return;
    }
    settingsNav.setActiveSection("basement");
    layout.openMore("settings");
  }

  const title = $derived(diagnosis?.title ?? "Medousa isn't connected");
  const body = $derived(
    diagnosis?.message ?? "Medousa needs to be running on this computer before you can send messages.",
  );

  const primaryLabel = $derived.by(() => {
    if (!diagnosis) return "Start Medousa";
    switch (diagnosis.issue) {
      case "stale_lock":
        return "Fix and start";
      case "port_blocked":
      case "wedged":
        return "Restart Medousa";
      default:
        return "Start Medousa";
    }
  });

  const primaryMode = $derived.by((): "start" | "restart" | "fix_lock" => {
    if (diagnosis?.issue === "stale_lock") return "fix_lock";
    if (diagnosis?.issue === "port_blocked" || diagnosis?.issue === "wedged") {
      return "restart";
    }
    return "start";
  });

  const showDesktopRecover = $derived(
    isTauri() && !isTauriMobilePlatform() && workshops.activeWorkshop?.kind === "local" && diagnosis?.issue !== "binary_missing",
  );
</script>

<!-- Reserve the same space through health changes so the transcript never jumps. -->
<div class="chat-connection-status">
  <span class="text-xs text-content-quiet" role="status">
    {#if connection.checking}Connecting…{:else if connection.offline}Reconnecting…{/if}
  </span>
  {#if connection.offline}
    <div class="flex items-center gap-3">
      <button
        type="button"
        class="chat-connection-action"
        disabled={connection.recovering || workshops.refreshing || workshops.switching || busy}
        onclick={() => void workshops.refreshConnection()}
      >Retry</button>
      {#if isTauri() && !isTauriMobilePlatform() && workshops.activeWorkshop?.kind === "local"}
        <button type="button" class="chat-connection-action" aria-expanded={detailsOpen} onclick={toggleDetails}>Details</button>
      {/if}
      <button type="button" class="chat-connection-action" onclick={openConnectionSettings}>Connection settings</button>
    </div>
  {/if}
</div>

<!-- Recovery details are opt-in, never an automatic modal over the conversation. -->
{#if connection.offline && detailsOpen && workshops.activeWorkshop?.kind === "local"}
  <div class="card mx-4 mb-2 space-y-3 p-4" aria-label="Connection recovery">
    <h2 class="text-sm font-semibold text-surface-50">{title}</h2>
    <p class="text-xs leading-relaxed text-content-secondary">{body}</p>
    {#if connection.health?.message && !connection.health.ok && !diagnosis}
      <p class="text-xs text-content-quiet">{connection.health.message}</p>
    {/if}
    {#if actionMessage}
      <p class="text-xs text-content-tertiary" role="status">{actionMessage}</p>
    {/if}
    {#if showDesktopRecover}
      <button type="button" class="btn variant-soft min-h-9" disabled={connection.recovering || busy} onclick={() => void recoverDesktop(primaryMode)}>
        {#if connection.recovering || busy}
          <LoaderCircle class="mr-2 inline h-4 w-4 animate-spin" aria-hidden="true" />
        {/if}
        {primaryLabel}
      </button>
    {/if}
    {#if diagnosis?.logPath && isTauri() && !isTauriMobilePlatform()}
      <button type="button" class="chat-connection-action" onclick={() => void openEngineLog(diagnosis?.logPath)}>Open engine log</button>
    {/if}
  </div>
{/if}

<style>
  .chat-connection-status {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    height: 2rem;
    flex-shrink: 0;
    margin: 0 1rem;
  }
  .chat-connection-action {
    min-height: 2rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.6875rem;
  }
  .chat-connection-action:hover { color: rgb(var(--theme-text-secondary)); }
  .chat-connection-action:disabled { opacity: 0.5; }
</style>
