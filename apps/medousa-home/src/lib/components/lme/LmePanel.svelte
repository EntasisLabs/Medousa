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
    onOpenChat: () => void;
    onOpenWork: () => void;
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let { visible, onOpenChat, onOpenWork, onSelectCard }: Props = $props();

  onMount(() => {
    const pending = automationsNav.consumeSection();
    if (pending) {
      lmeWorkspace.openAutomationsSection(pending);
    }
  });

  // Keep script tab titles fresh — never force-activate (mode bar must not steal focus).
  $effect(() => {
    if (!visible) return;
    const scriptTabs = graphemeScriptEditor.tabs;
    const activeId = graphemeScriptEditor.activeTabId;
    void scriptTabs;
    void activeId;
    untrack(() => {
      lmeWorkspace.syncScriptTabFromEditor({ activate: false });
    });
  });

  $effect(() => {
    if (!visible || lmeWorkspace.explorerMode !== "scripts") return;
    void workshop.refreshModulesAndScripts();
  });
</script>

<section
  class="lme-panel flex h-full min-h-0 min-w-0 flex-1 {visible ? '' : 'hidden'}"
  data-debug-label="lme-panel"
  aria-label="Workspace"
>
  <LmeEditorHost
    {visible}
    {onOpenChat}
    {onOpenWork}
    {onSelectCard}
  />
</section>

<VaultNewNoteDialog />
<VaultNewGroupDialog />
{#if visible}
  <ConnectionsInviteSheet />
{/if}
