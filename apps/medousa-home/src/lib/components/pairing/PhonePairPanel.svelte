<script lang="ts">
  import { onMount } from "svelte";
  import {
    CheckCircle2,
    LoaderCircle,
    Radio,
    RefreshCw,
    Smartphone,
    Trash2,
    Wifi,
  } from "@lucide/svelte";
  import {
    fetchBonjourStatus,
    fetchPairingQrImage,
    fetchPairingStatus,
    formatCountdown,
    formatShortCode,
    revokePairingDevice,
    secondsUntil,
    type BonjourStatus,
    type PairedDeviceSummary,
    type PairingQrImage,
  } from "$lib/utils/pairingApi";
  import { checkDaemonHealth } from "$lib/daemon";
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

  let countdownTimer: ReturnType<typeof setInterval> | null = null;
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let qrRefreshTimer: ReturnType<typeof setInterval> | null = null;

  onMount(() => {
    void bootstrap();
    return () => cleanupTimers();
  });

  function cleanupTimers() {
    if (countdownTimer) clearInterval(countdownTimer);
    if (pollTimer) clearInterval(pollTimer);
    if (qrRefreshTimer) clearInterval(qrRefreshTimer);
    countdownTimer = null;
    pollTimer = null;
    qrRefreshTimer = null;
  }

  async function bootstrap() {
    loading = true;
    error = null;
    try {
      if (!isTauri()) {
        error = "Phone pairing runs in the Medousa desktop app with the engine online.";
        return;
      }
      const health = await checkDaemonHealth();
      coreOnline = health.ok;
      if (!health.ok) {
        error = health.message;
        return;
      }
      await refreshAll();
      startTimers();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
      qrLoading = false;
    }
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
    pollTimer = setInterval(() => {
      void pollStatus();
    }, 3000);
    qrRefreshTimer = setInterval(() => {
      void refreshQr({ silent: true });
    }, 25_000);
  }

  async function refreshAll() {
    refreshing = true;
    try {
      const status = await fetchPairingStatus();
      devices = status.pairedDevices;
      knownPairingIds = status.pairedDevices.map((device) => device.pairingId);
      bonjour = await fetchBonjourStatus();
      await refreshQr({ retries: 5 });
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
        onPaired?.(fresh);
      }
    } catch {
      // Best-effort polling — keep last QR visible.
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
</script>

<div class="phone-pair-panel">
  {#if loading}
    <div class="flex items-center gap-2 text-sm text-surface-400">
      <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
      Loading pairing…
    </div>
  {:else if !coreOnline}
    <div class="rounded-xl border border-warning-500/35 bg-warning-500/10 px-4 py-4 text-sm text-warning-100">
      The engine is offline. Finish setup or start it from Settings → Connection before pairing
      your phone.
    </div>
  {:else if connectedDevice}
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
  {:else}
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
      {:else if qrLoading}
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
        manually.
      </p>
    </div>
  {/if}

  {#if error}
    <p class="mt-4 text-sm text-warning-200">{error}</p>
  {/if}

  {#if bonjour}
    <div
      class="mt-5 flex items-start gap-3 rounded-xl border border-surface-500/35 bg-surface-950/50 px-4 py-3 text-sm"
    >
      {#if bonjour.likelyAdvertising}
        <Radio class="mt-0.5 h-4 w-4 shrink-0 text-success-300" aria-hidden="true" />
      {:else}
        <Wifi class="mt-0.5 h-4 w-4 shrink-0 text-surface-400" aria-hidden="true" />
      {/if}
      <div class="min-w-0">
        <p class="font-medium text-surface-100">
          {bonjour.likelyAdvertising ? "Bonjour advertising" : "QR pairing ready"}
        </p>
        <p class="workshop-faint mt-1 text-xs leading-relaxed">{bonjour.message}</p>
        {#if bonjour.deviceId}
          <p class="workshop-faint mt-2 font-mono text-[11px]">
            Engine ID {bonjour.deviceId}
            {#if bonjour.peerName}
              · {bonjour.peerName}
            {/if}
          </p>
        {/if}
      </div>
    </div>
  {/if}

  {#if mode === "settings" && devices.length > 0}
    <div class="mt-6">
      <h3 class="text-sm font-semibold text-surface-100">Paired phones</h3>
      <ul class="mt-3 space-y-2">
        {#each devices as device (device.pairingId)}
          <li
            class="flex items-center justify-between gap-3 rounded-lg border border-surface-500/35 bg-surface-950/40 px-3 py-3"
          >
            <div class="min-w-0">
              <p class="truncate text-sm font-medium text-surface-50">{device.phoneName}</p>
              <p class="workshop-faint truncate font-mono text-[11px]">{device.phoneId}</p>
            </div>
            <button
              type="button"
              class="btn btn-sm variant-ghost text-error-200"
              aria-label="Forget {device.phoneName}"
              onclick={() => void forgetDevice(device.pairingId)}
            >
              <Trash2 class="h-4 w-4" aria-hidden="true" />
              Forget
            </button>
          </li>
        {/each}
      </ul>
    </div>
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
          For automatic discovery, start the engine with
          <span class="font-mono text-surface-200">medousa start daemon --public</span>.
        </li>
        <li>QR pairing works even when Bonjour is blocked — scan or use the short code.</li>
        <li>
          Firewall: allow incoming connections for
          <span class="font-mono">medousa_daemon</span> on the computer running Medousa.
        </li>
        <li>
          Test from terminal:
          <span class="font-mono text-surface-200">curl http://127.0.0.1:7419/qr</span>
        </li>
      </ul>
    </div>
  {/if}
</div>
