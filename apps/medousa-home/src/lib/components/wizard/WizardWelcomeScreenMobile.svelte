<script lang="ts">
  import { onMount } from "svelte";
  import { ChevronRight, Laptop, LoaderCircle, QrCode, Wifi } from "@lucide/svelte";
  import { checkDaemonHealth, getDaemonUrl, setDaemonUrl } from "$lib/daemon";
  import { inferDevDaemonUrl, isLoopbackDaemonUrl } from "$lib/daemonConnection";
  import { setPairDeepLinkHandler } from "$lib/mobileNative";
  import { wizard } from "$lib/stores/wizard.svelte";
  import { parsePairQrUrl } from "$lib/utils/pairingUrl";
  import { completePairingFromQr } from "$lib/utils/pairingClient";
  import {
    workshopPairingFromHostHint,
    workshopPairingStepsHint,
    workshopQrScanHint,
  } from "$lib/platformCopy";

  type ConnectMode = "address" | "pair";

  let connectMode = $state<ConnectMode>("address");
  let daemonUrl = $state("");
  let pairLink = $state("");
  let testing = $state(false);
  let statusMessage = $state<string | null>(null);
  let connected = $state(false);
  let showHelp = $state(false);

  onMount(() => {
    void loadInitialUrl();

    setPairDeepLinkHandler((url) => {
      if (applyPairLink(url)) {
        connectMode = "pair";
        statusMessage = "Pairing link received — tap Continue when ready.";
      }
    });

    return () => {
      setPairDeepLinkHandler(null);
    };
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

  function applyPairLink(raw: string): boolean {
    const parsed = parsePairQrUrl(raw);
    if (!parsed) return false;
    daemonUrl = parsed.daemonUrl;
    pairLink = raw.trim();
    return true;
  }

  function onPairLinkInput() {
    if (applyPairLink(pairLink)) {
      statusMessage = null;
    }
  }

  async function testConnection(showErrors = true) {
    if (connectMode === "pair" && pairLink.trim() && !urlLooksValid(daemonUrl)) {
      if (!applyPairLink(pairLink)) {
        if (showErrors) {
          statusMessage = "Paste the full medousa:// pairing link from your computer.";
        }
        connected = false;
        return;
      }
    }

    if (!urlLooksValid(daemonUrl)) {
      if (showErrors) {
        statusMessage =
          connectMode === "pair"
            ? workshopPairingFromHostHint()
            : "Enter your computer's address — e.g. http://192.168.1.42:7419";
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
        ? "Connected — you're ready to talk."
        : health.message || "Could not reach Medousa on that computer yet.";
    } catch (err) {
      connected = false;
      statusMessage = err instanceof Error ? err.message : String(err);
    } finally {
      testing = false;
    }
  }

  async function continueSetup() {
    if (connectMode === "pair" && pairLink.trim()) {
      applyPairLink(pairLink);
    }

    if (!urlLooksValid(daemonUrl)) {
      statusMessage =
        connectMode === "pair"
          ? "Paste a valid pairing link before continuing."
          : "Enter a valid address before continuing.";
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
          "Could not connect yet. Check that Medousa is running on your computer and on the same Wi‑Fi.";
        return;
      }

      if (connectMode === "pair" && pairLink.trim()) {
        try {
          const paired = await completePairingFromQr({
            qrUrl: pairLink.trim(),
            daemonUrl: daemonUrl.trim(),
          });
          statusMessage = `Paired with ${paired.workshopPeerName} — you're ready to talk.`;
        } catch (err) {
          wizard.error = err instanceof Error ? err.message : String(err);
          return;
        }
      }

      await wizard.continue("mobile-client");
    } finally {
      wizard.busy = false;
    }
  }

  const canContinue = $derived.by(() => {
    if (wizard.busy || testing) return false;
    if (connectMode === "pair") {
      return pairLink.trim().length > 0 || urlLooksValid(daemonUrl);
    }
    return urlLooksValid(daemonUrl);
  });
</script>

<div class="flex h-full flex-col">
  <p class="text-[11px] font-semibold uppercase tracking-wide text-primary-300">Connect</p>
  <h1 id="product-wizard-title" class="mt-2 text-2xl font-semibold text-surface-50">
    Link to your computer
  </h1>
  <p class="mt-3 text-sm leading-relaxed text-surface-300">
    Medousa on your phone talks to Medousa on your computer over home Wi‑Fi. Models and memory
    stay on the computer — this app is your window in.
  </p>

  <div class="mt-6 rounded-xl border border-primary-500/35 bg-primary-500/10 p-5">
    <div class="flex items-start gap-3">
      <Laptop class="mt-0.5 h-5 w-5 shrink-0 text-primary-300" aria-hidden="true" />
      <div class="min-w-0 text-sm text-surface-300">
        <p class="font-medium text-surface-50">On your computer first</p>
        <p class="mt-2 leading-relaxed">
          Open Medousa there and finish setup. On the Pair phone step you'll see a QR code — scan
          it with your phone camera, or paste the link below.
        </p>
      </div>
    </div>
  </div>

  <div class="mt-6 flex gap-2">
    <button
      type="button"
      class="btn min-h-10 flex-1 text-sm {connectMode === 'pair'
        ? 'variant-filled-primary'
        : 'variant-soft'}"
      disabled={wizard.busy || testing}
      onclick={() => (connectMode = "pair")}
    >
      <QrCode class="mr-2 inline h-4 w-4" aria-hidden="true" />
      Pairing link
    </button>
    <button
      type="button"
      class="btn min-h-10 flex-1 text-sm {connectMode === 'address'
        ? 'variant-filled-primary'
        : 'variant-soft'}"
      disabled={wizard.busy || testing}
      onclick={() => (connectMode = "address")}
    >
      Enter address
    </button>
  </div>

  {#if connectMode === "pair"}
    <label class="mt-6 block">
      <span class="block text-sm font-medium text-surface-100">Pairing link</span>
      <span class="workshop-faint mt-0.5 block text-xs">
        {workshopQrScanHint()}
      </span>
      <input
        class="input mt-2 w-full font-mono text-sm"
        type="text"
        inputmode="url"
        autocapitalize="off"
        autocorrect="off"
        spellcheck="false"
        placeholder="medousa://pair/1.0?a=…"
        bind:value={pairLink}
        oninput={onPairLinkInput}
        onchange={onPairLinkInput}
        disabled={wizard.busy || testing}
      />
    </label>
    {#if daemonUrl && urlLooksValid(daemonUrl)}
      <p class="workshop-faint mt-2 text-xs">
        Computer address: <span class="font-mono text-surface-300">{daemonUrl}</span>
      </p>
    {/if}
  {:else}
    <label class="mt-6 block">
      <span class="block text-sm font-medium text-surface-100">Computer address</span>
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
  {/if}

  <div class="mt-4 flex flex-wrap gap-3">
    <button
      type="button"
      class="btn variant-soft min-h-11"
      disabled={wizard.busy || testing || (connectMode === "address" && !daemonUrl.trim())}
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

  <button
    type="button"
    class="workshop-text-action mt-4 self-start text-xs"
    onclick={() => (showHelp = !showHelp)}
  >
    {showHelp ? "Hide" : "Connection not working?"}
  </button>

  {#if showHelp}
    <ul class="workshop-faint mt-2 list-disc space-y-1 pl-5 text-xs leading-relaxed">
      <li>Phone and computer must be on the same Wi‑Fi (guest networks often block this).</li>
      <li>Medousa must be running on the computer before you connect.</li>
      <li>
        {workshopPairingStepsHint()}
        if needed.
      </li>
      <li>You can change this address later in Settings → Connection.</li>
    </ul>
  {/if}

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
