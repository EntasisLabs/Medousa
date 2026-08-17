<script lang="ts">
  import "$lib/styles/lme.postcss";
  import { onMount, untrack } from "svelte";
  import LazyFeatureView from "$lib/components/layout/LazyFeatureView.svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import ConnectionsInviteSheet from "$lib/components/lme/ConnectionsInviteSheet.svelte";
  import EmptyState from "$lib/components/ui/EmptyState.svelte";
  import VaultEmptyState from "$lib/components/vault/VaultEmptyState.svelte";
  import VaultNewGroupDialog from "$lib/components/vault/VaultNewGroupDialog.svelte";
  import VaultNewNoteDialog from "$lib/components/vault/VaultNewNoteDialog.svelte";
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import { loadLmeEditorHost } from "$lib/runtime/viewLoaders";
  import { isLmeLibraryMode } from "$lib/utils/lmeExplorerModes";

  interface Props {
    visible: boolean;
    /** Focused pane — owns dialogs / hotkeys; background panes still render. */
    interactive?: boolean;
    /** Shell LME tab id — binds this pane to a specific Workspace tab (multi-pane). */
    lmeTabId?: string | null;
    /** Surface landing pages stay empty until their own explorer opens a tab. */
    useActiveTabWhenUnbound?: boolean;
    emptyMessage?: string;
    onOpenChat: () => void;
    onOpenWork: () => void;
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let {
    visible,
    interactive = true,
    lmeTabId = null,
    useActiveTabWhenUnbound = true,
    emptyMessage = "Open something from the side panel.",
    onOpenChat,
    onOpenWork,
    onSelectCard,
  }: Props = $props();

  onMount(() => {
    if (!interactive) return;
    const pending = automationsNav.consumeSection();
    if (pending) {
      lmeWorkspace.openAutomationsSection(pending);
    }
  });

  // Keep script tab titles fresh — never force-activate (mode bar must not steal focus).
  $effect(() => {
    if (!visible || !interactive) return;
    const scriptTabs = graphemeScriptEditor.tabs;
    const activeId = graphemeScriptEditor.activeTabId;
    void scriptTabs;
    void activeId;
    untrack(() => {
      lmeWorkspace.syncScriptTabFromEditor({ activate: false });
    });
  });

  $effect(() => {
    if (!visible || !interactive || lmeWorkspace.explorerMode !== "scripts") return;
    void workshop.refreshModulesAndScripts();
  });

  const hasEditorTarget = $derived.by(() => {
    const id = lmeTabId?.trim();
    if (id) return lmeWorkspace.tabs.some((tab) => tab.tabId === id);
    return useActiveTabWhenUnbound && lmeWorkspace.activeTab != null;
  });
</script>

<section
  class="lme-panel flex h-full min-h-0 min-w-0 max-w-full flex-1 overflow-hidden {visible
    ? ''
    : 'hidden'}"
  data-debug-label="lme-panel"
  aria-label="Workspace"
>
  {#if hasEditorTarget}
    <LazyFeatureView
      loader={loadLmeEditorHost}
      {visible}
      {interactive}
      {lmeTabId}
      {useActiveTabWhenUnbound}
      {emptyMessage}
      {onOpenChat}
      {onOpenWork}
      {onSelectCard}
    />
  {:else}
    <div class="flex flex-1 flex-col">
      {#if !layout.shellSidebarExpanded}
        <div class="flex items-center px-2 pt-1.5">
          <ShellSidebarExpandButton label="Show workspace browser" />
        </div>
      {/if}
      {#if isLmeLibraryMode(lmeWorkspace.explorerMode)}
        <div class="flex min-h-0 flex-1 overflow-auto">
          <VaultEmptyState />
        </div>
      {:else if lmeWorkspace.explorerMode === "code"}
        <div class="flex flex-1 items-center justify-center p-8">
          <EmptyState
            title="Open a file"
            description="Pick a project from the side panel, then a path from the tree."
          />
        </div>
      {:else}
        <div class="flex flex-1 items-center justify-center p-8">
          <EmptyState
            title={emptyMessage}
            description="Use the side panel to open something here."
          />
        </div>
      {/if}
    </div>
  {/if}
</section>

{#if interactive}
  <VaultNewNoteDialog />
  <VaultNewGroupDialog />
  {#if visible}
    <ConnectionsInviteSheet />
  {/if}
{/if}
