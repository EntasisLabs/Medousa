<script lang="ts">
  import { onMount } from "svelte";
  import { LoaderCircle } from "@lucide/svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import { checkDaemonHealth, type DaemonHealth } from "$lib/daemon";
  import { requireEngineReady } from "$lib/utils/providersApi";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { isTauri } from "$lib/window";

  let health = $state<DaemonHealth | null>(null);
  let checking = $state(true);
  let starting = $state(false);
  let statusLine = $state("Checking connection…");

  onMount(() => {
    void ensureCoreReady();
  });

  async function ensureCoreReady() {
    checking = true;
    try {
      if (!isTauri()) {
        statusLine = "Browser preview — start Medousa on your computer separately.";
        checking = false;
        return;
      }
      if (isTauriMobilePlatform()) {
        health = await checkDaemonHealth();
        statusLine = health.ok
          ? "Connected to your computer."
          : "Not connected yet — check the address in Settings → Connection.";
        checking = false;
        return;
      }
      health = await checkDaemonHealth();
      if (health.ok) {
        statusLine = health.message;
        checking = false;
        return;
      }

      starting = true;
      statusLine = "Starting Medousa…";
      try {
        const wait = await requireEngineReady({ timeoutSeconds: 30 });
        health = await checkDaemonHealth();
        statusLine = wait.message;
      } catch (err) {
        health = await checkDaemonHealth();
        statusLine =
          err instanceof Error
            ? err.message
            : "Medousa engine did not start — go back and finish setup, or try again.";
      }
    } catch (err) {
      statusLine = err instanceof Error ? err.message : String(err);
    } finally {
      checking = false;
      starting = false;
    }
  }
</script>

<div class="flex h-full flex-col items-center justify-center text-center">
  <p class="text-3xl" aria-hidden="true">🎉</p>
  <h2 class="mt-4 text-2xl font-semibold text-surface-50">You're ready</h2>
  <p class="mt-3 max-w-sm text-sm leading-relaxed text-surface-300">
    {#if health?.ok}
      {#if isTauriMobilePlatform()}
        You're linked to Medousa on your computer. Open Chat when you're ready.
      {:else}
        Medousa is running on this computer. Ask anything when you're back in the app.
      {/if}
    {:else if isTauriMobilePlatform()}
      Setup is saved. Link to your computer in Settings → Connection if chat stays offline.
    {:else}
      The engine is not running yet. Go back to finish setup, or check Settings → Connection.
    {/if}
  </p>

  <div
    class="mt-6 flex w-full max-w-md items-center justify-center gap-2 rounded-xl border border-surface-500/35 bg-surface-950/60 px-4 py-3 text-sm text-surface-300"
  >
    {#if checking || starting}
      <LoaderCircle class="h-4 w-4 shrink-0 animate-spin text-primary-300" aria-hidden="true" />
    {/if}
    <span>{statusLine}</span>
  </div>

  <div
    class="mt-8 w-full max-w-md rounded-xl border border-surface-500/35 bg-surface-950/60 px-4 py-5 text-left"
    aria-hidden="true"
  >
    <p class="text-sm text-surface-500">Preview — you'll type in chat next</p>
  </div>

  <button
    type="button"
    class="btn variant-filled-primary mt-10 min-h-11 px-8"
    disabled={wizard.busy || checking || starting || (!isTauriMobilePlatform() && !health?.ok && isTauri())}
    onclick={() => void wizard.finish()}
  >
    Start talking →
  </button>
</div>
