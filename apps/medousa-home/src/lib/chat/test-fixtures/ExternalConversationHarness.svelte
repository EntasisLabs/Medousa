<script lang="ts">
  import { untrack } from "svelte";
  import { createExternalConversationController } from "../externalConversationController.svelte";
  let { input, ready }: {
    input: Parameters<typeof createExternalConversationController>[0];
    ready: (controller: ReturnType<typeof createExternalConversationController>) => void;
  } = $props();
  untrack(() => ready(createExternalConversationController(input)));
</script>
