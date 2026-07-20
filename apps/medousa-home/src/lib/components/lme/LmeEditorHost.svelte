<script lang="ts">
  import ArtifactLibraryPreview from "$lib/components/artifacts/ArtifactLibraryPreview.svelte";
  import LmeAgentEditor from "$lib/components/lme/LmeAgentEditor.svelte";
  import LmeFlowEditor from "$lib/components/lme/LmeFlowEditor.svelte";
  import LmeScriptEditor from "$lib/components/lme/LmeScriptEditor.svelte";
  import ShellSidebarExpandButton from "$lib/components/layout/ShellSidebarExpandButton.svelte";
  import VaultAttachmentPreviewContent from "$lib/components/vault/VaultAttachmentPreviewContent.svelte";
  import VaultEditor from "$lib/components/vault/VaultEditor.svelte";
  import { artifacts } from "$lib/stores/artifacts.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { attachmentFileName } from "$lib/utils/vaultAttachments";

  interface Props {
    visible?: boolean;
    /** Focused pane may edit; background panes render read-only. */
    interactive?: boolean;
    /** Shell-bound LME tab — when set, resolve content from that tab (not global active). */
    lmeTabId?: string | null;
    onOpenChat: () => void;
    onOpenWork: () => void;
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let {
    visible = true,
    interactive = true,
    lmeTabId = null,
    onOpenChat,
    onOpenWork,
    onSelectCard,
  }: Props = $props();

  const active = $derived.by(() => {
    const id = lmeTabId?.trim();
    if (id) {
      return lmeWorkspace.tabs.find((tab) => tab.tabId === id) ?? null;
    }
    return lmeWorkspace.activeTab;
  });

  const notePath = $derived(
    active?.kind === "note" ? active.path : null,
  );

  // Keep background note panes populated without stealing vault focus.
  $effect(() => {
    const path = notePath;
    if (!path || !visible) return;
    if (vault.isFocusedPath(path)) return;
    void vault.warmBuffer(path);
  });

  const fileAttachment = $derived.by(() => {
    if (active?.kind !== "file") return null;
    if (vault.previewingAttachmentPath === active.path) {
      return vault.previewingAttachment;
    }
    return {
      path: active.path,
      label: active.title,
      mime: undefined as string | undefined,
    };
  });

  const deckArtifact = $derived(
    active?.kind === "deck" ? artifacts.selectedArtifact : null,
  );

  let deckPanelOpen = $state(true);

  async function openFileExternal() {
    if (active?.kind !== "file") return;
    await openAttachmentPath(active.path);
  }
</script>

<div
  class="lme-editor-host flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
  data-debug-label="lme-editor-host"
>
  {#if !active}
    <div class="flex flex-1 flex-col">
      {#if !layout.shellSidebarExpanded}
        <div class="flex items-center px-2 pt-1.5">
          <ShellSidebarExpandButton label="Show workspace browser" />
        </div>
      {/if}
      <div class="flex flex-1 items-center justify-center p-8 text-sm text-surface-500">
        Open something from the side panel.
      </div>
    </div>
  {:else if active.kind === "note"}
    {#key active.path}
      <VaultEditor
        visible={visible}
        {interactive}
        path={active.path}
        {onOpenChat}
        {onOpenWork}
        {onSelectCard}
      />
    {/key}
  {:else if active.kind === "script"}
    <LmeScriptEditor {visible} />
  {:else if active.kind === "file"}
    <div class="external-file-library-preview flex h-full min-h-0 min-w-0 flex-1 flex-col">
      {#if !fileAttachment}
        <div class="flex flex-1 items-center justify-center p-6 text-sm text-surface-500">
          Select a file to preview.
        </div>
      {:else}
        <header class="artifact-library-preview-header">
          <div class="min-w-0">
            <h2 class="truncate text-sm font-semibold text-surface-100">
              {fileAttachment.label || attachmentFileName(fileAttachment)}
            </h2>
            <p class="truncate text-xs text-surface-500">{fileAttachment.path}</p>
          </div>
          <div class="flex shrink-0 items-center gap-2">
            <button
              type="button"
              class="artifact-library-action"
              onclick={() => void openFileExternal()}
            >
              Open in app
            </button>
          </div>
        </header>
        <div class="min-h-0 flex-1 overflow-hidden">
          <VaultAttachmentPreviewContent attachment={fileAttachment} fill={true} />
        </div>
      {/if}
    </div>
  {:else if active.kind === "deck"}
    <ArtifactLibraryPreview
      artifact={deckArtifact}
      sessionTitle={deckArtifact ? artifacts.sessionTitle(deckArtifact.session_id) : ""}
      bind:panelOpen={deckPanelOpen}
      {onOpenChat}
      onOpenSession={(sessionId) => {
        chat.sessionId = sessionId;
        onOpenChat();
      }}
    />
  {:else if active.kind === "manuscript"}
    <LmeAgentEditor {onOpenChat} />
  {:else if active.kind === "flow"}
    <LmeFlowEditor />
  {/if}
</div>
