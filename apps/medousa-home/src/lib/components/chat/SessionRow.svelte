<script lang="ts">
  import { MessagesSquare, Pencil, Star, Trash2 } from "@lucide/svelte";
  import SessionChannelMarks from "$lib/components/chat/SessionChannelMarks.svelte";
  import type { SessionSummary } from "$lib/types/session";
  import { formatSessionLabel, formatSessionWhen } from "$lib/utils/formatSession";

  interface Props {
    session: SessionSummary;
    selected?: boolean;
    pinned?: boolean;
    /** Touch / sheet: keep actions visible without hover. */
    alwaysShowActions?: boolean;
    onSelect: () => void;
    onRename: () => void;
    onDelete: () => void;
    onTogglePin: () => void;
  }

  let {
    session,
    selected = false,
    pinned = false,
    alwaysShowActions = false,
    onSelect,
    onRename,
    onDelete,
    onTogglePin,
  }: Props = $props();

  const when = $derived(formatSessionWhen(session.last_timestamp));
  const hasMeta = $derived(Boolean(when) || session.turns > 0);
  const title = $derived(formatSessionLabel(session));
  const untitled = $derived(
    title === "New conversation" || /^\(empty session\)$/i.test(title),
  );
</script>

<div
  class="session-row group/session {selected ? 'session-row--selected' : ''} {alwaysShowActions
    ? 'session-row--touch'
    : ''}"
>
  <button type="button" class="session-row-main" onclick={onSelect}>
    <SessionChannelMarks
      originSurface={session.origin_surface}
      hasCodeWork={session.has_code_work}
    />
    <span class="session-row-title truncate" class:session-row-title--untitled={untitled}>
      {#if session.catalog === "shared"}
        <span class="session-row-shared-mark" title="Shared room">Room</span>
      {/if}
      {untitled ? "Untitled chat" : title}
    </span>
  </button>

  {#if hasMeta}
    <div class="session-row-meta">
      {#if when}
        <span class="session-row-when">{when}</span>
      {/if}
      {#if session.turns > 0}
        <span
          class="session-row-turns"
          title="{session.turns} turn{session.turns === 1 ? '' : 's'}"
        >
          <MessagesSquare size={10} strokeWidth={2} aria-hidden="true" />
          <span class="tabular-nums">{session.turns}</span>
        </span>
      {/if}
    </div>
  {/if}

  <div class="session-row-actions">
    <button
      type="button"
      class="session-row-action"
      title="Rename session"
      aria-label="Rename session"
      onclick={onRename}
    >
      <Pencil size={13} strokeWidth={1.75} />
    </button>
    <button
      type="button"
      class="session-row-action session-row-action--danger"
      title="Delete session"
      aria-label="Delete session"
      onclick={onDelete}
    >
      <Trash2 size={13} strokeWidth={1.75} />
    </button>
    <button
      type="button"
      class="session-row-action {pinned ? 'session-row-action--pinned' : ''}"
      title={pinned ? "Unpin session" : "Pin session"}
      aria-label={pinned ? "Unpin session" : "Pin session"}
      onclick={onTogglePin}
    >
      <Star
        size={13}
        strokeWidth={1.75}
        fill={pinned ? "currentColor" : "none"}
      />
    </button>
  </div>
</div>
