<script lang="ts">
  import { onMount } from "svelte";
  import {
    CheckCircle2,
    ChevronDown,
    Copy,
    LoaderCircle,
    RefreshCw,
    Smartphone,
    X,
  } from "@lucide/svelte";
  import {
    fetchBonjourStatus,
    fetchPairingQr,
    fetchPairingQrImage,
    fetchPairingStatus,
    formatCountdown,
    formatShortCode,
    revokePairingDevice,
    rotatePairingInvite,
    secondsUntil,
    waitForPairingQr,
    type BonjourStatus,
    type PairedDeviceSummary,
    type PairingQrImage,
  } from "$lib/utils/pairingApi";
  import { waitForEngine } from "$lib/utils/providersApi";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { sharedMode } from "$lib/stores/sharedMode.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import { isTauri } from "$lib/window";

  interface Props {
    mode?: "wizard" | "settings";
    onPaired?: (device: PairedDeviceSummary) => void;
  }

  let { mode = "settings", onPaired }: Props = $props();

  let loading = $state(true);
  let refreshing = $state(false);
  let qrLoading = $state(false);
  let error = $state<string | null>(null);
  let qr = $state<PairingQrImage | null>(null);
  let countdown = $state(0);
  let bonjour = $state<BonjourStatus | null>(null);
  let devices = $state<PairedDeviceSummary[]>([]);
  let knownPairingIds = $state<string[]>([]);
  let connectedDevice = $state<PairedDeviceSummary | null>(null);
  let showDiagnostics = $state(false);
  let coreOnline = $state(false);
  let copyFlash = $state(false);
  let copyHint = $state<string | null>(null);
  let inviteProfileId = $state("");
  let sheetOpen = $state(false);
  let sheetTitle = $state("Pair a phone");

  let countdownTimer: ReturnType<typeof setInterval> | null = null;
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let qrRefreshTimer: ReturnType<typeof setInterval> | null = null;

  const settingsMode = $derived(mode === "settings");
  const showInlineQr = $derived(!settingsMode);

  onMount(() => {
    void sharedMode.load();
    void userProfiles.load({ suppressRemoteNotice: true });
    void bootstrap();
    return () => cleanupTimers();
  });

  $effect(() => {
    if (!inviteProfileId && userProfiles.profiles.length > 0) {
      inviteProfileId = userProfiles.activeProfileId ?? userProfiles.profiles[0]?.profile_id ?? "";
    }
  });

  function cleanupTimers() {
    if (countdownTimer) clearInterval(countdownTimer);
    if (pollTimer) clearInterval(pollTimer);
    if (qrRefreshTimer) clearInterval(qrRefreshTimer);
    countdownTimer = null;
    pollTimer = null;
    qrRefreshTimer = null;
  }

  function stopQrTimers() {
    if (countdownTimer) clearInterval(countdownTimer);
    if (qrRefreshTimer) clearInterval(qrRefreshTimer);
    countdownTimer = null;
    qrRefreshTimer = null;
  }

  async function loadStatusOnly() {
    try {
      const status = await fetchPairingStatus();
      devices = status.pairedDevices;
      knownPairingIds = status.pairedDevices.map((device) => device.pairingId);
    } catch {
      // Best effort.
    }
    try {
      bonjour = await fetchBonjourStatus();
    } catch {
      bonjour = null;
    }
  }

  async function loadPairingBundle(timeoutSeconds = 45) {
    qrLoading = true;
    try {
      qr = await waitForPairingQr(timeoutSeconds);
      countdown = secondsUntil(qr.expiresAt);
      error = null;
      await loadStatusOnly();
    } finally {
      qrLoading = false;
    }
  }

  async function bootstrap() {
    loading = true;
    error = null;
    try {
      if (!isTauri()) {
        error =
          "Phone pairing needs Medousa running on this computer. Finish setup or open Connection settings.";
        return;
      }
      const health = await waitForEngine(45);
      coreOnline = health.ok;
      if (!health.ok) {
        error = health.message;
        return;
      }
      if (settingsMode) {
        await loadStatusOnly();
        startDevicePoll();
      } else {
        await loadPairingBundle(60);
        startTimers();
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function startDevicePoll() {
    if (pollTimer) clearInterval(pollTimer);
    pollTimer = setInterval(() => {
      void pollStatus();
    }, 3000);
  }

  function startTimers() {
    cleanupTimers();
    countdownTimer = setInterval(() => {
      if (!qr) return;
      countdown = secondsUntil(qr.expiresAt);
      if (countdown <= 0) {
        void refreshQr({ silent: true });
      }
    }, 1000);
    startDevicePoll();
    qrRefreshTimer = setInterval(() => {
      void refreshQr({ silent: true });
    }, 25_000);
  }

  async function openPairingSheet(options?: { title?: string; profileId?: string }) {
    if (!coreOnline || refreshing) return;
    sheetTitle = options?.title ?? "Pair a phone";
    sheetOpen = true;
    error = null;
    refreshing = true;
    try {
      if (options?.profileId) {
        await rotatePairingInvite({ profileId: options.profileId });
      }
      await loadPairingBundle(30);
      startTimers();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      refreshing = false;
    }
  }

  function closePairingSheet() {
    sheetOpen = false;
    stopQrTimers();
    qr = null;
    countdown = 0;
    copyHint = null;
    startDevicePoll();
  }

  async function rotateInvite(profileId?: string) {
    if (refreshing || qrLoading) return;
    refreshing = true;
    error = null;
    try {
      await rotatePairingInvite(profileId ? { profileId } : undefined);
      await loadPairingBundle(15);
      if (settingsMode && !sheetOpen) {
        sheetTitle = profileId ? "Seat invite" : "Pair a phone";
        sheetOpen = true;
      }
      startTimers();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      refreshing = false;
    }
  }

  async function inviteSeat() {
    const profileId = inviteProfileId.trim();
    if (!profileId) {
      error = "Choose a seat profile to invite.";
      return;
    }
    await openPairingSheet({ title: "Seat invite", profileId });
  }

  async function refreshAll() {
    if (settingsMode && !sheetOpen) {
      await openPairingSheet();
      return;
    }
    refreshing = true;
    error = null;
    try {
      await loadPairingBundle(30);
      startTimers();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      refreshing = false;
    }
  }

  async function refreshQr(options?: { silent?: boolean; retries?: number }) {
    const silent = options?.silent ?? false;
    const maxAttempts = options?.retries ?? 4;
    if (!silent) {
      qrLoading = true;
    }
    let lastError: string | null = null;
    for (let attempt = 0; attempt < maxAttempts; attempt += 1) {
      if (attempt > 0) {
        await new Promise((resolve) => setTimeout(resolve, 400 * attempt));
      }
      try {
        qr = await fetchPairingQrImage();
        countdown = secondsUntil(qr.expiresAt);
        error = null;
        if (!silent) qrLoading = false;
        return;
      } catch (err) {
        lastError = err instanceof Error ? err.message : String(err);
      }
    }
    if (!silent && !qr) {
      error = lastError;
    }
    if (!silent) {
      qrLoading = false;
    }
    if (qr == null && lastError) {
      throw new Error(lastError);
    }
  }

  async function pollStatus() {
    try {
      const status = await fetchPairingStatus();
      devices = status.pairedDevices;
      const fresh = status.pairedDevices.find(
        (device) => !knownPairingIds.includes(device.pairingId),
      );
      if (fresh) {
        connectedDevice = fresh;
        knownPairingIds = status.pairedDevices.map((device) => device.pairingId);
        void workshops.load();
        onPaired?.(fresh);
        if (settingsMode && sheetOpen) {
          closePairingSheet();
        }
      }
    } catch {
      // Best-effort polling.
    }
  }

  async function copyInviteLink(full = false) {
    try {
      const invite = full ? await fetchPairingQr({ full: true }) : qr;
      const url = invite?.url;
      if (!url) return;
      await navigator.clipboard.writeText(url);
      copyFlash = true;
      copyHint = full
        ? "Full invite copied (off-LAN paste)."
        : "Invite copied — same Wi‑Fi scan or open.";
      setTimeout(() => {
        copyFlash = false;
      }, 1500);
    } catch (err) {
      error = err instanceof Error ? err.message : "Could not copy invite link.";
    }
  }

  async function forgetDevice(pairingId: string) {
    try {
      await revokePairingDevice(pairingId);
      devices = devices.filter((device) => device.pairingId !== pairingId);
      knownPairingIds = knownPairingIds.filter((id) => id !== pairingId);
      if (connectedDevice?.pairingId === pairingId) {
        connectedDevice = null;
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function onSheetKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closePairingSheet();
    }
  }
</script>

{#snippet qrBody()}
  <div class="flex flex-col items-center">
    {#if qr?.dataUrl}
      <div class="rounded-2xl border border-surface-500/40 bg-white p-3 shadow-lg">
        <img
          src={qr.dataUrl}
          alt="QR code for phone pairing"
          class="h-44 w-44 object-contain"
          width="176"
          height="176"
        />
      </div>
    {:else if qrLoading || refreshing}
      <div
        class="flex h-44 w-44 flex-col items-center justify-center gap-2 rounded-2xl border border-dashed border-surface-500/45 bg-surface-950/50"
      >
        <LoaderCircle class="h-8 w-8 animate-spin text-surface-400" aria-hidden="true" />
        <span class="text-xs text-surface-400">Generating QR…</span>
      </div>
    {:else}
      <div
        class="flex h-44 w-44 items-center justify-center rounded-2xl border border-dashed border-surface-500/45 bg-surface-950/50"
      >
        <Smartphone class="h-10 w-10 text-surface-500" aria-hidden="true" />
      </div>
    {/if}

    {#if qr?.shortCode}
      <p class="mt-5 font-mono text-2xl tracking-[0.2em] text-surface-50">
        {formatShortCode(qr.shortCode)}
      </p>
      <p class="workshop-faint mt-1 text-xs">Short code fallback if the camera can't scan</p>
    {/if}

    {#if countdown > 0}
      <p class="mt-4 text-sm text-surface-300">
        Refreshes in <span class="font-mono text-primary-200">{formatCountdown(countdown)}</span>
      </p>
    {/if}

    <p class="mt-4 max-w-sm text-center text-sm leading-relaxed text-surface-400">
      Open Medousa on your phone on the same Wi‑Fi and scan this code — or enter the short code
      manually. Off-LAN? Use Full link and paste in the app.
    </p>
    {#if qr}
      <div class="mt-4 flex flex-wrap items-center justify-center gap-2">
        <button type="button" class="btn btn-sm btn-primary" onclick={() => void copyInviteLink(false)}>
          <Copy class="mr-1 inline h-3.5 w-3.5" aria-hidden="true" />
          {copyFlash ? "Copied" : "Copy link"}
        </button>
        <button type="button" class="btn btn-sm btn-ghost" onclick={() => void copyInviteLink(true)}>
          Full link
        </button>
        <button
          type="button"
          class="btn btn-sm btn-ghost"
          disabled={refreshing || qrLoading}
          onclick={() => void refreshQr()}
        >
          <RefreshCw class="mr-1 inline h-3.5 w-3.5" aria-hidden="true" />
          Refresh
        </button>
      </div>
      {#if copyHint}
        <p class="workshop-faint mt-2 text-center text-xs">{copyHint}</p>
      {/if}
    {/if}
  </div>
{/snippet}

<div class="phone-pair-panel">
  {#if loading}
    <div class="flex items-center gap-2 text-sm text-surface-400">
      <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
      Preparing phone pairing…
    </div>
  {:else if !coreOnline}
    <div class="rounded-xl border border-warning-500/35 bg-warning-500/10 px-4 py-4 text-sm text-warning-100">
      Medousa isn't running. Finish setup or open Settings → Workshop before pairing your phone.
    </div>
  {:else if connectedDevice && !settingsMode}
    <div
      class="flex flex-col items-center rounded-xl border border-success-500/35 bg-success-500/10 px-6 py-8 text-center"
    >
      <CheckCircle2 class="h-10 w-10 text-success-300" aria-hidden="true" />
      <p class="mt-4 text-lg font-semibold text-surface-50">{connectedDevice.phoneName} connected</p>
      <p class="mt-2 text-sm text-surface-300">
        Your phone can reach this brain on your home network.
      </p>
      <p class="workshop-faint mt-2 font-mono text-xs">{connectedDevice.phoneId}</p>
    </div>
  {:else if showInlineQr}
    {@render qrBody()}
    {#if error && !loading && !refreshing}
      <p class="mt-4 text-sm text-warning-200">{error}</p>
    {/if}
    <div class="mt-5">
      <button
        type="button"
        class="workshop-text-action inline-flex items-center gap-2 text-sm"
        disabled={refreshing}
        onclick={() => void refreshAll()}
      >
        <RefreshCw class="h-3.5 w-3.5 {refreshing ? 'animate-spin' : ''}" aria-hidden="true" />
        Refresh QR
      </button>
      <button
        type="button"
        class="workshop-text-action ml-4 text-sm"
        disabled={refreshing || !coreOnline}
        onclick={() => void rotateInvite()}
      >
        Rotate invite
      </button>
      <button
        type="button"
        class="workshop-text-action ml-4 text-sm"
        onclick={() => (showDiagnostics = !showDiagnostics)}
      >
        {showDiagnostics ? "Hide" : "Network"} troubleshooting
      </button>
    </div>
    {#if showDiagnostics}
      <div
        class="mt-4 rounded-xl border border-surface-500/35 bg-surface-950/60 px-4 py-4 text-sm leading-relaxed text-surface-300"
      >
        <ul class="list-disc space-y-2 pl-5 text-xs">
          <li>Phone and computer must be on the same Wi‑Fi (guest networks often block LAN discovery).</li>
          <li>
            For automatic discovery, turn on Always reachable on Wi‑Fi under Settings → Sharing.
          </li>
          <li>QR pairing works even when Bonjour is blocked — scan or use the short code.</li>
          <li>
            Firewall: allow incoming connections for Medousa on the computer running the app.
          </li>
        </ul>
      </div>
    {/if}
  {:else}
    <div class="pair-stack">
      <button
        type="button"
        class="pair-tile pair-tile-action"
        disabled={refreshing || !coreOnline}
        onclick={() => void openPairingSheet()}
      >
        <span class="pair-tile-copy">
          <span class="pair-tile-title">Show pairing QR</span>
          <span class="pair-tile-meta">Same Wi‑Fi — generated when you ask</span>
        </span>
        <span class="pair-tile-cta">
          {refreshing ? "…" : "Open"}
        </span>
      </button>

      {#if devices.length > 0}
        {#each devices as device (device.pairingId)}
          <div class="pair-tile">
            <span class="pair-tile-copy">
              <span class="pair-tile-title">{device.phoneName}</span>
              <span class="pair-tile-meta">
                {#if device.profileId}
                  Seat {device.profileId}
                {:else}
                  Paired phone
                {/if}
              </span>
            </span>
            <button
              type="button"
              class="pair-tile-cta pair-tile-cta-danger"
              aria-label="Forget {device.phoneName}"
              onclick={() => void forgetDevice(device.pairingId)}
            >
              Forget
            </button>
          </div>
        {/each}
      {:else}
        <p class="pair-empty">No phones paired yet.</p>
      {/if}

      {#if sharedMode.isShared}
        <details class="pair-more">
          <summary class="pair-more-summary">
            <span>Invite a seat</span>
            <ChevronDown size={14} strokeWidth={2} class="pair-more-chevron" aria-hidden="true" />
          </summary>
          <div class="pair-more-body">
            <p class="pair-footnote">
              Bind the next scan to a member profile.
            </p>
            <div class="mt-3 flex flex-wrap items-center gap-2">
              <select
                class="input min-w-[12rem] flex-1 text-sm"
                bind:value={inviteProfileId}
                aria-label="Seat profile for invite"
              >
                {#each userProfiles.profiles as profile (profile.profile_id)}
                  <option value={profile.profile_id}>
                    {profile.display_name}
                    {#if profile.profile_id === sharedMode.rootProfileId}
                      (root)
                    {/if}
                  </option>
                {/each}
              </select>
              <button
                type="button"
                class="btn btn-sm variant-soft"
                disabled={refreshing || !coreOnline || !inviteProfileId}
                onclick={() => void inviteSeat()}
              >
                Mint QR
              </button>
            </div>
          </div>
        </details>
      {/if}

      <details class="pair-more" bind:open={showDiagnostics}>
        <summary class="pair-more-summary">
          <span>Pairing help</span>
          <ChevronDown size={14} strokeWidth={2} class="pair-more-chevron" aria-hidden="true" />
        </summary>
        <div class="pair-more-body">
          {#if bonjour?.pairingAvailable}
            <p class="pair-footnote">
              {bonjour.likelyAdvertising ? "Discovery is advertising on this network." : "QR pairing is ready."}
              {#if bonjour.message}
                <span class="block mt-1">{bonjour.message}</span>
              {/if}
            </p>
          {/if}
          <ul class="pair-help-list">
            <li>Phone and computer must share the same Wi‑Fi.</li>
            <li>Guest networks often block LAN discovery — use the QR or short code.</li>
            <li>For automatic discovery, turn on Always reachable on Wi‑Fi in Sharing.</li>
          </ul>
        </div>
      </details>

      {#if error && !loading && !refreshing && !sheetOpen}
        <p class="text-sm text-warning-200">{error}</p>
      {/if}
    </div>
  {/if}
</div>

{#if settingsMode && sheetOpen}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="pair-sheet-backdrop"
    role="presentation"
    onclick={closePairingSheet}
    onkeydown={onSheetKeydown}
  >
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div
      class="pair-sheet"
      role="dialog"
      aria-modal="true"
      aria-label={sheetTitle}
      onclick={(event) => event.stopPropagation()}
    >
      <header class="pair-sheet-header">
        <div class="min-w-0">
          <h3 class="pair-sheet-title">{sheetTitle}</h3>
          <p class="pair-sheet-meta">Generated now — expires and refreshes while this is open.</p>
        </div>
        <button
          type="button"
          class="pair-sheet-close"
          aria-label="Close"
          onclick={closePairingSheet}
        >
          <X size={18} />
        </button>
      </header>
      <div class="pair-sheet-body">
        {@render qrBody()}
        {#if error}
          <p class="mt-4 text-center text-sm text-warning-200">{error}</p>
        {/if}
        <div class="mt-5 flex flex-wrap items-center justify-center gap-2">
          <button
            type="button"
            class="btn btn-sm variant-soft"
            disabled={refreshing || !coreOnline}
            onclick={() => void rotateInvite()}
          >
            Rotate invite
          </button>
          <button type="button" class="btn btn-sm variant-ghost" onclick={closePairingSheet}>
            Done
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .pair-stack {
    display: grid;
    gap: 0.5rem;
  }

  .pair-tile {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    width: 100%;
    min-height: 3.25rem;
    padding: 0.55rem 0.75rem;
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    background: rgb(var(--color-surface-900) / 0.28);
  }

  .pair-tile-action {
    color: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      border-color 120ms ease,
      background 120ms ease;
  }

  .pair-tile-action:hover:not(:disabled) {
    border-color: rgb(var(--color-surface-500) / 0.48);
    background: rgb(var(--color-surface-800) / 0.28);
  }

  .pair-tile-action:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .pair-tile-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.08rem;
  }

  .pair-tile-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .pair-tile-meta {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--color-surface-500));
  }

  .pair-tile-cta {
    flex-shrink: 0;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--color-surface-400));
    cursor: pointer;
  }

  .pair-tile-cta-danger {
    color: rgb(var(--color-error-300) / 0.9);
  }

  .pair-empty {
    margin: 0;
    padding: 0.15rem 0.15rem 0;
    font-size: 0.72rem;
    color: rgb(var(--color-surface-500));
  }

  .pair-more {
    border-radius: 0.65rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.32);
    background: rgb(var(--color-surface-900) / 0.28);
  }

  .pair-more-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    min-height: 3.25rem;
    padding: 0.55rem 0.75rem;
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--color-surface-300));
    cursor: pointer;
    list-style: none;
  }

  .pair-more-summary::-webkit-details-marker {
    display: none;
  }

  :global(.pair-more-chevron) {
    transition: transform 160ms ease;
  }

  .pair-more[open] :global(.pair-more-chevron) {
    transform: rotate(180deg);
  }

  .pair-more-body {
    padding: 0 0.75rem 0.75rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.22);
  }

  .pair-footnote {
    margin: 0.55rem 0 0;
    font-size: 0.7rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-500));
  }

  .pair-help-list {
    margin: 0.55rem 0 0;
    padding-left: 1.1rem;
    font-size: 0.7rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .pair-sheet-backdrop {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1.25rem;
    background: rgb(0 0 0 / 0.55);
  }

  .pair-sheet {
    display: flex;
    width: min(28rem, 100%);
    max-height: min(86vh, 40rem);
    flex-direction: column;
    overflow: hidden;
    border-radius: 0.85rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.4);
    background: rgb(var(--color-surface-900));
    box-shadow: 0 18px 48px rgb(0 0 0 / 0.45);
  }

  .pair-sheet-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.9rem 1rem;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.28);
  }

  .pair-sheet-title {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 600;
    color: rgb(var(--color-surface-50));
  }

  .pair-sheet-meta {
    margin: 0.2rem 0 0;
    font-size: 0.72rem;
    color: rgb(var(--color-surface-500));
  }

  .pair-sheet-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2rem;
    height: 2rem;
    border: 0;
    border-radius: 0.45rem;
    background: transparent;
    color: rgb(var(--color-surface-300));
    cursor: pointer;
  }

  .pair-sheet-close:hover {
    background: rgb(var(--color-surface-800) / 0.7);
  }

  .pair-sheet-body {
    min-height: 0;
    flex: 1 1 auto;
    overflow-y: auto;
    padding: 1rem 1rem 1.15rem;
  }
</style>
