<script lang="ts">
  import { Pencil, X } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import type { SessionSummary } from "$lib/types/session";
  import { formatSessionLabel } from "$lib/utils/formatSession";

  interface Props {
    open: boolean;
    onClose?: () => void;
    variant?: "drawer" | "inline" | "sheet";
  }

  let { open, onClose, variant = "drawer" }: Props = $props();

  let query = $state("");
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  let renamingSession = $state<SessionSummary | null>(null);
  let renameDraft = $state("");
  let renameError = $state<string | null>(null);
  let renameSaving = $state(false);

  $effect(() => {
    if (open) {
      query = chat.sessionListQuery;
      void chat.refreshSessions({ force: true, q: query });
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
    chat.sessions.filter((session) => !chat.isPinned(session.session_id)),
  );

  function formatWhen(iso?: string | null): string {
    if (!iso) return "";
    try {
      const date = new Date(iso);
      const diffMs = Date.now() - date.getTime();
      const mins = Math.floor(diffMs / 60_000);
      if (mins < 1) return "now";
      if (mins < 60) return `${mins}m`;
      const hours = Math.floor(mins / 60);
      if (hours < 48) return `${hours}h`;
      return date.toLocaleDateString([], { month: "short", day: "numeric" });
    } catch {
      return "";
    }
  }

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
</script>

{#if open}
  {#if variant === "sheet"}
    <div
      class="mobile-sheet-backdrop"
      role="presentation"
      onclick={(event) => {
        if (event.target === event.currentTarget) onClose?.();
      }}
    >
      <div
        class="mobile-sheet mobile-sheet-tall relative flex flex-col"
        role="dialog"
        aria-label="Chat sessions"
      >
        {@render sessionPanel()}
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
        : 'workshop-drawer relative w-56 shrink-0 border-r-2'} relative flex h-full flex-col"
      aria-label="Chat sessions"
    >
      {@render sessionPanel()}
    </aside>
  {/if}
{/if}

{#snippet sessionPanel()}
  <div class="flex min-h-0 flex-1 flex-col">
    <div class="workshop-header px-3 py-3">
      <p class="text-sm font-semibold text-surface-100">Sessions</p>
      {#if (variant === "drawer" || variant === "sheet") && onClose}
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          aria-label="Close sessions"
          onclick={onClose}
        >
          {variant === "sheet" ? "Done" : ""}
          {#if variant === "drawer"}
            <X size={16} strokeWidth={1.75} />
          {/if}
        </button>
      {/if}
    </div>

    <div class="border-b border-surface-500/45 p-3">
      <button
        type="button"
        class="btn variant-filled-primary w-full text-sm"
        onclick={createSession}
      >
        New chat
      </button>
      <input
        class="input mt-2 w-full text-sm"
        type="search"
        placeholder="Search sessions…"
        bind:value={query}
      />
    </div>

    {#if chat.sessionsError}
      <p class="px-3 py-2 text-xs text-error-400">{chat.sessionsError}</p>
    {:else if chat.sessionsRefreshing}
      <p class="workshop-faint px-3 py-1 text-[11px]">Updating sessions…</p>
    {/if}

    <ol class="flex-1 space-y-3 overflow-y-auto p-2">
      {#if pinned.length > 0}
        <li>
          <p class="workshop-section-title px-2">Pinned</p>
          <ul class="mt-1 space-y-1">
            {#each pinned as session (session.session_id)}
              <li>
                {@render sessionButton(session)}
              </li>
            {/each}
          </ul>
        </li>
      {/if}

      <li>
        <p class="workshop-section-title px-2">Recent</p>
        <ul class="mt-1 space-y-1">
          {#each recent as session (session.session_id)}
            <li>
              {@render sessionButton(session)}
            </li>
          {:else}
            {#if pinned.length === 0}
              <li class="workshop-muted px-3 py-6 text-center">
                No sessions yet
              </li>
            {/if}
          {/each}
        </ul>
      </li>
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
          class="input w-full text-sm"
          type="text"
          maxlength="80"
          bind:value={renameDraft}
          autofocus
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
{/snippet}

{#snippet sessionButton(session: SessionSummary)}
  <div
    class="group flex items-stretch rounded-container-token transition {chat.sessionId ===
    session.session_id
      ? 'bg-surface-700 ring-1 ring-primary-500/50'
      : 'hover:bg-surface-700/80'}"
  >
    <button
      type="button"
      class="min-w-0 flex-1 px-3 py-2 text-left"
      onclick={() => selectSession(session.session_id)}
    >
      <div class="flex items-center justify-between gap-2">
        <span class="truncate text-sm font-medium text-surface-100">
          {formatSessionLabel(session)}
        </span>
        <span class="workshop-faint shrink-0">
          {formatWhen(session.last_timestamp)}
        </span>
      </div>
      <p class="workshop-faint mt-0.5 truncate">
        {session.turns} turn{session.turns === 1 ? "" : "s"}
      </p>
    </button>
    <button
      type="button"
      class="px-2 text-xs text-surface-500 opacity-60 transition hover:text-primary-300 group-hover:opacity-100"
      title="Rename session"
      aria-label="Rename session"
      onclick={() => openRename(session)}
    >
      <Pencil size={14} strokeWidth={1.75} />
    </button>
    <button
      type="button"
      class="px-2 text-xs text-surface-500 opacity-60 transition hover:text-warning-300 group-hover:opacity-100"
      title={chat.isPinned(session.session_id) ? "Unpin session" : "Pin session"}
      onclick={() => chat.togglePin(session.session_id)}
    >
      {chat.isPinned(session.session_id) ? "★" : "☆"}
    </button>
  </div>
{/snippet}
