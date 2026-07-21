<script lang="ts">
  import {
    Bot,
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
  }

  let { groupId }: Props = $props();

  const tabs = $derived(shellTabs.tabsForGroup(groupId));
  const group = $derived(shellTabs.groups.find((entry) => entry.id === groupId));

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
    return FileText;
  }
</script>

{#if tabs.length > 0}
  <div
    class="shell-tab-strip flex h-7 w-max max-w-full min-w-0 items-center gap-0.5 overflow-x-auto rounded-b-md px-1.5
      bg-surface-950/90 backdrop-blur-sm"
    role="tablist"
    aria-label="Open tabs"
    data-debug-label="shell-tab-strip"
    data-group-id={groupId}
  >
    {#each tabs as tab (tab.id)}
      {@const active = group?.activeTabId === tab.id}
      {@const Icon = iconFor(tab)}
      <div
        class="group flex h-5 max-w-[180px] shrink-0 cursor-grab items-center gap-1 px-1.5 text-[11px] leading-none active:cursor-grabbing
          {active
          ? 'rounded-md bg-surface-700/90 text-surface-100'
          : 'rounded-md text-surface-400 hover:bg-surface-800/60 hover:text-surface-200'}"
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
          <Icon size={11} strokeWidth={1.75} class="shrink-0 opacity-65" />
          <span class="truncate">{tab.title}</span>
        </button>
        <button
          type="button"
          class="rounded p-0.5 opacity-0 transition-opacity hover:bg-surface-600/80 group-hover:opacity-100 focus:opacity-100"
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
{/if}
