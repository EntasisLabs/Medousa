<script lang="ts">
  import { Plus, Search, X } from "@lucide/svelte";
  import SessionRow from "$lib/components/chat/SessionRow.svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import type { SessionSummary } from "$lib/types/session";
  import { formatSessionLabel } from "$lib/utils/formatSession";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  interface Props {
    open: boolean;
    onClose?: () => void;
    variant?: "drawer" | "inline" | "sheet";
  }

  let { open, onClose, variant = "drawer" }: Props = $props();

  let query = $state("");
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  let searchInputEl = $state<HTMLInputElement | null>(null);
  let renamingSession = $state<SessionSummary | null>(null);
  let renameDraft = $state("");
  let renameError = $state<string | null>(null);
  let renameSaving = $state(false);
  let deletingSession = $state<SessionSummary | null>(null);
  let deleteError = $state<string | null>(null);
  let deleteSaving = $state(false);
  let renameInputEl = $state<HTMLInputElement | null>(null);
  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  const touchActions = $derived(variant === "sheet");

  $effect(() => {
    if (open) {
      query = chat.sessionListQuery;
      void chat.refreshSessions({ force: true, q: query });
      // Autofocus so the drawer feels interactive immediately.
      queueMicrotask(() => searchInputEl?.focus());
    }
  });

  $effect(() => {
    const needle = query;
    if (!open) return;
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      searchTimer = null;
      void chat.refreshSessions({ force: true, q: needle });
    }, 300);
    return () => {
      if (searchTimer) clearTimeout(searchTimer);
    };
  });

  function closeSheet() {
    layout.setSessionDrawerOpen(false);
    onClose?.();
  }

  function dismissSheet() {
    haptic("light");
    closeSheet();
  }

  $effect(() => {
    if (!open || variant !== "sheet") return;
    return registerMobileBackHandler(() => {
      closeSheet();
      return true;
    });
  });

  $effect(() => {
    if (!open || variant !== "sheet" || !sheetEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, { onDismiss: dismissSheet });
  });

  $effect(() => {
    if (renamingSession) {
      renameInputEl?.focus();
    }
  });

  function matchesQuery(session: SessionSummary): boolean {
    if (!query.trim()) return true;
    const needle = query.trim().toLowerCase();
    const label =
      session.display_name?.toLowerCase() ??
      session.preview.toLowerCase() ??
      session.session_id.toLowerCase();
    return (
      label.includes(needle) || session.session_id.toLowerCase().includes(needle)
    );
  }

  const pinned = $derived(
    chat.sessions.filter(
      (session) => chat.isPinned(session.session_id) && matchesQuery(session),
    ),
  );

  const recent = $derived(
    chat.sessions.filter(
      (session) => !chat.isPinned(session.session_id) && matchesQuery(session),
    ),
  );

  const listEmpty = $derived(pinned.length === 0 && recent.length === 0);

  async function selectSession(sessionId: string) {
    await chat.switchSession(sessionId);
    if (variant === "drawer" || variant === "sheet") {
      layout.setSessionDrawerOpen(false);
      onClose?.();
    }
  }

  async function createSession() {
    await chat.newSession();
    if (variant === "drawer" || variant === "sheet") {
      layout.setSessionDrawerOpen(false);
      onClose?.();
    }
  }

  function openRename(session: SessionSummary) {
    renamingSession = session;
    renameDraft = session.display_name?.trim() || formatSessionLabel(session);
    renameError = null;
  }

  function closeRename() {
    renamingSession = null;
    renameDraft = "";
    renameError = null;
    renameSaving = false;
  }

  async function submitRename(event: Event) {
    event.preventDefault();
    if (!renamingSession || !renameDraft.trim() || renameSaving) return;
    renameSaving = true;
    renameError = null;
    try {
      await chat.renameSession(renamingSession.session_id, renameDraft.trim());
      closeRename();
    } catch (err) {
      renameError = err instanceof Error ? err.message : String(err);
      renameSaving = false;
    }
  }

  function openDelete(session: SessionSummary) {
    deletingSession = session;
    deleteError = null;
  }

  function closeDelete() {
    deletingSession = null;
    deleteError = null;
    deleteSaving = false;
  }

  async function confirmDelete() {
    if (!deletingSession || deleteSaving) return;
    deleteSaving = true;
    deleteError = null;
    try {
      await chat.deleteSession(deletingSession.session_id);
      closeDelete();
      if (variant === "drawer" || variant === "sheet") {
        layout.setSessionDrawerOpen(false);
        onClose?.();
      }
    } catch (err) {
      deleteError = err instanceof Error ? err.message : String(err);
      deleteSaving = false;
    }
  }
</script>

{#if open}
  {#if variant === "sheet"}
    <div
      class="mobile-sheet-backdrop mobile-sheet-peek-backdrop"
      role="presentation"
      onclick={(event) => {
        if (event.target === event.currentTarget) dismissSheet();
      }}
    >
      <div
        bind:this={sheetEl}
        class="mobile-sheet mobile-sheet-peek relative flex flex-col"
        role="dialog"
        aria-label="Chat sessions"
      >
        <header
          bind:this={headerEl}
          class="mobile-sheet-header scripts-workbench-sheet-header mobile-chat-history-header"
        >
          <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
          <div class="flex w-full items-center justify-between gap-2">
            <h2 class="text-sm font-semibold text-surface-50">Sessions</h2>
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface"
              onclick={dismissSheet}
            >
              Done
            </button>
          </div>
        </header>
        {@render sessionPanelBody()}
      </div>
    </div>
  {:else}
    {#if variant === "drawer"}
      <button
        type="button"
        class="absolute inset-0 z-20 bg-surface-950/50"
        aria-label="Close sessions"
        onclick={onClose}
      ></button>
    {/if}

    <aside
      class="{variant === 'drawer'
        ? 'workshop-drawer absolute left-0 top-0 z-30 w-64 border-r-2'
        : variant === 'inline'
          ? 'relative flex h-full min-h-0 w-full flex-col'
          : 'workshop-drawer relative w-56 shrink-0 border-r-2'} relative flex h-full flex-col"
      aria-label="Chat sessions"
    >
      {#if variant !== "inline"}
        <div class="session-sidebar-header">
          <p class="text-sm font-semibold text-surface-100">Sessions</p>
          {#if onClose}
            <button
              type="button"
              class="session-sidebar-icon-btn"
              aria-label="Close sessions"
              onclick={onClose}
            >
              <X size={15} strokeWidth={1.75} />
            </button>
          {/if}
        </div>
      {/if}
      {@render sessionPanelBody()}
    </aside>
  {/if}
{/if}

{#snippet sessionPanelBody()}
  <div class="flex min-h-0 flex-1 flex-col">
    <div class="session-sidebar-toolbar {variant === 'sheet' ? 'session-sidebar-toolbar--sheet' : ''}">
      <label class="session-sidebar-search">
        <Search size={14} strokeWidth={1.75} class="session-sidebar-search-icon" aria-hidden="true" />
        <input
          bind:this={searchInputEl}
          class="session-sidebar-search-input"
          type="search"
          placeholder="Search titles…"
          bind:value={query}
        />
      </label>
      <button
        type="button"
        class="session-sidebar-new"
        title="New chat"
        aria-label="New chat"
        onclick={createSession}
      >
        <Plus size={15} strokeWidth={2} />
        <span class="session-sidebar-new-label">New</span>
      </button>
    </div>

    {#if chat.sessionsError}
      <p class="px-3 py-2 text-xs text-error-400">{chat.sessionsError}</p>
    {:else if chat.sessionsRefreshing}
      <p class="workshop-faint px-3 py-1 text-[11px]">Updating sessions…</p>
    {/if}

    <ol class="session-sidebar-list {variant === 'sheet' ? 'mobile-chat-history-list' : ''}">
      {#if pinned.length > 0}
        <li class="session-sidebar-section">
          <p class="session-sidebar-section-title">Pinned</p>
          <ul class="session-sidebar-section-list">
            {#each pinned as session (session.session_id)}
              <li>
                <SessionRow
                  {session}
                  selected={chat.sessionId === session.session_id}
                  pinned
                  alwaysShowActions={touchActions}
                  onSelect={() => void selectSession(session.session_id)}
                  onRename={() => openRename(session)}
                  onDelete={() => openDelete(session)}
                  onTogglePin={() => chat.togglePin(session.session_id)}
                />
              </li>
            {/each}
          </ul>
        </li>
      {/if}

      {#if recent.length > 0}
        <li class="session-sidebar-section">
          <p class="session-sidebar-section-title">Recent</p>
          <ul class="session-sidebar-section-list">
            {#each recent as session (session.session_id)}
              <li>
                <SessionRow
                  {session}
                  selected={chat.sessionId === session.session_id}
                  pinned={false}
                  alwaysShowActions={touchActions}
                  onSelect={() => void selectSession(session.session_id)}
                  onRename={() => openRename(session)}
                  onDelete={() => openDelete(session)}
                  onTogglePin={() => chat.togglePin(session.session_id)}
                />
              </li>
            {/each}
          </ul>
        </li>
      {:else if listEmpty}
        <li class="session-sidebar-empty">
          {#if query.trim()}
            No sessions match “{query.trim()}”.
          {:else}
            No chats for {userProfiles.activeDisplayName} yet.
            <span class="mt-2 block workshop-faint">
              Work and home stay separate — switch profile anytime in Settings → Memory.
            </span>
          {/if}
        </li>
      {/if}
    </ol>
  </div>

  {#if renamingSession}
    <div
      class="absolute inset-0 z-40 flex items-end bg-surface-950/70 p-3 sm:items-center sm:justify-center"
      role="dialog"
      aria-modal="true"
      aria-labelledby="session-rename-title"
    >
      <form
        class="card w-full space-y-3 p-4 shadow-xl sm:max-w-sm"
        onsubmit={submitRename}
      >
        <div class="flex items-start justify-between gap-3">
          <div>
            <p id="session-rename-title" class="text-sm font-semibold text-surface-100">
              Rename session
            </p>
            <p class="workshop-faint mt-0.5 text-xs">
              Saved on this device — searchable in your session list.
            </p>
          </div>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            aria-label="Cancel rename"
            onclick={closeRename}
          >
            <X size={16} strokeWidth={1.75} />
          </button>
        </div>
        <input
          bind:this={renameInputEl}
          class="input w-full text-sm"
          type="text"
          maxlength="80"
          bind:value={renameDraft}
        />
        {#if renameError}
          <p class="text-xs text-error-400">{renameError}</p>
        {/if}
        <div class="flex justify-end gap-2">
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            onclick={closeRename}
          >
            Cancel
          </button>
          <button
            type="submit"
            class="btn btn-sm variant-filled-primary"
            disabled={!renameDraft.trim() || renameSaving}
          >
            {renameSaving ? "Saving…" : "Save name"}
          </button>
        </div>
      </form>
    </div>
  {/if}

  {#if deletingSession}
    <div
      class="absolute inset-0 z-40 flex items-end bg-surface-950/70 p-3 sm:items-center sm:justify-center"
      role="dialog"
      aria-modal="true"
      aria-labelledby="session-delete-title"
    >
      <div class="card w-full space-y-3 p-4 shadow-xl sm:max-w-sm">
        <div class="flex items-start justify-between gap-3">
          <div>
            <p id="session-delete-title" class="text-sm font-semibold text-surface-100">
              Delete session?
            </p>
            <p class="workshop-faint mt-0.5 text-xs">
              Removes transcript, catalog entry, and Locus memory for
              {formatSessionLabel(deletingSession)}. This cannot be undone.
            </p>
          </div>
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            aria-label="Cancel delete"
            onclick={closeDelete}
          >
            <X size={16} strokeWidth={1.75} />
          </button>
        </div>
        {#if deleteError}
          <p class="text-xs text-error-400">{deleteError}</p>
        {/if}
        <div class="flex justify-end gap-2">
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            onclick={closeDelete}
          >
            Cancel
          </button>
          <button
            type="button"
            class="btn btn-sm variant-filled-error"
            disabled={deleteSaving}
            onclick={confirmDelete}
          >
            {deleteSaving ? "Deleting…" : "Delete"}
          </button>
        </div>
      </div>
    </div>
  {/if}
{/snippet}
