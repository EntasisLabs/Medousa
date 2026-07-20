<script lang="ts">
  import { onMount, untrack } from "svelte";
  import LmeEditorHost from "$lib/components/lme/LmeEditorHost.svelte";
  import ConnectionsInviteSheet from "$lib/components/lme/ConnectionsInviteSheet.svelte";
  import VaultNewGroupDialog from "$lib/components/vault/VaultNewGroupDialog.svelte";
  import VaultNewNoteDialog from "$lib/components/vault/VaultNewNoteDialog.svelte";
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";

  interface Props {
    visible: boolean;
    /** Focused pane — owns dialogs / hotkeys; background panes still render. */
    interactive?: boolean;
    /** Shell LME tab id — binds this pane to a specific Workspace tab (multi-pane). */
    lmeTabId?: string | null;
    onOpenChat: () => void;
    onOpenWork: () => void;
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let {
    visible,
    interactive = true,
    lmeTabId = null,
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
</script>

<section
  class="lme-panel flex h-full min-h-0 min-w-0 max-w-full flex-1 overflow-hidden {visible
    ? ''
    : 'hidden'}"
  data-debug-label="lme-panel"
  aria-label="Workspace"
>
  <LmeEditorHost
    {visible}
    {interactive}
    {lmeTabId}
    {onOpenChat}
    {onOpenWork}
    {onSelectCard}
  />
</section>

{#if interactive}
  <VaultNewNoteDialog />
  <VaultNewGroupDialog />
  {#if visible}
    <ConnectionsInviteSheet />
  {/if}
{/if}
