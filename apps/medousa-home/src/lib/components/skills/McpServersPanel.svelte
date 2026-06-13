<script lang="ts">
  import { onMount } from "svelte";
  import {
    LoaderCircle,
    Plug,
    Plus,
    RefreshCw,
    Trash2,
    Unplug,
  } from "@lucide/svelte";
  import type {
    McpServerConfig,
    McpServerRuntime,
    McpServerUpsertRequest,
  } from "$lib/types/mcpGateway";
  import {
    applyMcpServer,
    fetchMcpGatewayStatus,
    loadMcpGatewayConfig,
    removeMcpServer,
    restartMcpGateway,
    setMcpServerEnabled,
    upsertMcpServer,
  } from "$lib/utils/mcpGatewayApi";
  import { isTauri } from "$lib/window";

  type FormTransport = "stdio" | "http" | "sse" | "mock";

  let loading = $state(true);
  let busy = $state(false);
  let statusMessage = $state<string | null>(null);
  let error = $state<string | null>(null);
  let gatewayUrl = $state("");
  let gatewayReachable = $state(false);
  let configPath = $state("");
  let servers = $state<McpServerRuntime[]>([]);
  let showForm = $state(false);

  let formId = $state("");
  let formTitle = $state("");
  let formTransport = $state<FormTransport>("stdio");
  let formCommand = $state("");
  let formArgs = $state("");
  let formUrl = $state("");
  let formBearerToken = $state("");

  onMount(() => {
    void refresh();
  });

  async function refresh() {
    loading = true;
    error = null;
    try {
      const status = await fetchMcpGatewayStatus();
      gatewayUrl = status.gatewayUrl;
      gatewayReachable = status.reachable;
      configPath = status.configPath;
      servers = status.servers;
      statusMessage = status.message;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function resetForm() {
    formId = "";
    formTitle = "";
    formTransport = "stdio";
    formCommand = "";
    formArgs = "";
    formUrl = "";
    formBearerToken = "";
    showForm = false;
  }

  function transportFromConfig(server: McpServerConfig): FormTransport {
    if (server.useMock) return "mock";
    const transport = server.transport.trim().toLowerCase();
    if (transport === "sse" || transport === "http-sse") return "sse";
    if (
      transport === "http" ||
      transport === "https" ||
      transport === "streamable" ||
      transport === "streamable-http"
    ) {
      return "http";
    }
    return "stdio";
  }

  async function editServer(server: McpServerRuntime) {
    formId = server.serverId;
    formTitle = server.title;
    formCommand = "";
    formArgs = "";
    formUrl = "";
    formBearerToken = "";
    formTransport = "stdio";
    showForm = true;

    try {
      const loaded = await loadMcpGatewayConfig();
      const config = loaded.config.servers.find(
        (entry) => entry.id.toLowerCase() === server.serverId.toLowerCase(),
      );
      if (!config) return;
      formTransport = transportFromConfig(config);
      formCommand = config.command ?? "";
      formArgs = config.args.join(" ");
      formUrl = config.url ?? "";
      formBearerToken = config.bearerToken ?? "";
    } catch {
      // Keep id/title; user can re-enter connection details.
    }
  }

  function buildRequest(): McpServerUpsertRequest {
    if (formTransport === "mock") {
      return {
        id: formId.trim(),
        title: formTitle.trim(),
        enabled: true,
        transport: "stdio",
        useMock: true,
      };
    }

    if (formTransport === "http" || formTransport === "sse") {
      return {
        id: formId.trim(),
        title: formTitle.trim(),
        enabled: true,
        transport: formTransport,
        url: formUrl.trim() || null,
        bearerToken: formBearerToken.trim() || null,
        useMock: false,
      };
    }

    return {
      id: formId.trim(),
      title: formTitle.trim(),
      enabled: true,
      transport: "stdio",
      command: formCommand.trim() || null,
      args: formArgs
        .split(/\s+/)
        .map((part) => part.trim())
        .filter(Boolean),
      useMock: false,
    };
  }

  async function saveServer(restart: boolean) {
    busy = true;
    error = null;
    statusMessage = null;
    try {
      const request = buildRequest();
      if (restart) {
        const result = await applyMcpServer(request);
        statusMessage = result.message;
        if (!result.ok) error = result.message;
      } else {
        const result = await upsertMcpServer(request);
        statusMessage = result.message;
      }
      resetForm();
      await refresh();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function toggleEnabled(server: McpServerRuntime) {
    busy = true;
    error = null;
    try {
      const result = await setMcpServerEnabled(server.serverId, !server.enabled);
      statusMessage = result.message;
      await refresh();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function deleteServer(server: McpServerRuntime) {
    busy = true;
    error = null;
    try {
      const result = await removeMcpServer(server.serverId);
      statusMessage = result.message;
      await refresh();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function restartGateway() {
    busy = true;
    error = null;
    try {
      const result = await restartMcpGateway();
      statusMessage = result.message;
      await refresh();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

<div class="mcp-servers-panel">
  {#if !isTauri()}
    <p class="workshop-muted text-sm">
      Connect MCP servers from the Medousa desktop app.
    </p>
  {:else if loading}
    <div class="flex items-center gap-2 text-sm text-surface-400">
      <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
      Loading connected services…
    </div>
  {:else}
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div class="min-w-0">
        <p class="text-sm text-surface-200">
          {#if gatewayReachable}
            <Plug class="mr-1 inline h-4 w-4 text-success-300" aria-hidden="true" />
            Gateway running
          {:else}
            <Unplug class="mr-1 inline h-4 w-4 text-warning-300" aria-hidden="true" />
            Gateway offline
          {/if}
        </p>
        <p class="workshop-faint mt-1 font-mono text-[11px]">{gatewayUrl}</p>
      </div>
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          disabled={busy}
          onclick={() => void refresh()}
        >
          <RefreshCw class="h-3.5 w-3.5 {busy ? 'animate-spin' : ''}" aria-hidden="true" />
          Refresh
        </button>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          disabled={busy}
          onclick={() => void restartGateway()}
        >
          Restart gateway
        </button>
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={busy}
          onclick={() => {
            resetForm();
            showForm = true;
          }}
        >
          <Plus class="h-3.5 w-3.5" aria-hidden="true" />
          Add server
        </button>
      </div>
    </div>

    {#if showForm}
      <div class="mt-5 rounded-xl border border-surface-500/40 bg-surface-950/50 p-4">
        <h3 class="text-sm font-semibold text-surface-50">
          {formId ? "Edit MCP server" : "Add MCP server"}
        </h3>
        <p class="workshop-faint mt-1 text-xs leading-relaxed">
          Local stdio servers run a command (npx, uvx). Remote servers use HTTP or legacy SSE —
          typical for hosted MCP gateways.
        </p>

        <div class="mt-4 grid gap-3 sm:grid-cols-2">
          <label class="block">
            <span class="workshop-label">Server id</span>
            <input
              class="input mt-1 w-full font-mono text-sm"
              bind:value={formId}
              disabled={busy}
              placeholder="notion"
            />
          </label>
          <label class="block">
            <span class="workshop-label">Title</span>
            <input
              class="input mt-1 w-full text-sm"
              bind:value={formTitle}
              disabled={busy}
              placeholder="Notion MCP"
            />
          </label>
        </div>

        <fieldset class="mt-4">
          <legend class="workshop-label">Connection type</legend>
          <div class="mt-2 flex flex-wrap gap-2">
            {#each [
              ["stdio", "Local command"],
              ["http", "Remote HTTP"],
              ["sse", "Remote SSE"],
              ["mock", "Mock"],
            ] as [value, label] (value)}
              <label class="inline-flex items-center gap-2 rounded-lg border border-surface-500/40 px-3 py-2 text-xs">
                <input
                  type="radio"
                  name="mcp-transport"
                  value={value}
                  bind:group={formTransport}
                  disabled={busy}
                />
                {label}
              </label>
            {/each}
          </div>
        </fieldset>

        {#if formTransport === "mock"}
          <p class="workshop-faint mt-4 text-xs">
            Synthetic tools only — no subprocess or remote URL.
          </p>
        {:else if formTransport === "http" || formTransport === "sse"}
          <label class="mt-4 block">
            <span class="workshop-label">Server URL</span>
            <input
              class="input mt-1 w-full font-mono text-sm"
              bind:value={formUrl}
              disabled={busy}
              placeholder="https://mcp.example.com/mcp"
            />
          </label>
          <label class="mt-3 block">
            <span class="workshop-label">Bearer token (optional)</span>
            <input
              class="input mt-1 w-full font-mono text-sm"
              type="password"
              bind:value={formBearerToken}
              disabled={busy}
              placeholder="sk-…"
              autocomplete="off"
            />
          </label>
        {:else}
          <label class="mt-4 block">
            <span class="workshop-label">Command</span>
            <input
              class="input mt-1 w-full font-mono text-sm"
              bind:value={formCommand}
              disabled={busy}
              placeholder="npx"
            />
          </label>
          <label class="mt-3 block">
            <span class="workshop-label">Arguments (space-separated)</span>
            <input
              class="input mt-1 w-full font-mono text-sm"
              bind:value={formArgs}
              disabled={busy}
              placeholder="-y @modelcontextprotocol/server-filesystem /Users/you/projects"
            />
          </label>
        {/if}

        <div class="mt-5 flex flex-wrap gap-2">
          <button
            type="button"
            class="btn btn-sm variant-ghost"
            disabled={busy}
            onclick={resetForm}
          >
            Cancel
          </button>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            disabled={busy}
            onclick={() => void saveServer(false)}
          >
            Save
          </button>
          <button
            type="button"
            class="btn btn-sm variant-filled-primary"
            disabled={busy}
            onclick={() => void saveServer(true)}
          >
            {#if busy}
              <LoaderCircle class="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
            {/if}
            Save &amp; connect
          </button>
        </div>
      </div>
    {/if}

    <ul class="mt-5 divide-y divide-surface-500/35 border-y border-surface-500/35">
      {#if servers.length === 0}
        <li class="py-6 text-center text-sm text-surface-400">
          No MCP servers yet — add one to connect Notion, Gmail, filesystem tools, and more.
        </li>
      {:else}
        {#each servers as server (server.serverId)}
          <li class="flex flex-wrap items-center gap-3 py-3">
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-2">
                <p class="font-medium text-surface-100">{server.title}</p>
                <span class="font-mono text-[11px] text-surface-500">{server.serverId}</span>
                {#if server.connected}
                  <span class="text-[10px] uppercase tracking-wide text-success-300">connected</span>
                {:else if server.enabled}
                  <span class="text-[10px] uppercase tracking-wide text-warning-300">offline</span>
                {:else}
                  <span class="text-[10px] uppercase tracking-wide text-surface-500">disabled</span>
                {/if}
              </div>
              <p class="workshop-faint mt-0.5 text-xs">
                {server.toolCount} tool{server.toolCount === 1 ? "" : "s"}
              </p>
            </div>
            <div class="flex flex-wrap items-center gap-2">
              <label class="inline-flex items-center gap-2 text-xs text-surface-300">
                <input
                  type="checkbox"
                  class="checkbox"
                  checked={server.enabled}
                  disabled={busy}
                  onchange={() => void toggleEnabled(server)}
                />
                Enabled
              </label>
              <button
                type="button"
                class="workshop-text-action text-xs"
                disabled={busy}
                onclick={() => void editServer(server)}
              >
                Edit
              </button>
              <button
                type="button"
                class="workshop-text-action text-xs text-error-200"
                disabled={busy}
                aria-label="Remove {server.title}"
                onclick={() => void deleteServer(server)}
              >
                <Trash2 class="h-3.5 w-3.5" aria-hidden="true" />
              </button>
            </div>
          </li>
        {/each}
      {/if}
    </ul>

    {#if configPath}
      <p class="workshop-faint mt-4 text-[11px]">Config: {configPath}</p>
    {/if}
  {/if}

  {#if statusMessage && !error}
    <p class="mt-4 text-sm text-surface-300">{statusMessage}</p>
  {/if}
  {#if error}
    <p class="mt-4 text-sm text-warning-200">{error}</p>
  {/if}
</div>
