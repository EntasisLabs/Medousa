<script lang="ts">
  import { Bot, MessageSquare, Plus } from "@lucide/svelte";
  import type { ConversationTarget } from "$lib/spotlight/conversationTargets";
  let { targets, index, loading, error, onSelect, onHighlight, idPrefix = "spotlight", inline = false }: {
    targets: ConversationTarget[]; index: number; loading: boolean; error: string | null;
    onSelect: (target: ConversationTarget) => void; onHighlight: (index: number) => void;
    idPrefix?: string;
    inline?: boolean;
  } = $props();
  let list: HTMLDivElement;
  $effect(() => {
    const active = index;
    if (list) list.querySelector<HTMLElement>(`[data-target-index="${active}"]`)?.scrollIntoView({ block: "nearest" });
  });
</script>

<div class="command-spotlight-target-picker" class:command-spotlight-target-picker-inline={inline} id={`${idPrefix}-targets`} role="listbox" aria-label="Message destination" bind:this={list}>
  {#each targets as target, i (target.id)}
    {#if i === 0 || targets[i - 1].group !== target.group}<div class="command-spotlight-section-label" role="presentation">{target.group}</div>{/if}
    <button type="button" role="option" aria-selected={i === index} tabindex="-1" id={`${idPrefix}-target-${i}`} data-target-index={i}
      class="command-spotlight-row" class:command-spotlight-row-active={i === index}
      onpointermove={() => onHighlight(i)} onpointerdown={event => event.preventDefault()} onclick={() => onSelect(target)}>
      <span class="command-spotlight-row-icon" aria-hidden="true">
        {#if target.kind === "bot"}<Bot size={15}/>{:else if target.kind === "new"}<Plus size={15}/>{:else}<MessageSquare size={15}/>{/if}
      </span>
      <span class="command-spotlight-row-copy"><span class="command-spotlight-row-title">{target.label}</span><span class="command-spotlight-row-subtitle">{target.description}</span></span>
      <span class="command-spotlight-row-meta">{target.kind === "new" ? "New" : "Continue"}</span>
    </button>
  {:else}
    <p class="command-spotlight-empty">{loading ? "Loading destinations…" : "No matching destinations"}</p>
  {/each}
  {#if error}<p class="command-spotlight-target-notice" role="status">{error}</p>{/if}
</div>
