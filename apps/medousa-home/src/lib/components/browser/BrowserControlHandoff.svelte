<script lang="ts">
  import { browser } from "$lib/stores/browser.svelte";

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  const label = $derived.by(() => {
    switch (browser.control) {
      case "agent":
        return "Medousa is browsing";
      case "awaiting_operator":
        return "Verification needed";
      default:
        return "You are browsing";
    }
  });
</script>

<div
  class="{compact
    ? 'flex items-center gap-2 text-[11px]'
    : 'flex items-center gap-2 rounded-md border border-surface-700/80 bg-surface-900/60 px-2 py-1 text-xs'}"
>
  <span
    class="inline-block h-2 w-2 rounded-full {browser.control === 'agent'
      ? 'bg-primary-400'
      : browser.control === 'awaiting_operator'
        ? 'bg-amber-400'
        : 'bg-emerald-400'}"
    aria-hidden="true"
  ></span>
  <span class="text-surface-200">{label}</span>
  {#if browser.control === "agent"}
    <button type="button" class="btn btn-xs variant-soft-primary" onclick={() => browser.takeControl()}>
      Take control
    </button>
  {:else if browser.control === "user"}
    <button
      type="button"
      class="btn btn-xs variant-soft-surface"
      onclick={() => browser.handBackToAgent()}
    >
      Hand back
    </button>
  {/if}
</div>
