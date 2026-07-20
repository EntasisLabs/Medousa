<script lang="ts">
  import ScriptWorkbenchChatPanel from "$lib/components/automations/ScriptWorkbenchChatPanel.svelte";
  import ScriptWorkbenchConsole from "$lib/components/automations/ScriptWorkbenchConsole.svelte";
  import ScriptWorkbenchStatusBar from "$lib/components/automations/ScriptWorkbenchStatusBar.svelte";
  import ScriptWorkbenchTitlebar from "$lib/components/automations/ScriptWorkbenchTitlebar.svelte";
  import GraphemeScriptEditorPanel from "$lib/components/grapheme/GraphemeScriptEditorPanel.svelte";
  import { layout } from "$lib/stores/layout.svelte";

  interface Props {
    visible?: boolean;
  }

  let { visible = true }: Props = $props();

  let consoleOpen = $state(true);
  let chatOpen = $state(false);
</script>

<div
  class="lme-script-editor relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
  data-debug-label="lme-script-editor"
>
  <ScriptWorkbenchTitlebar
    leftOpen={true}
    {consoleOpen}
    {chatOpen}
    hideTabStrip={true}
    onShowSidebar={() => {}}
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

  <ScriptWorkbenchStatusBar onToggleConsole={() => (consoleOpen = true)} />
</div>
