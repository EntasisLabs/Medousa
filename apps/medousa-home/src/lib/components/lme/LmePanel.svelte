<script lang="ts">
  import { onMount, untrack } from "svelte";
  import LmeEditorHost from "$lib/components/lme/LmeEditorHost.svelte";
  import LmeSidePanel from "$lib/components/lme/LmeSidePanel.svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import VaultNewGroupDialog from "$lib/components/vault/VaultNewGroupDialog.svelte";
  import VaultNewNoteDialog from "$lib/components/vault/VaultNewNoteDialog.svelte";
  import VaultSidebarCollapsedStrip from "$lib/components/vault/VaultSidebarCollapsedStrip.svelte";
  import { automationsNav } from "$lib/stores/automationsNav.svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { layout } from "$lib/stores/layout.svelte";
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
  {#if layout.vaultSidebarCollapsed}
    <VaultSidebarCollapsedStrip onExpand={() => layout.setVaultSidebarCollapsed(false)} />
  {:else}
    <SplitPane
      width={layout.vaultTreeWidth}
      side="left"
      min={200}
      max={420}
      onResize={(width) => layout.setVaultTreeWidth(width)}
    >
      <LmeSidePanel {onOpenChat} />
    </SplitPane>
  {/if}

  <LmeEditorHost
    {visible}
    {onOpenChat}
    {onOpenWork}
    {onSelectCard}
  />
</section>

<VaultNewNoteDialog />
<VaultNewGroupDialog />
