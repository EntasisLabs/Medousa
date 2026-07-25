<script lang="ts">
  /**
   * Quiet tab finder for the notch — searches every virtual desktop / pane.
   */
  import {
    FileText,
    Globe,
    LayoutGrid,
    MessageSquare,
  } from "@lucide/svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import {
    filterTabSearchHits,
    tabKindLabel,
    type ShellTabSearchHit,
  } from "$lib/utils/shellTabSearch";
  import { tick } from "svelte";

  interface Props {
    query?: string;
    onPick?: () => void;
  }

  let { query = $bindable(""), onPick }: Props = $props();

  let listEl = $state<HTMLDivElement | null>(null);
  let highlight = $state(0);

  const hits = $derived(filterTabSearchHits(shellTabs.collectSearchHits(), query));

  $effect(() => {
    void hits.length;
    void query;
    highlight = 0;
  });

  function iconFor(hit: ShellTabSearchHit) {
    if (hit.kind === "chat") return MessageSquare;
    if (hit.kind === "web") return Globe;
    if (hit.kind === "surface") return LayoutGrid;
    return FileText;
  }

  function metaFor(hit: ShellTabSearchHit): string {
    const bits = [hit.desktopName, `Pane ${hit.paneIndex}`, tabKindLabel(hit.kind)];
    return bits.join(" · ");
  }

  async function pick(hit: ShellTabSearchHit) {
    const ok = await shellTabs.revealSearchHit(hit.desktopId, hit.tabId);
    if (ok) onPick?.();
  }

  export async function moveHighlight(delta: number) {
    if (hits.length === 0) return;
    highlight = (highlight + delta + hits.length) % hits.length;
    await tick();
    const row = listEl?.querySelector<HTMLElement>(`[data-hit-index="${highlight}"]`);
    row?.scrollIntoView({ block: "nearest" });
  }

  export async function confirmHighlight() {
    const hit = hits[highlight];
    if (hit) await pick(hit);
  }
</script>

<div class="shell-tab-notch-search" role="listbox" aria-label="Search tabs">
  {#if hits.length === 0}
    <div class="shell-tab-notch-search-empty">
      {query.trim() ? "No tabs match" : "No open tabs"}
    </div>
  {:else}
    <div bind:this={listEl} class="shell-tab-notch-search-list">
      {#each hits as hit, index (hit.desktopId + ":" + hit.tabId)}
        {@const Icon = iconFor(hit)}
        <button
          type="button"
          role="option"
          data-hit-index={index}
          class="shell-tab-notch-search-row"
          class:shell-tab-notch-search-row--active={hit.isActive}
          class:shell-tab-notch-search-row--hl={index === highlight}
          aria-selected={index === highlight}
          onclick={() => void pick(hit)}
          onpointerenter={() => {
            highlight = index;
          }}
        >
          <Icon size={13} strokeWidth={1.75} class="shell-tab-notch-search-icon" aria-hidden="true" />
          <span class="shell-tab-notch-search-title">{hit.title}</span>
          <span class="shell-tab-notch-search-meta">{metaFor(hit)}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>
