<script lang="ts">
  import { ChevronDown, ExternalLink } from "@lucide/svelte";
  import { attachmentPreviewUrl } from "$lib/utils/vaultAttachmentPicker";
  import { isLocalImageHref, isRemoteImageHref } from "$lib/utils/vaultLocalImages";
  import {
    parseKanbanCardText,
    type KanbanCardPresentation,
    type KanbanCardWikilink,
  } from "$lib/utils/vaultKanbanCardParse";

  interface Props {
    text: string;
    checked?: boolean;
    disabled?: boolean;
    onEdit: () => void;
    onWikilink?: (target: string) => void;
    onPeek?: (target: string, anchor: DOMRect) => void;
  }

  let {
    text,
    checked = false,
    disabled = false,
    onEdit,
    onWikilink,
    onPeek,
  }: Props = $props();

  let expanded = $state(false);
  let thumbUrl = $state<string | null>(null);

  const presentation = $derived(parseKanbanCardText(text));

  /** Hide link chips that merely repeat the title (wikilink-primary or alias in title). */
  const linkChips = $derived.by((): KanbanCardWikilink[] => {
    const view = presentation;
    if (view.wikilinkPrimary) return [];
    const titleNorm = view.title.trim().toLowerCase();
    return view.wikilinks.filter((link) => link.label.trim().toLowerCase() !== titleNorm);
  });

  const showChipRow = $derived(linkChips.length > 0 || presentation.tags.length > 0);

  $effect(() => {
    const href = presentation.imageHref;
    thumbUrl = null;
    if (!href) return;

    if (isRemoteImageHref(href)) {
      thumbUrl = href;
      return;
    }
    if (!isLocalImageHref(href) && !href.includes("/") && !href.includes("\\")) {
      return;
    }
    void (async () => {
      try {
        thumbUrl = await attachmentPreviewUrl(href);
      } catch {
        thumbUrl = null;
      }
    })();
  });

  function handleWikilink(event: MouseEvent, target: string) {
    event.preventDefault();
    event.stopPropagation();
    const anchor = (event.currentTarget as HTMLElement).getBoundingClientRect();
    if (onPeek) {
      onPeek(target, anchor);
      return;
    }
    onWikilink?.(target);
  }

  function toggleExpand(event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    expanded = !expanded;
  }

  function handleOpenPrimary(event: MouseEvent, view: KanbanCardPresentation) {
    event.preventDefault();
    event.stopPropagation();
    const link = view.wikilinkPrimary ? view.wikilinks[0] : null;
    if (link && (onPeek || onWikilink)) {
      const anchor = (event.currentTarget as HTMLElement).getBoundingClientRect();
      if (onPeek) {
        onPeek(link.target, anchor);
        return;
      }
      onWikilink?.(link.target);
      return;
    }
    if (!disabled) onEdit();
  }
</script>

{#if thumbUrl}
  <div class="vault-kanban-card-media">
    <img src={thumbUrl} alt="" loading="lazy" decoding="async" />
  </div>
{/if}

<div class="vault-kanban-card-face" class:vault-kanban-card-face--done={checked}>
  {#if presentation.emoji}
    <span class="vault-kanban-card-emoji" aria-hidden="true">{presentation.emoji}</span>
  {/if}

  <div class="vault-kanban-card-copy min-w-0 flex-1">
    <button
      type="button"
      class="vault-kanban-card-title"
      class:vault-kanban-card-title--empty={!text.trim()}
      disabled={disabled &&
        !(presentation.wikilinkPrimary && (onPeek || onWikilink))}
      onclick={(event) => handleOpenPrimary(event, presentation)}
      ondblclick={(event) => {
        event.preventDefault();
        event.stopPropagation();
        if (!disabled) onEdit();
      }}
      title={presentation.wikilinkPrimary
        ? "Peek linked note · double-click to edit"
        : "Click to edit"}
    >
      {presentation.title}
    </button>

    {#if presentation.body}
      <button
        type="button"
        class="vault-kanban-card-body"
        class:vault-kanban-card-body--expanded={expanded}
        {disabled}
        onclick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          if (!disabled) onEdit();
        }}
      >
        {presentation.body}
      </button>
    {/if}

    {#if showChipRow}
      <div class="vault-kanban-card-chips">
        {#each linkChips as link (link.target)}
          <button
            type="button"
            class="vault-kanban-chip vault-kanban-chip--link"
            disabled={!onPeek && !onWikilink}
            onclick={(event) => handleWikilink(event, link.target)}
            title="Peek [[{link.target}]]"
          >
            {link.label}
          </button>
        {/each}
        {#each presentation.tags as tag (tag)}
          <span class="vault-kanban-chip vault-kanban-chip--tag">#{tag}</span>
        {/each}
      </div>
    {/if}

    {#if presentation.wikilinkPrimary && onWikilink && presentation.wikilinks[0]}
      <button
        type="button"
        class="vault-kanban-card-open"
        onclick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          onWikilink(presentation.wikilinks[0].target);
        }}
      >
        <ExternalLink size={12} strokeWidth={2} />
        Open note
      </button>
    {/if}
  </div>

  {#if presentation.body}
    <button
      type="button"
      class="vault-kanban-card-expand"
      class:vault-kanban-card-expand--open={expanded}
      aria-label={expanded ? "Collapse card" : "Expand card"}
      aria-expanded={expanded}
      onclick={toggleExpand}
    >
      <ChevronDown size={14} strokeWidth={2} />
    </button>
  {/if}
</div>
