<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";

  interface Props {
    visible: boolean;
  }

  let { visible }: Props = $props();

  let query = $state("");

  const filtered = $derived(
    chat.sessions.filter((session) => {
      if (!query.trim()) return true;
      const needle = query.trim().toLowerCase();
      const label =
        session.display_name?.toLowerCase() ??
        session.preview.toLowerCase() ??
        session.session_id.toLowerCase();
      return (
        label.includes(needle) || session.session_id.toLowerCase().includes(needle)
      );
    }),
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

  function sessionLabel(session: (typeof chat.sessions)[number]): string {
    return (
      session.display_name?.trim() ||
      session.preview.trim() ||
      session.session_id
    );
  }
</script>

<aside
  class="flex h-full w-56 shrink-0 flex-col border-r border-surface-500/20 bg-surface-900/50 {visible
    ? ''
    : 'hidden'}"
  aria-label="Chat sessions"
>
  <div class="border-b border-surface-500/20 p-3">
    <button
      type="button"
      class="btn variant-filled-primary w-full text-sm"
      onclick={() => chat.newSession()}
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

  <ol class="flex-1 space-y-1 overflow-y-auto p-2">
    {#each filtered as session (session.session_id)}
      <li>
        <button
          type="button"
          class="w-full rounded-container-token px-3 py-2 text-left transition {chat.sessionId ===
          session.session_id
            ? 'bg-surface-800 ring-1 ring-primary-500/40'
            : 'hover:bg-surface-800/70'}"
          onclick={() => chat.switchSession(session.session_id)}
        >
          <div class="flex items-center justify-between gap-2">
            <span class="truncate text-sm font-medium text-surface-100">
              {sessionLabel(session)}
            </span>
            <span class="shrink-0 text-xs text-surface-500">
              {formatWhen(session.last_timestamp)}
            </span>
          </div>
          <p class="mt-0.5 truncate text-xs text-surface-500">
            {session.turns} turn{session.turns === 1 ? "" : "s"}
          </p>
        </button>
      </li>
    {:else}
      <li class="px-3 py-6 text-center text-sm text-surface-500">
        No sessions yet
      </li>
    {/each}
  </ol>
</aside>
