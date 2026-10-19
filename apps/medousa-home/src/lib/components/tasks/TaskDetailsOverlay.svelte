<script lang="ts">
  import "$lib/styles/work.postcss";
  import { X } from "@lucide/svelte";
  import TaskDialogFrame from "./TaskDialogFrame.svelte";
  import TaskDetails from "./TaskDetails.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { focusChatComposer, openVaultNote } from "$lib/runtime/shellUseCases";

  function close() { workspace.clearSelection(); }
  function openNote(path: string) { close(); void openVaultNote(path); }
  function openChat() { close(); focusChatComposer(); }
</script>

<TaskDialogFrame label="Task details" mobile={layout.isMobile} onClose={close}>
  {#if layout.isMobile}
    <header class="shrink-0 border-b border-surface-500/40 px-4 py-3">
      <button class="workshop-text-action text-sm" onclick={close}>← Back</button>
    </header>
  {:else}
    <button class="task-close" aria-label="Close task details" onclick={close}><X size={18} /></button>
  {/if}
  <TaskDetails popover={!layout.isMobile} mobile={layout.isMobile} onOpenNote={openNote} onOpenChat={openChat} onClose={close} />
</TaskDialogFrame>

<style>
  .task-close { position:absolute; right:12px; top:12px; z-index:1; display:flex; align-items:center; justify-content:center; width:32px; height:32px; border-radius:6px; color:rgb(var(--theme-text-tertiary)); }
  .task-close:hover { background:rgb(var(--color-surface-800)); }
</style>
