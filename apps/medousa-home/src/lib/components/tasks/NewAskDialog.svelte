<script lang="ts">
  import "$lib/styles/work.postcss";
  import "$lib/styles/composer.postcss";
  import TaskDialogFrame from "./TaskDialogFrame.svelte";
  import AskComposer from "./AskComposer.svelte";
  import { taskOverlays } from "$lib/runtime/taskOverlayState.svelte";
  function close() { taskOverlays.newAskOpen = false; }
</script>

<TaskDialogFrame label="New ask" initialFocus="textarea" compact onClose={close}>
  <div class="p-5">
    <header class="mb-4 flex items-center justify-between">
      <h2 class="text-sm font-semibold">New ask</h2>
      <button class="workshop-text-action text-xs" aria-label="Close ask" onclick={close}>Esc</button>
    </header>
    <AskComposer autofocus onQueued={close} />
  </div>
</TaskDialogFrame>
