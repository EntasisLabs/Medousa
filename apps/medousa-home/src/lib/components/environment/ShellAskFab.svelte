<script lang="ts">
  import { MessageCircle } from "@lucide/svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { haptic } from "$lib/haptics";

  interface Props {
    label?: string;
  }

  let { label = "Ask Medousa" }: Props = $props();

  function openAsk() {
    haptic("light");
    layout.openAskSheet();
  }
</script>

<button
  type="button"
  class="shell-ask-fab"
  aria-label={label}
  title={label}
  onclick={openAsk}
>
  <MessageCircle size={22} strokeWidth={2} />
</button>

<style>
  .shell-ask-fab {
    position: fixed;
    right: 1rem;
    bottom: calc(5.5rem + env(safe-area-inset-bottom, 0px));
    z-index: 40;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 3.25rem;
    height: 3.25rem;
    padding: 0;
    border: 0;
    border-radius: 999px;
    cursor: pointer;
    background: rgb(var(--color-primary-500));
    color: white;
    box-shadow: 0 12px 28px rgb(0 0 0 / 0.28);
  }
</style>
