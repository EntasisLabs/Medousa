<script lang="ts">
  import { onMount } from "svelte";
  import { LoaderCircle } from "@lucide/svelte";
  import { wizard } from "$lib/stores/wizard.svelte";
  import { checkDaemonHealth, type DaemonHealth } from "$lib/daemon";
  import { startDaemonCore, waitForDaemonCore } from "$lib/utils/providersApi";
  import { isTauri } from "$lib/window";

  let health = $state<DaemonHealth | null>(null);
  let checking = $state(true);
  let starting = $state(false);
  let statusLine = $state("Checking Medousa Core…");

  onMount(() => {
    void ensureCoreReady();
  });

  async function ensureCoreReady() {
    checking = true;
    try {
      if (!isTauri()) {
        statusLine = "Dev browser mode — start medousa_daemon separately.";
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
      statusLine = "Starting Medousa Core…";
      await startDaemonCore();
      const wait = await waitForDaemonCore(30);
      health = await checkDaemonHealth();
      statusLine = wait.ok ? wait.message : wait.message;
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
  <h2 class="mt-4 text-2xl font-semibold text-surface-50">You're ready!</h2>
  <p class="mt-3 max-w-sm text-sm leading-relaxed text-surface-300">
    {#if health?.ok}
      Medousa Core is running. Your brain is online. Ask me anything when you're back in the
      workshop.
    {:else}
      Your model is configured. Medousa Core may still be starting — you can retry from Settings if
      chat doesn't connect.
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
  >
    <p class="text-sm text-surface-400">Ask me anything…</p>
  </div>

  <button
    type="button"
    class="btn variant-filled-primary mt-10 min-h-11 px-8"
    disabled={wizard.busy || checking || starting}
    onclick={() => void wizard.finish()}
  >
    Start talking →
  </button>
</div>
