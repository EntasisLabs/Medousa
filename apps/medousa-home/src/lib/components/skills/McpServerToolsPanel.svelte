<script lang="ts">
  import { onMount } from "svelte";
  import { LoaderCircle, RefreshCw } from "@lucide/svelte";
  import type { McpGatewayTool, McpServerRuntime } from "$lib/types/mcpGateway";
  import { listMcpServerTools, updateMcpTool } from "$lib/utils/mcpGatewayApi";

  interface Props {
    server: McpServerRuntime;
    disabled?: boolean;
    mobile?: boolean;
    onBusyChange?: (busy: boolean) => void;
    onStatus?: (message: string) => void;
    onError?: (message: string) => void;
    onChanged?: () => Promise<void> | void;
  }

  let {
    server,
    disabled = false,
    mobile = false,
    onBusyChange,
    onStatus,
    onError,
    onChanged,
  }: Props = $props();

  let tools = $state<McpGatewayTool[]>([]);
  let loading = $state(true);
  let mutationKey = $state<string | null>(null);
  let hintDrafts = $state<Record<string, string>>({});

  onMount(() => {
    void loadTools();
  });

  function keyFor(toolName: string): string {
    return toolName.toLowerCase();
  }

  function parseDiscoveryHints(raw: string): string[] {
    const hints: string[] = [];
    for (const part of raw.split(/[,\n]/)) {
      const hint = part.trim();
      if (!hint || hints.some((existing) => existing.toLowerCase() === hint.toLowerCase())) continue;
      hints.push(hint);
    }
    return hints;
  }

  function setHintDraft(toolName: string, value: string) {
    hintDrafts = { ...hintDrafts, [keyFor(toolName)]: value };
  }

  async function loadTools() {
    loading = true;
    try {
      const result = await listMcpServerTools(server.serverId);
      tools = result.tools;
      const nextDrafts = { ...hintDrafts };
      for (const tool of result.tools) {
        nextDrafts[keyFor(tool.toolName)] = tool.discoveryHints.join(", ");
      }
      hintDrafts = nextDrafts;
      if (result.message) onStatus?.(result.message);
    } catch (error) {
      onError?.(error instanceof Error ? error.message : String(error));
    } finally {
      loading = false;
    }
  }

  async function saveTool(tool: McpGatewayTool, enabled = tool.enabled) {
    const key = keyFor(tool.toolName);
    mutationKey = key;
    onBusyChange?.(true);
    try {
      const result = await updateMcpTool({
        serverId: server.serverId,
        toolName: tool.toolName,
        enabled,
        discoveryHints: parseDiscoveryHints(hintDrafts[key] ?? ""),
      });
      await onChanged?.();
      await loadTools();
      onStatus?.(result.message);
    } catch (error) {
      onError?.(error instanceof Error ? error.message : String(error));
    } finally {
      mutationKey = null;
      onBusyChange?.(false);
    }
  }
</script>

<div class="mcp-tools-content" class:mcp-mobile={mobile}>
  <div class="mcp-tools-heading">
    <div>
      <p class="text-sm font-semibold text-surface-100">Available tools</p>
      <p class="workshop-faint mt-1 text-xs leading-relaxed">
        Turn tools on or off. Discovery hints teach Medousa the words you would use to ask for one.
      </p>
    </div>
    <button
      type="button"
      class="btn btn-sm variant-ghost-surface shrink-0"
      disabled={disabled || loading || mutationKey !== null}
      onclick={() => void loadTools()}
    >
      <RefreshCw class="h-3.5 w-3.5 {loading ? 'animate-spin' : ''}" aria-hidden="true" />
      <span class="sr-only">Refresh {server.title} tools</span>
    </button>
  </div>

  {#if loading && tools.length === 0}
    <div class="flex items-center gap-2 py-5 text-xs text-content-tertiary">
      <LoaderCircle class="h-4 w-4 animate-spin" aria-hidden="true" />
      Loading tools…
    </div>
  {:else if tools.length === 0}
    <p class="workshop-faint py-5 text-xs leading-relaxed">
      No tools are visible yet. Connect this server, then reload its catalog.
    </p>
  {:else}
    <div class="mcp-tool-list">
      {#each tools as tool (tool.toolName)}
        {@const key = keyFor(tool.toolName)}
        <div class="mcp-tool-row" class:mcp-tool-row-disabled={!tool.enabled}>
          <div class="mcp-tool-summary">
            <span class="min-w-0 flex-1">
              <span class="flex flex-wrap items-center gap-x-2 gap-y-1">
                <span class="text-sm font-medium text-surface-100">{tool.title}</span>
                {#if tool.enabled && !tool.available}
                  <span class="text-[10px] uppercase tracking-wide text-content-warning">Unavailable</span>
                {/if}
              </span>
              <span class="mt-0.5 block truncate font-mono text-[11px] text-content-quiet">
                {tool.toolName}
              </span>
            </span>
            <label class="mcp-tool-switch" title={tool.enabled ? "Disable tool" : "Enable tool"}>
              <span class="sr-only">{tool.enabled ? "Disable" : "Enable"} {tool.title}</span>
              <input
                type="checkbox"
                class="mcp-tool-switch-input"
                role="switch"
                checked={tool.enabled}
                disabled={disabled || mutationKey !== null}
                onchange={() => void saveTool(tool, !tool.enabled)}
              />
              <span class="mcp-tool-switch-track" aria-hidden="true">
                <span class="mcp-tool-switch-thumb"></span>
              </span>
            </label>
          </div>

          <label class="mt-3 block">
            <span class="workshop-label">Discovery hints</span>
            <span class="mcp-tool-hint-editor mt-1.5">
              <input
                class="input min-w-0 flex-1 text-sm"
                value={hintDrafts[key] ?? ""}
                disabled={disabled || mutationKey !== null}
                placeholder="web_research, web, internet"
                oninput={(event) =>
                  setHintDraft(tool.toolName, (event.currentTarget as HTMLInputElement).value)}
              />
              <button
                type="button"
                class="btn btn-sm variant-soft-primary shrink-0"
                disabled={disabled || mutationKey !== null}
                onclick={() => void saveTool(tool)}
              >
                {#if mutationKey === key}
                  <LoaderCircle class="mr-1.5 h-3.5 w-3.5 animate-spin" aria-hidden="true" />
                {/if}
                Save
              </button>
            </span>
            <span class="workshop-faint mt-1.5 block text-[11px] leading-relaxed">
              Comma-separated. Use capability ids or natural words—for example web_research, web, and internet.
            </span>
          </label>

          {#if tool.capabilityIds.length > 0}
            <p class="workshop-faint mt-2 text-[11px] leading-relaxed">
              Matches {tool.capabilityIds.join(" · ")}
            </p>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .mcp-tools-heading,
  .mcp-tool-summary,
  .mcp-tool-hint-editor {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .mcp-tools-heading {
    align-items: flex-start;
    justify-content: space-between;
    padding: 0 0.25rem 0.9rem;
  }

  .mcp-tool-list {
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-500) / 0.3);
    border-radius: 0.9rem;
    background: rgb(var(--color-surface-800) / 0.22);
  }

  .mcp-tool-row {
    padding: 0.9rem 1rem 1rem;
    transition: opacity 140ms ease;
  }

  .mcp-tool-row + .mcp-tool-row {
    border-top: 1px solid rgb(var(--color-surface-600) / 0.25);
  }

  .mcp-tool-row-disabled {
    opacity: 0.62;
  }

  .mcp-tool-summary {
    min-height: 2.75rem;
  }

  .mcp-tool-hint-editor {
    align-items: stretch;
  }

  .mcp-tool-switch {
    position: relative;
    display: inline-flex;
    height: 2.75rem;
    width: 3.25rem;
    flex-shrink: 0;
    cursor: pointer;
    align-items: center;
    justify-content: center;
  }

  .mcp-tool-switch-input {
    position: absolute;
    height: 1px;
    width: 1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .mcp-tool-switch-track {
    position: relative;
    display: block;
    height: 1.5rem;
    width: 2.55rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.52);
    border-radius: 999px;
    background: rgb(var(--color-surface-700) / 0.72);
    transition: border-color 150ms ease, background-color 150ms ease;
  }

  .mcp-tool-switch-thumb {
    position: absolute;
    top: 0.15rem;
    left: 0.15rem;
    height: 1.075rem;
    width: 1.075rem;
    border-radius: 999px;
    background: rgb(var(--color-surface-200));
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.35);
    transition: transform 150ms ease, background-color 150ms ease;
  }

  .mcp-tool-switch-input:checked + .mcp-tool-switch-track {
    border-color: rgb(var(--color-primary-500) / 0.72);
    background: rgb(var(--color-primary-500) / 0.72);
  }

  .mcp-tool-switch-input:checked + .mcp-tool-switch-track .mcp-tool-switch-thumb {
    transform: translateX(1.05rem);
    background: rgb(var(--color-surface-50));
  }

  .mcp-tool-switch-input:focus-visible + .mcp-tool-switch-track {
    outline: 2px solid rgb(var(--color-primary-400) / 0.85);
    outline-offset: 2px;
  }

  .mcp-tool-switch-input:disabled + .mcp-tool-switch-track {
    opacity: 0.45;
  }

  .mcp-mobile .mcp-tool-hint-editor {
    flex-wrap: wrap;
  }

  .mcp-mobile .mcp-tool-hint-editor :global(.input) {
    flex-basis: 12rem;
  }
</style>
