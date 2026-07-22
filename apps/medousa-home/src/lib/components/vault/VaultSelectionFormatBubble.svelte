<script lang="ts">
  /**
   * Live selection format capsule — single row, Word/dock progressive disclosure,
   * optional < > page rotation (Shape ↔ Voice).
   */
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { Check, ChevronDown, ChevronLeft, ChevronRight } from "@lucide/svelte";
  import { tick } from "svelte";
  import {
    liveBlockStyleFromActions,
    VAULT_LIVE_BLOCK_STYLE_OPTIONS,
    VAULT_LIVE_EMPHASIS_ACTIONS,
    VAULT_LIVE_LINK_ACTION,
    VAULT_LIVE_LIST_ACTIONS,
  } from "$lib/utils/vaultFormatActions";
  import type { MarkdownFormatAction } from "$lib/utils/vaultMarkdownEdit";
  import {
    MARKDOWN_COLOR_OPTIONS,
    normalizeMarkdownHexColor,
    type MarkdownColorToken,
  } from "$lib/utils/vaultMarkdownColors";
  import {
    MARKDOWN_FONT_FAMILY_OPTIONS,
    MARKDOWN_FONT_SIZE_OPTIONS,
    type MarkdownFontFamily,
  } from "$lib/utils/vaultMarkdownFonts";

  type MenuId = "blockStyle" | "type" | "color" | null;
  type PageId = 0 | 1;

  /** Gap between bubble bottom and selection top — keep highlight readable. */
  const SELECTION_GAP_PX = 14;

  interface Props {
    open: boolean;
    /** Viewport coords for the selection (bubble anchors above). */
    anchor: { left: number; top: number; width: number; height: number } | null;
    activeActions?: MarkdownFormatAction[];
    activeFontFamily?: MarkdownFontFamily | null;
    activeFontSize?: string | null;
    activeColor?: string | null;
    disabled?: boolean;
    onFormat: (action: MarkdownFormatAction) => void;
    onColor: (color: MarkdownColorToken) => void;
    onFontFamily: (font: MarkdownFontFamily) => void;
    onFontSize: (size: string) => void;
    onClose?: () => void;
  }

  let {
    open,
    anchor,
    activeActions = [],
    activeFontFamily = null,
    activeFontSize = null,
    activeColor = null,
    disabled = false,
    onFormat,
    onColor,
    onFontFamily,
    onFontSize,
    onClose,
  }: Props = $props();

  const activeSet = $derived(new Set(activeActions));
  const blockStyle = $derived(liveBlockStyleFromActions(activeSet));
  const typeTriggerLabel = $derived(
    activeFontFamily
      ? activeFontFamily.charAt(0).toUpperCase() + activeFontFamily.slice(1)
      : "Aa",
  );
  const colorUnderline = $derived(activeColor?.trim() || "currentColor");

  let page = $state<PageId>(0);
  let menu = $state<MenuId>(null);
  let hexDraft = $state("#F87171");
  let hexPickerOpen = $state(false);
  let pageDir = $state<"next" | "prev">("next");
  let bubbleEl = $state<HTMLElement | null>(null);
  /** Measured capsule height (row + open menu) — drives placement above the highlight. */
  let measuredH = $state(44);

  /** Rough first-paint guess before measure; type menu is the tall one. */
  function estimateClearance(): number {
    const row = 38;
    if (!menu) return row;
    if (menu === "color") return row + (hexPickerOpen ? 96 : 52);
    if (menu === "type") return row + 236;
    if (menu === "blockStyle") return row + 176;
    return row + 120;
  }

  const style = $derived.by(() => {
    if (!anchor) return "";
    const pad = 8;
    const bubbleW = Math.max(bubbleEl?.offsetWidth ?? 280, 200);
    const bubbleH = Math.max(measuredH, estimateClearance());
    let left = anchor.left + anchor.width / 2 - bubbleW / 2;
    left = Math.max(pad, Math.min(left, window.innerWidth - bubbleW - pad));
    let top = anchor.top - bubbleH - SELECTION_GAP_PX;
    if (top < pad) {
      // Not enough room above — sit below the highlight instead of covering it.
      top = anchor.top + anchor.height + SELECTION_GAP_PX;
    }
    top = Math.max(pad, Math.min(top, window.innerHeight - bubbleH - pad));
    return `left:${Math.round(left)}px;top:${Math.round(top)}px;`;
  });

  $effect(() => {
    if (!open) {
      page = 0;
      menu = null;
      hexPickerOpen = false;
      measuredH = 44;
    }
  });

  // Re-measure after open / menu / page / hex-picker changes so tall menus clear the text.
  $effect(() => {
    if (!open) return;
    void page;
    void menu;
    void hexPickerOpen;
    void anchor?.top;
    void tick().then(() => {
      requestAnimationFrame(() => {
        if (!bubbleEl) return;
        const h = bubbleEl.offsetHeight;
        if (h > 0) measuredH = h;
      });
    });
  });

  function closeMenus() {
    menu = null;
    hexPickerOpen = false;
  }

  function toggleMenu(next: MenuId) {
    if (menu === next) {
      closeMenus();
      return;
    }
    hexPickerOpen = false;
    menu = next;
  }

  function setPage(next: PageId) {
    if (next === page) return;
    pageDir = next > page ? "next" : "prev";
    closeMenus();
    page = next;
  }

  function seedHexDraft() {
    const fromActive = normalizeMarkdownHexColor(activeColor ?? "");
    if (fromActive) {
      hexDraft = fromActive;
      return;
    }
    if (!normalizeMarkdownHexColor(hexDraft)) {
      hexDraft = "#F87171";
    }
  }

  function openHexPicker() {
    seedHexDraft();
    hexPickerOpen = true;
  }

  function applyHex() {
    const normalized =
      normalizeMarkdownHexColor(hexDraft) ??
      normalizeMarkdownHexColor(`#${hexDraft.replace(/^#/, "")}`);
    if (!normalized) return;
    hexDraft = normalized;
    onColor(normalized);
    hexPickerOpen = false;
  }

  function onToolbarKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      if (hexPickerOpen) {
        hexPickerOpen = false;
        return;
      }
      if (menu) {
        closeMenus();
        return;
      }
      onClose?.();
    }
  }
</script>

{#if open && anchor}
  <BodyPortal>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- mousedown preventDefault keeps TipTap selection alive (else click clears it). -->
    <div
      bind:this={bubbleEl}
      class="vault-selection-format-bubble"
      style={style}
      role="toolbar"
      tabindex="-1"
      aria-label="Format selection"
      onmousedown={(event) => {
        event.preventDefault();
      }}
      onkeydown={onToolbarKeydown}
    >
      <div
        class="vault-selection-format-bubble__track"
        class:vault-selection-format-bubble__track--next={pageDir === "next"}
        class:vault-selection-format-bubble__track--prev={pageDir === "prev"}
      >
        {#key page}
          {#if page === 0}
          <!-- Page 1: Shape -->
          <div
            class="vault-selection-format-bubble__row"
            role="group"
            aria-label="Shape"
          >
            {#each VAULT_LIVE_EMPHASIS_ACTIONS as item (item.action)}
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

            <span class="vault-selection-format-bubble__sep" aria-hidden="true"></span>

            <button
              type="button"
              class="vault-selection-format-bubble__state"
              class:vault-selection-format-bubble__state--open={menu === "blockStyle"}
              title="Block style"
              aria-label={`Block style: ${blockStyle.label}`}
              aria-expanded={menu === "blockStyle"}
              aria-haspopup="menu"
              {disabled}
              onclick={() => toggleMenu("blockStyle")}
            >
              <span class="vault-selection-format-bubble__state-label">{blockStyle.short}</span>
              <ChevronDown size={12} strokeWidth={2} aria-hidden="true" />
            </button>

            <span class="vault-selection-format-bubble__sep" aria-hidden="true"></span>

            {#each VAULT_LIVE_LIST_ACTIONS as item (item.action)}
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

            <span class="vault-selection-format-bubble__sep" aria-hidden="true"></span>

            <button
              type="button"
              class="vault-selection-format-bubble__btn"
              class:vault-selection-format-bubble__btn--active={activeSet.has(
                VAULT_LIVE_LINK_ACTION.action,
              )}
              title={VAULT_LIVE_LINK_ACTION.title}
              aria-label={VAULT_LIVE_LINK_ACTION.title}
              aria-pressed={activeSet.has(VAULT_LIVE_LINK_ACTION.action)}
              {disabled}
              onclick={() => onFormat(VAULT_LIVE_LINK_ACTION.action)}
            >
              <VAULT_LIVE_LINK_ACTION.Icon size={14} strokeWidth={2} />
            </button>

            <span class="vault-selection-format-bubble__sep" aria-hidden="true"></span>

            <button
              type="button"
              class="vault-selection-format-bubble__pager"
              title="Voice tools"
              aria-label="Next toolbar page"
              {disabled}
              onclick={() => setPage(1)}
            >
              <ChevronRight size={14} strokeWidth={2} />
            </button>
          </div>
          {:else}
          <!-- Page 2: Voice -->
          <div
            class="vault-selection-format-bubble__row"
            role="group"
            aria-label="Voice"
          >
            <button
              type="button"
              class="vault-selection-format-bubble__pager"
              title="Shape tools"
              aria-label="Previous toolbar page"
              {disabled}
              onclick={() => setPage(0)}
            >
              <ChevronLeft size={14} strokeWidth={2} />
            </button>

            <span class="vault-selection-format-bubble__sep" aria-hidden="true"></span>

            <button
              type="button"
              class="vault-selection-format-bubble__state vault-selection-format-bubble__state--type"
              class:vault-selection-format-bubble__state--open={menu === "type"}
              class:vault-selection-format-bubble__state--font-sans={activeFontFamily === "sans"}
              class:vault-selection-format-bubble__state--font-serif={activeFontFamily === "serif"}
              class:vault-selection-format-bubble__state--font-mono={activeFontFamily === "mono"}
              title="Font and size"
              aria-label={`Type: ${typeTriggerLabel}${activeFontSize ? `, ${activeFontSize}` : ""}`}
              aria-expanded={menu === "type"}
              aria-haspopup="menu"
              {disabled}
              onclick={() => toggleMenu("type")}
            >
              <span class="vault-selection-format-bubble__state-label">{typeTriggerLabel}</span>
              {#if activeFontSize}
                <span class="vault-selection-format-bubble__state-meta"
                  >{activeFontSize.toUpperCase()}</span
                >
              {/if}
              <ChevronDown size={12} strokeWidth={2} aria-hidden="true" />
            </button>

            <span class="vault-selection-format-bubble__sep" aria-hidden="true"></span>

            <button
              type="button"
              class="vault-selection-format-bubble__state vault-selection-format-bubble__state--color"
              class:vault-selection-format-bubble__state--open={menu === "color"}
              title="Text color"
              aria-label="Text color"
              aria-expanded={menu === "color"}
              aria-haspopup="menu"
              {disabled}
              onclick={() => toggleMenu("color")}
            >
              <span
                class="vault-selection-format-bubble__color-letter"
                style:--vault-format-color={colorUnderline}>A</span
              >
              <ChevronDown size={12} strokeWidth={2} aria-hidden="true" />
            </button>
          </div>
          {/if}
        {/key}
      </div>

      {#if menu === "blockStyle"}
        <div
          class="vault-selection-format-bubble__menu"
          role="menu"
          aria-label="Block style"
        >
          {#each VAULT_LIVE_BLOCK_STYLE_OPTIONS as opt (opt.action)}
            {@const selected =
              opt.action === "paragraph"
                ? blockStyle.action === "paragraph"
                : activeSet.has(opt.action)}
            <button
              type="button"
              class="vault-selection-format-bubble__option"
              class:vault-selection-format-bubble__option--selected={selected}
              role="menuitemradio"
              aria-checked={selected}
              {disabled}
              onclick={() => {
                onFormat(opt.action);
                closeMenus();
              }}
            >
              <span class="vault-selection-format-bubble__option-main">
                <opt.Icon size={14} strokeWidth={2} />
                <span class="vault-selection-format-bubble__option-label">{opt.label}</span>
                <span class="vault-selection-format-bubble__option-meta">{opt.short}</span>
              </span>
              {#if selected}
                <Check
                  class="vault-selection-format-bubble__check"
                  size={14}
                  strokeWidth={2.25}
                />
              {/if}
            </button>
          {/each}
        </div>
      {/if}

      {#if menu === "type"}
        <div class="vault-selection-format-bubble__menu" role="menu" aria-label="Type">
          <div class="vault-selection-format-bubble__menu-section" role="group" aria-label="Family">
            <div class="vault-selection-format-bubble__menu-heading">Family</div>
            {#each MARKDOWN_FONT_FAMILY_OPTIONS as font (font.id)}
              {@const selected = activeFontFamily === font.id}
              <button
                type="button"
                class="vault-selection-format-bubble__option vault-selection-format-bubble__option--font-{font.id}"
                class:vault-selection-format-bubble__option--selected={selected}
                role="menuitemradio"
                aria-checked={selected}
                {disabled}
                onclick={() => onFontFamily(font.id)}
              >
                <span class="vault-selection-format-bubble__option-main">
                  <span class="vault-selection-format-bubble__option-label">{font.label}</span>
                </span>
                {#if selected}
                  <Check
                    class="vault-selection-format-bubble__check"
                    size={14}
                    strokeWidth={2.25}
                  />
                {/if}
              </button>
            {/each}
          </div>
          <div class="vault-selection-format-bubble__menu-sep" aria-hidden="true"></div>
          <div class="vault-selection-format-bubble__menu-section" role="group" aria-label="Size">
            <div class="vault-selection-format-bubble__menu-heading">Size</div>
            <div class="vault-selection-format-bubble__size-row">
              {#each MARKDOWN_FONT_SIZE_OPTIONS as size (size.id)}
                {@const selected = activeFontSize === size.id}
                <button
                  type="button"
                  class="vault-selection-format-bubble__size"
                  class:vault-selection-format-bubble__size--selected={selected}
                  role="menuitemradio"
                  aria-checked={selected}
                  title={`Size: ${size.id}`}
                  aria-label={`Size: ${size.id}`}
                  {disabled}
                  onclick={() => onFontSize(size.id)}
                >
                  {size.label}
                </button>
              {/each}
            </div>
          </div>
        </div>
      {/if}

      {#if menu === "color"}
        <div
          class="vault-selection-format-bubble__menu"
          role="menu"
          aria-label="Text color"
        >
          <div class="vault-selection-format-bubble__swatches" role="group">
            {#each MARKDOWN_COLOR_OPTIONS as color (color.id)}
              <button
                type="button"
                class="vault-selection-format-bubble__swatch"
                class:vault-selection-format-bubble__swatch--selected={activeColor ===
                  color.id}
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
              title="Custom color"
              aria-label="Custom color"
              aria-expanded={hexPickerOpen}
              style:background={hexPickerOpen ? hexDraft : undefined}
              {disabled}
              onclick={() => {
                if (hexPickerOpen) hexPickerOpen = false;
                else openHexPicker();
              }}
            ></button>
          </div>
          {#if hexPickerOpen}
            <div
              class="vault-selection-format-bubble__hex-row"
              role="dialog"
              aria-label="Pick a custom color"
            >
              <input
                type="color"
                class="vault-selection-format-bubble__hex-wheel"
                bind:value={hexDraft}
                {disabled}
                onmousedown={(event) => event.stopPropagation()}
              />
              <input
                class="vault-selection-format-bubble__hex"
                type="text"
                spellcheck="false"
                maxlength="9"
                aria-label="Hex value"
                bind:value={hexDraft}
                {disabled}
                onmousedown={(event) => event.stopPropagation()}
                onkeydown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    applyHex();
                  }
                  if (event.key === "Escape") {
                    event.preventDefault();
                    hexPickerOpen = false;
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
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </BodyPortal>
{/if}
