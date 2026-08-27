<script lang="ts">
  import GraphemeRunResultCard from "$lib/components/grapheme/GraphemeRunResultCard.svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  function dismiss() {
    haptic("light");
    onClose();
  }

  $effect(() => {
    if (!open) return;
    return registerMobileBackHandler(() => {
      dismiss();
      return true;
    });
  });

  $effect(() => {
    if (!open || !sheetEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, { onDismiss: dismiss });
  });
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
      bind:this={sheetEl}
      class="mobile-sheet mobile-sheet-tall scripts-workbench-output-sheet"
      role="dialog"
      aria-label="Script output"
    >
      <header bind:this={headerEl} class="mobile-sheet-stack-header">
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <div class="mobile-sheet-header-row">
          <h2 class="text-sm font-medium text-surface-100">Output</h2>
          <button type="button" class="workshop-text-action text-xs" onclick={dismiss}>
            Done
          </button>
        </div>
      </header>
      <div class="mobile-sheet-scroll">
        {#if workshop.runBusy}
          <p class="mb-3 text-xs text-content-tertiary">Running…</p>
        {:else if graphemeScriptEditor.compileBusy}
          <p class="mb-3 text-xs text-content-tertiary">Compiling…</p>
        {/if}
        {#if graphemeScriptEditor.compileError}
          <p class="text-xs text-content-error">{graphemeScriptEditor.compileError}</p>
        {:else if graphemeScriptEditor.compileResult}
          <div class="space-y-1 text-[11px] text-content-secondary">
            <p class="font-medium text-surface-100">
              {graphemeScriptEditor.compileResult.mode} ·
              {graphemeScriptEditor.compileResult.validated ? "valid" : "invalid"}
            </p>
            {#each graphemeScriptEditor.compileResult.compile_hints as hint (hint)}
              <p>{hint}</p>
            {/each}
            {#each graphemeScriptEditor.compileResult.lint_warnings as warning (warning)}
              <p class="text-content-warning">{warning}</p>
            {/each}
          </div>
        {/if}
        <GraphemeRunResultCard
          result={workshop.runResult?.result}
          error={workshop.runError}
          emptyMessage={workshop.runBusy || graphemeScriptEditor.compileBusy
            ? ""
            : "Run or compile to see output here."}
        />
      </div>
    </div>
  </div>
{/if}
