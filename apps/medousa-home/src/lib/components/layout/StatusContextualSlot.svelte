<script lang="ts">
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultVersions } from "$lib/stores/vaultVersions.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import { formatVaultNoteStats, vaultNoteStats } from "$lib/utils/vaultNoteStats";
  import { dispatchScriptWorkbenchOpenConsole } from "$lib/utils/scriptWorkbenchChromeEvents";

  const activeLme = $derived(lmeWorkspace.activeTab);
  const onLibrary = $derived(layout.desktopSurface === "library");

  const showVault = $derived(
    onLibrary &&
      activeLme?.kind === "note" &&
      Boolean(vault.selectedPath) &&
      !vault.noteLoading,
  );

  const showScript = $derived(
    onLibrary &&
      Boolean(graphemeScriptEditor.activeTab) &&
      (activeLme?.kind === "script" ||
        (lmeWorkspace.explorerMode === "scripts" && activeLme?.kind !== "note")),
  );

  const noteSummary = $derived(formatVaultNoteStats(vaultNoteStats(vault.content)));
  const saveWhisper = $derived(vault.saveWhisper());
  const versionsDirtyLabel = $derived.by(() => {
    if (!vaultVersions.enabled || !vaultVersions.status?.isRepo) return "";
    const dirty = vaultVersions.status.dirtyCount;
    if (dirty <= 0) return "";
    const branch = vaultVersions.status.branch ?? "main";
    return `${branch} · ${dirty} changed`;
  });

  /** Dirty / unsaved only — saved scripts stay silent. */
  const scriptDirty = $derived.by(() => {
    const tab = graphemeScriptEditor.activeTab;
    if (!tab) return "No script";
    if (tab.dirty) return "Unsaved";
    if (!tab.scriptId) return "Unsaved";
    return null;
  });

  /** In-flight or failed only — Ready / success stay off the bar. */
  const scriptAction = $derived.by(() => {
    if (graphemeScriptEditor.compileBusy) {
      return { label: "Compiling…", tone: "busy" as const };
    }
    if (graphemeScriptEditor.compileError) {
      return { label: "Compile failed", tone: "error" as const };
    }
    if (
      graphemeScriptEditor.compileResult &&
      !graphemeScriptEditor.compileResult.validated
    ) {
      return { label: "Compile invalid", tone: "warn" as const };
    }
    if (workshop.runBusy) {
      return { label: "Running…", tone: "busy" as const };
    }
    if (workshop.runError || workshop.runResult?.result?.succeeded === false) {
      return { label: "Run failed", tone: "error" as const };
    }
    return null;
  });

  const scriptError = $derived(
    graphemeScriptEditor.saveError?.trim() || null,
  );

  const scriptToneClass = $derived.by(() => {
    if (!scriptAction) return "";
    if (scriptAction.tone === "error") return "text-error-400";
    if (scriptAction.tone === "warn") return "text-warning-400";
    return "text-surface-300";
  });

  const hasScriptChrome = $derived(
    Boolean(scriptDirty || scriptAction || scriptError),
  );

  $effect(() => {
    if (!showVault || !vaultVersions.enabled) return;
    void vaultVersions.refresh();
  });
</script>

{#if showVault}
  <div class="status-contextual status-contextual--vault" aria-label="Note status">
    <span class="status-contextual-item truncate">{noteSummary}</span>
    {#if versionsDirtyLabel}
      <span class="status-contextual-sep" aria-hidden="true">·</span>
      <button
        type="button"
        class="status-contextual-action truncate text-warning-400/85"
        title="Open Versions"
        onclick={() => vaultVersions.openPanel()}
      >
        {versionsDirtyLabel}
      </button>
    {/if}
    {#if saveWhisper}
      <span class="status-contextual-sep" aria-hidden="true">·</span>
      <span class="status-contextual-whisper">{saveWhisper}</span>
    {/if}
  </div>
{:else if showScript && hasScriptChrome}
  <div class="status-contextual status-contextual--script" aria-label="Script status">
    {#if scriptDirty}
      <span class="status-contextual-item truncate">{scriptDirty}</span>
    {/if}
    {#if scriptAction}
      {#if scriptDirty}
        <span class="status-contextual-sep" aria-hidden="true">·</span>
      {/if}
      <button
        type="button"
        class="status-contextual-action truncate {scriptToneClass}"
        title="Show output"
        onclick={() => dispatchScriptWorkbenchOpenConsole()}
      >
        {scriptAction.label}
      </button>
    {/if}
    {#if scriptError}
      {#if scriptDirty || scriptAction}
        <span class="status-contextual-sep" aria-hidden="true">·</span>
      {/if}
      <span class="status-contextual-item truncate text-error-400">{scriptError}</span>
    {/if}
  </div>
{/if}

<style>
  .status-contextual {
    display: inline-flex;
    min-width: 0;
    max-width: 18rem;
    flex: 0 1 auto;
    align-items: center;
    gap: 0.35rem;
    margin-left: auto;
    color: rgb(var(--color-surface-500));
    overflow: hidden;
  }

  .status-contextual-item {
    min-width: 0;
  }

  .status-contextual-sep {
    flex-shrink: 0;
    opacity: 0.45;
  }

  .status-contextual-action {
    min-width: 0;
    border: 0;
    background: transparent;
    padding: 0;
    color: inherit;
    font: inherit;
    text-align: left;
    transition: color 140ms ease;
  }

  .status-contextual-action:hover {
    color: rgb(var(--color-surface-200));
  }

  .status-contextual-whisper {
    flex-shrink: 0;
    color: rgb(var(--color-surface-400));
  }
</style>
