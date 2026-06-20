<script lang="ts">
  import { onMount } from "svelte";
  import { Wifi, WifiOff } from "@lucide/svelte";
  import {
    getMedousaConfigPaths,
    openConfigPath,
    openConnectionRunbook,
    type MedousaConfigPaths,
  } from "$lib/config";
  import { getDaemonUrl, setDaemonUrl, type DaemonHealth } from "$lib/daemon";
  import {
    loadConnectionPrefs,
    setAutostart,
    setPublicBind,
    type ConnectionPrefsSummary,
  } from "$lib/connection";
  import { reconnectWorkshop } from "$lib/workshopConnection";
  import { restartEngine, waitForEngine } from "$lib/utils/providersApi";
  import { vault } from "$lib/stores/vault.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { resetGarageOnboarding } from "$lib/utils/garageOnboarding";
  import { wizard } from "$lib/stores/wizard.svelte";
  import SettingsLocalBrainPanel from "$lib/components/settings/SettingsLocalBrainPanel.svelte";
  import SettingsWorkshopsSection from "$lib/components/settings/SettingsWorkshopsSection.svelte";
  import { isTauri } from "$lib/window";
  import {
    workshopBasementConnectionLabel,
    workshopBasementRestartHint,
  } from "$lib/platformCopy";

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
  let connectionPrefs = $state<ConnectionPrefsSummary | null>(null);
  let prefsBusy = $state(false);
  let prefsMessage = $state<string | null>(null);
  let restartingEngine = $state(false);
  let restartMessage = $state<string | null>(null);
  let runbookError = $state<string | null>(null);

  const connected = $derived(Boolean(health?.ok));
  const connectionLabel = $derived(connectionHumanLabel(settings.daemonUrl));
  const backendLabel = $derived(health?.backend ?? "unknown backend");
  const lastTurnLabel = $derived(formatLastTurn(health?.last_agent_turn_at_utc));
  const toolsReadyLabel = $derived(
    health?.tool_registry_count != null ? String(health.tool_registry_count) : "—",
  );
  const engineVersionLabel = $derived(health?.agent_runtime_version ?? "—");

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
            hint: "Full charter — Models & Voice edit the human fields above",
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
        return workshopBasementConnectionLabel(mobile);
      }
      return `Remote · ${host}`;
    } catch {
      return trimmed;
    }
  }

  function formatLastTurn(iso: string | null | undefined): string {
    if (!iso) return "No turns yet";
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return iso;
    const diffMs = Date.now() - date.getTime();
    if (diffMs < 60_000) return "Just now";
    if (diffMs < 3_600_000) {
      const minutes = Math.floor(diffMs / 60_000);
      return `${minutes}m ago`;
    }
    if (diffMs < 86_400_000) {
      const hours = Math.floor(diffMs / 3_600_000);
      return `${hours}h ago`;
    }
    return date.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    });
  }

  async function restartWorkshopEngine() {
    if (!isTauri() || mobile) return;
    restartingEngine = true;
    restartMessage = null;
    try {
      const result = await restartEngine();
      restartMessage = result.message;
      const wait = await waitForEngine(30);
      if (!wait.ok) {
        restartMessage = wait.message;
      } else {
        restartMessage = "Engine restarted.";
      }
      await reconnectWorkshop(onDaemonHealth);
    } catch (err) {
      restartMessage = err instanceof Error ? err.message : String(err);
    } finally {
      restartingEngine = false;
    }
  }

  async function openRunbook() {
    runbookError = null;
    try {
      await openConnectionRunbook();
    } catch (err) {
      runbookError = err instanceof Error ? err.message : String(err);
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

  onMount(() => {
    if (isTauri() && !mobile) {
      void loadConnectionPrefsState();
    }
  });

  async function loadConnectionPrefsState() {
    try {
      connectionPrefs = await loadConnectionPrefs();
    } catch {
      connectionPrefs = null;
    }
  }

  async function togglePublicBind(enabled: boolean) {
    if (!isTauri()) return;
    prefsBusy = true;
    prefsMessage = null;
    try {
      const result = await setPublicBind(enabled);
      prefsMessage = result.message;
      await loadConnectionPrefsState();
      await reconnectWorkshop(onDaemonHealth);
    } catch (err) {
      prefsMessage = err instanceof Error ? err.message : String(err);
    } finally {
      prefsBusy = false;
    }
  }

  async function toggleAutostart(enabled: boolean) {
    if (!isTauri()) return;
    prefsBusy = true;
    prefsMessage = null;
    try {
      await setAutostart(enabled);
      await loadConnectionPrefsState();
      prefsMessage = enabled
        ? "Medousa will start when you log in."
        : "Auto-start turned off.";
    } catch (err) {
      prefsMessage = err instanceof Error ? err.message : String(err);
      await loadConnectionPrefsState();
    } finally {
      prefsBusy = false;
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
    <h2 class="text-base font-semibold text-surface-50">Connection</h2>
    <p class="workshop-faint mt-1 text-sm">
      Medousa on this device, phone pairing, and advanced files.
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

  {#if isTauri() && !mobile}
    <div class="settings-connection-card mt-6">
      <div class="flex items-start justify-between gap-3">
        <div>
          <h3 class="text-sm font-semibold text-surface-50">Workshop health</h3>
          <p class="workshop-faint mt-0.5 text-xs">
            Medousa engine on this device — version and recent activity.
          </p>
        </div>
        <span
          class="shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium {connected
            ? 'bg-success-500/15 text-success-400'
            : 'bg-warning-500/15 text-warning-400'}"
        >
          {connected ? "Running" : "Offline"}
        </span>
      </div>

      <dl class="mt-4 space-y-2 text-xs">
        <div class="flex items-baseline justify-between gap-4">
          <dt class="workshop-label">Engine</dt>
          <dd class="font-mono text-surface-300">{engineVersionLabel}</dd>
        </div>
        <div class="flex items-baseline justify-between gap-4">
          <dt class="workshop-label">Last activity</dt>
          <dd class="text-surface-300">{lastTurnLabel}</dd>
        </div>
        <div class="flex items-baseline justify-between gap-4">
          <dt class="workshop-label">Tools ready</dt>
          <dd class="font-mono text-surface-300">{toolsReadyLabel}</dd>
        </div>
        {#if health?.active_profile_display_name}
          <div class="flex items-baseline justify-between gap-4">
            <dt class="workshop-label">Profile</dt>
            <dd class="text-surface-300">{health.active_profile_display_name}</dd>
          </div>
        {/if}
      </dl>

      <button
        type="button"
        class="btn btn-sm variant-soft-surface mt-4"
        disabled={restartingEngine}
        onclick={() => void restartWorkshopEngine()}
      >
        {restartingEngine ? "Restarting…" : "Restart engine"}
      </button>
      {#if restartMessage}
        <p
          class="mt-2 text-xs {restartMessage.toLowerCase().includes('restart') ||
          restartMessage.toLowerCase().includes('ready') ||
          restartMessage.toLowerCase().includes('running')
            ? 'text-success-400'
            : 'text-warning-400'}"
        >
          {restartMessage}
        </p>
      {/if}
    </div>
  {/if}

  {#if isTauri() && !mobile && connectionPrefs}
    <div class="settings-toggle-list mt-6">
      <label class="settings-toggle-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">
            Let phones on your Wi‑Fi connect
          </span>
          <span class="workshop-faint mt-0.5 block text-xs leading-relaxed">
            {workshopBasementRestartHint()}
            without typing an IP address.
          </span>
        </span>
        <input
          type="checkbox"
          class="checkbox shrink-0"
          checked={connectionPrefs.publicBind}
          disabled={prefsBusy}
          onchange={(event) =>
            void togglePublicBind((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>
      {#if connectionPrefs.autostartSupported}
        <label class="settings-toggle-row">
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-medium text-surface-100">Start Medousa when I log in</span>
            <span class="workshop-faint mt-0.5 block text-xs">
              Keeps the engine ready in the background — channels and chat connect faster.
            </span>
          </span>
          <input
            type="checkbox"
            class="checkbox shrink-0"
            checked={connectionPrefs.autostartEnabled}
            disabled={prefsBusy}
            onchange={(event) =>
              void toggleAutostart((event.currentTarget as HTMLInputElement).checked)}
          />
        </label>
      {/if}
    </div>
    {#if prefsMessage}
      <p class="mt-2 text-xs text-surface-300">{prefsMessage}</p>
    {/if}
  {/if}

  {#if isTauri() && !mobile}
    <SettingsLocalBrainPanel />
  {/if}

  <div class="mt-6">
    <h3 class="text-sm font-semibold text-surface-100">First-run setup</h3>
    <p class="workshop-faint mt-1 text-xs">
      Re-open the welcome wizard — model choice and optional phone pairing.
    </p>
    <button
      type="button"
      class="workshop-text-action mt-3 text-sm"
      onclick={() => void wizard.beginRerun()}
    >
      Re-run first-run wizard
    </button>
  </div>

  <div class="settings-toggle-list mt-6">
    <label class="settings-toggle-row">
      <span class="min-w-0 flex-1">
        <span class="block text-sm font-medium text-surface-100">Stamp completion inline</span>
        <span class="workshop-faint mt-0.5 block text-xs">
          When checking a to-do in preview, append (done YYYY-MM-DD) to the line
        </span>
      </span>
      <input
        type="checkbox"
        class="checkbox shrink-0"
        checked={vault.stampCompletionInline}
        onchange={(event) =>
          vault.setStampCompletionInline((event.currentTarget as HTMLInputElement).checked)}
      />
    </label>
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
      <p class="workshop-faint mt-4 text-xs leading-relaxed">
        If chat freezes while status stays green, restart the engine above or open the connection
        guide.
      </p>
      <button
        type="button"
        class="workshop-text-action mt-2 text-xs"
        onclick={() => void openRunbook()}
      >
        Open connection troubleshooting guide →
      </button>
      {#if runbookError}
        <p class="mt-2 text-xs text-warning-400">{runbookError}</p>
      {/if}
    {/if}
  </div>

  <SettingsWorkshopsSection {onDaemonHealth} />
</section>
