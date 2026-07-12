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
  import { fetchPackageStatus, type PackageStatusSummary } from "$lib/utils/packagesApi";
  import SettingsWorkshopsSection from "$lib/components/settings/SettingsWorkshopsSection.svelte";
  import { isTauri } from "$lib/window";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
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
  let packageStatus = $state<PackageStatusSummary | null>(null);
  let advancedOpen = $state(false);
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
      void fetchPackageStatus().then((status) => {
        packageStatus = status;
      });
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
      Pick which workshop you’re in — then how this Mac runs it.
    </p>
  </header>

  <!-- 1. Story lead: your workshops -->
  <SettingsWorkshopsSection {onDaemonHealth} lead />

  <!-- 2. Live link to that workshop -->
  <div class="mt-8">
    <h3 class="settings-subsection-heading">This connection</h3>
    <p class="settings-subsection-lead">
      Live status for the active workshop. Change the address only when something’s wrong.
    </p>

    <div class="settings-connection-card">
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
        {#if !connectionEditing}
          <button
            type="button"
            class="btn btn-sm variant-soft-surface shrink-0"
            onclick={() => {
              connectionEditing = true;
              settings.daemonMessage = null;
            }}
          >
            Address…
          </button>
        {/if}
      </div>

      {#if connectionEditing}
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
  </div>

  <!-- 3. How this Mac behaves -->
  {#if isTauri() && !mobile}
    <div class="mt-8">
      <h3 class="settings-subsection-heading">This Mac</h3>
      <p class="settings-subsection-lead">
        Engine on this device — who can reach it, and whether it starts with login.
      </p>

      {#if connectionPrefs}
        <div class="settings-toggle-list">
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
                  Keeps the engine ready in the background.
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

      <div class="settings-connection-card mt-4">
        <div class="flex items-center justify-between gap-3">
          <div class="min-w-0">
            <p class="text-sm font-medium text-surface-100">Engine</p>
            <p class="workshop-faint mt-0.5 text-xs">
              {engineVersionLabel}
              · {toolsReadyLabel} tools
              · {lastTurnLabel}
              {#if health?.active_profile_display_name}
                · {health.active_profile_display_name}
              {/if}
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
        <button
          type="button"
          class="btn btn-sm variant-soft-surface mt-3"
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
    </div>
  {/if}

  <!-- 4. Optional extras -->
  {#if isTauri() && !mobile}
    <div class="mt-8">
      <h3 class="settings-subsection-heading">Extras</h3>
      <p class="settings-subsection-lead">Packages, private brain, and welcome setup.</p>
      <div class="settings-toggle-list">
        <button
          type="button"
          class="settings-toggle-row w-full text-left"
          onclick={() => settingsNav.openSection("packages")}
        >
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-medium text-surface-100">Packages</span>
            <span class="workshop-faint mt-0.5 block text-xs">
              Offline brain, adapters, CLI & MCP
              {#if packageStatus && !packageStatus.localBrainInstalled}
                · brain not installed
              {/if}
            </span>
          </span>
          <span class="workshop-text-action shrink-0 text-xs">Open…</span>
        </button>
        <button
          type="button"
          class="settings-toggle-row w-full text-left"
          onclick={() => void wizard.beginRerun()}
        >
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-medium text-surface-100">Welcome wizard</span>
            <span class="workshop-faint mt-0.5 block text-xs">
              Re-run model choice and optional phone pairing
            </span>
          </span>
          <span class="workshop-text-action shrink-0 text-xs">Re-run…</span>
        </button>
        <label class="settings-toggle-row">
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-medium text-surface-100">Stamp completion inline</span>
            <span class="workshop-faint mt-0.5 block text-xs">
              Append (done YYYY-MM-DD) when checking a to-do in preview
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
      <div class="mt-4">
        <SettingsLocalBrainPanel />
      </div>
    </div>
  {:else}
    <div class="mt-8">
      <h3 class="settings-subsection-heading">Extras</h3>
      <div class="settings-toggle-list">
        <button
          type="button"
          class="settings-toggle-row w-full text-left"
          onclick={() => void wizard.beginRerun()}
        >
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-medium text-surface-100">Welcome wizard</span>
            <span class="workshop-faint mt-0.5 block text-xs">
              Re-run model choice and optional phone pairing
            </span>
          </span>
          <span class="workshop-text-action shrink-0 text-xs">Re-run…</span>
        </button>
        <label class="settings-toggle-row">
          <span class="min-w-0 flex-1">
            <span class="block text-sm font-medium text-surface-100">Stamp completion inline</span>
            <span class="workshop-faint mt-0.5 block text-xs">
              Append (done YYYY-MM-DD) when checking a to-do in preview
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
    </div>
  {/if}

  <!-- 5. Advanced — one door -->
  {#if !mobile}
    <div class="mt-8">
      <button
        type="button"
        class="flex w-full items-center justify-between gap-3 text-left"
        onclick={() => (advancedOpen = !advancedOpen)}
        aria-expanded={advancedOpen}
      >
        <div>
          <h3 class="settings-subsection-heading mb-0">Advanced</h3>
          <p class="settings-subsection-lead mb-0 mt-1">
            Paths, config files, diagnostics{#if isDevBuild}, developer toggles{/if}
          </p>
        </div>
        <span class="workshop-faint shrink-0">{advancedOpen ? "▾" : "▸"}</span>
      </button>

      {#if advancedOpen}
        {#if configPaths}
          <div class="mt-4">
            <h4 class="settings-subsection-heading">Storage</h4>
            <dl class="settings-connection-meta">
              <div class="settings-connection-meta-row">
                <dt>Engine data</dt>
                <dd>
                  <span class="font-mono text-[11px]">{configPaths.dataDir}</span>
                  <span class="mt-0.5 block text-[10px] text-surface-500"
                    >via {configPaths.dataDirSource}</span
                  >
                </dd>
              </div>
              <div class="settings-connection-meta-row">
                <dt>Vault</dt>
                <dd class="font-mono text-[11px]">{configPaths.vaultDir}</dd>
              </div>
            </dl>
          </div>
        {/if}

        {#if workshopFiles.length > 0}
          <div class="mt-5">
            <h4 class="settings-subsection-heading">Workshop files</h4>
            <p class="settings-subsection-lead">
              Host config for operators — day-to-day charter stays in Settings; Engine has the rest.
            </p>
            <div class="settings-toggle-list">
              {#each workshopFiles as file (file.id)}
                <div class="settings-toggle-row settings-metric-row">
                  <span class="min-w-0 flex-1">
                    <span class="block font-mono text-[11px] font-medium text-surface-100"
                      >{file.label}</span
                    >
                    <span class="workshop-faint mt-0.5 block text-xs">{file.hint}</span>
                  </span>
                  <button
                    type="button"
                    class="btn btn-sm variant-soft-surface shrink-0"
                    onclick={() => openConfigPath(file.path)}
                  >
                    Open
                  </button>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        <div class="mt-5">
          <button
            type="button"
            class="flex w-full items-center justify-between text-left"
            onclick={() => (settings.diagnosticsOpen = !settings.diagnosticsOpen)}
          >
            <div>
              <h4 class="settings-subsection-heading mb-0">Diagnostics</h4>
              <p class="settings-subsection-lead mb-0 mt-1">Connection detail for support</p>
            </div>
            <span class="workshop-faint shrink-0">
              {settings.diagnosticsOpen ? "▾" : "▸"}
            </span>
          </button>
          {#if settings.diagnosticsOpen}
            <dl
              class="settings-connection-meta mt-3 rounded-container-token border border-surface-500/35 bg-surface-900/40 p-3"
            >
              <div class="settings-connection-meta-row">
                <dt>Status</dt>
                <dd class="font-mono">{health?.ok ? "connected" : "offline"}</dd>
              </div>
              <div class="settings-connection-meta-row">
                <dt>Base URL</dt>
                <dd class="font-mono">{settings.daemonUrl || "—"}</dd>
              </div>
              <div class="settings-connection-meta-row">
                <dt>Backend</dt>
                <dd class="font-mono">{health?.backend ?? "—"}</dd>
              </div>
              <div class="settings-connection-meta-row">
                <dt>Revision</dt>
                <dd class="font-mono">{revision}</dd>
              </div>
              <div class="settings-connection-meta-row">
                <dt>Worker</dt>
                <dd class="font-mono">{health?.worker_id ?? "—"}</dd>
              </div>
              <div class="settings-connection-meta-row">
                <dt>Tools</dt>
                <dd class="font-mono">{health?.tool_registry_count ?? "—"}</dd>
              </div>
              {#if health && !health.ok}
                <div class="settings-connection-meta-row">
                  <dt>Detail</dt>
                  <dd class="font-mono text-warning-400">{health.message}</dd>
                </div>
              {/if}
            </dl>
            <p class="workshop-faint mt-3 text-xs leading-relaxed">
              If chat freezes while status stays green, restart the engine above or open the
              connection guide.
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

        {#if isDevBuild}
          <div class="mt-5">
            <h4 class="settings-subsection-heading">Developer</h4>
            <div class="settings-toggle-list">
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
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</section>
