<script lang="ts">
  import ShareToPeerSheet from "$lib/components/settings/ShareToPeerSheet.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { listTrustedWorkshops } from "$lib/utils/lanShareApi";
  import { saveVaultUserTemplate } from "$lib/utils/vaultUserTemplates";
  import { isTauri } from "$lib/window";
  import { onMount, tick } from "svelte";
  import {
    ArrowLeft,
    Bookmark,
    FilePen,
    FolderInput,
    Share2,
    Trash2,
    X,
  } from "@lucide/svelte";

  type Panel = "menu" | "rename" | "move" | "template" | "delete";

  let panel = $state<Panel>("menu");
  let titleDraft = $state("");
  let pathDraft = $state("");
  let templateName = $state("");
  let templateMessage = $state<string | null>(null);
  let peerShareOpen = $state(false);
  let hasTrustedPeers = $state(false);
  let peerStatus = $state<string | null>(null);

  onMount(() => {
    if (!isTauri()) return;
    void listTrustedWorkshops()
      .then((peers) => {
        hasTrustedPeers = peers.some((peer) => peer.hasSessionToken);
      })
      .catch(() => {
        hasTrustedPeers = false;
      });
  });

  $effect(() => {
    if (vault.noteActionsOpen) {
      titleDraft = vault.title;
      pathDraft = vault.selectedPath ?? "";
      templateName = vault.title.trim() || "";
      templateMessage = null;
      peerStatus = null;
      panel = "menu";
    }
  });

  async function focusPanelInput(selector: string) {
    await tick();
    const el = document.querySelector(selector) as HTMLInputElement | null;
    el?.focus();
    el?.select();
  }

  function openPanel(next: Panel) {
    panel = next;
    if (next === "rename") void focusPanelInput("[data-note-action-rename]");
    if (next === "move") void focusPanelInput("[data-note-action-move]");
    if (next === "template") void focusPanelInput("[data-note-action-template]");
  }

  function backToMenu() {
    panel = "menu";
    templateMessage = null;
  }

  async function commitRename() {
    if (!titleDraft.trim() || vault.saving) return;
    await vault.renameNoteTitle(titleDraft.trim());
    vault.closeNoteActions();
  }

  async function commitMove() {
    if (!pathDraft.trim() || vault.saving) return;
    await vault.relocateNote(pathDraft.trim());
    vault.closeNoteActions();
  }

  function commitTemplate() {
    templateMessage = null;
    if (!vault.selectedPath || !vault.content.trim()) {
      templateMessage = "Open a note with content first.";
      return;
    }
    const name = templateName.trim() || vault.title.trim() || "Saved template";
    const saved = saveVaultUserTemplate({
      name,
      content: vault.content,
      spaceId: vault.activeSpace?.id,
    });
    if (!saved) {
      templateMessage = "Could not save template.";
      return;
    }
    templateMessage = `Saved “${saved.name}”.`;
    templateName = "";
  }

  async function commitDelete() {
    if (!vault.selectedPath || vault.saving) return;
    if (panel !== "delete") {
      openPanel("delete");
      return;
    }
    const path = vault.selectedPath;
    vault.closeNoteActions();
    await vault.archiveNote(path);
  }

  function onSheetKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      if (panel === "menu") {
        vault.closeNoteActions();
      } else {
        backToMenu();
      }
    }
  }
</script>

{#if vault.noteActionsOpen && vault.selectedPath}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="vault-interact-backdrop"
    role="dialog"
    aria-modal="true"
    aria-labelledby="note-actions-title"
    tabindex="-1"
    onkeydown={onSheetKeydown}
    onclick={(event) => {
      if (event.target === event.currentTarget) vault.closeNoteActions();
    }}
  >
    <div class="vault-interact-sheet vault-interact-sheet--quiet">
      {#if panel === "menu"}
        <header class="vault-interact-header">
          <div class="min-w-0">
            <p class="vault-interact-kicker">This note</p>
            <h3 id="note-actions-title" class="vault-interact-title truncate">
              {vault.title || "Untitled"}
            </h3>
          </div>
          <button
            type="button"
            class="vault-interact-dismiss"
            aria-label="Close"
            onclick={() => vault.closeNoteActions()}
          >
            <X size={14} strokeWidth={2} />
          </button>
        </header>

        <div class="vault-verb-list">
          <button
            type="button"
            class="vault-verb"
            disabled={vault.saving}
            onclick={() => openPanel("rename")}
          >
            <span class="vault-verb-icon"><FilePen size={15} strokeWidth={1.75} /></span>
            <span class="vault-verb-label">Rename</span>
          </button>
          <button
            type="button"
            class="vault-verb"
            disabled={vault.saving}
            onclick={() => openPanel("move")}
          >
            <span class="vault-verb-icon"><FolderInput size={15} strokeWidth={1.75} /></span>
            <span class="vault-verb-label">Move</span>
          </button>
          <button
            type="button"
            class="vault-verb"
            disabled={vault.saving || !vault.content.trim()}
            onclick={() => openPanel("template")}
          >
            <span class="vault-verb-icon"><Bookmark size={15} strokeWidth={1.75} /></span>
            <span class="vault-verb-label">Save as template</span>
          </button>
          {#if hasTrustedPeers}
            <button
              type="button"
              class="vault-verb"
              disabled={vault.saving}
              onclick={() => {
                peerStatus = null;
                peerShareOpen = true;
              }}
            >
              <span class="vault-verb-icon"><Share2 size={15} strokeWidth={1.75} /></span>
              <span class="vault-verb-label">Share to peer</span>
            </button>
          {/if}
          {#if peerStatus}
            <p class="vault-interact-note">{peerStatus}</p>
          {/if}
        </div>

        <div class="vault-verb-danger">
          <button
            type="button"
            class="vault-verb vault-verb--danger"
            disabled={vault.saving}
            onclick={() => void commitDelete()}
          >
            <span class="vault-verb-icon"><Trash2 size={15} strokeWidth={1.75} /></span>
            <span class="vault-verb-label">Delete</span>
          </button>
        </div>
      {:else}
        <header class="vault-interact-header">
          <button type="button" class="vault-interact-back" onclick={backToMenu}>
            <ArrowLeft size={14} strokeWidth={2} />
            Back
          </button>
          <button
            type="button"
            class="vault-interact-dismiss"
            aria-label="Close"
            onclick={() => vault.closeNoteActions()}
          >
            <X size={14} strokeWidth={2} />
          </button>
        </header>

        {#if panel === "rename"}
          <form
            class="vault-interact-editor"
            onsubmit={(event) => {
              event.preventDefault();
              void commitRename();
            }}
          >
            <p class="vault-interact-prompt">Rename this note</p>
            <input
              class="vault-interact-field"
              type="text"
              bind:value={titleDraft}
              data-note-action-rename
              placeholder="Title"
            />
            <button
              type="submit"
              class="vault-interact-commit"
              disabled={vault.saving || !titleDraft.trim()}
            >
              Save title
            </button>
          </form>
        {:else if panel === "move"}
          <form
            class="vault-interact-editor"
            onsubmit={(event) => {
              event.preventDefault();
              void commitMove();
            }}
          >
            <p class="vault-interact-prompt">Move or rename the file</p>
            <input
              class="vault-interact-field vault-interact-field--mono"
              type="text"
              bind:value={pathDraft}
              data-note-action-move
              placeholder="journal/my-note.md"
            />
            <p class="vault-interact-note">
              Or drag the note onto a folder in the sidebar.
            </p>
            <button
              type="submit"
              class="vault-interact-commit"
              disabled={vault.saving || !pathDraft.trim()}
            >
              Move file
            </button>
          </form>
        {:else if panel === "template"}
          <form
            class="vault-interact-editor"
            onsubmit={(event) => {
              event.preventDefault();
              commitTemplate();
            }}
          >
            <p class="vault-interact-prompt">Save current note as a template</p>
            <input
              class="vault-interact-field"
              type="text"
              bind:value={templateName}
              data-note-action-template
              placeholder="Template name"
            />
            {#if templateMessage}
              <p class="vault-interact-note">{templateMessage}</p>
            {/if}
            <button
              type="submit"
              class="vault-interact-commit"
              disabled={!vault.selectedPath || !vault.content.trim()}
            >
              Save template
            </button>
          </form>
        {:else if panel === "delete"}
          <div class="vault-interact-editor">
            <p class="vault-interact-prompt">Delete “{vault.title || "this note"}”?</p>
            <p class="vault-interact-note">
              Moves it to vault trash. You can undo from there later.
            </p>
            <button
              type="button"
              class="vault-interact-commit vault-interact-commit--danger"
              disabled={vault.saving}
              onclick={() => void commitDelete()}
            >
              Confirm delete
            </button>
          </div>
        {/if}
      {/if}
    </div>
  </div>
{/if}

<ShareToPeerSheet
  open={peerShareOpen}
  vaultPath={vault.selectedPath}
  label={vault.title}
  onClose={() => {
    peerShareOpen = false;
  }}
  onShared={(message) => {
    peerStatus = message;
  }}
  onError={(message) => {
    peerStatus = message;
  }}
/>
