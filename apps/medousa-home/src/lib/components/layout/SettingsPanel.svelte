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
  import { runtime } from "$lib/stores/runtime.svelte";
  import type { DepthMode } from "$lib/types/runtime";
  import { isTauri } from "$lib/window";

  interface Props {
    visible: boolean;
    revision: number;
    health: DaemonHealth | null;
    onOpenRuntime: () => void;
    onDaemonHealth: () => void | Promise<void>;
  }

  let { visible, revision, health, onOpenRuntime, onDaemonHealth }: Props = $props();

  let draftProvider = $state(runtime.provider);
  let draftModel = $state(runtime.model);
  let configPaths = $state<MedousaConfigPaths | null>(null);

  const workshopFiles = $derived(
    configPaths
      ? [
          {
            id: "product",
            label: "Product settings",
            hint: "Channels, heartbeat, engine policy",
            path: configPaths.productConfig,
          },
          {
            id: "workspace",
            label: "Workspace prefs",
            hint: "Model, depth, runtime — shared with TUI",
            path: configPaths.tuiDefaults,
          },
          {
            id: "capabilities",
            label: "Capabilities",
            hint: "Tool and service bindings",
            path: configPaths.capabilities,
          },
          {
            id: "gateway",
            label: "MCP gateway",
            hint: "Connected app servers",
            path: configPaths.mcpGateway,
          },
        ]
      : [],
  );

  $effect(() => {
    if (visible && !settings.daemonUrl) {
      void loadDaemonUrl();
    }
    if (visible) {
      draftProvider = runtime.provider;
      draftModel = runtime.model;
      if (isTauri() && !configPaths) {
        void loadConfigPaths();
      }
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
    <p class="workshop-faint">Home preferences and shared workshop files</p>
  </header>

  <div class="flex-1 space-y-4 overflow-y-auto px-4 py-4">
    {#if workshopFiles.length > 0}
      <section class="workshop-inset p-3">
        <h2 class="text-sm font-semibold text-surface-100">Workshop files</h2>
        <p class="workshop-faint mt-1">
          Same on-disk settings as the TUI and CLI — edit in your editor.
        </p>
        <ul class="mt-3 divide-y divide-surface-500/35">
          {#each workshopFiles as file (file.id)}
            <li class="flex items-start justify-between gap-3 py-2.5 first:pt-0 last:pb-0">
              <div class="min-w-0">
                <p class="text-sm text-surface-100">{file.label}</p>
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
      </section>
    {/if}

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
      <div class="flex items-start justify-between gap-3">
        <div>
          <h2 class="text-sm font-semibold text-surface-100">Runtime controls</h2>
          <p class="workshop-faint mt-1">
            Model and depth for chat — saved to <code class="markdown-inline-code">tui_defaults.json</code>.
          </p>
        </div>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface shrink-0"
          onclick={onOpenRuntime}
        >
          Open Runtime
        </button>
      </div>
      <div class="mt-4 grid gap-3 sm:grid-cols-2">
        <label class="workshop-label block" for="settings-provider">
          Provider
        </label>
        <label class="workshop-label block" for="settings-model">
          Model
        </label>
        <input
          id="settings-provider"
          class="input"
          bind:value={draftProvider}
          placeholder="ollama"
        />
        <input
          id="settings-model"
          class="input"
          bind:value={draftModel}
          placeholder="qwen2.5:7b"
        />
      </div>
      <button
        type="button"
        class="btn variant-filled-primary mt-4"
        disabled={runtime.savingControls || !draftProvider.trim() || !draftModel.trim()}
        onclick={() => runtime.applyModel(draftProvider, draftModel)}
      >
        {runtime.savingControls ? "Applying…" : "Apply model"}
      </button>
      <div class="mt-4 flex flex-wrap gap-2">
        {#each ["concise", "standard", "deep"] as mode (mode)}
          <button
            type="button"
            class="rounded-container-token px-3 py-2 text-sm transition {runtime.depthMode ===
            mode
              ? 'bg-primary-500/20 font-medium text-primary-200'
              : 'bg-surface-800 text-surface-300 hover:text-surface-100'}"
            disabled={runtime.savingControls}
            onclick={() => runtime.setDepthMode(mode as DepthMode)}
          >
            {mode}
          </button>
        {/each}
      </div>
      <p class="workshop-faint mt-3">
        Current {runtime.modelLabel()} · depth {runtime.depthMode}
      </p>
      {#if runtime.controlsMessage}
        <p class="workshop-faint mt-2">{runtime.controlsMessage}</p>
      {/if}
    </section>

    <section class="workshop-inset p-3">
      <h2 class="text-sm font-semibold text-surface-100">Appearance</h2>
      <p class="workshop-faint mt-1">Home-only — not shared with other clients.</p>
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
