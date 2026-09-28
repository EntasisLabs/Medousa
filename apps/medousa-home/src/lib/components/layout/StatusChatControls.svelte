<script lang="ts">
  import ChatExecutionTargetPicker from "$lib/components/chat/ChatExecutionTargetPicker.svelte";
  import UndertakingContextChip from "$lib/components/work/UndertakingContextChip.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { connection } from "$lib/stores/connection.svelte";
</script>

<div class="status-chat-controls" aria-label="Chat project and workers">
  <UndertakingContextChip chatOnly composer />
  {#if chat.sessionId}
    <ChatExecutionTargetPicker sessionId={chat.sessionId} disabled={connection.offline || chat.composerBlocked} />
  {/if}
</div>

<style>
  .status-chat-controls {
    display: inline-flex;
    min-width: 0;
    max-width: 100%;
    align-items: center;
    gap: 0.35rem;
    overflow: hidden;
  }

  .status-chat-controls :global(.chat-runtime-trigger) {
    color: inherit;
    font-size: inherit !important;
    padding-block: 0.1rem !important;
  }
</style>
