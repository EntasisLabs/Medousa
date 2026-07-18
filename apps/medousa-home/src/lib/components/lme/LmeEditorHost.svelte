<script lang="ts">
  import ArtifactLibraryPreview from "$lib/components/artifacts/ArtifactLibraryPreview.svelte";
  import LmeScriptEditor from "$lib/components/lme/LmeScriptEditor.svelte";
  import LmeTabStrip from "$lib/components/lme/LmeTabStrip.svelte";
  import VaultAttachmentPreviewContent from "$lib/components/vault/VaultAttachmentPreviewContent.svelte";
  import VaultEditor from "$lib/components/vault/VaultEditor.svelte";
  import { artifacts } from "$lib/stores/artifacts.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { attachmentFileName } from "$lib/utils/vaultAttachments";

  interface Props {
    visible?: boolean;
    onOpenChat: () => void;
    onOpenWork: () => void;
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let { visible = true, onOpenChat, onOpenWork, onSelectCard }: Props = $props();

  const active = $derived(lmeWorkspace.activeTab);

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
  <LmeTabStrip />

  {#if !active}
    <div class="flex flex-1 items-center justify-center p-8 text-sm text-surface-500">
      Open something from the side panel.
    </div>
  {:else if active.kind === "note"}
    <VaultEditor
      visible={visible}
      {onOpenChat}
      {onOpenWork}
      {onSelectCard}
    />
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
  {/if}
</div>
