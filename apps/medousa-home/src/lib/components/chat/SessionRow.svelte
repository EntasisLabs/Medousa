<script lang="ts">
  import { Pencil, Star, Trash2 } from "@lucide/svelte";
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
</script>

<div
  class="session-row group/session {selected ? 'session-row-active' : ''} {alwaysShowActions
    ? 'session-row--touch'
    : ''}"
>
  <button type="button" class="session-row-main" onclick={onSelect}>
    <div class="flex min-w-0 items-baseline justify-between gap-2">
      <span class="session-row-title truncate">
        {#if session.catalog === "shared"}
          <span class="session-row-shared-mark" title="Shared room">Room</span>
        {/if}
        {formatSessionLabel(session)}
      </span>
      {#if when}
        <span class="session-row-when shrink-0">{when}</span>
      {/if}
    </div>
    {#if session.turns > 0}
      <p class="session-row-meta truncate">
        {session.turns} turn{session.turns === 1 ? "" : "s"}
      </p>
    {/if}
  </button>

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

<style>
  .session-row-shared-mark {
    display: inline-block;
    margin-right: 0.35rem;
    padding: 0.05rem 0.35rem;
    border-radius: 0.25rem;
    border: 1px solid color-mix(in oklab, var(--color-surface-400, #94a3b8) 45%, transparent);
    font-size: 0.65rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: var(--color-surface-300, #cbd5e1);
    vertical-align: 0.05em;
  }
</style>
