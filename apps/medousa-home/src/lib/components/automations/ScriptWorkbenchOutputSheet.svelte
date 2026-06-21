<script lang="ts">
  import GraphemeRunResultCard from "$lib/components/grapheme/GraphemeRunResultCard.svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="mobile-sheet-backdrop scripts-workbench-output-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <div
      class="mobile-sheet mobile-sheet-tall scripts-workbench-output-sheet"
      role="dialog"
      aria-label="Script output"
    >
      <header class="mobile-sheet-header scripts-workbench-sheet-header">
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <h2 class="text-sm font-medium text-surface-100">Output</h2>
        <button type="button" class="workshop-text-action text-xs" onclick={onClose}>
          Done
        </button>
      </header>
      <div class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto px-3 pb-4 pt-2">
        {#if graphemeScriptEditor.compileError}
          <p class="text-xs text-error-400">{graphemeScriptEditor.compileError}</p>
        {:else if graphemeScriptEditor.compileResult}
          <div class="space-y-1 text-[11px] text-surface-300">
            <p class="font-medium text-surface-100">
              {graphemeScriptEditor.compileResult.mode} ·
              {graphemeScriptEditor.compileResult.validated ? "valid" : "invalid"}
            </p>
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
  </div>
{/if}
