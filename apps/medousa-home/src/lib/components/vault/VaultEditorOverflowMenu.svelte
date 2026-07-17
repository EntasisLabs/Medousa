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

  interface ToggleItem {
    id: string;
    label: string;
    on: boolean;
    onToggle: () => void;
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
    showPreviewToggle?: boolean;
    showSplitToggle?: boolean;
    splitEnabled?: boolean;
    showLinksToggle?: boolean;
    linksOpen?: boolean;
    linkCount?: number;
    showEditSource?: boolean;
    showBackToLive?: boolean;
    /** Build plane: show Editor toggles (wrap / line numbers / autosave / mono). */
    showEditorToggles?: boolean;
    buildWordWrap?: boolean;
    buildLineNumbers?: boolean;
    buildAutoSave?: boolean;
    monoSource?: boolean;
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
    onTogglePreview?: () => void;
    onToggleSplit?: () => void;
    onToggleLinks?: () => void;
    onEditSource?: () => void;
    onBackToLive?: () => void;
    onToggleWordWrap?: () => void;
    onToggleLineNumbers?: () => void;
    onToggleAutoSave?: () => void;
    onToggleMonoSource?: () => void;
    /** Tauri: float current note into sticky Live window. */
    onFloatNote?: () => void | Promise<void>;
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
    showPreviewToggle = false,
    showSplitToggle = false,
    splitEnabled = false,
    showLinksToggle = false,
    linksOpen = false,
    linkCount = 0,
    showEditSource = false,
    showBackToLive = false,
    showEditorToggles = false,
    buildWordWrap = true,
    buildLineNumbers = false,
    buildAutoSave = true,
    monoSource = false,
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
    onTogglePreview,
    onToggleSplit,
    onToggleLinks,
    onEditSource,
    onBackToLive,
    onToggleWordWrap,
    onToggleLineNumbers,
    onToggleAutoSave,
    onToggleMonoSource,
    onFloatNote,
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

    if (showEditSource && onEditSource) {
      rows.push({
        id: "edit-source",
        label: "Edit source",
        onClick: () => {
          open = false;
          onEditSource();
        },
      });
    }

    if (showBackToLive && onBackToLive) {
      rows.push({
        id: "back-live",
        label: "Back to Live",
        onClick: () => {
          open = false;
          onBackToLive();
        },
      });
    }

    if (showPreviewToggle && onTogglePreview) {
      rows.push({
        id: "preview",
        label: editorMode === "preview" ? "Back to editing" : "Preview",
        onClick: () => {
          open = false;
          onTogglePreview();
        },
      });
    }

    if (showSplitToggle && onToggleSplit) {
      rows.push({
        id: "split",
        label: splitEnabled ? "Hide split preview" : "Split preview",
        onClick: () => {
          open = false;
          onToggleSplit();
        },
      });
    }

    if (showLinksToggle && onToggleLinks) {
      rows.push({
        id: "links",
        label: linksOpen
          ? "Hide links"
          : linkCount > 0
            ? `Links (${linkCount})`
            : "Links",
        onClick: () => {
          open = false;
          onToggleLinks();
        },
      });
    }

    if (onAskInChat) {
      rows.push({
        id: "send-chat",
        label: "Talk about this note",
        disabled: noteLoading,
        dividerBefore: rows.length > 0,
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

    if (onFloatNote) {
      rows.push({
        id: "float-note",
        label: "Float note",
        disabled: noteLoading || saving,
        dividerBefore: rows.length > 0,
        onClick: async () => {
          open = false;
          await onFloatNote();
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

  const editorToggles = $derived.by((): ToggleItem[] => {
    if (!showEditorToggles) return [];
    const rows: ToggleItem[] = [];
    if (onToggleLineNumbers) {
      rows.push({
        id: "line-numbers",
        label: "Line numbers",
        on: buildLineNumbers,
        onToggle: onToggleLineNumbers,
      });
    }
    if (onToggleWordWrap) {
      rows.push({
        id: "word-wrap",
        label: "Word wrap",
        on: buildWordWrap,
        onToggle: onToggleWordWrap,
      });
    }
    if (onToggleAutoSave) {
      rows.push({
        id: "auto-save",
        label: "Auto save",
        on: buildAutoSave,
        onToggle: onToggleAutoSave,
      });
    }
    if (onToggleMonoSource) {
      rows.push({
        id: "mono-source",
        label: "Mono source",
        on: monoSource,
        onToggle: onToggleMonoSource,
      });
    }
    return rows;
  });

  const hasItems = $derived(items.length > 0 || editorToggles.length > 0);
</script>

{#if hasItems}
  <div class="relative">
    <button
      type="button"
      class="vault-editor-icon-btn"
      class:vault-editor-icon-btn--active={open}
      aria-label="More note actions"
      aria-expanded={open}
      aria-haspopup="menu"
      onclick={() => {
        open = !open;
      }}
    >
      <Ellipsis size={15} strokeWidth={1.75} />
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
        class="vault-editor-overflow absolute right-0 top-full z-50 mt-1 min-w-[13.5rem] max-w-[17rem] rounded-container-token border border-surface-500/40 bg-surface-900 py-1 shadow-lg"
        role="menu"
      >
        {#each items as item (item.id)}
          {#if item.dividerBefore}
            <div class="vault-editor-overflow__sep" role="separator"></div>
          {/if}
          <button
            type="button"
            class="vault-editor-overflow__item"
            role="menuitem"
            disabled={item.disabled}
            onclick={() => void item.onClick()}
          >
            <span class="vault-editor-overflow__icon" aria-hidden="true">
              {#if item.id === "send-chat"}
                <MessageSquare size={14} />
              {:else if item.id === "send-work"}
                <Send size={14} />
              {:else if item.id === "export-pdf"}
                <FileDown size={14} />
              {:else if item.id === "open-loose"}
                <FileText size={14} />
              {/if}
            </span>
            <span class="vault-editor-overflow__label">{item.label}</span>
          </button>
        {/each}

        {#if editorToggles.length > 0}
          {#if items.length > 0}
            <div class="vault-editor-overflow__sep" role="separator"></div>
          {/if}
          {#each editorToggles as toggle (toggle.id)}
            <button
              type="button"
              class="vault-editor-overflow__toggle"
              role="menuitemcheckbox"
              aria-checked={toggle.on}
              onclick={() => toggle.onToggle()}
            >
              <span class="vault-editor-overflow__label">{toggle.label}</span>
              <span
                class="vault-editor-overflow__switch"
                class:vault-editor-overflow__switch--on={toggle.on}
                aria-hidden="true"
              >
                <span class="vault-editor-overflow__knob"></span>
              </span>
            </button>
          {/each}
        {/if}
      </div>
    {/if}
  </div>
{/if}
