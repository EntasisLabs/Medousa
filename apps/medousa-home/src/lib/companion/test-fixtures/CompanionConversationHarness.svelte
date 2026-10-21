<script lang="ts">
  import { CompanionConversation } from "../companionConversation.svelte";
  import CompanionConversationView from "$lib/components/brand/CompanionConversation.svelte";
  let { visible = true, connected = true, expose, actions = [], onBodyChange = () => {} }: { visible?: boolean; connected?: boolean; expose: (value: CompanionConversation) => void; actions?: import("../companionResults").CompanionAction[]; onBodyChange?: (value: boolean) => void } = $props();
  const model = new CompanionConversation(() => visible, () => connected);
  $effect(() => expose(model));
  export function setVisible(value: boolean) { visible = value; }
</script>
{#if visible}<CompanionConversationView {model} {connected} {actions} {onBodyChange} onOpen={() => {}} onClipboard={() => {}} onCollapse={() => {}}/>{/if}
