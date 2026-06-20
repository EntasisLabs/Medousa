<script lang="ts">
  import { ExternalLink, History } from "@lucide/svelte";
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import ScriptWorkbenchChatSessionMenu from "$lib/components/automations/ScriptWorkbenchChatSessionMenu.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import {
    buildScriptWorkbenchContextScope,
    scriptWorkbenchContextHint,
  } from "$lib/utils/scriptWorkbenchBridge";
  import { launchScriptWorkbenchChat } from "$lib/utils/scriptWorkbenchChat";

  interface Props {
    visible: boolean;
    onOpenFullChat?: () => void;
  }

  let { visible, onOpenFullChat }: Props = $props();

  let sessionMenuOpen = $state(false);

  const scope = $derived.by(() => {
    const tab = graphemeScriptEditor.activeTab;
    if (!tab) return null;
    return buildScriptWorkbenchContextScope({
      tabId: tab.tabId,
      scriptId: tab.scriptId,
      name: tab.name,
      body: tab.body,
      dirty: tab.dirty,
    });
  });

  const hint = $derived(scope ? scriptWorkbenchContextHint(scope) : null);

  $effect(() => {
    if (!visible) {
      chat.clearScriptWorkbenchContext();
      return;
    }
    if (scope) {
      chat.syncScriptWorkbenchContext(scope);
    }
  });

  $effect(() => {
    if (!visible) return;
    void chat.ensureSessionHydrated();
  });

  async function handleSessionSelect(session: "fresh" | string) {
    const tab = graphemeScriptEditor.activeTab;
    if (!tab || !scope) return;
    sessionMenuOpen = false;
    await launchScriptWorkbenchChat({
      scope,
      body: tab.body,
      session,
      runError: workshop.runError,
      runResult: workshop.runResult?.result ?? null,
      compileError: graphemeScriptEditor.compileError,
      compileHints: graphemeScriptEditor.compileResult?.compile_hints ?? [],
    });
  }

  function handleOpenFullChat() {
    onOpenFullChat?.();
    layout.navigateDesktop("chat", { bump: true });
  }
</script>

<div class="script-workbench-chat-panel flex min-h-0 flex-1 flex-col overflow-hidden">
  <header class="flex shrink-0 items-center gap-2 border-b border-surface-500/35 px-2 py-2">
    <div class="min-w-0 flex-1">
      <p class="truncate text-xs font-medium text-surface-100">
        {scope?.name ?? "Script chat"}
      </p>
      {#if hint}
        <p class="truncate text-[10px] text-surface-400">{hint}</p>
      {/if}
    </div>
    <div class="relative shrink-0">
      <button
        type="button"
        class="workshop-text-action rounded p-1.5 {sessionMenuOpen
          ? 'bg-surface-800 text-surface-100'
          : ''}"
        aria-label="Switch chat"
        title="Switch chat"
        aria-haspopup="menu"
        aria-expanded={sessionMenuOpen}
        onclick={() => (sessionMenuOpen = !sessionMenuOpen)}
      >
        <History size={14} strokeWidth={1.75} />
      </button>
      <ScriptWorkbenchChatSessionMenu
        open={sessionMenuOpen}
        onClose={() => (sessionMenuOpen = false)}
        onSelect={handleSessionSelect}
        class="vault-note-chat-session-menu-workshop"
      />
    </div>
    <button
      type="button"
      class="workshop-text-action shrink-0 rounded p-1.5"
      aria-label="Open in main Chat"
      title="Open in main Chat"
      onclick={handleOpenFullChat}
    >
      <ExternalLink size={14} strokeWidth={1.75} />
    </button>
  </header>

  <ChatPanel
    visible={visible}
    embedded={true}
    workshop={true}
    scriptWorkbench={true}
    showPopout={false}
  />
</div>
