<script lang="ts">
  import ScriptWorkbenchChatPanel from "$lib/components/automations/ScriptWorkbenchChatPanel.svelte";
  import ScriptWorkbenchConsole from "$lib/components/automations/ScriptWorkbenchConsole.svelte";
  import ScriptWorkbenchTitlebar from "$lib/components/automations/ScriptWorkbenchTitlebar.svelte";
  import GraphemeScriptEditorPanel from "$lib/components/grapheme/GraphemeScriptEditorPanel.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { SCRIPT_WORKBENCH_OPEN_CONSOLE_EVENT } from "$lib/utils/scriptWorkbenchChromeEvents";

  interface Props {
    visible?: boolean;
  }

  let { visible = true }: Props = $props();

  let consoleOpen = $state(true);
  let chatOpen = $state(false);

  function showWorkspaceBrowser() {
    layout.openShellSidebarView(layout.desktopSurface);
    void environment.patchShellChromeDesktop({ navStyle: "rail" }).catch(() => {});
  }

  $effect(() => {
    const onOpen = () => {
      consoleOpen = true;
    };
    window.addEventListener(SCRIPT_WORKBENCH_OPEN_CONSOLE_EVENT, onOpen);
    return () => window.removeEventListener(SCRIPT_WORKBENCH_OPEN_CONSOLE_EVENT, onOpen);
  });
</script>

<div
  class="lme-script-editor relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
  data-debug-label="lme-script-editor"
>
  <ScriptWorkbenchTitlebar
    leftOpen={layout.shellSidebarExpanded}
    {consoleOpen}
    {chatOpen}
    hideTabStrip={true}
    onShowSidebar={showWorkspaceBrowser}
    onToggleConsole={() => (consoleOpen = !consoleOpen)}
    onToggleChat={() => (chatOpen = !chatOpen)}
  />

  <div class="flex min-h-0 flex-1 overflow-hidden">
    <div class="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <GraphemeScriptEditorPanel {visible} workbenchMode />
      {#if consoleOpen}
        <ScriptWorkbenchConsole onHide={() => (consoleOpen = false)} />
      {/if}
    </div>

    {#if chatOpen}
      <ScriptWorkbenchChatPanel
        visible={visible}
        onOpenFullChat={() => layout.navigateDesktop("chat", { bump: true })}
      />
    {/if}
  </div>
</div>
