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
    class="shell-tab-strip flex min-w-0 shrink-0 items-center gap-0.5 overflow-x-auto border-b border-surface-500/40 bg-surface-950/60 px-1.5 pt-1"
    role="tablist"
    aria-label="Open tabs"
    data-debug-label="shell-tab-strip"
    data-group-id={groupId}
  >
    {#each tabs as tab (tab.id)}
      {@const active = group?.activeTabId === tab.id}
      {@const Icon = iconFor(tab)}
      <div
        class="group flex max-w-[200px] shrink-0 items-center gap-1 rounded-t-md border border-b-0 px-2 py-1 text-[11px]
          {active
          ? 'border-surface-500/55 bg-surface-900 text-primary-300'
          : 'border-transparent text-surface-400 hover:bg-surface-800/70'}"
        role="presentation"
      >
        <button
          type="button"
          role="tab"
          aria-selected={active}
          class="flex min-w-0 flex-1 items-center gap-1 text-left"
          title={tab.title}
          onclick={() => void shellTabs.activate(tab.id)}
        >
          <Icon size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
          <span class="truncate">{tab.title}</span>
        </button>
        <button
          type="button"
          class="rounded p-0.5 opacity-0 transition-opacity hover:bg-surface-700 group-hover:opacity-100 focus:opacity-100"
          aria-label="Close {tab.title}"
          onclick={() => shellTabs.close(tab.id)}
        >
          <X size={11} strokeWidth={2} />
        </button>
      </div>
    {/each}
  </div>
{/if}
