<script lang="ts">
  import { X } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import type { SessionSummary } from "$lib/types/session";
  import { formatSessionLabel } from "$lib/utils/formatSession";

  interface Props {
    open: boolean;
    onClose?: () => void;
    variant?: "drawer" | "inline";
  }

  let { open, onClose, variant = "drawer" }: Props = $props();

  let query = $state("");

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
    if (variant === "drawer") {
      layout.setSessionDrawerOpen(false);
    }
  }

  async function createSession() {
    await chat.newSession();
    if (variant === "drawer") {
      layout.setSessionDrawerOpen(false);
    }
  }
</script>

{#if open}
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
      : 'workshop-drawer relative w-56 shrink-0 border-r-2'} flex h-full flex-col"
    aria-label="Chat sessions"
  >
    <div class="workshop-header px-3 py-3">
      <p class="text-sm font-semibold text-surface-100">Sessions</p>
      {#if variant === "drawer" && onClose}
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          aria-label="Close sessions drawer"
          onclick={onClose}
        >
          <X size={16} strokeWidth={1.75} />
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
  </aside>
{/if}

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
      class="px-2 text-xs text-surface-500 opacity-60 transition hover:text-warning-300 group-hover:opacity-100"
      title={chat.isPinned(session.session_id) ? "Unpin session" : "Pin session"}
      onclick={() => chat.togglePin(session.session_id)}
    >
      {chat.isPinned(session.session_id) ? "★" : "☆"}
    </button>
  </div>
{/snippet}
