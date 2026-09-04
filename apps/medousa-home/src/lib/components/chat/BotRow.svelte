<script lang="ts">
  import { Archive, ArchiveRestore, Copy, Pencil, Sparkles } from "@lucide/svelte";
  import type { BotProfile } from "$lib/types/generated/daemon_api";

  interface Props {
    bot: BotProfile;
    specialistLabel: string;
    selected?: boolean;
    alwaysShowActions?: boolean;
    onSelect: () => void;
    onEdit: () => void;
    onDuplicate: () => void;
    onArchive: () => void;
  }

  let {
    bot,
    specialistLabel,
    selected = false,
    alwaysShowActions = false,
    onSelect,
    onEdit,
    onDuplicate,
    onArchive,
  }: Props = $props();

  const avatar = $derived(bot.avatar_ref?.trim() || null);
</script>

<div
  class="session-row bot-row {selected ? 'session-row--selected' : ''} {alwaysShowActions
    ? 'session-row--touch'
    : ''}"
>
  <button type="button" class="session-row-main bot-row-main" onclick={onSelect}>
    <span class="bot-row-avatar" aria-hidden="true">
      {#if avatar}
        {avatar}
      {:else}
        <Sparkles size={13} strokeWidth={1.8} />
      {/if}
    </span>
    <span class="min-w-0 flex-1">
      <span class="session-row-title">{bot.display_name}</span>
      <span class="bot-row-specialist truncate">{specialistLabel}</span>
    </span>
  </button>

  <div class="session-row-actions">
    <button
      type="button"
      class="session-row-action"
      title="Edit Bot"
      aria-label="Edit {bot.display_name}"
      onclick={onEdit}
    >
      <Pencil size={13} strokeWidth={1.75} />
    </button>
    <button
      type="button"
      class="session-row-action"
      title="Duplicate Bot"
      aria-label="Duplicate {bot.display_name}"
      onclick={onDuplicate}
    >
      <Copy size={13} strokeWidth={1.75} />
    </button>
    <button
      type="button"
      class="session-row-action"
      class:session-row-action--danger={!bot.archived}
      title={bot.archived ? "Restore Bot" : "Archive Bot"}
      aria-label={bot.archived ? `Restore ${bot.display_name}` : `Archive ${bot.display_name}`}
      onclick={onArchive}
    >
      {#if bot.archived}
        <ArchiveRestore size={13} strokeWidth={1.75} />
      {:else}
        <Archive size={13} strokeWidth={1.75} />
      {/if}
    </button>
  </div>
</div>
