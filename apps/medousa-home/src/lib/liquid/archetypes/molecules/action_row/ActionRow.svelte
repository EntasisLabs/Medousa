<script lang="ts">
  /**
   * `action_row` molecule — a "what would you like next?" suggestion. Emits
   * `submit` carrying its intent, seeding the next turn (the visible intent loop).
   */
  import { ChevronRight } from "@lucide/svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const label = $derived(typeof node.props.label === "string" ? node.props.label : "");
  const emoji = $derived(typeof node.props.emoji === "string" ? node.props.emoji : "");
  const chevron = $derived(node.props.chevron !== false);

  function submit() {
    const intent = node.props.intent ?? label;
    ctx.sink?.emit(createSceneEvent(node.id, "submit", { intent }));
  }
</script>

<button type="button" class="liquid-action-row" onclick={submit}>
  {#if emoji}<span class="liquid-action-emoji" aria-hidden="true">{emoji}</span>{/if}
  <span class="liquid-action-label">{label}</span>
  {#if chevron}
    <ChevronRight class="liquid-action-chevron" size={15} aria-hidden="true" />
  {/if}
</button>

<style>
  .liquid-action-row {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    width: 100%;
    padding: 0.6rem 0.75rem;
    text-align: left;
    cursor: pointer;
    border-radius: 0.6rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 30%, transparent);
    background: color-mix(in srgb, var(--color-surface-800) 45%, transparent);
    color: rgb(var(--color-surface-100));
    transition: background 0.15s ease;
  }

  .liquid-action-row:hover {
    background: color-mix(in srgb, var(--color-surface-700) 55%, transparent);
  }

  .liquid-action-emoji {
    font-size: 1rem;
    line-height: 1;
    flex-shrink: 0;
  }

  .liquid-action-label {
    flex: 1 1 auto;
    min-width: 0;
    font-size: 0.8rem;
  }

  .liquid-action-row :global(.liquid-action-chevron) {
    flex-shrink: 0;
    color: rgb(var(--color-surface-400));
  }
</style>
