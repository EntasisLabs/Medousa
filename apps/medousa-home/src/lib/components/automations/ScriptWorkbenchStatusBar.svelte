<script lang="ts">
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";

  interface Props {
    lspReady?: boolean;
    onToggleConsole?: () => void;
  }

  let { onToggleConsole }: Props = $props();

  const lineCount = $derived.by(() => {
    const body = graphemeScriptEditor.activeTab?.body ?? "";
    if (!body) return 0;
    return body.split("\n").length;
  });

  const leftLabel = $derived.by(() => {
    const tab = graphemeScriptEditor.activeTab;
    if (!tab) return "No script";
    const parts: string[] = [];
    if (tab.dirty) parts.push("Modified");
    else if (tab.scriptId) parts.push("Saved");
    else parts.push("Unsaved");
    parts.push(`${lineCount} line${lineCount === 1 ? "" : "s"}`);
    if (graphemeScriptEditor.lspReady) parts.push("LSP");
    return parts.join(" · ");
  });

  const centerLabel = $derived.by(() => {
    if (graphemeScriptEditor.compileBusy) return "Compiling…";
    if (graphemeScriptEditor.compileError) return "Compile failed";
    if (graphemeScriptEditor.compileResult) {
      const valid = graphemeScriptEditor.compileResult.validated ? "valid" : "invalid";
      return `Compile ${valid}`;
    }
    if (workshop.runBusy) return "Running…";
    if (workshop.runError) return "Run failed";
    if (workshop.runResult?.result?.succeeded === true) return "Run succeeded";
    if (workshop.runResult?.result?.succeeded === false) return "Run failed";
    return null;
  });

  const centerClass = $derived.by(() => {
    if (graphemeScriptEditor.compileError || workshop.runError) return "text-error-400";
    if (workshop.runResult?.result?.succeeded === false) return "text-warning-400";
    if (centerLabel?.includes("valid") || centerLabel === "Run succeeded") {
      return "text-success-400/90";
    }
    return "text-surface-400";
  });

  const rightLabel = $derived.by(() => {
    if (graphemeScriptEditor.saveError) return graphemeScriptEditor.saveError;
    if (graphemeScriptEditor.statusMessage) return graphemeScriptEditor.statusMessage;
    const tab = graphemeScriptEditor.activeTab;
    if (tab?.scriptId) return tab.scriptId;
    return "Grapheme";
  });

  const rightClass = $derived(
    graphemeScriptEditor.saveError ? "text-error-400" : "text-surface-500",
  );
</script>

<footer class="scripts-workbench-status-bar" aria-label="Script workbench status">
  <span class="scripts-workbench-status-item min-w-0 truncate">{leftLabel}</span>

  {#if centerLabel}
    <button
      type="button"
      class="scripts-workbench-status-item scripts-workbench-status-center {centerClass}"
      title="Show output"
      onclick={() => onToggleConsole?.()}
    >
      {centerLabel}
    </button>
  {:else}
    <button
      type="button"
      class="scripts-workbench-status-item scripts-workbench-status-center text-surface-600"
      title="Show output"
      onclick={() => onToggleConsole?.()}
    >
      Ready
    </button>
  {/if}

  <span class="scripts-workbench-status-item min-w-0 truncate text-right {rightClass}">
    {rightLabel}
  </span>
</footer>
