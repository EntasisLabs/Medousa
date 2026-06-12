<script lang="ts">
  import { LoaderCircle } from "@lucide/svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { isTauriMobilePlatform } from "$lib/platform";
  import { isTauri } from "$lib/window";
  import { startEngine, waitForEngine } from "$lib/utils/providersApi";
  import { reconnectWorkshop } from "$lib/workshopConnection";

  interface Props {
    mobile?: boolean;
    onOpenConnection?: () => void;
  }

  let { mobile = false, onOpenConnection }: Props = $props();

  let message = $state<string | null>(null);

  async function recoverDesktop() {
    if (connection.recovering) return;
    connection.setRecovering(true);
    message = "Starting Medousa…";
    try {
      await startEngine();
      const wait = await waitForEngine(30);
      const health = await reconnectWorkshop((next) => connection.setHealth(next));
      message = health.ok
        ? "Connected — you can send a message now."
        : wait.message || health.message;
    } catch (err) {
      message = err instanceof Error ? err.message : String(err);
    } finally {
      connection.setRecovering(false);
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
</script>

<div
  class="absolute inset-0 z-20 flex items-center justify-center bg-surface-950/92 p-6 backdrop-blur-sm"
  role="alertdialog"
  aria-labelledby="offline-chat-title"
  aria-describedby="offline-chat-body"
>
  <div class="card w-full max-w-md space-y-4 p-6 text-center shadow-xl">
    <p class="text-3xl" aria-hidden="true">☁️</p>
    <h2 id="offline-chat-title" class="text-lg font-semibold text-surface-50">
      Medousa isn't connected
    </h2>
    <p id="offline-chat-body" class="text-sm leading-relaxed text-surface-300">
      {#if isTauriMobilePlatform()}
        This phone can't reach Medousa on your computer yet. Check the address in Connection
        settings — same Wi‑Fi helps.
      {:else if isTauri()}
        Medousa needs to be running on this computer before you can chat.
      {:else}
        Browser preview — run the Medousa app on your computer to chat.
      {/if}
    </p>
    {#if connection.health?.message && !connection.health.ok}
      <p class="text-xs text-surface-500">{connection.health.message}</p>
    {/if}
    {#if message}
      <p class="text-xs text-surface-400">{message}</p>
    {/if}
    <div class="flex flex-col gap-2 pt-2 sm:flex-row sm:justify-center">
      {#if isTauri() && !isTauriMobilePlatform()}
        <button
          type="button"
          class="btn variant-filled-primary min-h-11"
          disabled={connection.recovering}
          onclick={() => void recoverDesktop()}
        >
          {#if connection.recovering}
            <LoaderCircle class="mr-2 inline h-4 w-4 animate-spin" aria-hidden="true" />
          {/if}
          Start Medousa
        </button>
      {/if}
      <button
        type="button"
        class="btn variant-soft min-h-11"
        disabled={connection.recovering}
        onclick={openConnectionSettings}
      >
        Connection settings
      </button>
    </div>
  </div>
</div>
