<script lang="ts">
  import { MessageSquarePlus } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { formatSessionLabel } from "$lib/utils/formatSession";

  interface Props {
    open: boolean;
    onClose?: () => void;
    onSelect: (session: "fresh" | string) => void | Promise<void>;
    class?: string;
  }

  let { open, onClose, onSelect, class: className = "" }: Props = $props();

  $effect(() => {
    if (open) void chat.refreshSessions({ force: true });
  });

  const recentSessions = $derived(chat.sessions.slice(0, 10));

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

  async function pick(session: "fresh" | string) {
    await onSelect(session);
    onClose?.();
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="vault-note-chat-session-backdrop"
    role="presentation"
    onclick={() => onClose?.()}
  ></div>
  <div
    class="vault-note-chat-session-menu {className}"
    role="menu"
    aria-label="Choose a chat for this script"
  >
    <p class="vault-note-chat-session-menu-heading">Talk about this script</p>
    <button
      type="button"
      role="menuitem"
      class="vault-note-chat-session-item vault-note-chat-session-item-new"
      onclick={() => void pick("fresh")}
    >
      <MessageSquarePlus size={16} strokeWidth={1.75} />
      <span class="min-w-0 flex-1 text-left">
        <span class="block font-medium">New chat</span>
        <span class="block text-[11px] text-surface-400">Start fresh with this script in scope</span>
      </span>
    </button>

    {#if recentSessions.length > 0}
      <p class="vault-note-chat-session-menu-subheading">Recent chats</p>
      <ul class="vault-note-chat-session-list">
        {#each recentSessions as session (session.session_id)}
          {@const active = session.session_id === chat.sessionId}
          <li>
            <button
              type="button"
              role="menuitem"
              class="vault-note-chat-session-item {active ? 'vault-note-chat-session-item-active' : ''}"
              onclick={() => void pick(session.session_id)}
            >
              <span class="min-w-0 flex-1 truncate text-left font-medium">
                {formatSessionLabel(session)}
              </span>
              <span class="shrink-0 text-[11px] text-surface-500">
                {formatWhen(session.last_timestamp)}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {:else if !chat.sessionsRefreshing}
      <p class="vault-note-chat-session-empty">No prior chats yet</p>
    {/if}
  </div>
{/if}
