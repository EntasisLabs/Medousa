<script lang="ts">
  import {
    getMedousaConfigPaths,
    openConfigPath,
    type MedousaConfigPaths,
  } from "$lib/config";
  import {
    checkDaemonHealth,
    getDaemonUrl,
    setDaemonUrl,
    type DaemonHealth,
  } from "$lib/daemon";
  import { settings } from "$lib/stores/settings.svelte";
  import { isTauri } from "$lib/window";

  interface Props {
    visible: boolean;
    revision: number;
    health: DaemonHealth | null;
    onOpenRuntime: () => void;
    onOpenMessaging?: () => void;
    onOpenCron?: () => void;
    onDaemonHealth: () => void | Promise<void>;
  }

  let {
    visible,
    revision,
    health,
    onOpenRuntime,
    onOpenMessaging,
    onOpenCron,
    onDaemonHealth,
  }: Props = $props();

  let configPaths = $state<MedousaConfigPaths | null>(null);
  let advancedOpen = $state(false);

  const workshopFiles = $derived(
    configPaths
      ? [
          {
            id: "product",
            label: "product_config.json",
            hint: "Full product policy — channels live in Messaging",
            path: configPaths.productConfig,
          },
          {
            id: "workspace",
            label: "tui_defaults.json",
            hint: "Model and depth — edit in Runtime → Controls",
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

  $effect(() => {
    if (visible && !settings.daemonUrl) {
      void loadDaemonUrl();
    }
    if (visible && isTauri() && !configPaths) {
      void loadConfigPaths();
    }
  });

  async function loadConfigPaths() {
    try {
      configPaths = await getMedousaConfigPaths();
    } catch {
      configPaths = null;
    }
  }

  async function loadDaemonUrl() {
    try {
      settings.daemonUrl = await getDaemonUrl();
    } catch (err) {
      settings.daemonMessage =
        err instanceof Error ? err.message : String(err);
    }
  }

  async function saveDaemonUrl() {
    settings.savingDaemon = true;
    settings.daemonMessage = null;
    try {
      await setDaemonUrl(settings.daemonUrl);
      const probe = await checkDaemonHealth();
      settings.daemonMessage = probe.ok ? "Connected" : probe.message;
      await onDaemonHealth();
    } catch (err) {
      settings.daemonMessage =
        err instanceof Error ? err.message : String(err);
    } finally {
      settings.savingDaemon = false;
    }
  }
</script>

<section class="flex h-full min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  <header class="workshop-header">
    <h1 class="text-sm font-semibold text-surface-50">Settings</h1>
    <p class="workshop-faint">Home preferences and daemon connection</p>
  </header>

  <div class="flex-1 space-y-4 overflow-y-auto px-4 py-4">
    <section class="workshop-inset p-3">
      <h2 class="text-sm font-semibold text-surface-100">Connection</h2>
      <p class="workshop-faint mt-1">
        Where Medousa Home reaches the running workshop backend.
      </p>
      <label class="workshop-label mt-4 block" for="daemon-url">
        Base URL
      </label>
      <input
        id="daemon-url"
        class="input mt-1 w-full max-w-xl"
        bind:value={settings.daemonUrl}
        placeholder="http://127.0.0.1:7419"
      />
      <div class="mt-3 flex items-center gap-2">
        <button
          type="button"
          class="btn variant-filled-primary"
          disabled={settings.savingDaemon || !settings.daemonUrl.trim()}
          onclick={saveDaemonUrl}
        >
          {settings.savingDaemon ? "Saving…" : "Save & test"}
        </button>
        {#if settings.daemonMessage}
          <p
            class="text-xs {settings.daemonMessage === 'Connected' ||
            settings.daemonMessage.includes('connected')
              ? 'text-success-400'
              : 'text-warning-400'}"
          >
            {settings.daemonMessage}
          </p>
        {/if}
      </div>
    </section>

    <section class="workshop-inset p-3">
      <h2 class="text-sm font-semibold text-surface-100">Workshop surfaces</h2>
      <p class="workshop-faint mt-1">
        Configure runtime, channels, and schedules in their dedicated views.
      </p>
      <div class="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs">
        <button type="button" class="workshop-text-action" onclick={onOpenRuntime}>
          Runtime
        </button>
        {#if onOpenMessaging}
          <button type="button" class="workshop-text-action" onclick={onOpenMessaging}>
            Messaging
          </button>
        {/if}
        {#if onOpenCron}
          <button type="button" class="workshop-text-action" onclick={onOpenCron}>
            Cron
          </button>
        {/if}
      </div>
    </section>

    <section class="workshop-inset p-3">
      <h2 class="text-sm font-semibold text-surface-100">Appearance</h2>
      <p class="workshop-faint mt-1">Home-only — not shared with TUI or CLI.</p>
      <div class="mt-4 flex items-center gap-4">
        <div
          class="flex h-12 w-28 shrink-0 overflow-hidden rounded-container-token border border-surface-500/30"
          aria-hidden="true"
        >
          <span class="flex-[2] bg-surface-950"></span>
          <span class="flex-1 bg-primary-500"></span>
          <span class="flex-[1.5] bg-surface-800"></span>
        </div>
        <div class="text-xs">
          <p class="font-medium text-surface-200">Obsidian</p>
          <p class="workshop-faint mt-0.5">Near-black canvas, violet accent</p>
        </div>
      </div>
      <label class="mt-4 flex cursor-pointer items-center gap-3">
        <input
          type="checkbox"
          class="checkbox"
          checked={settings.darkMode}
          onchange={(event) =>
            settings.setDarkMode((event.currentTarget as HTMLInputElement).checked)}
        />
        <span class="text-sm text-surface-200">Dark mode (Obsidian theme)</span>
      </label>
    </section>

    <section class="workshop-inset p-3">
      <h2 class="text-sm font-semibold text-surface-100">Activity feed</h2>
      <label class="mt-4 flex cursor-pointer items-center gap-3">
        <input
          type="checkbox"
          class="checkbox"
          checked={settings.showTechnicalActivity}
          onchange={(event) =>
            settings.setShowTechnicalActivity(
              (event.currentTarget as HTMLInputElement).checked,
            )}
        />
        <span class="text-sm text-surface-200">Show technical events</span>
      </label>
      <p class="mt-2 text-xs text-surface-500">
        Includes repeated job failures and internal workflow noise.
      </p>
    </section>

    <section class="workshop-inset p-3">
      <h2 class="text-sm font-semibold text-surface-100">Notifications</h2>
      <label class="mt-4 flex cursor-pointer items-center gap-3">
        <input
          type="checkbox"
          class="checkbox"
          checked={settings.notificationsEnabled}
          onchange={(event) =>
            settings.setNotificationsEnabled(
              (event.currentTarget as HTMLInputElement).checked,
            )}
        />
        <span class="text-sm text-surface-200">
          Notify when work cards reach done
        </span>
      </label>
    </section>

    {#if workshopFiles.length > 0}
      <section class="workshop-inset p-3">
        <button
          type="button"
          class="flex w-full items-center justify-between text-left"
          onclick={() => (advancedOpen = !advancedOpen)}
        >
          <div>
            <h2 class="text-sm font-semibold text-surface-100">Advanced</h2>
            <p class="workshop-faint mt-0.5">
              On-disk files shared with TUI and CLI
            </p>
          </div>
          <span class="workshop-faint shrink-0">
            {advancedOpen ? "▾" : "▸"}
          </span>
        </button>
        {#if advancedOpen}
          <ul class="mt-3 divide-y divide-surface-500/35">
            {#each workshopFiles as file (file.id)}
              <li class="flex items-start justify-between gap-3 py-2.5 first:pt-0 last:pb-0">
                <div class="min-w-0">
                  <p class="font-mono text-[11px] text-surface-200">{file.label}</p>
                  <p class="workshop-faint">{file.hint}</p>
                  <p class="mt-0.5 truncate font-mono text-[10px] text-surface-500">
                    {file.path}
                  </p>
                </div>
                <button
                  type="button"
                  class="workshop-text-action shrink-0"
                  onclick={() => openConfigPath(file.path)}
                >
                  Open
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    <section class="workshop-inset p-3">
      <button
        type="button"
        class="flex w-full items-center justify-between text-left"
        onclick={() => (settings.diagnosticsOpen = !settings.diagnosticsOpen)}
      >
        <h2 class="text-sm font-semibold text-surface-100">Diagnostics</h2>
        <span class="workshop-faint">
          {settings.diagnosticsOpen ? "▾" : "▸"}
        </span>
      </button>
      {#if settings.diagnosticsOpen}
        <dl class="mt-4 space-y-2 text-xs">
          <div class="grid grid-cols-[7rem_1fr] gap-2">
            <dt class="workshop-label">Status</dt>
            <dd class="font-mono text-surface-300">
              {health?.ok ? "connected" : "offline"}
            </dd>
          </div>
          <div class="grid grid-cols-[7rem_1fr] gap-2">
            <dt class="workshop-label">Base URL</dt>
            <dd class="break-all font-mono text-surface-300">
              {settings.daemonUrl || "—"}
            </dd>
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
            <dd class="font-mono text-surface-300">
              {health?.tool_registry_count ?? "—"}
            </dd>
          </div>
          {#if health && !health.ok}
            <div class="grid grid-cols-[7rem_1fr] gap-2">
              <dt class="workshop-label">Detail</dt>
              <dd class="break-all font-mono text-warning-400">{health.message}</dd>
            </div>
          {/if}
        </dl>
      {/if}
    </section>
  </div>
</section>
