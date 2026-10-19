<script lang="ts">
  import { untrack } from "svelte";
  import type { ChatHistoryController } from "../chatHistoryController.svelte";
  import { useChatHistoryPanel } from "../chatHistoryPanel.svelte";
  let { history, sessionId, visible }: { history: ChatHistoryController; sessionId: string; visible: boolean } = $props();
  const reader = useChatHistoryPanel(untrack(() => history), () => sessionId, () => visible);
</script>
<p>{reader.messages().length} messages</p>
