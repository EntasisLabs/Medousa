<script lang="ts">
  import { onMount } from "svelte";
  import { ChevronRight, Laptop, LoaderCircle, Wifi } from "@lucide/svelte";
  import { checkDaemonHealth, getDaemonUrl, setDaemonUrl } from "$lib/daemon";
  import { inferDevDaemonUrl, isLoopbackDaemonUrl } from "$lib/daemonConnection";
  import { wizard } from "$lib/stores/wizard.svelte";

  let daemonUrl = $state("");
  let testing = $state(false);
  let statusMessage = $state<string | null>(null);
  let connected = $state(false);

  onMount(() => {
    void loadInitialUrl();
  });

  async function loadInitialUrl() {
    try {
      const current = (await getDaemonUrl()).trim();
      if (current && !isLoopbackDaemonUrl(current)) {
        daemonUrl = current;
      } else {
        daemonUrl = inferDevDaemonUrl() ?? "";
      }
      if (daemonUrl) {
        await testConnection(false);
      }
    } catch {
      daemonUrl = inferDevDaemonUrl() ?? "";
    }
  }

  function urlLooksValid(value: string): boolean {
    try {
      const parsed = new URL(value.trim());
      return (
        (parsed.protocol === "http:" || parsed.protocol === "https:") &&
        !isLoopbackDaemonUrl(parsed.toString())
      );
    } catch {
      return false;
    }
  }

  async function testConnection(showErrors = true) {
    if (!urlLooksValid(daemonUrl)) {
      if (showErrors) {
        statusMessage = "Enter your Mac's workshop address — e.g. http://192.168.1.42:7419";
      }
      connected = false;
      return;
    }

    testing = true;
    statusMessage = null;
    wizard.error = null;
    try {
      await setDaemonUrl(daemonUrl.trim());
      const health = await checkDaemonHealth();
      connected = health.ok;
      statusMessage = health.ok
        ? health.message
        : health.message || "Could not reach Medousa Core on your Mac";
    } catch (err) {
      connected = false;
      statusMessage = err instanceof Error ? err.message : String(err);
    } finally {
      testing = false;
    }
  }

  async function continueSetup() {
    if (!urlLooksValid(daemonUrl)) {
      statusMessage = "Enter a valid Mac workshop URL before continuing.";
      return;
    }

    wizard.busy = true;
    wizard.error = null;
    try {
      await setDaemonUrl(daemonUrl.trim());
      if (!connected) {
        await testConnection(false);
      }
      if (!connected) {
        wizard.error =
          statusMessage ??
          "Could not reach your Mac yet — fix the URL or start Core with `medousa start daemon --public`.";
      }
      await wizard.continue("mobile-client");
    } finally {
      wizard.busy = false;
    }
  }

  const canContinue = $derived.by(
    () => !wizard.busy && !testing && urlLooksValid(daemonUrl),
  );
</script>

<div class="flex h-full flex-col">
  <p class="text-[11px] font-semibold uppercase tracking-wide text-primary-300">Step 1 of 3</p>
  <h1 id="product-wizard-title" class="mt-2 text-2xl font-semibold text-surface-50">
    Connect to your Mac
  </h1>
  <p class="mt-3 text-sm leading-relaxed text-surface-300">
    Your phone is a window into the brain on your Mac. Models and memory live there — this app talks
    to Medousa Core over your home Wi‑Fi.
  </p>

  <div class="mt-6 rounded-xl border border-primary-500/35 bg-primary-500/10 p-5">
    <div class="flex items-start gap-3">
      <Laptop class="mt-0.5 h-5 w-5 shrink-0 text-primary-300" aria-hidden="true" />
      <div class="min-w-0 text-sm text-surface-300">
        <p class="font-medium text-surface-50">On your Mac first</p>
        <p class="mt-2 leading-relaxed">
          Run
          <span class="font-mono text-xs text-surface-200">medousa start daemon --public</span>
          and note the LAN address it prints (not 127.0.0.1).
        </p>
      </div>
    </div>
  </div>

  <label class="mt-6 block">
    <span class="block text-sm font-medium text-surface-100">Mac workshop URL</span>
    <span class="workshop-faint mt-0.5 block text-xs">Same Wi‑Fi as this phone</span>
    <input
      class="input mt-2 w-full font-mono text-sm"
      type="url"
      inputmode="url"
      autocapitalize="off"
      autocorrect="off"
      spellcheck="false"
      placeholder="http://192.168.1.42:7419"
      bind:value={daemonUrl}
      disabled={wizard.busy || testing}
    />
  </label>

  <div class="mt-4 flex flex-wrap gap-3">
    <button
      type="button"
      class="btn variant-soft min-h-11"
      disabled={wizard.busy || testing || !daemonUrl.trim()}
      onclick={() => void testConnection()}
    >
      {#if testing}
        <LoaderCircle class="mr-2 h-4 w-4 animate-spin" aria-hidden="true" />
      {:else}
        <Wifi class="mr-2 h-4 w-4" aria-hidden="true" />
      {/if}
      Test connection
    </button>
  </div>

  {#if statusMessage}
    <p class="mt-4 text-sm {connected ? 'text-success-200' : 'text-warning-200'}">{statusMessage}</p>
  {/if}

  <p class="workshop-faint mt-4 text-xs leading-relaxed">
    Ollama and API keys are configured on the Mac workshop — not on your phone.
  </p>

  <div class="mt-auto flex justify-end pt-8">
    <button
      type="button"
      class="btn variant-filled-primary inline-flex min-h-11 items-center gap-2 px-6"
      disabled={!canContinue}
      onclick={() => void continueSetup()}
    >
      Continue
      <ChevronRight class="h-4 w-4" aria-hidden="true" />
    </button>
  </div>
</div>
