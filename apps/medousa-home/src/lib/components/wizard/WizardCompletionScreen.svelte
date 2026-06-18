<script lang="ts">
  import { onMount } from "svelte";
  import { LoaderCircle, MessageCircle, Orbit, BookOpen } from "@lucide/svelte";
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
  <h2 class="mt-4 text-2xl font-semibold text-surface-50">Your workshop is ready</h2>
  <p class="mt-3 max-w-md text-sm leading-relaxed text-surface-300">
    {#if health?.ok}
      {#if isTauriMobilePlatform()}
        You're linked to Medousa on your computer. She remembers what you share and leaves notes
        you can find again.
      {:else}
        Medousa is running on this computer. Share something on your mind — she'll hold the thread
        and can turn work into notes in your Library.
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

  {#if health?.ok}
    <div class="mt-8 w-full max-w-md text-left">
      <p class="text-xs font-medium uppercase tracking-wide text-surface-500">Try this first</p>
      <ul class="mt-3 space-y-2">
        <li
          class="flex items-start gap-3 rounded-xl border border-surface-500/35 bg-surface-950/60 px-4 py-3"
        >
          <MessageCircle class="mt-0.5 h-4 w-4 shrink-0 text-primary-300" aria-hidden="true" />
          <div>
            <p class="text-sm font-medium text-surface-100">Chat</p>
            <p class="mt-0.5 text-xs leading-relaxed text-surface-400">
              Tell her what's on your mind — a worry, a plan, a messy list.
            </p>
          </div>
        </li>
        <li
          class="flex items-start gap-3 rounded-xl border border-surface-500/35 bg-surface-950/60 px-4 py-3"
        >
          <Orbit class="mt-0.5 h-4 w-4 shrink-0 text-primary-300" aria-hidden="true" />
          <div>
            <p class="text-sm font-medium text-surface-100">Context</p>
            <p class="mt-0.5 text-xs leading-relaxed text-surface-400">
              After a few turns, open Threads &amp; memory to see what she's holding.
            </p>
          </div>
        </li>
        <li
          class="flex items-start gap-3 rounded-xl border border-surface-500/35 bg-surface-950/60 px-4 py-3"
        >
          <BookOpen class="mt-0.5 h-4 w-4 shrink-0 text-primary-300" aria-hidden="true" />
          <div>
            <p class="text-sm font-medium text-surface-100">Library</p>
            <p class="mt-0.5 text-xs leading-relaxed text-surface-400">
              Ask her to save a guide or checklist — it lands here as stone you can revisit.
            </p>
          </div>
        </li>
      </ul>
    </div>
  {/if}

  <button
    type="button"
    class="btn variant-filled-primary mt-10 min-h-11 px-8"
    disabled={wizard.busy || checking || starting || (!isTauriMobilePlatform() && !health?.ok && isTauri())}
    onclick={() => void wizard.finish()}
  >
    {health?.ok ? "Start with chat →" : "Continue anyway →"}
  </button>
</div>
