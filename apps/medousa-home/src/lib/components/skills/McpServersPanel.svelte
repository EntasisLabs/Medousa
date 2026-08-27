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
    McpOAuthStatus,
  } from "$lib/types/mcpGateway";
  import {
    applyMcpServer,
    authorizeMcpOAuth,
    disconnectMcpOAuth,
    fetchMcpGatewayStatus,
    fetchMcpOAuthStatus,
    loadMcpGatewayConfig,
    removeMcpServer,
    restartMcpGateway,
    setMcpServerEnabled,
  } from "$lib/utils/mcpGatewayApi";
  import { isTauri } from "$lib/window";
  import { onThisHostPhrase } from "$lib/platformCopy";

  interface Props {
    embedded?: boolean;
    readOnly?: boolean;
  }

  let { embedded = false, readOnly = false }: Props = $props();

  type FormTransport = "stdio" | "http" | "sse" | "mock";

  const TRANSPORTS: { id: FormTransport; label: string; hint: string }[] = [
    { id: "stdio", label: "Local command", hint: `npx, uvx, or a binary ${onThisHostPhrase()}` },
    { id: "http", label: "Remote HTTP", hint: "Hosted MCP over streamable HTTP" },
    { id: "sse", label: "Remote SSE", hint: "Legacy SSE gateways" },
    { id: "mock", label: "Mock", hint: "Synthetic tools only" },
  ];
  const availableTransports = $derived(
    embedded ? TRANSPORTS.filter((transport) => transport.id !== "stdio") : TRANSPORTS,
  );

  let loading = $state(true);
  let busy = $state(false);
  let statusMessage = $state<string | null>(null);
  let error = $state<string | null>(null);
  let gatewayUrl = $state("");
  let gatewayReachable = $state(false);
  let configPath = $state("");
  let servers = $state<McpServerRuntime[]>([]);
  let oauthStatuses = $state<Record<string, McpOAuthStatus>>({});
  let showForm = $state(false);
  let advancedOpen = $state(false);
  let setupAction = $state<"connect" | "sign_in" | null>(null);

  let formId = $state("");
  let formTitle = $state("");
  let formTransport = $state<FormTransport>("stdio");
  let formCommand = $state("");
  let formArgs = $state("");
  let formUrl = $state("");
  let formBearerToken = $state("");

  const connectedCount = $derived(servers.filter((s) => s.connected).length);
  const editingExisting = $derived(Boolean(formId.trim()) && servers.some(
    (s) => s.serverId.toLowerCase() === formId.trim().toLowerCase(),
  ));
  const canSignIn = $derived(
    (formTransport === "http" || formTransport === "sse") && !formBearerToken.trim(),
  );

  function defaultTransport(): FormTransport {
    return embedded ? "http" : "stdio";
  }

  onMount(() => {
    void refresh();
  });

  async function refreshOAuthStatuses(nextServers: McpServerRuntime[]) {
    const entries = await Promise.all(nextServers.map(async (server) => {
      try {
        return [server.serverId, await fetchMcpOAuthStatus(server.serverId)] as const;
      } catch {
        return null;
      }
    }));
    oauthStatuses = Object.fromEntries(entries.filter((entry) => entry !== null));
  }

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
      await refreshOAuthStatuses(status.servers);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function resetForm() {
    formId = "";
    formTitle = "";
    formTransport = defaultTransport();
    formCommand = "";
    formArgs = "";
    formUrl = "";
    formBearerToken = "";
    showForm = false;
  }

  function startAdd() {
    if (readOnly) return;
    formId = "";
    formTitle = "";
    formTransport = defaultTransport();
    formCommand = "";
    formArgs = "";
    formUrl = "";
    formBearerToken = "";
    showForm = true;
    error = null;
    statusMessage = null;
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
    if (readOnly) return;
    formId = server.serverId;
    formTitle = server.title;
    formCommand = "";
    formArgs = "";
    formUrl = "";
    formBearerToken = "";
    formTransport = "stdio";
    showForm = true;
    error = null;
    statusMessage = null;

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

  async function saveServer(signIn: boolean) {
    if (readOnly) return;
    setupAction = signIn ? "sign_in" : "connect";
    busy = true;
    error = null;
    statusMessage = null;
    try {
      const request = buildRequest();
      const result = await applyMcpServer(request);
      resetForm();
      if (signIn) {
        await authorizeMcpOAuth(request.id);
        await refresh();
        statusMessage = `${request.title} connected and signed in.`;
      } else {
        await refresh();
        statusMessage = result.message;
        if (!result.ok) error = result.message;
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
      setupAction = null;
    }
  }

  async function toggleEnabled(server: McpServerRuntime) {
    if (readOnly) return;
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
    if (readOnly) return;
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
    if (readOnly) return;
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

  async function connectOAuth(server: McpServerRuntime) {
    busy = true;
    error = null;
    statusMessage = null;
    try {
      await authorizeMcpOAuth(server.serverId);
      await refresh();
      statusMessage = `${server.title} connected.`;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function disconnectOAuth(server: McpServerRuntime) {
    busy = true;
    error = null;
    try {
      await disconnectMcpOAuth(server.serverId);
      await refresh();
      statusMessage = `${server.title} signed out.`;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  function statusLabel(server: McpServerRuntime): { text: string; className: string } {
    if (server.connected) {
      return { text: "Connected", className: "text-content-success" };
    }
    if (server.enabled) {
      return { text: "Offline", className: "text-content-warning" };
    }
    return { text: "Disabled", className: "text-content-quiet" };
  }
</script>

<div class="mcp-servers-panel w-full">
  {#if !isTauri()}
    <p class="workshop-muted text-sm">
      Connect MCP servers from the Medousa desktop app.
    </p>
  {:else if loading}
    <div class="flex items-center gap-2 text-sm text-content-tertiary">
      <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
      Loading connected services…
    </div>
  {:else if showForm}
    <!-- Editing chapter: one job -->
    <div class="mb-4">
      <button
        type="button"
        class="workshop-text-action text-sm"
        disabled={busy}
        onclick={resetForm}
      >
        ← Back to servers
      </button>
      <h3 class="settings-subsection-heading mt-3">
        {editingExisting ? "Edit server" : "Add server"}
      </h3>
      <p class="settings-subsection-lead">
        {#if formTransport === "mock"}
          Synthetic tools only — nothing leaves this machine.
        {:else if formTransport === "http" || formTransport === "sse"}
          Point at a hosted MCP endpoint. Token is optional.
        {:else}
          Run a local command (npx, uvx, or a binary). Medousa starts it with the gateway.
        {/if}
      </p>
    </div>

    <div class="settings-connection-card mcp-full-card space-y-4">
      <div class="grid gap-3 sm:grid-cols-2">
        <label class="block">
          <span class="workshop-label">Server id</span>
          <input
            class="input mt-1 w-full font-mono text-sm"
            bind:value={formId}
            disabled={busy || editingExisting}
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

      <div>
        <p class="workshop-label mb-2">How it connects</p>
        <div class="grid gap-2 sm:grid-cols-2">
          {#each availableTransports as option (option.id)}
            <button
              type="button"
              class="settings-depth-card {formTransport === option.id
                ? 'settings-depth-card-active'
                : ''}"
              disabled={busy}
              onclick={() => (formTransport = option.id)}
            >
              <span class="block text-sm font-medium text-surface-100">{option.label}</span>
              <span class="workshop-faint mt-0.5 block text-xs leading-relaxed">{option.hint}</span>
            </button>
          {/each}
        </div>
      </div>

      {#if formTransport === "http" || formTransport === "sse"}
        <label class="block">
          <span class="workshop-label">Server URL</span>
          <input
            class="input mt-1 w-full font-mono text-sm"
            bind:value={formUrl}
            disabled={busy}
            placeholder="https://mcp.example.com/mcp"
          />
        </label>
        <label class="block">
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
      {:else if formTransport === "stdio"}
        <label class="block">
          <span class="workshop-label">Command</span>
          <input
            class="input mt-1 w-full font-mono text-sm"
            bind:value={formCommand}
            disabled={busy}
            placeholder="npx"
          />
        </label>
        <label class="block">
          <span class="workshop-label">Arguments</span>
          <input
            class="input mt-1 w-full font-mono text-sm"
            bind:value={formArgs}
            disabled={busy}
            placeholder="-y @modelcontextprotocol/server-filesystem /Users/you/projects"
          />
          <span class="workshop-faint mt-1 block text-xs">Space-separated.</span>
        </label>
      {/if}

      <div class="flex flex-wrap items-center gap-2 border-t border-surface-500/35 pt-4">
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={busy || !formId.trim()}
          onclick={() => void saveServer(false)}
        >
          {#if setupAction === "connect"}
            <LoaderCircle class="mr-1.5 h-3.5 w-3.5 animate-spin" aria-hidden="true" />
          {/if}
          Connect
        </button>
        <button
          type="button"
          class="btn btn-sm variant-soft-primary"
          disabled={busy || !formId.trim() || !canSignIn}
          title={canSignIn
            ? "Save, connect, and sign in"
            : "Sign in is available for remote servers without a bearer token"}
          onclick={() => void saveServer(true)}
        >
          {#if setupAction === "sign_in"}
            <LoaderCircle class="mr-1.5 h-3.5 w-3.5 animate-spin" aria-hidden="true" />
          {/if}
          Sign in
        </button>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          disabled={busy}
          onclick={resetForm}
        >
          Cancel
        </button>
      </div>
      {#if !canSignIn}
        <p class="workshop-faint text-xs">
          {#if (formTransport === "http" || formTransport === "sse") && formBearerToken.trim()}
            Remove the bearer token to sign in with OAuth.
          {:else}
            Sign in is available for hosted HTTP and SSE servers.
          {/if}
        </p>
      {/if}
    </div>
  {:else}
    <!-- Browse chapter: gateway → servers → advanced -->
    <div class="settings-connection-card mcp-full-card">
      <div class="flex items-start gap-3">
        <span
          class="settings-connection-icon {gatewayReachable
            ? 'settings-connection-icon-ok'
            : 'settings-connection-icon-off'}"
          aria-hidden="true"
        >
          {#if gatewayReachable}
            <Plug size={18} strokeWidth={2} />
          {:else}
            <Unplug size={18} strokeWidth={2} />
          {/if}
        </span>
        <div class="min-w-0 flex-1">
          <p class="text-sm font-semibold text-surface-50">
            {gatewayReachable
              ? embedded
                ? "Adapter running"
                : "Gateway running"
              : embedded
                ? "Adapter offline"
                : "Gateway offline"}
          </p>
          <p class="workshop-faint mt-0.5 text-xs">
            {#if servers.length === 0}
              No servers yet — add one when you need external tools.
            {:else}
              {connectedCount} of {servers.length} connected
            {/if}
          </p>
          <p class="workshop-faint mt-1 font-mono text-[11px]">{gatewayUrl}</p>
        </div>
        <div class="flex shrink-0 flex-wrap justify-end gap-2">
          <button
            type="button"
            class="btn btn-sm variant-soft-surface"
            disabled={busy}
            aria-label="Refresh gateway"
            onclick={() => void refresh()}
          >
            <RefreshCw class="h-3.5 w-3.5 {busy ? 'animate-spin' : ''}" aria-hidden="true" />
          </button>
          {#if !readOnly}
            <button
              type="button"
              class="btn btn-sm variant-soft-surface"
              disabled={busy}
              onclick={() => void restartGateway()}
            >
              {embedded ? "Reload" : "Restart"}
            </button>
          {/if}
        </div>
      </div>
    </div>

    <div class="mt-8">
      <div class="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h3 class="settings-subsection-heading">Your servers</h3>
          <p class="settings-subsection-lead mb-0">
            Tools from these MCP servers show up for specialists and chat.
          </p>
        </div>
        {#if !readOnly}
          <button
            type="button"
            class="btn btn-sm variant-filled-primary shrink-0"
            disabled={busy}
            onclick={startAdd}
          >
            <Plus class="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
            Add server
          </button>
        {/if}
      </div>

      {#if servers.length === 0}
        <div class="mt-4 rounded-container-token border border-dashed border-surface-500/40 px-4 py-8 text-center">
          <p class="text-sm font-medium text-surface-100">Nothing connected yet</p>
          <p class="workshop-faint mx-auto mt-1 max-w-sm text-xs leading-relaxed">
            {readOnly
              ? "This workshop owns its MCP configuration. Add servers on the workshop host."
              : "Add Notion, search, filesystem, or any MCP server — Medousa pulls their tools into the gateway."}
          </p>
          {#if !readOnly}
            <button
              type="button"
              class="btn btn-sm variant-soft-primary mt-4"
              disabled={busy}
              onclick={startAdd}
            >
              <Plus class="mr-1.5 h-3.5 w-3.5" aria-hidden="true" />
              Add your first server
            </button>
          {/if}
        </div>
      {:else}
        <div class="settings-toggle-list mt-4">
          {#each servers as server (server.serverId)}
            {@const status = statusLabel(server)}
            {@const oauth = oauthStatuses[server.serverId]}
            <div class="settings-toggle-row settings-metric-row flex-wrap">
              <span class="min-w-0 flex-1">
                <span class="flex flex-wrap items-center gap-2">
                  <span class="text-sm font-medium text-surface-100">{server.title}</span>
                  <span class="font-mono text-[11px] text-content-quiet">{server.serverId}</span>
                  <span class="text-[10px] font-medium uppercase tracking-wide {status.className}">
                    {status.text}
                  </span>
                </span>
                <span class="workshop-faint mt-0.5 block text-xs">
                  {server.toolCount} tool{server.toolCount === 1 ? "" : "s"}
                </span>
              </span>
              <div class="flex shrink-0 items-center gap-2">
                {#if oauth?.connected}
                  <span class="text-[10px] font-medium uppercase tracking-wide text-content-success">
                    Signed in
                  </span>
                  <button
                    type="button"
                    class="workshop-text-action text-xs"
                    disabled={busy}
                    onclick={() => void disconnectOAuth(server)}
                  >
                    Sign out
                  </button>
                {:else if oauth}
                  <button
                    type="button"
                    class="workshop-text-action text-xs"
                    disabled={busy}
                    onclick={() => void connectOAuth(server)}
                  >
                    Sign in
                  </button>
                {/if}
                <label class="inline-flex items-center gap-2 text-xs text-content-secondary">
                  <input
                    type="checkbox"
                    class="checkbox"
                    checked={server.enabled}
                    disabled={busy || readOnly}
                    onchange={() => void toggleEnabled(server)}
                  />
                  On
                </label>
                {#if !readOnly}
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
                    class="workshop-text-action text-xs text-content-error"
                    disabled={busy}
                    aria-label="Remove {server.title}"
                    onclick={() => void deleteServer(server)}
                  >
                    <Trash2 class="h-3.5 w-3.5" aria-hidden="true" />
                  </button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    {#if configPath}
      <div class="mt-8">
        <button
          type="button"
          class="flex w-full items-center justify-between gap-3 text-left"
          onclick={() => (advancedOpen = !advancedOpen)}
          aria-expanded={advancedOpen}
        >
          <div>
            <h3 class="settings-subsection-heading mb-0">Advanced</h3>
            <p class="settings-subsection-lead mb-0 mt-1">Gateway config file {onThisHostPhrase()}</p>
          </div>
          <span class="workshop-faint shrink-0">{advancedOpen ? "▾" : "▸"}</span>
        </button>
        {#if advancedOpen}
          <p class="workshop-faint mt-3 font-mono text-[11px] leading-relaxed">{configPath}</p>
        {/if}
      </div>
    {/if}
  {/if}

  {#if statusMessage && !error}
    <p class="mt-4 text-sm text-content-secondary">{statusMessage}</p>
  {/if}
  {#if error}
    <p class="mt-4 text-sm text-content-warning">{error}</p>
  {/if}
</div>

<style>
  :global(.mcp-servers-panel .mcp-full-card) {
    max-width: none;
  }
</style>
