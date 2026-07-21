<script lang="ts">
  import {
    GitBranchPlus,
    Hammer,
    MessageSquare,
    PanelLeftOpen,
    PanelRightClose,
    Play,
    Save,
    Terminal,
    Zap,
  } from "@lucide/svelte";
  import ScriptEditorTabStrip from "$lib/components/automations/ScriptEditorTabStrip.svelte";
  import GraphemeModuleLibraryPicker from "$lib/components/grapheme/GraphemeModuleLibraryPicker.svelte";
  import { promoteScriptToFlow } from "$lib/grapheme/graphemeFlowBridge";
  import {
    closeActiveScriptTab,
    isPlainTextEditingTarget,
  } from "$lib/grapheme/scriptWorkbenchActions";
  import { compileGraphemeSource, saveGraphemeScript } from "$lib/daemon";
  import { formatShortcut } from "$lib/platform";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { scriptRenameUi } from "$lib/stores/scriptRenameUi.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";

  interface Props {
    mobile?: boolean;
    leftOpen: boolean;
    consoleOpen: boolean;
    chatOpen: boolean;
    onShowSidebar: () => void;
    onToggleConsole: () => void;
    onToggleChat: () => void;
    onOpenOutput?: () => void;
    /** When true, document tabs live on the shell hover strip (not this titlebar). */
    hideTabStrip?: boolean;
  }

  let {
    mobile = false,
    leftOpen,
    consoleOpen,
    chatOpen,
    onShowSidebar,
    onToggleConsole,
    onToggleChat,
    onOpenOutput,
    hideTabStrip = false,
  }: Props = $props();

  let flowError = $state<string | null>(null);

  function addActiveScriptToFlow() {
    flowError = null;
    const tab = graphemeScriptEditor.activeTab;
    if (!tab?.body.trim()) {
      flowError = "Write script source before adding to a flow.";
      return;
    }
    try {
      promoteScriptToFlow(tab.body, tab.name, tab.scriptId);
    } catch (err) {
      flowError = err instanceof Error ? err.message : String(err);
    }
  }

  async function saveActive() {
    const tab = graphemeScriptEditor.activeTab;
    if (!tab) return;
    graphemeScriptEditor.saveBusy = true;
    graphemeScriptEditor.saveError = null;
    try {
      const response = await saveGraphemeScript({
        id: tab.scriptId,
        name: tab.name.trim() || "Untitled script",
        body: tab.body,
        intent: tab.intent.trim() || null,
        tags: tab.tags,
      });
      graphemeScriptEditor.markActiveSaved(response.script);
      await workshop.refreshModulesAndScripts();
    } catch (err) {
      graphemeScriptEditor.saveError =
        err instanceof Error ? err.message : String(err);
    } finally {
      graphemeScriptEditor.saveBusy = false;
    }
  }

  async function compileActive(mode: "check" | "aot") {
    const tab = graphemeScriptEditor.activeTab;
    if (!tab?.body.trim()) return;
    graphemeScriptEditor.compileBusy = true;
    graphemeScriptEditor.compileError = null;
    graphemeScriptEditor.compileResult = null;
    try {
      graphemeScriptEditor.compileResult = await compileGraphemeSource(
        tab.body,
        mode,
      );
      onOpenOutput?.();
    } catch (err) {
      graphemeScriptEditor.compileError =
        err instanceof Error ? err.message : String(err);
      onOpenOutput?.();
    } finally {
      graphemeScriptEditor.compileBusy = false;
    }
  }

  async function runActive() {
    const tab = graphemeScriptEditor.activeTab;
    if (!tab?.body.trim()) return;
    await workshop.runScriptSource(tab.body);
    graphemeScriptEditor.runError = workshop.runError;
    onOpenOutput?.();
  }

  function handleWorkbenchKeydown(event: KeyboardEvent) {
    if (mobile) return;
    const inPlainField = isPlainTextEditingTarget(event.target);

    if (event.key === "F2" && !event.metaKey && !event.ctrlKey && !event.altKey) {
      // Allow from CodeMirror; ignore when typing in plain form fields.
      if (inPlainField) return;
      if (!graphemeScriptEditor.activeTab) return;
      event.preventDefault();
      scriptRenameUi.startActiveRename();
      return;
    }

    const mod = event.metaKey || event.ctrlKey;
    if (!mod || event.altKey) return;
    if (inPlainField) return;

    const key = event.key.toLowerCase();
    if (key === "s") {
      event.preventDefault();
      void saveActive();
      return;
    }
    if (key === "enter") {
      event.preventDefault();
      void runActive();
      return;
    }
    if (key === "b" && !event.shiftKey) {
      event.preventDefault();
      void compileActive("check");
      return;
    }
    if (key === "w" && !event.shiftKey) {
      event.preventDefault();
      void closeActiveScriptTab();
    }
  }
</script>

<svelte:window onkeydown={handleWorkbenchKeydown} />

<div class="scripts-workbench-titlebar flex shrink-0 items-center gap-1 border-b border-surface-500/35 px-1 py-0.5">
  {#if !mobile && !leftOpen}
    <button
      type="button"
      class="scripts-workbench-toolbar-btn shrink-0"
      title="Show workspace browser"
      aria-label="Show workspace browser"
      onclick={onShowSidebar}
    >
      <PanelLeftOpen size={15} strokeWidth={1.75} />
    </button>
  {/if}

  {#if !hideTabStrip}
    <ScriptEditorTabStrip compact {mobile} />
  {:else}
    <!-- Spacer: document tabs live on the shell hover strip; keep actions right. -->
    <div class="min-w-0 flex-1" aria-hidden="true"></div>
  {/if}

  <div
    class="scripts-workbench-titlebar-actions flex shrink-0 items-center gap-0.5 pl-1
      {hideTabStrip ? 'ml-auto' : ''}"
  >
    <button
      type="button"
      class="scripts-workbench-toolbar-btn"
      title="Add to flow"
      aria-label="Add to flow"
      disabled={!graphemeScriptEditor.activeTab?.body.trim()}
      onclick={addActiveScriptToFlow}
    >
      <GitBranchPlus size={15} strokeWidth={1.75} />
    </button>
    <button
      type="button"
      class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary"
      title={graphemeScriptEditor.saveBusy ? "Saving…" : `Save (${formatShortcut("S")})`}
      aria-label="Save script"
      disabled={graphemeScriptEditor.saveBusy || !graphemeScriptEditor.activeTab}
      onclick={() => void saveActive()}
    >
      <Save size={15} strokeWidth={1.75} />
    </button>
    <button
      type="button"
      class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-run"
      title={workshop.runBusy ? "Running…" : `Run (${formatShortcut("Enter")})`}
      aria-label="Run script"
      disabled={workshop.runBusy || !graphemeScriptEditor.activeTab?.body.trim()}
      onclick={() => void runActive()}
    >
      <Play size={15} strokeWidth={1.75} />
    </button>
    <button
      type="button"
      class="scripts-workbench-toolbar-btn"
      title={graphemeScriptEditor.compileBusy
        ? "Compiling…"
        : `Compile (${formatShortcut("B")})`}
      aria-label="Compile script"
      disabled={graphemeScriptEditor.compileBusy || !graphemeScriptEditor.activeTab?.body.trim()}
      onclick={() => void compileActive("check")}
    >
      <Hammer size={15} strokeWidth={1.75} />
    </button>
    <button
      type="button"
      class="scripts-workbench-toolbar-btn"
      title={graphemeScriptEditor.compileBusy ? "Optimizing…" : "Optimize (AOT)"}
      aria-label="Optimize with AOT compile"
      disabled={graphemeScriptEditor.compileBusy || !graphemeScriptEditor.activeTab?.body.trim()}
      onclick={() => void compileActive("aot")}
    >
      <Zap size={15} strokeWidth={1.75} />
    </button>

    <GraphemeModuleLibraryPicker />

    {#if mobile}
      <button
        type="button"
        class="scripts-workbench-toolbar-btn {consoleOpen ? 'scripts-workbench-toolbar-btn-active' : ''}"
        title="Output"
        aria-label="Show output"
        onclick={onToggleConsole}
      >
        <Terminal size={15} strokeWidth={1.75} />
      </button>
    {:else}
      <span class="mx-0.5 h-4 w-px shrink-0 bg-surface-500/40" aria-hidden="true"></span>
      <button
        type="button"
        class="scripts-workbench-toolbar-btn {consoleOpen ? 'scripts-workbench-toolbar-btn-active' : ''}"
        title="{consoleOpen ? 'Hide' : 'Show'} output"
        aria-label="{consoleOpen ? 'Hide' : 'Show'} output panel"
        aria-pressed={consoleOpen}
        onclick={onToggleConsole}
      >
        <Terminal size={15} strokeWidth={1.75} />
      </button>
      <button
        type="button"
        class="scripts-workbench-toolbar-btn {chatOpen ? 'scripts-workbench-toolbar-btn-active' : ''}"
        title="{chatOpen ? 'Hide' : 'Show'} chat"
        aria-label="{chatOpen ? 'Hide' : 'Show'} script chat"
        aria-pressed={chatOpen}
        onclick={onToggleChat}
      >
        {#if chatOpen}
          <PanelRightClose size={15} strokeWidth={1.75} />
        {:else}
          <MessageSquare size={15} strokeWidth={1.75} />
        {/if}
      </button>
    {/if}
  </div>
</div>

{#if flowError}
  <p class="shrink-0 border-b border-surface-500/35 px-3 py-1 text-[10px] text-error-400">
    {flowError}
  </p>
{/if}
