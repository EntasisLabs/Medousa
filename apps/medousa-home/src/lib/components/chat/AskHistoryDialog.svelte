<script lang="ts">
  import TaskDialogFrame from "$lib/components/tasks/TaskDialogFrame.svelte";
  import ChatAsksPanel from "./ChatAsksPanel.svelte";
  import { taskOverlays } from "$lib/runtime/taskOverlayState.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  let { sessionId }: { sessionId: string } = $props();
  function close() { taskOverlays.askHistorySessionId = null; }
</script>

<TaskDialogFrame label="Conversation asks" mobile={layout.isMobile} onClose={close}>
  <div class="flex shrink-0 justify-end border-b border-surface-700/30 px-4 py-2">
    <button class="workshop-text-action text-xs" aria-label="Close asks" onclick={close}>Close</button>
  </div>
  <ChatAsksPanel {sessionId} />
</TaskDialogFrame>
