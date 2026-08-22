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
  const groupedHits = $derived.by(() => {
    const groups = new Map<
      string,
      { label: string; items: Array<{ hit: ShellTabSearchHit; index: number }> }
    >();
    hits.forEach((hit, index) => {
      const key = `${hit.desktopId}:${hit.paneIndex}`;
      const group = groups.get(key) ?? {
        label: `${hit.desktopName} · Pane ${hit.paneIndex}`,
        items: [],
      };
      group.items.push({ hit, index });
      groups.set(key, group);
    });
    return [...groups.entries()].map(([key, group]) => ({ key, ...group }));
  });

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
      {#each groupedHits as group (group.key)}
        <section class="shell-tab-notch-search-group" aria-label={group.label}>
          <div class="shell-tab-notch-search-group-label">{group.label}</div>
          {#each group.items as item (item.hit.desktopId + ":" + item.hit.tabId)}
            {@const hit = item.hit}
            {@const index = item.index}
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
              <span class="shell-tab-notch-search-meta">{tabKindLabel(hit.kind)}</span>
            </button>
          {/each}
        </section>
      {/each}
    </div>
  {/if}
</div>
