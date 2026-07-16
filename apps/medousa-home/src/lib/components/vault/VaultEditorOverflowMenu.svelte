<script lang="ts">
  import { Ellipsis, FileDown, FileText, MessageSquare, Send } from "@lucide/svelte";
  import type { WorkCard } from "$lib/types/workspace";
  import { formatCardTitle } from "$lib/utils/formatWork";
  import type { VaultNoteKind } from "$lib/utils/vaultFrontmatter";
  import type { VaultSaveStatus } from "$lib/utils/vaultSave";

  interface MenuItem {
    id: string;
    label: string;
    disabled?: boolean;
    hidden?: boolean;
    dividerBefore?: boolean;
    onClick: () => void | Promise<void>;
  }

  interface Props {
    selectedPath: string | null;
    selectedKind: VaultNoteKind;
    editorMode: "edit" | "preview";
    noteLoading?: boolean;
    saving?: boolean;
    dirty?: boolean;
    saveStatus?: VaultSaveStatus;
    exportingPdf?: boolean;
    askSubmitting?: boolean;
    hasKanbanBoard?: boolean;
    boardEditMode?: "board" | "raw";
    linkedWork?: WorkCard[];
    onOpenChat?: () => void;
    onOpenWork?: () => void;
    onSelectCard?: (id: string) => void | Promise<void>;
    onExportPdf?: () => void | Promise<void>;
    onAskInChat?: () => void | Promise<void>;
    onSendToWork?: () => void | Promise<void>;
    onSave?: () => void | Promise<void>;
    onOpenNoteActions?: () => void;
    onOpenLooseMarkdown?: () => void | Promise<void>;
    onInsertWeeklyReview?: () => void;
    onPromoteJournal?: () => void | Promise<void>;
    onPromoteProject?: () => void | Promise<void>;
    onToggleBoard?: () => void;
  }

  let {
    selectedPath,
    selectedKind,
    editorMode,
    noteLoading = false,
    saving = false,
    dirty = false,
    saveStatus = "idle",
    exportingPdf = false,
    askSubmitting = false,
    hasKanbanBoard = false,
    boardEditMode = "board",
    linkedWork = [],
    onOpenChat,
    onOpenWork,
    onSelectCard,
    onExportPdf,
    onAskInChat,
    onSendToWork,
    onSave,
    onOpenNoteActions,
    onOpenLooseMarkdown,
    onInsertWeeklyReview,
    onPromoteJournal,
    onPromoteProject,
    onToggleBoard,
  }: Props = $props();

  let open = $state(false);

  const items = $derived.by((): MenuItem[] => {
    const rows: MenuItem[] = [];

    if (onOpenLooseMarkdown) {
      rows.push({
        id: "open-loose",
        label: "Open markdown file…",
        onClick: async () => {
          open = false;
          await onOpenLooseMarkdown();
        },
      });
    }

    if (!selectedPath) return rows;

    if (onAskInChat) {
      rows.push({
        id: "send-chat",
        label: "Talk about this note",
        disabled: noteLoading,
        onClick: async () => {
          open = false;
          await onAskInChat();
        },
      });
    }

    if (onOpenWork && onSendToWork) {
      rows.push({
        id: "send-work",
        label: askSubmitting ? "Sending to Work…" : "Send to Work",
        disabled: noteLoading || askSubmitting,
        onClick: async () => {
          open = false;
          await onSendToWork();
        },
      });
    }

    if (onExportPdf) {
      rows.push({
        id: "export-pdf",
        label: exportingPdf ? "Preparing PDF…" : "Export PDF…",
        disabled: exportingPdf || noteLoading,
        onClick: async () => {
          open = false;
          await onExportPdf();
        },
      });
    }

    if (selectedKind === "daily" && editorMode === "edit" && onInsertWeeklyReview) {
      rows.push({
        id: "weekly-review",
        label: "Link weekly review",
        dividerBefore: rows.length > 0,
        onClick: () => {
          open = false;
          onInsertWeeklyReview();
        },
      });
    }

    if (selectedKind === "inbox") {
      if (onPromoteJournal) {
        rows.push({
          id: "promote-journal",
          label: "Move to Journal",
          disabled: saving,
          dividerBefore: rows.length > 0,
          onClick: async () => {
            open = false;
            await onPromoteJournal();
          },
        });
      }
      if (onPromoteProject) {
        rows.push({
          id: "promote-project",
          label: "Move to Project",
          disabled: saving,
          onClick: async () => {
            open = false;
            await onPromoteProject();
          },
        });
      }
    }

    if (hasKanbanBoard && editorMode === "edit" && onToggleBoard) {
      rows.push({
        id: "board-mode",
        label: boardEditMode === "board" ? "Raw markdown" : "Board view",
        dividerBefore: rows.length > 0,
        onClick: () => {
          open = false;
          onToggleBoard();
        },
      });
    }

    if (linkedWork.length > 0 && onSelectCard) {
      let linkedIndex = 0;
      for (const card of linkedWork.slice(0, 4)) {
        rows.push({
          id: `linked-${card.id}`,
          label: `Open linked · ${formatCardTitle(card)}`,
          dividerBefore: linkedIndex === 0 && rows.length > 0,
          onClick: () => {
            open = false;
            void onSelectCard(card.id);
          },
        });
        linkedIndex += 1;
      }
    }

    if (dirty && saveStatus !== "conflict" && onSave) {
      rows.push({
        id: "save-now",
        label: saving ? "Saving…" : "Save now",
        disabled: saving,
        dividerBefore: rows.length > 0,
        onClick: async () => {
          open = false;
          await onSave();
        },
      });
    }

    if (onOpenNoteActions) {
      rows.push({
        id: "note-actions",
        label: "Rename / move / delete…",
        disabled: noteLoading,
        dividerBefore: rows.length > 0,
        onClick: () => {
          open = false;
          onOpenNoteActions();
        },
      });
    }

    return rows.filter((row) => !row.hidden);
  });

  const hasItems = $derived(items.length > 0);
</script>

{#if hasItems}
  <div class="relative">
    <button
      type="button"
      class="btn btn-sm variant-ghost-surface"
      aria-label="More note actions"
      aria-expanded={open}
      aria-haspopup="menu"
      onclick={() => {
        open = !open;
      }}
    >
      <Ellipsis size={14} strokeWidth={2} />
    </button>

    {#if open}
      <button
        type="button"
        class="fixed inset-0 z-40 cursor-default"
        aria-label="Close menu"
        onclick={() => {
          open = false;
        }}
      ></button>
      <div
        class="absolute right-0 top-full z-50 mt-1 min-w-[12.5rem] max-w-[16rem] rounded-container-token border border-surface-500/40 bg-surface-900 py-1 shadow-lg"
        role="menu"
      >
        {#each items as item (item.id)}
          {#if item.dividerBefore}
            <div class="my-1 border-t border-surface-500/35" role="separator"></div>
          {/if}
          <button
            type="button"
            class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-surface-200 hover:bg-surface-800/80 disabled:cursor-not-allowed disabled:opacity-50"
            role="menuitem"
            disabled={item.disabled}
            onclick={() => void item.onClick()}
          >
            {#if item.id === "send-chat"}
              <MessageSquare size={14} aria-hidden="true" />
            {:else if item.id === "send-work"}
              <Send size={14} aria-hidden="true" />
            {:else if item.id === "export-pdf"}
              <FileDown size={14} aria-hidden="true" />
            {:else if item.id === "open-loose"}
              <FileText size={14} aria-hidden="true" />
            {/if}
            {item.label}
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}
