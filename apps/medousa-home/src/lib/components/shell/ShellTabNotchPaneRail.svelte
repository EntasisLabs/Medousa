<script lang="ts">
  /**
   * Arc/Zen-style vertical tab rail for a notch map pane.
   */
  import {
    Bot,
    CalendarClock,
    Code2,
    FileCode2,
    FileText,
    Files,
    GitBranch,
    Globe,
    LayoutGrid,
    MessageSquare,
    Presentation,
    SquareTerminal,
    X,
  } from "@lucide/svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { shellContextMenu } from "$lib/stores/shellContextMenu.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import type { ShellTab } from "$lib/types/shellTabs";
  import { beginShellTabDrag } from "$lib/utils/shellTabDrag";

  interface Props {
    groupId: string;
    onTabSettled?: (info: { tabId: string; didMove: boolean }) => void;
  }

  let { groupId, onTabSettled }: Props = $props();

  const tabs = $derived(shellTabs.tabsForGroup(groupId));
  const group = $derived(shellTabs.groups.find((entry) => entry.id === groupId));
  const activeTabId = $derived(group?.activeTabId ?? null);

  function iconFor(tab: ShellTab) {
    if (tab.kind === "chat") return MessageSquare;
    if (tab.kind === "web") return Globe;
    if (tab.kind === "surface") return LayoutGrid;
    if (tab.kind === "terminal") return SquareTerminal;
    const lme = lmeWorkspace.tabs.find((entry) => entry.tabId === tab.lmeTabId);
    if (!lme) return FileText;
    if (lme.kind === "script") return FileCode2;
    if (lme.kind === "file") return Files;
    if (lme.kind === "code") return Code2;
    if (lme.kind === "deck") return Presentation;
    if (lme.kind === "manuscript") return Bot;
    if (lme.kind === "flow") return GitBranch;
    if (lme.kind === "schedule") return CalendarClock;
    return FileText;
  }
</script>

{#if tabs.length === 0}
  <div class="shell-tab-notch-rail-vacant">
    <span>Drop a tab here</span>
  </div>
{:else}
  <ul class="shell-tab-notch-rail" role="tablist" aria-label="Pane tabs">
    {#each tabs as tab (tab.id)}
      {@const active = activeTabId === tab.id}
      {@const Icon = iconFor(tab)}
      <li class="shell-tab-notch-rail-item">
        <div
          data-tab-id={tab.id}
          class="shell-tab-notch-rail-row"
          class:shell-tab-notch-rail-row--active={active}
          role="presentation"
          onpointerdown={(event) =>
            beginShellTabDrag(event, tab.id, groupId, {
              onDragEnd: (didMove) => {
                onTabSettled?.({ tabId: tab.id, didMove });
              },
            })}
          oncontextmenu={(event) => {
            event.preventDefault();
            event.stopPropagation();
            shellContextMenu.showTab(event.clientX, event.clientY, tab.id, groupId, tab.title);
          }}
        >
          <button
            type="button"
            role="tab"
            aria-selected={active}
            class="shell-tab-notch-rail-main"
            title="{tab.title} — drag to another pane"
            tabindex={-1}
          >
            <Icon size={13} strokeWidth={1.75} class="shell-tab-notch-rail-icon" aria-hidden="true" />
            <span class="shell-tab-notch-rail-title">{tab.title}</span>
          </button>
          <button
            type="button"
            class="shell-tab-notch-rail-close"
            aria-label="Close {tab.title}"
            onclick={(event) => {
              event.stopPropagation();
              shellTabs.close(tab.id);
            }}
            onpointerdown={(event) => event.stopPropagation()}
          >
            <X size={11} strokeWidth={2} />
          </button>
        </div>
      </li>
    {/each}
  </ul>
{/if}
