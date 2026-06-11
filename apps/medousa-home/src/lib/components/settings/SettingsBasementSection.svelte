<script lang="ts">
  import { Wifi, WifiOff } from "@lucide/svelte";
  import {
    getMedousaConfigPaths,
    openConfigPath,
    type MedousaConfigPaths,
  } from "$lib/config";
  import { getDaemonUrl, setDaemonUrl, type DaemonHealth } from "$lib/daemon";
  import { reconnectWorkshop } from "$lib/workshopConnection";
  import { vault } from "$lib/stores/vault.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { resetGarageOnboarding } from "$lib/utils/garageOnboarding";
  import { isTauri } from "$lib/window";

  const isDevBuild = import.meta.env.DEV;

  interface Props {
    revision: number;
    health: DaemonHealth | null;
    onDaemonHealth: () => void | Promise<void>;
    mobile?: boolean;
  }

  let { revision, health, onDaemonHealth, mobile = false }: Props = $props();

  let configPaths = $state<MedousaConfigPaths | null>(null);
  let connectionEditing = $state(false);

  const connected = $derived(Boolean(health?.ok));
  const connectionLabel = $derived(connectionHumanLabel(settings.daemonUrl));
  const backendLabel = $derived(health?.backend ?? "unknown backend");

  const workshopFiles = $derived(
    configPaths
      ? [
          {
            id: "product",
            label: "product_config.json",
            hint: "Product policy — channels live in Messaging",
            path: configPaths.productConfig,
          },
          {
            id: "workspace",
            label: "tui_defaults.json",
            hint: "Full charter — Memory & Voice edit the human fields above",
            path: configPaths.tuiDefaults,
          },
          {
            id: "capabilities",
            label: "capabilities.toml",
            hint: "Tool bindings — catalog in Skills → Tools",
            path: configPaths.capabilities,
          },
          {
            id: "gateway",
            label: "mcp-gateway.toml",
            hint: "Connected MCP servers",
            path: configPaths.mcpGateway,
          },
        ]
      : [],
  );

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

  $effect(() => {
    if (isTauri() && !mobile && !configPaths) {
      void loadConfigPaths();
    }
  });

  async function loadDaemonUrl() {
    try {
      settings.daemonUrl = await getDaemonUrl();
    } catch (err) {
      settings.daemonMessage = err instanceof Error ? err.message : String(err);
    }
  }

  async function loadConfigPaths() {
    try {
      configPaths = await getMedousaConfigPaths();
    } catch {
      configPaths = null;
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
        connectionEditing = false;
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
    <h2 class="text-base font-semibold text-surface-50">Basement</h2>
    <p class="workshop-faint mt-1 text-sm">
      Workshop address, on-disk files, and diagnostics — for when the charter isn’t enough.
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
        <p class="workshop-faint mt-2 text-xs">
          Connection status also lives in the status bar — change the address here only when you
          need to.
        </p>
        {#if settings.daemonMessage && !connectionEditing}
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

    {#if !connectionEditing}
      <button
        type="button"
        class="btn btn-sm variant-soft-surface mt-4"
        onclick={() => {
          connectionEditing = true;
          settings.daemonMessage = null;
        }}
      >
        Change workshop address…
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
              connectionEditing = false;
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

  {#if isDevBuild && !mobile}
    <div class="settings-toggle-list mt-6">
      <label class="settings-toggle-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Developer vault notes</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            Show bugs/ and system paths in Library
          </span>
        </span>
        <input
          type="checkbox"
          class="checkbox shrink-0"
          checked={vault.showSystemNotes}
          onchange={(event) =>
            vault.setShowSystemNotes((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>
    </div>
    <button
      type="button"
      class="workshop-text-action mt-3 text-sm"
      onclick={() => {
        resetGarageOnboarding();
        vault.openGarageWizard();
      }}
    >
      Reset garage onboarding wizard
    </button>
  {/if}

  {#if workshopFiles.length > 0 && !mobile}
    <div class="mt-6">
      <h3 class="workshop-label">Workshop files</h3>
      <ul class="mt-2 divide-y divide-surface-500/35 rounded-container-token border border-surface-500/35">
        {#each workshopFiles as file (file.id)}
          <li class="flex items-start justify-between gap-3 px-3 py-2.5">
            <div class="min-w-0">
              <p class="font-mono text-[11px] text-surface-200">{file.label}</p>
              <p class="workshop-faint text-xs">{file.hint}</p>
            </div>
            <button
              type="button"
              class="workshop-text-action shrink-0 text-xs"
              onclick={() => openConfigPath(file.path)}
            >
              Open
            </button>
          </li>
        {/each}
      </ul>
      <p class="workshop-faint mt-2 text-xs">
        Terminal view of all defaults: Runtime → Workshop tab.
      </p>
    </div>
  {/if}

  <div class="mt-6">
    <button
      type="button"
      class="flex w-full items-center justify-between text-left"
      onclick={() => (settings.diagnosticsOpen = !settings.diagnosticsOpen)}
    >
      <div>
        <h3 class="text-sm font-semibold text-surface-100">Diagnostics</h3>
        <p class="workshop-faint mt-0.5 text-xs">Connection detail for support</p>
      </div>
      <span class="workshop-faint shrink-0">
        {settings.diagnosticsOpen ? "▾" : "▸"}
      </span>
    </button>
    {#if settings.diagnosticsOpen}
      <dl class="mt-4 space-y-2 rounded-container-token border border-surface-500/35 bg-surface-900/40 p-3 text-xs">
        <div class="grid grid-cols-[7rem_1fr] gap-2">
          <dt class="workshop-label">Status</dt>
          <dd class="font-mono text-surface-300">{health?.ok ? "connected" : "offline"}</dd>
        </div>
        <div class="grid grid-cols-[7rem_1fr] gap-2">
          <dt class="workshop-label">Base URL</dt>
          <dd class="break-all font-mono text-surface-300">{settings.daemonUrl || "—"}</dd>
        </div>
        <div class="grid grid-cols-[7rem_1fr] gap-2">
          <dt class="workshop-label">Backend</dt>
          <dd class="font-mono text-surface-300">{health?.backend ?? "—"}</dd>
        </div>
        <div class="grid grid-cols-[7rem_1fr] gap-2">
          <dt class="workshop-label">Revision</dt>
          <dd class="font-mono text-surface-300">{revision}</dd>
        </div>
        <div class="grid grid-cols-[7rem_1fr] gap-2">
          <dt class="workshop-label">Worker</dt>
          <dd class="font-mono text-surface-300">{health?.worker_id ?? "—"}</dd>
        </div>
        <div class="grid grid-cols-[7rem_1fr] gap-2">
          <dt class="workshop-label">Tools</dt>
          <dd class="font-mono text-surface-300">{health?.tool_registry_count ?? "—"}</dd>
        </div>
        {#if health && !health.ok}
          <div class="grid grid-cols-[7rem_1fr] gap-2">
            <dt class="workshop-label">Detail</dt>
            <dd class="break-all font-mono text-warning-400">{health.message}</dd>
          </div>
        {/if}
      </dl>
    {/if}
  </div>
</section>
