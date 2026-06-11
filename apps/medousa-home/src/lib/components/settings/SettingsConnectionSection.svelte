<script lang="ts">
  import { ChevronRight, Wifi, WifiOff } from "@lucide/svelte";
  import { getDaemonUrl, setDaemonUrl, type DaemonHealth } from "$lib/daemon";
  import { reconnectWorkshop } from "$lib/workshopConnection";
  import { settings } from "$lib/stores/settings.svelte";

  interface Props {
    health: DaemonHealth | null;
    onDaemonHealth: () => void | Promise<void>;
    mobile?: boolean;
  }

  let { health, onDaemonHealth, mobile = false }: Props = $props();

  let editing = $state(false);

  const connected = $derived(Boolean(health?.ok));
  const connectionLabel = $derived(connectionHumanLabel(settings.daemonUrl));
  const backendLabel = $derived(health?.backend ?? "unknown backend");

  function connectionHumanLabel(url: string): string {
    const trimmed = url.trim();
    if (!trimmed) return "Not configured";
    try {
      const parsed = new URL(trimmed);
      const host = parsed.hostname;
      if (host === "127.0.0.1" || host === "localhost") {
        return mobile ? "Mac workshop (local)" : "Local workshop";
      }
      return `Remote · ${host}`;
    } catch {
      return trimmed;
    }
  }

  $effect(() => {
    if (!settings.daemonUrl) {
      void loadDaemonUrl();
    }
  });

  async function loadDaemonUrl() {
    try {
      settings.daemonUrl = await getDaemonUrl();
    } catch (err) {
      settings.daemonMessage = err instanceof Error ? err.message : String(err);
    }
  }

  async function saveDaemonUrl() {
    settings.savingDaemon = true;
    settings.daemonMessage = null;
    try {
      await setDaemonUrl(settings.daemonUrl);
      const probe = await reconnectWorkshop(onDaemonHealth);
      settings.daemonMessage = probe.ok ? "Connected" : probe.message;
      if (probe.ok) {
        editing = false;
      }
    } catch (err) {
      settings.daemonMessage = err instanceof Error ? err.message : String(err);
    } finally {
      settings.savingDaemon = false;
    }
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Connection</h2>
    <p class="workshop-faint mt-1 text-sm">
      {#if mobile}
        Your phone’s link to the Mac workshop. Model and provider are configured on the Mac under
        Runtime → Controls.
      {:else}
        How this app reaches your running Medousa workshop.
      {/if}
    </p>
  </header>

  <div class="settings-connection-card mt-5">
    <div class="flex items-start gap-3">
      <span
        class="settings-connection-icon {connected
          ? 'settings-connection-icon-ok'
          : 'settings-connection-icon-off'}"
        aria-hidden="true"
      >
        {#if connected}
          <Wifi size={18} strokeWidth={2} />
        {:else}
          <WifiOff size={18} strokeWidth={2} />
        {/if}
      </span>
      <div class="min-w-0 flex-1">
        <p class="text-sm font-semibold text-surface-50">
          {connected ? "Connected" : "Offline"}
        </p>
        <p class="mt-0.5 text-sm text-surface-200">{connectionLabel}</p>
        <p class="workshop-faint mt-1 text-xs">{backendLabel}</p>
        {#if settings.daemonMessage && !editing}
          <p
            class="mt-2 text-xs {settings.daemonMessage === 'Connected' ||
            settings.daemonMessage.toLowerCase().includes('connected')
              ? 'text-success-400'
              : 'text-warning-400'}"
          >
            {settings.daemonMessage}
          </p>
        {/if}
      </div>
    </div>

    {#if !editing}
      <button
        type="button"
        class="btn btn-sm variant-soft-surface mt-4"
        onclick={() => {
          editing = true;
          settings.daemonMessage = null;
        }}
      >
        Change connection…
      </button>
    {:else}
      <div class="mt-4 space-y-3 border-t border-surface-500/35 pt-4">
        <label class="block" for="daemon-url">
          <span class="workshop-label">Workshop address</span>
          <input
            id="daemon-url"
            class="input mt-1 w-full"
            bind:value={settings.daemonUrl}
            placeholder={mobile ? "http://192.168.1.42:7419" : "http://127.0.0.1:7419"}
          />
        </label>
        <div class="flex flex-wrap items-center gap-2">
          <button
            type="button"
            class="btn btn-sm variant-filled-primary"
            disabled={settings.savingDaemon || !settings.daemonUrl.trim()}
            onclick={() => void saveDaemonUrl()}
          >
            {settings.savingDaemon ? "Saving…" : "Save & test"}
          </button>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            disabled={settings.savingDaemon}
            onclick={() => {
              editing = false;
              settings.daemonMessage = null;
            }}
          >
            Cancel
          </button>
          {#if settings.daemonMessage}
            <p
              class="text-xs {settings.daemonMessage === 'Connected' ||
              settings.daemonMessage.toLowerCase().includes('connected')
                ? 'text-success-400'
                : 'text-warning-400'}"
            >
              {settings.daemonMessage}
            </p>
          {/if}
        </div>
      </div>
    {/if}
  </div>
</section>
