<script lang="ts">
  import { ChevronDown, ExternalLink } from "@lucide/svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import type { MarkdownFormatAction } from "$lib/utils/vaultMarkdownEdit";
  import {
    MARKDOWN_COLOR_OPTIONS,
    normalizeMarkdownHexColor,
    type MarkdownColorToken,
  } from "$lib/utils/vaultMarkdownColors";
  import { VAULT_FORMAT_ACTION_GROUPS } from "$lib/utils/vaultFormatActions";

  interface Props {
    disabled?: boolean;
    /** Force compact Format menu (narrow sticky pop-out). */
    compact?: boolean;
    /** Subtle float-note control at the trailing end of the bar. */
    showFloat?: boolean;
    onFloat?: () => void;
    /** Format actions that already wrap the current selection. */
    activeActions?: MarkdownFormatAction[];
    onFormat: (action: MarkdownFormatAction) => void;
    onColor: (color: MarkdownColorToken) => void;
  }

  let {
    disabled = false,
    compact: compactProp,
    showFloat = false,
    onFloat,
    activeActions = [],
    onFormat,
    onColor,
  }: Props = $props();

  const activeSet = $derived(new Set(activeActions));

  let expanded = $state(false);
  let customHex = $state("#F87171");
  let hexDraft = $state("#F87171");
  let hexPickerOpen = $state(false);
  const compact = $derived(compactProp ?? layout.isMobile);

  function openHexPicker() {
    hexDraft = customHex;
    hexPickerOpen = true;
  }

  function cancelHexPicker() {
    hexPickerOpen = false;
  }

  function applyHexPicker() {
    const normalized = normalizeMarkdownHexColor(hexDraft) ?? normalizeMarkdownHexColor(`#${hexDraft.replace(/^#/, "")}`);
    if (!normalized) return;
    hexDraft = normalized;
    customHex = normalized;
    onColor(normalized);
    hexPickerOpen = false;
  }

  const groups = VAULT_FORMAT_ACTION_GROUPS;
</script>

<div
  class="vault-format-bar border-b border-surface-500/40 bg-surface-900/80 px-3 py-1.5 {compact
    ? 'flex items-center gap-2'
    : 'flex flex-wrap items-center gap-1'}"
  role="toolbar"
  aria-label="Formatting"
>
  {#if compact}
    <button
      type="button"
      class="btn btn-sm variant-soft-surface inline-flex items-center gap-1"
      aria-expanded={expanded}
      onclick={() => (expanded = !expanded)}
    >
      Format
      <ChevronDown
        size={14}
        strokeWidth={2}
        class="transition-transform {expanded ? 'rotate-180' : ''}"
      />
    </button>
    <p class="workshop-faint text-[11px]">Type <kbd class="vault-kbd">/</kbd> for blocks</p>
  {/if}

  {#if !compact || expanded}
    <div class="{compact ? 'flex w-full flex-wrap items-center gap-1 border-t border-surface-500/30 pt-2' : 'contents'}">
      {#each groups as group, groupIndex (group.label)}
        {#if groupIndex > 0}
          <span class="mx-0.5 h-5 w-px bg-surface-600/80" aria-hidden="true"></span>
        {/if}
        {#each group.items as item (item.action)}
          <button
            type="button"
            class="vault-format-btn"
            class:vault-format-btn--active={activeSet.has(item.action)}
            title={item.title}
            aria-label={item.title}
            aria-pressed={activeSet.has(item.action)}
            {disabled}
            onclick={() => onFormat(item.action)}
          >
            <item.Icon size={15} strokeWidth={2} />
          </button>
        {/each}
      {/each}

      <span class="mx-0.5 h-5 w-px bg-surface-600/80" aria-hidden="true"></span>

      <div class="flex items-center gap-1" role="group" aria-label="Text color">
        {#each MARKDOWN_COLOR_OPTIONS as color (color.id)}
          <button
            type="button"
            class="vault-color-swatch"
            title={color.label}
            aria-label={`Color: ${color.label}`}
            style:background-color={color.swatch}
            {disabled}
            onclick={() => onColor(color.id)}
          ></button>
        {/each}
        <div class="vault-color-hex">
          <button
            type="button"
            class="vault-color-swatch vault-color-swatch--custom"
            title="Custom hex color"
            aria-label="Custom hex color"
            aria-expanded={hexPickerOpen}
            style:background={hexPickerOpen ? hexDraft : undefined}
            {disabled}
            onclick={() => {
              if (hexPickerOpen) cancelHexPicker();
              else openHexPicker();
            }}
          ></button>
          {#if hexPickerOpen}
            <div
              class="vault-color-hex-popover"
              role="dialog"
              aria-label="Pick a custom color"
            >
              <input
                type="color"
                class="vault-color-hex-wheel"
                bind:value={hexDraft}
                {disabled}
              />
              <input
                class="vault-color-hex-field"
                type="text"
                spellcheck="false"
                maxlength="9"
                aria-label="Hex value"
                bind:value={hexDraft}
                {disabled}
                onkeydown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    applyHexPicker();
                  }
                  if (event.key === "Escape") {
                    event.preventDefault();
                    cancelHexPicker();
                  }
                }}
              />
              <button
                type="button"
                class="vault-color-hex-apply"
                {disabled}
                onclick={applyHexPicker}
              >
                Apply
              </button>
            </div>
          {/if}
        </div>
      </div>

      {#if showFloat && onFloat}
        <button
          type="button"
          class="vault-format-btn vault-format-btn--float ml-auto"
          title="Float note (always on top)"
          aria-label="Float note"
          {disabled}
          onclick={() => onFloat()}
        >
          <ExternalLink size={14} strokeWidth={1.75} />
        </button>
      {/if}
    </div>
  {/if}
</div>

{#if hexPickerOpen}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="vault-color-hex-scrim"
    role="presentation"
    onclick={cancelHexPicker}
  ></div>
{/if}
