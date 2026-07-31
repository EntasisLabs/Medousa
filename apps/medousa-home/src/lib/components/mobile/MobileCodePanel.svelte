<script lang="ts">
  import { ChevronLeft, Code2 } from "@lucide/svelte";
  import LmeCodeExplorer from "$lib/components/lme/explorers/LmeCodeExplorer.svelte";
  import UndertakingsPanel from "$lib/components/work/UndertakingsPanel.svelte";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { undertakings } from "$lib/stores/undertakings.svelte";

  function showWorkspaces() {
    void undertakings.select("");
  }

  $effect(() => {
    if (!undertakings.selectedId) return;
    return registerMobileBackHandler(() => {
      showWorkspaces();
      return true;
    });
  });
</script>

<section class="flex h-full min-h-0 flex-col bg-surface-950" aria-label="Code projects">
  {#if undertakings.selectedId && undertakings.detail}
    <header class="flex shrink-0 items-center gap-2 border-b border-surface-500/30 px-3 py-2">
      <button
        type="button"
        class="rounded p-1 text-surface-400 hover:bg-surface-800 hover:text-surface-100"
        aria-label="Back to code projects"
        onclick={showWorkspaces}
      ><ChevronLeft size={17} /></button>
      <p class="text-sm font-medium text-surface-300">Projects</p>
    </header>
    <div class="min-h-0 flex-1 overflow-hidden">
      <UndertakingsPanel showBrowser={false} />
    </div>
  {:else}
    <header class="flex shrink-0 items-center gap-2 border-b border-surface-500/30 px-4 py-3">
      <Code2 size={17} class="text-primary-300" />
      <div>
        <h1 class="text-base font-semibold text-surface-50">Code</h1>
        <p class="text-[10px] text-surface-500">Make changes without losing your place</p>
      </div>
    </header>
    <div class="min-h-0 flex-1 overflow-hidden">
      <LmeCodeExplorer />
    </div>
  {/if}
</section>
