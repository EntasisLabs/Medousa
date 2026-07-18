<script lang="ts">
  import ScriptWorkbenchChatPanel from "$lib/components/automations/ScriptWorkbenchChatPanel.svelte";
  import ScriptWorkbenchStatusBar from "$lib/components/automations/ScriptWorkbenchStatusBar.svelte";
  import ScriptWorkbenchTitlebar from "$lib/components/automations/ScriptWorkbenchTitlebar.svelte";
  import GraphemeRunResultCard from "$lib/components/grapheme/GraphemeRunResultCard.svelte";
  import GraphemeScriptEditorPanel from "$lib/components/grapheme/GraphemeScriptEditorPanel.svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";

  interface Props {
    visible?: boolean;
  }

  let { visible = true }: Props = $props();

  let consoleOpen = $state(true);
  let chatOpen = $state(false);
</script>

<div
  class="lme-script-editor relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
  data-debug-label="lme-script-editor"
>
  <ScriptWorkbenchTitlebar
    leftOpen={true}
    {consoleOpen}
    {chatOpen}
    hideTabStrip={true}
    onShowSidebar={() => {}}
    onToggleConsole={() => (consoleOpen = !consoleOpen)}
    onToggleChat={() => (chatOpen = !chatOpen)}
  />

  <div class="flex min-h-0 flex-1 overflow-hidden">
    <div class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <GraphemeScriptEditorPanel {visible} workbenchMode />
      {#if consoleOpen}
        <div class="scripts-workbench-console shrink-0 border-t border-surface-500/40">
          <div class="flex items-center justify-between gap-2 px-3 py-1.5">
            <p class="workshop-label text-[10px]">Output</p>
            <button
              type="button"
              class="workshop-text-action text-[10px]"
              onclick={() => (consoleOpen = false)}
            >
              Hide
            </button>
          </div>
          <div class="max-h-40 overflow-y-auto px-3 pb-3">
            {#if graphemeScriptEditor.compileError}
              <p class="text-xs text-error-400">{graphemeScriptEditor.compileError}</p>
            {:else if graphemeScriptEditor.compileResult}
              <div class="space-y-1 text-[11px] text-surface-300">
                {#each graphemeScriptEditor.compileResult.compile_hints as hint (hint)}
                  <p>{hint}</p>
                {/each}
                {#each graphemeScriptEditor.compileResult.lint_warnings as warning (warning)}
                  <p class="text-warning-400">{warning}</p>
                {/each}
              </div>
            {/if}
            <GraphemeRunResultCard
              result={workshop.runResult?.result}
              error={workshop.runError}
              emptyMessage="Run or compile to see output here."
            />
          </div>
        </div>
      {/if}
    </div>

    {#if chatOpen}
      <ScriptWorkbenchChatPanel
        visible={visible}
        onOpenFullChat={() => layout.navigateDesktop("chat", { bump: true })}
      />
    {/if}
  </div>

  <ScriptWorkbenchStatusBar onToggleConsole={() => (consoleOpen = true)} />
</div>
