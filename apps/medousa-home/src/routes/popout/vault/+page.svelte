<script lang="ts">
  import { onMount } from "svelte";
  import { FilePlus, Pin, PinOff, Search, X } from "@lucide/svelte";
  import VaultEditor from "$lib/components/vault/VaultEditor.svelte";
  import VaultEditorOverflowMenu from "$lib/components/vault/VaultEditorOverflowMenu.svelte";
  import VaultNewNoteDialog from "$lib/components/vault/VaultNewNoteDialog.svelte";
  import VaultNoteWorkshop from "$lib/components/vault/VaultNoteWorkshop.svelte";
  import VaultQuickSwitcher from "$lib/components/vault/VaultQuickSwitcher.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { supportsPreviewSplit } from "$lib/utils/vaultNoteKind";
  import {
    VAULT_STICKY_PATH_KEY,
    readVaultStickyPath,
    writeVaultStickyPath,
  } from "$lib/utils/vaultSticky";
  import {
    hideVaultSticky,
    isTauri,
    setVaultStickyAlwaysOnTop,
    setVaultStickyWindowTitle,
  } from "$lib/window";
  import { connectWorkshop } from "$lib/workshopConnection";
  import { formatShortcut } from "$lib/platform";

  let alwaysOnTop = $state(true);
  let loadingPath = $state(true);
  let missingPath = $state(false);
  let findOpen = $state(false);

  const displayTitle = $derived(
    vault.selectedPath
      ? (vault.labelByPathMap.get(vault.selectedPath) ??
        vaultDisplayTitle(vault.title, vault.selectedPath))
      : "Medousa Note",
  );

  const showPreviewToggle = $derived(
    Boolean(vault.selectedPath) && supportsPreviewSplit(vault.selectedKind),
  );

  const saveWhisper = $derived(vault.saveWhisper());

  $effect(() => {
    if (!isTauri()) return;
    void setVaultStickyWindowTitle(displayTitle);
  });

  $effect(() => {
    const path = vault.selectedPath;
    if (path) {
      writeVaultStickyPath(path);
      missingPath = false;
    }
  });

  onMount(() => {
    const detachWorkshop = connectWorkshop({
      onHealthChange: () => {},
      mode: "observer",
    });

    async function openStickyPath(path: string | null) {
      if (!path) {
        missingPath = true;
        loadingPath = false;
        return;
      }
      loadingPath = true;
      missingPath = false;
      try {
        await vault.openNote(path);
      } catch {
        missingPath = true;
      } finally {
        loadingPath = false;
      }
    }

    void openStickyPath(readVaultStickyPath());

    const onStorage = (event: StorageEvent) => {
      if (event.key !== VAULT_STICKY_PATH_KEY) return;
      const next = event.newValue?.trim() || null;
      void openStickyPath(next);
    };

    const onKeydown = (event: KeyboardEvent) => {
      const mod = event.metaKey || event.ctrlKey;
      if (!mod) return;
      const key = event.key.toLowerCase();
      const target = event.target as HTMLElement | null;
      const typing =
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable);

      if (key === "o") {
        event.preventDefault();
        event.stopPropagation();
        findOpen = !findOpen;
        return;
      }

      if (key === "n") {
        if (typing && target?.tagName === "INPUT" && findOpen) return;
        event.preventDefault();
        event.stopPropagation();
        findOpen = false;
        void openNewNote();
      }
    };

    window.addEventListener("storage", onStorage);
    window.addEventListener("keydown", onKeydown, true);

    return () => {
      window.removeEventListener("storage", onStorage);
      window.removeEventListener("keydown", onKeydown, true);
      detachWorkshop();
    };
  });

  async function openNewNote() {
    if (vault.dirty) await vault.flushSave();
    vault.openNewNoteDialog();
  }

  async function handleClose() {
    if (vault.dirty) {
      await vault.flushSave();
    }
    if (isTauri()) {
      await hideVaultSticky();
    }
  }

  async function togglePin() {
    if (!isTauri()) return;
    const next = !alwaysOnTop;
    await setVaultStickyAlwaysOnTop(next);
    alwaysOnTop = next;
  }

  function togglePreview() {
    if (vault.editorMode === "edit") vault.enterPreviewMode();
    else vault.enterEditMode();
  }
</script>

<div class="vault-sticky-shell flex h-screen w-screen flex-col bg-surface-950 text-surface-50">
  <header class="vault-sticky-chrome">
    <div class="min-w-0 flex-1">
      <p class="truncate text-[13px] font-medium leading-tight text-surface-100">
        {displayTitle}
      </p>
      {#if saveWhisper}
        <p
          class="truncate text-[10px] leading-tight {saveWhisper === 'Saved'
            ? 'text-success-400/90'
            : 'text-surface-500'}"
        >
          {saveWhisper}
        </p>
      {/if}
    </div>

    <div class="flex shrink-0 items-center gap-0.5">
      <button
        type="button"
        class="vault-sticky-icon"
        title="Find note ({formatShortcut('O')})"
        aria-label="Find note"
        onclick={() => (findOpen = true)}
      >
        <Search size={13} strokeWidth={2} />
      </button>
      <button
        type="button"
        class="vault-sticky-icon"
        title="New note ({formatShortcut('N')})"
        aria-label="New note"
        onclick={() => void openNewNote()}
      >
        <FilePlus size={13} strokeWidth={2} />
      </button>

      {#if showPreviewToggle}
        <button
          type="button"
          class="vault-sticky-action {vault.editorMode === 'preview'
            ? 'vault-sticky-action-active'
            : ''}"
          title={vault.editorMode === "edit" ? "Preview" : "Edit"}
          aria-label={vault.editorMode === "edit" ? "Preview" : "Edit"}
          onclick={togglePreview}
        >
          {vault.editorMode === "edit" ? "Preview" : "Edit"}
        </button>
      {/if}

      {#if vault.selectedPath}
        <VaultEditorOverflowMenu
          selectedPath={vault.selectedPath}
          selectedKind={vault.selectedKind}
          editorMode={vault.editorMode}
          noteLoading={vault.noteLoading}
          saving={vault.saving}
          dirty={vault.dirty}
          saveStatus={vault.saveStatus}
          onAskInChat={async () => {
            await vault.flushSave();
            const { launchVaultNoteWorkshop } = await import(
              "$lib/utils/vaultNoteWorkshop"
            );
            await launchVaultNoteWorkshop({
              path: vault.selectedPath!,
              title: vault.title,
              content: vault.content,
              wikilinksOut: vault.wikilinksOut,
              backlinks: vault.backlinks,
              session: "fresh",
            });
          }}
          onSave={async () => {
            await vault.flushSave();
          }}
          onOpenNoteActions={() => vault.openNoteActions()}
          onToggleBoard={() => vault.toggleBoardEditMode()}
        />
      {/if}

      {#if isTauri()}
        <button
          type="button"
          class="vault-sticky-icon {alwaysOnTop ? 'vault-sticky-action-active' : ''}"
          title={alwaysOnTop ? "Unpin from top" : "Keep on top"}
          aria-label={alwaysOnTop ? "Unpin from top" : "Keep on top"}
          aria-pressed={alwaysOnTop}
          onclick={() => void togglePin()}
        >
          {#if alwaysOnTop}
            <Pin size={13} strokeWidth={2} />
          {:else}
            <PinOff size={13} strokeWidth={2} />
          {/if}
        </button>
        <button
          type="button"
          class="vault-sticky-icon"
          title="Close sticky note"
          aria-label="Close sticky note"
          onclick={() => void handleClose()}
        >
          <X size={13} strokeWidth={2} />
        </button>
      {/if}
    </div>
  </header>

  <div class="relative flex min-h-0 flex-1 flex-col">
    {#if loadingPath}
      <p class="px-4 py-6 text-sm text-surface-500">Opening note…</p>
    {:else if missingPath && !vault.selectedPath}
      <div class="flex flex-1 flex-col items-start justify-center gap-3 px-4 py-6">
        <div>
          <p class="text-sm font-medium text-surface-200">No note open</p>
          <p class="mt-1 text-xs text-surface-500">
            Find an existing note or start a new one.
          </p>
        </div>
        <div class="flex flex-wrap gap-2">
          <button
            type="button"
            class="btn btn-sm variant-soft-primary"
            onclick={() => (findOpen = true)}
          >
            <Search size={14} strokeWidth={2} />
            Find note
          </button>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            onclick={() => void openNewNote()}
          >
            <FilePlus size={14} strokeWidth={2} />
            New note
          </button>
        </div>
      </div>
    {:else}
      <VaultEditor visible={true} stickyNote={true} />
    {/if}
  </div>

  <VaultNoteWorkshop stickyHost />
  <VaultNewNoteDialog />
  <VaultQuickSwitcher
    open={findOpen}
    stickyHost
    onClose={() => (findOpen = false)}
  />
</div>
