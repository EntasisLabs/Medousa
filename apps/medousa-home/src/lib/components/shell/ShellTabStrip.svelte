<script lang="ts">
  import {
    Bot,
    CalendarClock,
    ChevronLeft,
    ChevronRight,
    FileCode2,
    FileText,
    Files,
    GitBranch,
    Globe,
    LayoutGrid,
    MessageSquare,
    Presentation,
    X,
  } from "@lucide/svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import type { ShellTab } from "$lib/types/shellTabs";
  import { beginShellTabDrag } from "$lib/utils/shellTabDrag";

  interface Props {
    groupId: string;
    /** titlebar = always-on unified chrome; default = hover/web strip. */
    variant?: "default" | "titlebar";
  }

  let { groupId, variant = "default" }: Props = $props();

  const tabs = $derived(shellTabs.tabsForGroup(groupId));
  const group = $derived(shellTabs.groups.find((entry) => entry.id === groupId));
  const activeTabId = $derived(group?.activeTabId ?? null);
  const isTitlebar = $derived(variant === "titlebar");

  let scrollerEl: HTMLDivElement | undefined = $state();
  let canScrollLeft = $state(false);
  let canScrollRight = $state(false);

  function iconFor(tab: ShellTab) {
    if (tab.kind === "chat") return MessageSquare;
    if (tab.kind === "web") return Globe;
    if (tab.kind === "surface") return LayoutGrid;
    const lme = lmeWorkspace.tabs.find((entry) => entry.tabId === tab.lmeTabId);
    if (!lme) return FileText;
    if (lme.kind === "script") return FileCode2;
    if (lme.kind === "file") return Files;
    if (lme.kind === "deck") return Presentation;
    if (lme.kind === "manuscript") return Bot;
    if (lme.kind === "flow") return GitBranch;
    if (lme.kind === "schedule") return CalendarClock;
    return FileText;
  }

  function updateScrollState() {
    const el = scrollerEl;
    if (!el) {
      canScrollLeft = false;
      canScrollRight = false;
      return;
    }
    const { scrollLeft, scrollWidth, clientWidth } = el;
    canScrollLeft = scrollLeft > 1;
    canScrollRight = scrollLeft + clientWidth < scrollWidth - 1;
  }

  function scrollByDir(dir: -1 | 1) {
    scrollerEl?.scrollBy({ left: dir * 140, behavior: "smooth" });
  }

  function scrollActiveIntoView() {
    const el = scrollerEl;
    if (!el || !activeTabId) return;
    const chip = el.querySelector<HTMLElement>(`[data-tab-id="${activeTabId}"]`);
    chip?.scrollIntoView({ inline: "nearest", block: "nearest" });
    updateScrollState();
  }

  $effect(() => {
    void tabs.length;
    void activeTabId;
    requestAnimationFrame(() => {
      scrollActiveIntoView();
      updateScrollState();
    });
  });

  $effect(() => {
    const el = scrollerEl;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => updateScrollState());
    ro.observe(el);
    return () => ro.disconnect();
  });
</script>

{#if tabs.length > 0}
  <div
    class="shell-tab-strip flex min-w-0 items-center gap-0.5"
    class:w-full={!isTitlebar}
    class:shell-tab-strip--default={!isTitlebar}
    class:shell-tab-strip--titlebar={isTitlebar}
    role="tablist"
    aria-label="Open tabs"
    data-debug-label="shell-tab-strip"
    data-group-id={groupId}
  >
    {#if canScrollLeft}
      <button
        type="button"
        class="shell-tab-strip-nav shrink-0"
        title="Earlier tabs"
        aria-label="Scroll to earlier tabs"
        onclick={() => scrollByDir(-1)}
      >
        <ChevronLeft size={12} strokeWidth={2} />
      </button>
    {/if}

    <div
      bind:this={scrollerEl}
      class="shell-tab-strip-scroll flex min-w-0 items-center gap-0.5 overflow-x-auto px-0.5
        {isTitlebar ? 'max-w-full' : 'flex-1'}"
      onscroll={updateScrollState}
    >
      {#each tabs as tab (tab.id)}
        {@const active = activeTabId === tab.id}
        {@const Icon = iconFor(tab)}
        <div
          data-tab-id={tab.id}
          class="shell-tab-chip group flex shrink-0 cursor-grab items-center gap-1 leading-none active:cursor-grabbing
            {active ? 'shell-tab-chip--active' : 'shell-tab-chip--idle'}"
          role="presentation"
          onpointerdown={(event) => beginShellTabDrag(event, tab.id, groupId)}
        >
          <button
            type="button"
            role="tab"
            aria-selected={active}
            class="pointer-events-none flex min-w-0 flex-1 items-center gap-1 text-left"
            title="{tab.title} — drag to another pane"
            tabindex={-1}
          >
            <Icon
              size={isTitlebar ? 12 : 11}
              strokeWidth={1.75}
              class="shrink-0 opacity-65"
            />
            <span class="truncate">{tab.title}</span>
          </button>
          <button
            type="button"
            class="shell-tab-close rounded p-0.5 opacity-0 transition-opacity hover:bg-surface-600/80 group-hover:opacity-100 focus:opacity-100"
            aria-label="Close {tab.title}"
            onclick={(event) => {
              event.stopPropagation();
              shellTabs.close(tab.id);
            }}
            onpointerdown={(event) => event.stopPropagation()}
          >
            <X size={10} strokeWidth={2} />
          </button>
        </div>
      {/each}
    </div>

    {#if canScrollRight}
      <button
        type="button"
        class="shell-tab-strip-nav shrink-0"
        title="More tabs"
        aria-label="Scroll to more tabs"
        onclick={() => scrollByDir(1)}
      >
        <ChevronRight size={12} strokeWidth={2} />
      </button>
    {/if}
  </div>
{/if}

<style>
  .shell-tab-strip--default {
    height: 1.75rem;
    border-radius: 0 0 0.375rem 0.375rem;
    padding: 0 0.125rem;
    background: rgb(var(--color-surface-950) / 0.9);
    backdrop-filter: blur(8px);
  }

  .shell-tab-strip--titlebar {
    height: 100%;
    max-width: min(100%, 70vw);
    flex: 0 1 auto;
    align-items: center;
    background: transparent;
  }

  .shell-tab-strip--titlebar .shell-tab-strip-scroll {
    flex: 0 1 auto;
    height: 100%;
    align-items: center;
    gap: 1px;
    padding: 0;
  }

  .shell-tab-chip {
    max-width: 180px;
    height: 1.25rem;
    padding: 0 0.375rem;
    font-size: 0.6875rem;
    border-radius: 0.375rem;
  }

  /* Cursor-flat title tabs: slim chips in a ~30px bar. */
  .shell-tab-strip--titlebar .shell-tab-chip {
    height: 22px;
    max-width: 200px;
    padding: 0 8px;
    gap: 5px;
    font-size: 12px;
    line-height: 1;
    border-radius: 5px;
  }

  .shell-tab-chip--active {
    background: rgb(var(--color-surface-700) / 0.85);
    color: rgb(var(--color-surface-100));
  }

  .shell-tab-strip--titlebar .shell-tab-chip--idle {
    background: transparent;
    color: rgb(var(--color-surface-400));
  }

  .shell-tab-chip--idle {
    color: rgb(var(--color-surface-400));
  }

  .shell-tab-chip--idle:hover {
    background: rgb(var(--color-surface-800) / 0.55);
    color: rgb(var(--color-surface-200));
  }

  .shell-tab-strip--titlebar .shell-tab-chip--idle:hover {
    background: rgb(var(--color-surface-800) / 0.45);
  }

  .shell-tab-strip-scroll {
    scrollbar-width: none;
  }

  .shell-tab-strip-scroll::-webkit-scrollbar {
    display: none;
  }

  .shell-tab-strip-nav {
    display: inline-flex;
    height: 1.25rem;
    width: 1.25rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.25rem;
    color: rgb(var(--color-surface-400));
    transition:
      color 120ms ease,
      background 120ms ease;
  }

  .shell-tab-strip-nav:hover {
    background: rgb(var(--color-surface-800) / 0.85);
    color: rgb(var(--color-surface-100));
  }
</style>
