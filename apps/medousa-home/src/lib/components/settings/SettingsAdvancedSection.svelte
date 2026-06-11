<script lang="ts">
  import {
    getMedousaConfigPaths,
    openConfigPath,
    type MedousaConfigPaths,
  } from "$lib/config";
  import type { DaemonHealth } from "$lib/daemon";
  import { vault } from "$lib/stores/vault.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { resetGarageOnboarding } from "$lib/utils/garageOnboarding";
  import { isTauri } from "$lib/window";

  const isDevBuild = import.meta.env.DEV;

  interface Props {
    revision: number;
    health: DaemonHealth | null;
    mobile?: boolean;
  }

  let { revision, health, mobile = false }: Props = $props();

  let configPaths = $state<MedousaConfigPaths | null>(null);

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
            hint: "Workshop defaults — edit under Runtime → Workshop",
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
    if (isTauri() && !mobile && !configPaths) {
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
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Advanced</h2>
    <p class="workshop-faint mt-1 text-sm">
      On-disk workshop files, diagnostics, and developer options.
    </p>
  </header>

  {#if isDevBuild && !mobile}
    <div class="settings-toggle-list mt-5">
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
