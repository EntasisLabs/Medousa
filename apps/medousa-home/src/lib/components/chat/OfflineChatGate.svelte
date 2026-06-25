<script lang="ts">
  import { onMount } from "svelte";
  import { LoaderCircle } from "@lucide/svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { layout } from "$lib/stores/layout.svelte";
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

  interface Props {
    mobile?: boolean;
    onOpenConnection?: () => void;
  }

  let { mobile = false, onOpenConnection }: Props = $props();

  let diagnosis = $state<EngineDiagnosis | null>(null);
  let actionMessage = $state<string | null>(null);
  let busy = $state(false);

  onMount(() => {
    if (isTauri() && !isTauriMobilePlatform()) {
      void refreshDiagnosis();
    }
  });

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
    if (mobile) {
      layout.openYou("settings");
    }
  }

  const title = $derived(
    diagnosis?.title ??
      (isTauriMobilePlatform()
        ? "Medousa isn't connected"
        : "Medousa isn't connected"),
  );

  const body = $derived(
    diagnosis?.message ??
      (isTauriMobilePlatform()
        ? "This phone can't reach Medousa on your computer yet. Check the address in Connection settings — same Wi‑Fi for the first pairing, then it should stay linked."
        : isTauri()
          ? "Medousa needs to be running on this computer before you can chat."
          : "Browser preview — run the Medousa app on your computer to chat."),
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
    isTauri() && !isTauriMobilePlatform() && diagnosis?.issue !== "binary_missing",
  );
</script>

<div
  class="absolute inset-0 z-20 flex items-center justify-center bg-surface-950/92 p-6 backdrop-blur-sm"
  role="alertdialog"
  aria-labelledby="offline-chat-title"
  aria-describedby="offline-chat-body"
>
  <div class="card w-full max-w-md space-y-4 p-6 text-center shadow-xl">
    <p class="text-3xl" aria-hidden="true">
      {diagnosis?.issue === "stale_lock" ? "🔒" : "☁️"}
    </p>
    <h2 id="offline-chat-title" class="text-lg font-semibold text-surface-50">
      {title}
    </h2>
    <p id="offline-chat-body" class="text-sm leading-relaxed text-surface-300">
      {body}
    </p>
    {#if connection.health?.message && !connection.health.ok && !diagnosis}
      <p class="text-xs text-surface-500">{connection.health.message}</p>
    {/if}
    {#if actionMessage}
      <p class="text-xs text-surface-400" role="status">{actionMessage}</p>
    {/if}
    <div class="flex flex-col gap-2 pt-2 sm:flex-row sm:justify-center">
      {#if showDesktopRecover}
        <button
          type="button"
          class="btn variant-filled-primary min-h-11"
          disabled={connection.recovering || busy}
          onclick={() => void recoverDesktop(primaryMode)}
        >
          {#if connection.recovering || busy}
            <LoaderCircle class="mr-2 inline h-4 w-4 animate-spin" aria-hidden="true" />
          {/if}
          {primaryLabel}
        </button>
      {/if}
      <button
        type="button"
        class="btn variant-soft min-h-11"
        disabled={connection.recovering || busy}
        onclick={openConnectionSettings}
      >
        Connection settings
      </button>
    </div>
    {#if diagnosis?.logPath && isTauri() && !isTauriMobilePlatform()}
      <button
        type="button"
        class="text-xs text-surface-500 underline-offset-2 hover:text-surface-300 hover:underline"
        onclick={() => void openEngineLog(diagnosis?.logPath)}
      >
        Open engine log
      </button>
    {/if}
    {#if diagnosis?.issue === "binary_missing"}
      <p class="text-xs text-surface-500">
        If this keeps happening, reinstall Medousa from the installer.
      </p>
    {/if}
  </div>
</div>
