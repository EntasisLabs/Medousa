<script lang="ts">
  import { onMount } from "svelte";
  import { ChevronRight, Laptop, QrCode } from "@lucide/svelte";
  import { setPairDeepLinkHandler } from "$lib/mobileNative";
  import { wizard } from "$lib/stores/wizard.svelte";
  import { parsePairQrUrl } from "$lib/utils/pairingUrl";
  import { workshops } from "$lib/stores/workshops.svelte";
  import {
    workshopPairingStepsHint,
    workshopQrScanHint,
  } from "$lib/platformCopy";

  let daemonUrl = $state("");
  let pairLink = $state("");
  let statusMessage = $state<string | null>(null);
  let showHelp = $state(false);

  onMount(() => {
    setPairDeepLinkHandler((url) => {
      if (applyPairLink(url)) {
        statusMessage = "Pairing link received — tap Continue when ready.";
      }
    });

    return () => {
      setPairDeepLinkHandler(null);
    };
  });

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

  async function continueSetup() {
    if (!applyPairLink(pairLink)) {
      statusMessage = "Paste a valid pairing link before continuing.";
      return;
    }

    wizard.busy = true;
    wizard.error = null;
    try {
      const paired = await workshops.joinFromPairLink(pairLink.trim());
      statusMessage =
        `Paired with ${paired.workshopPeerName}. Personal stays active until you choose to switch.`;

      await wizard.continue("mobile-client");
    } catch (err) {
      wizard.error = err instanceof Error ? err.message : String(err);
    } finally {
      wizard.busy = false;
    }
  }

  /** Linking is optional — let the shell open and connect later in Settings. */
  async function skipSetup() {
    wizard.error = null;
    statusMessage = null;
    await wizard.skipCurrent();
  }

  const canContinue = $derived.by(() => {
    if (wizard.busy) return false;
    return parsePairQrUrl(pairLink) !== null;
  });
</script>

<div class="flex h-full flex-col">
  <p class="text-[11px] font-semibold uppercase tracking-wide text-content-link">Connect</p>
  <h1 id="product-wizard-title" class="mt-2 text-2xl font-semibold text-surface-50">
    Link to your computer
  </h1>
  <p class="mt-3 text-sm leading-relaxed text-content-secondary">
    Your phone keeps its own Personal workshop. Pairing saves your computer as another workshop;
    it will not switch or send work there unless you choose it.
  </p>

  <div class="mt-6 rounded-xl border border-primary-500/35 bg-primary-500/10 p-5">
    <div class="flex items-start gap-3">
      <Laptop class="mt-0.5 h-5 w-5 shrink-0 text-content-link" aria-hidden="true" />
      <div class="min-w-0 text-sm text-content-secondary">
        <p class="font-medium text-surface-50">On your computer first</p>
        <p class="mt-2 leading-relaxed">
          Open Medousa there and finish setup. On the Pair phone step you'll see a QR code — scan
          it with your phone camera, or paste the link below.
        </p>
      </div>
    </div>
  </div>

  <label class="mt-6 block">
    <span class="block text-sm font-medium text-surface-100">
      <QrCode class="mr-2 inline h-4 w-4" aria-hidden="true" />
      Pairing link
    </span>
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
      disabled={wizard.busy}
    />
  </label>
  {#if daemonUrl}
    <p class="workshop-faint mt-2 text-xs">
      Computer address: <span class="font-mono text-content-secondary">{daemonUrl}</span>
    </p>
  {/if}

  {#if statusMessage}
    <p class="mt-4 text-sm text-content-warning">{statusMessage}</p>
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
      <li>You can pair or switch workshops later in Settings → Connection.</li>
    </ul>
  {/if}

  <div class="mt-auto flex items-center justify-between gap-3 pt-8">
    <button
      type="button"
      class="btn variant-ghost min-h-11"
      disabled={wizard.busy}
      onclick={() => void skipSetup()}
    >
      Skip for now
    </button>
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
