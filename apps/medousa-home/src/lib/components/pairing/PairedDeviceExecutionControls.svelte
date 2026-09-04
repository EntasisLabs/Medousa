<script lang="ts">
  import { untrack } from "svelte";
  import { LoaderCircle } from "@lucide/svelte";
  import {
    updatePeerExecutionPolicy,
    type PeerExecutionPolicyEntry,
    type PeerExecutionPolicyPreset,
    type PeerNetworkPolicy,
  } from "$lib/utils/pairingApi";

  type ExpiryChoice = "never" | "1d" | "7d" | "30d" | "custom";

  interface Props {
    entry: PeerExecutionPolicyEntry;
    onupdated?: (entry: PeerExecutionPolicyEntry) => void;
    onerror?: (message: string) => void;
  }

  let { entry, onupdated, onerror }: Props = $props();
  const initial = untrack(() => entry.execution.policy);
  let preset = $state<PeerExecutionPolicyPreset>(initial.preset);
  let expiry = $state<ExpiryChoice>(initial.expiresAt ? "custom" : "never");
  let customDate = $state(dateInputValue(initial.expiresAt));
  let assistantWork = $state(initial.assistantWork);
  let sandboxExecution = $state(initial.sandboxExecution);
  let hostShell = $state(initial.hostShell);
  let coderWork = $state(initial.coderWork);
  let workEnvironmentMaterialization = $state(initial.workEnvironmentMaterialization);
  let allowAgentTargeting = $state(initial.allowAgentTargeting);
  let networkPolicy = $state<PeerNetworkPolicy>(initial.networkPolicy);
  let projectIds = $state(joinValues(initial.allowedProjectIds));
  let rootRefs = $state(joinValues(initial.allowedRootRefs));
  let toolDomains = $state(joinValues(initial.allowedToolDomains));
  let mcpServerIds = $state(joinValues(initial.allowedMcpServerIds));
  let secretRefs = $state(joinValues(initial.allowedSecretRefs));
  let saving = $state(false);
  let saved = $state(false);
  let cancelledWorkCount = $state(0);
  let savedRevision = $state(initial.revision);
  let savedSource = $state(untrack(() => entry.execution.source));

  const showAdvanced = $derived(preset === "custom" || preset === "approved_projects");

  function joinValues(values: string[] | undefined): string {
    return (values ?? []).join(", ");
  }

  function parseValues(value: string): string[] {
    return [...new Set(value.split(/[\n,]/).map((item) => item.trim()).filter(Boolean))];
  }

  function dateInputValue(value: string | null | undefined): string {
    if (!value) return "";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return "";
    const month = String(date.getMonth() + 1).padStart(2, "0");
    const day = String(date.getDate()).padStart(2, "0");
    return `${date.getFullYear()}-${month}-${day}`;
  }

  function expiresAt(): string | null {
    if (expiry === "never") return null;
    if (expiry === "custom") {
      if (!customDate) throw new Error("Choose a permission expiration date.");
      const value = new Date(`${customDate}T23:59:59`);
      if (Number.isNaN(value.getTime()) || value.getTime() <= Date.now()) {
        throw new Error("Permission expiration must be in the future.");
      }
      return value.toISOString();
    }
    const days = Number.parseInt(expiry, 10);
    return new Date(Date.now() + days * 86_400_000).toISOString();
  }

  function presetSummary(value: PeerExecutionPolicyPreset): string {
    switch (value) {
      case "assistant_work":
        return "Can run bounded assistant turns with utility and web tools.";
      case "sandboxed_work":
        return "Can run assistant work and isolated sandbox execution; no host shell.";
      case "approved_projects":
        return "Can use Coder only with the project identities listed below.";
      case "custom":
        return "Uses the exact scopes in Advanced permissions.";
      default:
        return "Can connect and share, but cannot run work on this workshop.";
    }
  }

  async function savePermissions() {
    if (saving) return;
    saving = true;
    saved = false;
    cancelledWorkCount = 0;
    try {
      const allowedProjectIds = parseValues(projectIds);
      if (preset === "approved_projects" && allowedProjectIds.length === 0) {
        throw new Error("Add at least one approved project id.");
      }
      const policy = {
        preset,
        expiresAt: expiresAt(),
        ...(preset === "approved_projects" ? { allowedProjectIds } : {}),
        ...(preset === "custom"
          ? {
              assistantWork,
              sandboxExecution,
              hostShell,
              coderWork,
              workEnvironmentMaterialization,
              allowAgentTargeting,
              networkPolicy,
              allowedProjectIds,
              allowedRootRefs: parseValues(rootRefs),
              allowedToolDomains: parseValues(toolDomains),
              allowedMcpServerIds: parseValues(mcpServerIds),
              allowedSecretRefs: parseValues(secretRefs),
            }
          : {}),
      };
      const result = await updatePeerExecutionPolicy(entry.peerDeviceId, policy);
      cancelledWorkCount = result.cancelledWorkCount;
      savedRevision = result.peer.execution.policy.revision;
      savedSource = result.peer.execution.source;
      saved = true;
      onupdated?.(result.peer);
    } catch (error) {
      onerror?.(error instanceof Error ? error.message : String(error));
    } finally {
      saving = false;
    }
  }
</script>

<div class="peer-execution-body">
  <div class="peer-execution-heading">
    <div>
      <p class="peer-execution-title">Allowed on this workshop</p>
      <p class="peer-execution-summary">{presetSummary(preset)}</p>
    </div>
    {#if savedSource === "legacy_task_request"}
      <span class="peer-execution-legacy">Legacy</span>
    {/if}
  </div>

  <div class="peer-execution-grid">
    <label class="peer-execution-field">
      <span>Permission</span>
      <select class="input text-sm" bind:value={preset}>
        <option value="connected_only">Connected only</option>
        <option value="assistant_work">Assistant work</option>
        <option value="sandboxed_work">Sandboxed work</option>
        <option value="approved_projects">Approved projects</option>
        <option value="custom">Custom</option>
      </select>
    </label>
    <label class="peer-execution-field">
      <span>Expires</span>
      <select class="input text-sm" bind:value={expiry}>
        <option value="never">When removed</option>
        <option value="1d">In 1 day</option>
        <option value="7d">In 7 days</option>
        <option value="30d">In 30 days</option>
        <option value="custom">Custom date</option>
      </select>
    </label>
  </div>

  {#if expiry === "custom"}
    <label class="peer-execution-field peer-execution-date">
      <span>Expiration date</span>
      <input
        class="input text-sm"
        type="date"
        min={dateInputValue(new Date(Date.now() + 86_400_000).toISOString())}
        bind:value={customDate}
      />
    </label>
  {/if}

  {#if showAdvanced}
    <details class="peer-execution-advanced" open={preset === "approved_projects"}>
      <summary>Advanced permissions</summary>
      <div class="peer-execution-advanced-body">
        {#if preset === "custom"}
          <div class="peer-execution-toggles">
            <label><input type="checkbox" bind:checked={assistantWork} /> Assistant work</label>
            <label><input type="checkbox" bind:checked={sandboxExecution} /> Sandbox execution</label>
            <label><input type="checkbox" bind:checked={hostShell} /> Host shell</label>
            <label><input type="checkbox" bind:checked={coderWork} /> Coder work</label>
            <label>
              <input type="checkbox" bind:checked={workEnvironmentMaterialization} /> Work environments
            </label>
            <label><input type="checkbox" bind:checked={allowAgentTargeting} /> Agent targeting</label>
          </div>
          <label class="peer-execution-field">
            <span>Network</span>
            <select class="input text-sm" bind:value={networkPolicy}>
              <option value="deny">No network</option>
              <option value="web_only">Web tools only</option>
              <option value="unrestricted">Unrestricted</option>
            </select>
          </label>
        {/if}
        <label class="peer-execution-field">
          <span>Project ids</span>
          <input class="input text-sm" placeholder="project-a, project-b" bind:value={projectIds} />
        </label>
        {#if preset === "custom"}
          <label class="peer-execution-field">
            <span>Allowed root references</span>
            <input class="input text-sm" placeholder="root ids, not host paths" bind:value={rootRefs} />
          </label>
          <label class="peer-execution-field">
            <span>Tool domains</span>
            <input class="input text-sm" placeholder="turn, utility, web" bind:value={toolDomains} />
          </label>
          <label class="peer-execution-field">
            <span>MCP server ids</span>
            <input class="input text-sm" placeholder="optional" bind:value={mcpServerIds} />
          </label>
          <label class="peer-execution-field">
            <span>Secret references</span>
            <input class="input text-sm" placeholder="optional named references" bind:value={secretRefs} />
          </label>
        {/if}
      </div>
    </details>
  {/if}

  <div class="peer-execution-actions">
    <button
      type="button"
      class="btn btn-sm variant-soft"
      disabled={saving}
      onclick={savePermissions}
    >
      {#if saving}
        <LoaderCircle class="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
        Saving…
      {:else}
        Save permissions
      {/if}
    </button>
    <span class="peer-execution-note">
      {#if saved}
        Saved · revision {savedRevision}{cancelledWorkCount > 0 ? ` · stopped ${cancelledWorkCount} active` : ""}
      {:else if savedSource === "legacy_task_request"}
        Save to replace the legacy grant with an explicit policy.
      {:else}
        Changes affect new work immediately.
      {/if}
    </span>
  </div>
</div>

<style>
  .peer-execution-body {
    display: grid;
    gap: 0.7rem;
    padding: 0.7rem 0.75rem 0.8rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.22);
  }

  .peer-execution-heading,
  .peer-execution-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .peer-execution-title,
  .peer-execution-summary {
    margin: 0;
  }

  .peer-execution-title {
    font-size: 0.72rem;
    font-weight: 650;
    color: rgb(var(--theme-text-secondary));
  }

  .peer-execution-summary,
  .peer-execution-note {
    font-size: 0.68rem;
    line-height: 1.4;
    color: rgb(var(--theme-text-quiet));
  }

  .peer-execution-legacy {
    flex-shrink: 0;
    font-size: 0.62rem;
    color: rgb(var(--theme-text-tertiary));
  }

  .peer-execution-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.65rem;
  }

  .peer-execution-field {
    display: grid;
    gap: 0.3rem;
    min-width: 0;
    font-size: 0.68rem;
    font-weight: 600;
    color: rgb(var(--theme-text-tertiary));
  }

  .peer-execution-date {
    max-width: 14rem;
  }

  .peer-execution-advanced {
    border-top: 1px solid rgb(var(--color-surface-500) / 0.18);
    padding-top: 0.6rem;
  }

  .peer-execution-advanced summary {
    cursor: pointer;
    list-style: none;
    font-size: 0.68rem;
    font-weight: 600;
    color: rgb(var(--theme-text-tertiary));
  }

  .peer-execution-advanced summary::-webkit-details-marker {
    display: none;
  }

  .peer-execution-advanced-body {
    display: grid;
    gap: 0.65rem;
    margin-top: 0.65rem;
  }

  .peer-execution-toggles {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.45rem 0.75rem;
    font-size: 0.68rem;
    color: rgb(var(--theme-text-secondary));
  }

  .peer-execution-toggles label {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .peer-execution-actions {
    justify-content: flex-start;
  }

  .peer-execution-actions :global(.btn) {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    flex-shrink: 0;
  }

  @media (max-width: 34rem) {
    .peer-execution-grid,
    .peer-execution-toggles {
      grid-template-columns: minmax(0, 1fr);
    }

    .peer-execution-actions {
      align-items: flex-start;
      flex-direction: column;
      gap: 0.45rem;
    }
  }
</style>
