<script lang="ts">
  import { onMount } from "svelte";
  import type { ConnectorCard, McpElicitationPrompt } from "$lib/types/mcpGateway";
  import {
    fetchConnectorCatalog,
    fetchElicitations,
    installConnector,
    respondElicitation,
  } from "$lib/utils/mcpGatewayApi";

  interface Props {
    mobile?: boolean;
    readOnly?: boolean;
    oninstalled?: () => void;
  }

  let { mobile = false, readOnly = false, oninstalled }: Props = $props();

  let busy = $state(false);
  let error = $state<string | null>(null);
  let catalog = $state<ConnectorCard[]>([]);
  let elicitations = $state<McpElicitationPrompt[]>([]);
  let selectedConnectorId = $state<string | null>(null);
  let connectorSetup = $state<Record<string, string>>({});
  let connectorClientId = $state("");
  let connectorClientSecret = $state("");

  const selectedConnector = $derived(
    catalog.find((entry) => entry.id === selectedConnectorId) ?? null,
  );

  onMount(() => {
    void load();
    const timer = window.setInterval(() => {
      void refreshElicitations();
    }, 3000);
    return () => window.clearInterval(timer);
  });

  async function load() {
    catalog = await fetchConnectorCatalog();
    await refreshElicitations();
  }

  async function refreshElicitations() {
    try {
      elicitations = await fetchElicitations();
    } catch {
      elicitations = [];
    }
  }

  async function connectCatalogEntry(entry: ConnectorCard) {
    busy = true;
    error = null;
    try {
      await installConnector({
        id: entry.id,
        setup: connectorSetup,
        clientId: connectorClientId.trim() || null,
        clientSecret: connectorClientSecret.trim() || null,
      });
      connectorClientSecret = "";
      selectedConnectorId = null;
      oninstalled?.();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function answerElicitation(id: string, action: "accept" | "decline" | "cancel") {
    busy = true;
    error = null;
    try {
      await respondElicitation(id, action, action === "accept" ? { confirm: true } : undefined);
      await refreshElicitations();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }
</script>

{#if elicitations.length > 0}
  <div class="mt-6 rounded-container-token border border-warning-500/40 bg-warning-500/10 p-4">
    <h3 class="text-sm font-medium text-surface-50">Confirmation needed</h3>
    {#each elicitations as prompt (prompt.id)}
      <p class="mt-2 text-sm text-surface-100">{prompt.message}</p>
      {#if prompt.url}
        <p class="workshop-faint mt-1 break-all font-mono text-xs">{prompt.url}</p>
      {/if}
      <div class="mt-3 flex flex-wrap gap-2">
        <button type="button" class="btn btn-sm variant-filled-primary" disabled={busy} onclick={() => void answerElicitation(prompt.id, "accept")}>Allow</button>
        <button type="button" class="btn btn-sm variant-soft-surface" disabled={busy} onclick={() => void answerElicitation(prompt.id, "decline")}>Decline</button>
        <button type="button" class="btn btn-sm variant-ghost-surface" disabled={busy} onclick={() => void answerElicitation(prompt.id, "cancel")}>Cancel</button>
      </div>
    {/each}
  </div>
{/if}

{#if error}
  <p class="mt-3 text-sm text-error-500">{error}</p>
{/if}

{#if !readOnly && catalog.length > 0}
  <div class="mt-8">
    <h3 class="settings-subsection-heading">Connectors</h3>
    <p class="settings-subsection-lead">
      Install a reviewed server, its sign-in, and a short usage note. Nothing is saved until the first tool list succeeds.
    </p>
    <div class="mt-3 grid gap-2 sm:grid-cols-2">
      {#each catalog as entry (entry.id)}
        {@const unavailable = entry.desktopOnly && mobile}
        <button
          type="button"
          class="settings-depth-card text-left"
          disabled={busy || unavailable}
          onclick={() => {
            selectedConnectorId = selectedConnectorId === entry.id ? null : entry.id;
            connectorSetup = {};
            connectorClientId = "";
            connectorClientSecret = "";
          }}
        >
          <span class="block text-sm font-medium text-surface-100">{entry.title}</span>
          <span class="workshop-faint mt-0.5 block text-xs">
            {entry.publisher} · {unavailable ? "Desktop only" : entry.transport}
          </span>
        </button>
      {/each}
    </div>
    {#if selectedConnector}
      <div class="mt-3 space-y-3 rounded-container-token border border-surface-500/35 p-4">
        {#if selectedConnector.googleAgentClient}
          <p class="workshop-faint text-xs leading-relaxed">
            Google requires an OAuth client flagged for an AI agent. Use a Desktop client on this computer, or your own iOS or Android client on a phone. Medousa does not ship that secret.
          </p>
        {/if}
        {#if selectedConnector.needsClientId}
          <label class="block">
            <span class="workshop-label">OAuth client id</span>
            <input class="input mt-1 w-full font-mono text-sm" bind:value={connectorClientId} disabled={busy} />
          </label>
          <label class="block">
            <span class="workshop-label">OAuth client secret (optional)</span>
            <input class="input mt-1 w-full font-mono text-sm" type="password" bind:value={connectorClientSecret} disabled={busy} autocomplete="off" />
          </label>
        {/if}
        {#each selectedConnector.setup as field (field.key)}
          <label class="block">
            <span class="workshop-label">{field.title}</span>
            <input
              class="input mt-1 w-full font-mono text-sm"
              type={field.sensitive ? "password" : "text"}
              disabled={busy}
              autocomplete="off"
              value={connectorSetup[field.key] ?? ""}
              oninput={(event) => {
                const target = event.currentTarget as HTMLInputElement;
                connectorSetup = { ...connectorSetup, [field.key]: target.value };
              }}
            />
          </label>
        {/each}
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={busy || (selectedConnector.desktopOnly && mobile)}
          onclick={() => selectedConnector && void connectCatalogEntry(selectedConnector)}
        >
          Connect {selectedConnector.title}
        </button>
      </div>
    {/if}
  </div>
{/if}
