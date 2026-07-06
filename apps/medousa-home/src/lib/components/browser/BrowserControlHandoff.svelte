<script lang="ts">
  import { browser } from "$lib/stores/browser.svelte";

  interface Props {
    /** Inline pill for workshop headers; banner for main browser chrome. */
    variant?: "banner" | "inline";
    compact?: boolean;
  }

  let { variant = "banner", compact = false }: Props = $props();

  const resolvedVariant = $derived(compact ? "inline" : variant);

  const label = $derived.by(() => {
    switch (browser.control) {
      case "agent":
        return "Medousa is exploring";
      case "awaiting_operator":
        return "Verification needed";
      default:
        return "You are browsing";
    }
  });
</script>

{#if resolvedVariant === "inline"}
  <div class="flex items-center gap-2 text-[11px]">
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
{:else if browser.control === "agent"}
  <div class="browser-agent-banner" role="status">
    <span class="browser-agent-banner-label">
      <span class="browser-agent-banner-dot" aria-hidden="true"></span>
      Medousa is exploring
    </span>
    <button
      type="button"
      class="btn btn-xs variant-soft-primary shrink-0"
      onclick={() => browser.takeControl()}
    >
      Take control
    </button>
  </div>
{/if}
