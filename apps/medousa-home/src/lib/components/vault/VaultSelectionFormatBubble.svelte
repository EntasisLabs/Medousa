<script lang="ts">
  /**
   * Cursor-style selection hover for Live formatting — only when text is selected.
   */
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { VAULT_FORMAT_ACTION_GROUPS } from "$lib/utils/vaultFormatActions";
  import type { MarkdownFormatAction } from "$lib/utils/vaultMarkdownEdit";
  import {
    MARKDOWN_COLOR_OPTIONS,
    normalizeMarkdownHexColor,
    type MarkdownColorToken,
  } from "$lib/utils/vaultMarkdownColors";

  interface Props {
    open: boolean;
    /** Viewport coords for the selection (bubble anchors above). */
    anchor: { left: number; top: number; width: number; height: number } | null;
    activeActions?: MarkdownFormatAction[];
    disabled?: boolean;
    onFormat: (action: MarkdownFormatAction) => void;
    onColor: (color: MarkdownColorToken) => void;
    onClose?: () => void;
  }

  let {
    open,
    anchor,
    activeActions = [],
    disabled = false,
    onFormat,
    onColor,
    onClose,
  }: Props = $props();

  const activeSet = $derived(new Set(activeActions));
  let hexDraft = $state("#F87171");
  let hexPickerOpen = $state(false);
  let colorsOpen = $state(false);

  const style = $derived.by(() => {
    if (!anchor) return "";
    const pad = 8;
    const bubbleW = 360;
    const bubbleH = colorsOpen || hexPickerOpen ? 72 : 40;
    let left = anchor.left + anchor.width / 2 - bubbleW / 2;
    left = Math.max(pad, Math.min(left, window.innerWidth - bubbleW - pad));
    let top = anchor.top - bubbleH - 8;
    if (top < pad) top = anchor.top + anchor.height + 8;
    top = Math.max(pad, Math.min(top, window.innerHeight - bubbleH - pad));
    return `left:${Math.round(left)}px;top:${Math.round(top)}px;`;
  });

  $effect(() => {
    if (!open) {
      colorsOpen = false;
      hexPickerOpen = false;
    }
  });

  function applyHex() {
    const normalized =
      normalizeMarkdownHexColor(hexDraft) ??
      normalizeMarkdownHexColor(`#${hexDraft.replace(/^#/, "")}`);
    if (!normalized) return;
    hexDraft = normalized;
    onColor(normalized);
    hexPickerOpen = false;
  }
</script>

{#if open && anchor}
  <BodyPortal>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- mousedown preventDefault keeps TipTap selection alive (else click clears it). -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="vault-selection-format-bubble"
      class:vault-selection-format-bubble--wide={colorsOpen || hexPickerOpen}
      style={style}
      role="toolbar"
      tabindex="-1"
      aria-label="Format selection"
      onmousedown={(event) => {
        event.preventDefault();
      }}
      onkeydown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          onClose?.();
        }
      }}
    >
      <div class="vault-selection-format-bubble__row">
        {#each VAULT_FORMAT_ACTION_GROUPS as group, groupIndex (group.label)}
          {#if groupIndex > 0}
            <span class="vault-selection-format-bubble__sep" aria-hidden="true"></span>
          {/if}
          {#each group.items as item (item.action)}
            <button
              type="button"
              class="vault-selection-format-bubble__btn"
              class:vault-selection-format-bubble__btn--active={activeSet.has(item.action)}
              title={item.title}
              aria-label={item.title}
              aria-pressed={activeSet.has(item.action)}
              {disabled}
              onclick={() => onFormat(item.action)}
            >
              <item.Icon size={14} strokeWidth={2} />
            </button>
          {/each}
        {/each}
        <span class="vault-selection-format-bubble__sep" aria-hidden="true"></span>
        <button
          type="button"
          class="vault-selection-format-bubble__btn vault-selection-format-bubble__btn--label"
          class:vault-selection-format-bubble__btn--active={colorsOpen}
          title="Text color"
          aria-label="Text color"
          aria-expanded={colorsOpen}
          {disabled}
          onclick={() => {
            colorsOpen = !colorsOpen;
            hexPickerOpen = false;
          }}
        >
          A
        </button>
      </div>

      {#if colorsOpen}
        <div class="vault-selection-format-bubble__colors" role="group" aria-label="Text color">
          {#each MARKDOWN_COLOR_OPTIONS as color (color.id)}
            <button
              type="button"
              class="vault-selection-format-bubble__swatch"
              title={color.label}
              aria-label={`Color: ${color.label}`}
              style:background-color={color.swatch}
              {disabled}
              onclick={() => onColor(color.id)}
            ></button>
          {/each}
          <button
            type="button"
            class="vault-selection-format-bubble__swatch vault-selection-format-bubble__swatch--custom"
            title="Custom hex"
            aria-label="Custom hex color"
            aria-expanded={hexPickerOpen}
            {disabled}
            onclick={() => (hexPickerOpen = !hexPickerOpen)}
          ></button>
          {#if hexPickerOpen}
            <input
              class="vault-selection-format-bubble__hex"
              type="text"
              spellcheck="false"
              maxlength="9"
              aria-label="Hex value"
              bind:value={hexDraft}
              {disabled}
              onkeydown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  applyHex();
                }
              }}
            />
            <button
              type="button"
              class="vault-selection-format-bubble__hex-apply"
              {disabled}
              onclick={applyHex}
            >
              Apply
            </button>
          {/if}
        </div>
      {/if}
    </div>
  </BodyPortal>
{/if}
