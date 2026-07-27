<script lang="ts">
  import { Sparkles, Terminal, Wrench } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import type { SlashMenuAnchor } from "$lib/utils/slashMenuPlacement";
  import {
    groupComposerSlashItems,
    type ComposerSlashItem,
  } from "$lib/utils/composerSkillSlash";

  interface Props {
    open: boolean;
    items: ComposerSlashItem[];
    anchor?: SlashMenuAnchor | null;
    highlightIndex?: number;
    onSelect: (item: ComposerSlashItem) => void;
    onClose: () => void;
    onHighlight?: (index: number) => void;
  }

  let {
    open,
    items,
    anchor = null,
    highlightIndex = 0,
    onSelect,
    onClose,
    onHighlight,
  }: Props = $props();

  let menuEl = $state<HTMLDivElement | null>(null);
  const groups = $derived(groupComposerSlashItems(items));

  $effect(() => {
    if (!open || !menuEl) return;
    void highlightIndex;
    const row = menuEl.querySelector<HTMLElement>(
      `[data-slash-index="${highlightIndex}"]`,
    );
    row?.scrollIntoView({ block: "nearest" });
  });

  // Keep onClose referenced for API parity with hosts.
  void onClose;

  /** Cursor-style order: Skills → Commands → Tools. */
  function flatIndexFor(
    section: "skills" | "commands" | "tools",
    localIndex: number,
  ): number {
    let index = 0;
    if (section === "skills") return localIndex;
    index += groups.skills.length;
    if (section === "commands") return index + localIndex;
    index += groups.commands.length;
    return index + localIndex;
  }

  function iconFor(kind: ComposerSlashItem["kind"]) {
    if (kind === "skill") return Sparkles;
    if (kind === "command") return Terminal;
    return Wrench;
  }
</script>

{#if open && anchor && items.length > 0 && anchor.maxHeight >= 72}
  <BodyPortal>
    <div
      bind:this={menuEl}
      class="composer-skill-slash-menu"
      class:composer-skill-slash-menu-above={anchor.placement === "above"}
      style="left:{anchor.left}px;top:{anchor.top}px;max-height:{anchor.maxHeight}px"
      role="listbox"
      aria-label="Skills, tools, and commands"
    >
      {#if groups.skills.length > 0}
        <p class="composer-skill-slash-section">Skills</p>
        {#each groups.skills as item, i (item.id)}
          {@const idx = flatIndexFor("skills", i)}
          {@const Icon = iconFor(item.kind)}
          <button
            type="button"
            role="option"
            aria-selected={idx === highlightIndex}
            class="composer-skill-slash-row"
            class:composer-skill-slash-row-active={idx === highlightIndex}
            data-slash-index={idx}
            onpointerdown={(event) => {
              event.preventDefault();
              onSelect(item);
            }}
            onmouseenter={() => onHighlight?.(idx)}
          >
            <span class="composer-skill-slash-icon" aria-hidden="true">
              <Icon size={14} strokeWidth={1.75} />
            </span>
            <span class="composer-skill-slash-copy">
              <span class="composer-skill-slash-label">{item.label}</span>
              {#if item.hint}
                <span class="composer-skill-slash-hint">{item.hint}</span>
              {/if}
            </span>
          </button>
        {/each}
      {/if}

      {#if groups.commands.length > 0}
        <p class="composer-skill-slash-section">Commands</p>
        {#each groups.commands as item, i (item.id)}
          {@const idx = flatIndexFor("commands", i)}
          {@const Icon = iconFor(item.kind)}
          <button
            type="button"
            role="option"
            aria-selected={idx === highlightIndex}
            class="composer-skill-slash-row"
            class:composer-skill-slash-row-active={idx === highlightIndex}
            data-slash-index={idx}
            onpointerdown={(event) => {
              event.preventDefault();
              onSelect(item);
            }}
            onmouseenter={() => onHighlight?.(idx)}
          >
            <span class="composer-skill-slash-icon" aria-hidden="true">
              <Icon size={14} strokeWidth={1.75} />
            </span>
            <span class="composer-skill-slash-copy">
              <span class="composer-skill-slash-label">{item.label}</span>
              {#if item.hint}
                <span class="composer-skill-slash-hint">{item.hint}</span>
              {/if}
            </span>
          </button>
        {/each}
      {/if}

      {#if groups.tools.length > 0}
        <p class="composer-skill-slash-section">Tools</p>
        {#each groups.tools as item, i (item.id)}
          {@const idx = flatIndexFor("tools", i)}
          {@const Icon = iconFor(item.kind)}
          <button
            type="button"
            role="option"
            aria-selected={idx === highlightIndex}
            class="composer-skill-slash-row"
            class:composer-skill-slash-row-active={idx === highlightIndex}
            data-slash-index={idx}
            onpointerdown={(event) => {
              event.preventDefault();
              onSelect(item);
            }}
            onmouseenter={() => onHighlight?.(idx)}
          >
            <span class="composer-skill-slash-icon" aria-hidden="true">
              <Icon size={14} strokeWidth={1.75} />
            </span>
            <span class="composer-skill-slash-copy">
              <span class="composer-skill-slash-label">{item.label}</span>
              {#if item.hint}
                <span class="composer-skill-slash-hint">{item.hint}</span>
              {/if}
            </span>
          </button>
        {/each}
      {/if}
    </div>
  </BodyPortal>
{/if}
