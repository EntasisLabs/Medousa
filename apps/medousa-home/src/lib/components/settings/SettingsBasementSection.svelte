<script lang="ts">
  import { onMount } from "svelte";
  import { ChevronDown, Wifi, WifiOff } from "@lucide/svelte";
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
    type ConnectionPrefsSummary,
  } from "$lib/connection";
  import { reconnectWorkshop } from "$lib/workshopConnection";
  import { restartEngine, waitForEngine } from "$lib/utils/providersApi";
  import { vault } from "$lib/stores/vault.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { resetGarageOnboarding } from "$lib/utils/garageOnboarding";
  import { wizard } from "$lib/stores/wizard.svelte";
  import SettingsAppUpdateCard from "$lib/components/settings/SettingsAppUpdateCard.svelte";
  import SettingsLocalBrainPanel from "$lib/components/settings/SettingsLocalBrainPanel.svelte";
  import SettingsWorkshopsSection from "$lib/components/settings/SettingsWorkshopsSection.svelte";
  import { isTauri } from "$lib/window";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { workshopBasementConnectionLabel } from "$lib/platformCopy";

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
  let moreOpen = $state(false);
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
  const statusMeta = $derived(
    connected
      ? `${connectionLabel} · ${backendLabel}`
      : connectionLabel === "Not configured"
        ? "No workshop address yet"
        : `${connectionLabel} · offline`,
  );
  const engineMeta = $derived(
    [
      engineVersionLabel,
      `${toolsReadyLabel} tools`,
      lastTurnLabel,
      health?.active_profile_display_name,
    ]
      .filter(Boolean)
      .join(" · "),
  );

  const workshopFiles = $derived(
    configPaths
      ? [
          {
            id: "product",
            label: "product_config.json",
            hint: "Product policy — channels live in Sharing",
            path: configPaths.productConfig,
          },
          {
            id: "workspace",
            label: "tui_defaults.json",
            hint: "Full charter — Agent settings edit the human fields",
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
            hint: "MCP gateway — manage servers in Settings → MCP",
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
    const ok = window.confirm(
      "Restart the workshop engine? Active chats and tools will pause until it comes back.",
    );
    if (!ok) return;
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

<section class="settings-section prefs connection">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Workshop</h2>
    <p class="workshop-faint mt-1 text-sm">
      Which workshop you’re in — and how this machine runs it.
    </p>
  </header>

  <SettingsWorkshopsSection {onDaemonHealth} lead />

  <div class="prefs-band">
    <SettingsAppUpdateCard />
  </div>

  <div class="prefs-band">
    <div class="prefs-band-head">
      <h3 class="settings-subsection-heading">Status</h3>
      <p class="settings-subsection-lead">
        Live link to the active workshop.
      </p>
    </div>

    <div class="prefs-stack">
      <div class="prefs-tile">
        <span
          class="conn-status-icon"
          class:conn-status-ok={connected}
          class:conn-status-off={!connected}
          aria-hidden="true"
        >
          {#if connected}
            <Wifi size={16} strokeWidth={2} />
          {:else}
            <WifiOff size={16} strokeWidth={2} />
          {/if}
        </span>
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">{connected ? "Connected" : "Offline"}</span>
          <span class="prefs-tile-meta">{statusMeta}</span>
        </span>
        {#if !connectionEditing}
          <button
            type="button"
            class="prefs-tile-cta"
            onclick={() => {
              connectionEditing = true;
              settings.daemonMessage = null;
            }}
          >
            Address
          </button>
        {/if}
      </div>

      {#if connectionEditing}
        <div class="conn-edit">
          <label class="block" for="daemon-url">
            <span class="workshop-label">Workshop address</span>
            <input
              id="daemon-url"
              class="input mt-1 w-full"
              bind:value={settings.daemonUrl}
              placeholder={mobile ? "http://192.168.1.42:7419" : "http://127.0.0.1:7419"}
            />
          </label>
          <div class="mt-3 flex flex-wrap items-center gap-2">
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
          </div>
          {#if settings.daemonMessage}
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
      {:else if settings.daemonMessage}
        <p
          class="prefs-footnote {settings.daemonMessage === 'Connected' ||
          settings.daemonMessage.toLowerCase().includes('connected')
            ? 'text-success-400'
            : 'text-warning-400'}"
        >
          {settings.daemonMessage}
        </p>
      {/if}

      {#if isTauri() && !mobile}
        <div class="prefs-tile">
          <span class="prefs-tile-copy">
            <span class="prefs-tile-title">Engine</span>
            <span class="prefs-tile-meta">{engineMeta}</span>
          </span>
          <span class="conn-pill" class:conn-pill-ok={connected}>
            {connected ? "Running" : "Offline"}
          </span>
          <button
            type="button"
            class="prefs-tile-cta"
            disabled={restartingEngine}
            onclick={() => void restartWorkshopEngine()}
          >
            {restartingEngine ? "…" : "Restart"}
          </button>
        </div>
        {#if restartMessage}
          <p class="prefs-footnote text-surface-400">{restartMessage}</p>
        {/if}
      {/if}
    </div>
  </div>

  {#if isTauri() && !mobile}
    <div class="prefs-band">
      <div class="prefs-band-head">
        <h3 class="settings-subsection-heading">This Mac</h3>
        <p class="settings-subsection-lead">
          Login start. Phone & LAN reachability live in Sharing.
        </p>
      </div>

      <div class="prefs-stack">
        {#if connectionPrefs?.autostartSupported}
          <label class="prefs-tile">
            <span class="prefs-tile-copy">
              <span class="prefs-tile-title">Start Medousa when I log in</span>
              <span class="prefs-tile-meta">Engine only — never the offline brain</span>
            </span>
            <input
              type="checkbox"
              class="prefs-switch"
              checked={connectionPrefs.autostartEnabled}
              disabled={prefsBusy}
              onchange={(event) =>
                void toggleAutostart((event.currentTarget as HTMLInputElement).checked)}
            />
          </label>
        {/if}

        {#if connectionPrefs}
          <button
            type="button"
            class="prefs-tile prefs-tile-action"
            onclick={() => settingsNav.openSection("network")}
          >
            <span class="prefs-tile-copy">
              <span class="prefs-tile-title">Phone & LAN reachability</span>
              <span class="prefs-tile-meta">
                {#if connectionPrefs.publicBind}
                  Always reachable on Wi‑Fi · Sharing
                {:else}
                  Pairing window and Wi‑Fi · Sharing
                {/if}
              </span>
            </span>
            <span class="prefs-tile-cta">Open</span>
          </button>
        {/if}
      </div>

      {#if prefsMessage}
        <p class="prefs-footnote mt-2">{prefsMessage}</p>
      {/if}
    </div>
  {/if}

  {#if isTauri() && !mobile}
    <SettingsLocalBrainPanel />
  {/if}

  <details class="prefs-more" bind:open={moreOpen}>
    <summary class="prefs-more-summary">
      <span>More on this device</span>
      <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
    </summary>
    <div class="prefs-more-body prefs-stack">
      <button
        type="button"
        class="prefs-tile prefs-tile-action"
        onclick={() => void wizard.beginRerun()}
      >
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Welcome wizard</span>
          <span class="prefs-tile-meta">Re-run model choice and optional phone pairing</span>
        </span>
        <span class="prefs-tile-cta">Re-run</span>
      </button>

      <label class="prefs-tile">
        <span class="prefs-tile-copy">
          <span class="prefs-tile-title">Stamp completion inline</span>
          <span class="prefs-tile-meta">Append (done YYYY-MM-DD) on to-do check</span>
        </span>
        <input
          type="checkbox"
          class="prefs-switch"
          checked={vault.stampCompletionInline}
          onchange={(event) =>
            vault.setStampCompletionInline((event.currentTarget as HTMLInputElement).checked)}
        />
      </label>

      {#if !mobile}
        <details class="prefs-nested" bind:open={advancedOpen}>
          <summary class="prefs-nested-summary">
            <span>Files & diagnostics</span>
            <ChevronDown size={14} strokeWidth={2} class="prefs-more-chevron" aria-hidden="true" />
          </summary>
          <div class="prefs-nested-body prefs-stack">
            {#if configPaths}
              <div class="conn-kv">
                <p class="prefs-footnote mb-1">Storage</p>
                <p class="conn-kv-row">
                  <span>Engine data</span>
                  <span class="font-mono text-[10px]">{configPaths.dataDir}</span>
                </p>
                <p class="conn-kv-row">
                  <span>Vault</span>
                  <span class="font-mono text-[10px]">{configPaths.vaultDir}</span>
                </p>
              </div>
            {/if}

            {#each workshopFiles as file (file.id)}
              <div class="prefs-tile">
                <span class="prefs-tile-copy">
                  <span class="prefs-tile-title font-mono text-[0.72rem]">{file.label}</span>
                  <span class="prefs-tile-meta">{file.hint}</span>
                </span>
                <button
                  type="button"
                  class="prefs-tile-cta"
                  onclick={() => openConfigPath(file.path)}
                >
                  Open
                </button>
              </div>
            {/each}

            <div class="conn-kv">
              <p class="prefs-footnote mb-1">Diagnostics</p>
              <p class="conn-kv-row">
                <span>Status</span>
                <span class="font-mono">{health?.ok ? "connected" : "offline"}</span>
              </p>
              <p class="conn-kv-row">
                <span>Base URL</span>
                <span class="font-mono">{settings.daemonUrl || "—"}</span>
              </p>
              <p class="conn-kv-row">
                <span>Backend</span>
                <span class="font-mono">{health?.backend ?? "—"}</span>
              </p>
              <p class="conn-kv-row">
                <span>Revision</span>
                <span class="font-mono">{revision}</span>
              </p>
              <p class="conn-kv-row">
                <span>Worker</span>
                <span class="font-mono">{health?.worker_id ?? "—"}</span>
              </p>
            </div>

            <button
              type="button"
              class="prefs-tile prefs-tile-action"
              onclick={() => void openRunbook()}
            >
              <span class="prefs-tile-copy">
                <span class="prefs-tile-title">Troubleshooting guide</span>
                <span class="prefs-tile-meta">Connection runbook for support</span>
              </span>
              <span class="prefs-tile-cta">Open</span>
            </button>
            {#if runbookError}
              <p class="prefs-footnote text-warning-300">{runbookError}</p>
            {/if}

            {#if isDevBuild}
              <label class="prefs-tile">
                <span class="prefs-tile-copy">
                  <span class="prefs-tile-title">Developer vault notes</span>
                  <span class="prefs-tile-meta">Show bugs/ and system paths in Library</span>
                </span>
                <input
                  type="checkbox"
                  class="prefs-switch"
                  checked={vault.showSystemNotes}
                  onchange={(event) =>
                    vault.setShowSystemNotes((event.currentTarget as HTMLInputElement).checked)}
                />
              </label>
              <button
                type="button"
                class="prefs-tile prefs-tile-action"
                onclick={() => {
                  resetGarageOnboarding();
                  vault.openGarageWizard();
                }}
              >
                <span class="prefs-tile-copy">
                  <span class="prefs-tile-title">Reset garage onboarding</span>
                  <span class="prefs-tile-meta">Developer only</span>
                </span>
                <span class="prefs-tile-cta">Reset</span>
              </button>
            {/if}
          </div>
        </details>
      {/if}
    </div>
  </details>
</section>

<style>
  .prefs {
    --prefs-gap: 0.5rem;
    --prefs-tile-radius: 0.65rem;
    --prefs-tile-pad: 0.55rem 0.75rem;
    --prefs-tile-min-h: 3.25rem;
    --prefs-tile-border: rgb(var(--color-surface-500) / 0.32);
    --prefs-tile-bg: rgb(var(--color-surface-900) / 0.28);
  }

  .prefs-band {
    margin-top: 1.25rem;
  }

  .prefs-band-head .settings-subsection-heading {
    margin-bottom: 0.15rem;
  }

  .prefs-band-head .settings-subsection-lead {
    margin-bottom: 0.6rem;
  }

  .prefs-stack {
    display: grid;
    gap: var(--prefs-gap);
  }

  .prefs-tile {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: var(--prefs-tile-bg);
  }

  .prefs-tile-action {
    width: 100%;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .prefs-tile-action:hover {
    border-color: rgb(var(--color-surface-500) / 0.48);
    background: rgb(var(--color-surface-800) / 0.28);
  }

  .prefs-tile-copy {
    display: flex;
    min-width: 0;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 0.1rem;
  }

  .prefs-tile-title {
    font-size: 0.8rem;
    font-weight: 550;
    color: rgb(var(--color-surface-100));
  }

  .prefs-tile-meta {
    font-size: 0.68rem;
    line-height: 1.3;
    color: rgb(var(--color-surface-500));
  }

  .prefs-tile-cta {
    flex-shrink: 0;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--color-surface-400));
    cursor: pointer;
  }

  .prefs-tile-cta:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .prefs-switch {
    position: relative;
    flex-shrink: 0;
    width: 2.35rem;
    height: 1.3rem;
    margin: 0;
    appearance: none;
    border: 0;
    border-radius: 999px;
    background: rgb(var(--color-surface-600) / 0.55);
    cursor: pointer;
    transition: background 140ms ease;
  }

  .prefs-switch::after {
    content: "";
    position: absolute;
    top: 0.15rem;
    left: 0.15rem;
    width: 1rem;
    height: 1rem;
    border-radius: 999px;
    background: rgb(var(--color-surface-100));
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.25);
    transition: transform 140ms ease;
  }

  .prefs-switch:checked {
    background: rgb(var(--color-primary-500) / 0.85);
  }

  .prefs-switch:checked::after {
    transform: translateX(1.05rem);
  }

  .prefs-switch:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .prefs-footnote {
    margin: 0;
    font-size: 0.7rem;
    line-height: 1.4;
    color: rgb(var(--color-surface-500));
  }

  .conn-status-icon {
    display: flex;
    height: 1.75rem;
    width: 1.75rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.45rem;
  }

  .conn-status-ok {
    background: rgb(var(--color-success-500) / 0.15);
    color: rgb(var(--color-success-400));
  }

  .conn-status-off {
    background: rgb(var(--color-warning-500) / 0.12);
    color: rgb(var(--color-warning-400));
  }

  .conn-pill {
    flex-shrink: 0;
    font-size: 0.65rem;
    font-weight: 600;
    color: rgb(var(--color-warning-400));
  }

  .conn-pill-ok {
    color: rgb(var(--color-success-400));
  }

  .conn-edit {
    padding: 0.65rem 0.75rem;
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: rgb(var(--color-surface-950) / 0.35);
  }

  .prefs-more,
  .prefs-nested {
    margin-top: 1.25rem;
    border-radius: var(--prefs-tile-radius);
    border: 1px solid var(--prefs-tile-border);
    background: rgb(var(--color-surface-950) / 0.35);
  }

  .prefs-nested {
    margin-top: 0;
  }

  .prefs-more-summary,
  .prefs-nested-summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    min-height: var(--prefs-tile-min-h);
    padding: var(--prefs-tile-pad);
    font-size: 0.75rem;
    font-weight: 550;
    color: rgb(var(--color-surface-200));
    cursor: pointer;
    list-style: none;
  }

  .prefs-more-summary::-webkit-details-marker,
  .prefs-nested-summary::-webkit-details-marker {
    display: none;
  }

  :global(.prefs-more-chevron) {
    flex-shrink: 0;
    color: rgb(var(--color-surface-500));
    transition: transform 140ms ease;
  }

  .prefs-more[open] :global(.prefs-more-chevron),
  .prefs-nested[open] :global(.prefs-more-chevron) {
    transform: rotate(180deg);
  }

  .prefs-more-body,
  .prefs-nested-body {
    padding: 0 0.75rem 0.75rem;
  }

  .conn-kv {
    padding: 0.55rem 0.15rem;
  }

  .conn-kv-row {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    margin: 0.2rem 0 0;
    font-size: 0.68rem;
    color: rgb(var(--color-surface-400));
  }

  .conn-kv-row span:last-child {
    min-width: 0;
    text-align: right;
    color: rgb(var(--color-surface-300));
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
