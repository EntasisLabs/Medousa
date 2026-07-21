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
  import { lmeWorkspace, type LmeTab } from "$lib/stores/lmeWorkspace.svelte";
  import { noteEditorRuntimes } from "$lib/stores/noteEditorRuntimes.svelte";
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

  /** Soft cap for live TipTap hosts in the focused pane (ADR-006 keep-alive). */
  const MAX_LIVE_NOTE_HOSTS = 10;

  type NoteTab = Extract<LmeTab, { kind: "note" }>;

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

  const openNoteTabs = $derived(
    lmeWorkspace.tabs.filter((tab): tab is NoteTab => tab.kind === "note"),
  );

  /**
   * Focused-pane keep-alive pool: active note + LRU of other open notes.
   * Background (non-interactive) panes stay single-host preview.
   */
  const keepAliveNoteTabs = $derived.by(() => {
    void noteEditorRuntimes.revision;
    if (!interactive) {
      return active?.kind === "note" ? [active] : [];
    }
    const activeNote = active?.kind === "note" ? active : null;
    const ranked = [...openNoteTabs].sort(
      (a, b) =>
        noteEditorRuntimes.lastFocusedAt(b.path) -
        noteEditorRuntimes.lastFocusedAt(a.path),
    );
    const picked: NoteTab[] = [];
    const seen = new Set<string>();
    if (activeNote) {
      picked.push(activeNote);
      seen.add(activeNote.tabId);
    }
    for (const tab of ranked) {
      if (picked.length >= MAX_LIVE_NOTE_HOSTS) break;
      if (seen.has(tab.tabId)) continue;
      picked.push(tab);
      seen.add(tab.tabId);
    }
    return picked;
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
  class="lme-editor-host relative flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
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
  {:else}
    <!-- Note keep-alive pool: hide inactive hosts; do not destroy TipTap on activate. -->
    {#each keepAliveNoteTabs as tab (tab.tabId)}
      {@const isActiveNote = active.kind === "note" && active.tabId === tab.tabId}
      <div
        class="absolute inset-0 flex min-h-0 min-w-0 flex-col overflow-hidden"
        class:invisible={!isActiveNote}
        class:pointer-events-none={!isActiveNote}
        aria-hidden={!isActiveNote}
        inert={!isActiveNote ? true : undefined}
      >
        <VaultEditor
          visible={visible && isActiveNote}
          interactive={interactive && isActiveNote}
          keepAlive={interactive}
          path={tab.path}
          {onOpenChat}
          {onOpenWork}
          {onSelectCard}
        />
      </div>
    {/each}

    {#if active.kind === "script"}
      <div class="relative z-10 flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <LmeScriptEditor {visible} />
      </div>
    {:else if active.kind === "file"}
      <div class="external-file-library-preview relative z-10 flex h-full min-h-0 min-w-0 flex-1 flex-col">
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
      <div class="relative z-10 flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
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
      </div>
    {:else if active.kind === "manuscript"}
      <div class="relative z-10 flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <LmeAgentEditor {onOpenChat} />
      </div>
    {:else if active.kind === "flow"}
      <div class="relative z-10 flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <LmeFlowEditor />
      </div>
    {/if}
  {/if}
</div>
