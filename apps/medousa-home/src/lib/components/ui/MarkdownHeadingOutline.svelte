<script lang="ts">
  /**
   * Shared “On this page” outline — full panel (guide) or minimized tick rail (vault).
   * Rail: click jumps; double-click expands to panel when `onToggleMode` is set.
   */
  import { PanelRightClose } from "@lucide/svelte";

  export interface OutlineItem {
    id: string;
    text: string;
    depth: number;
  }

  interface Props {
    items: OutlineItem[];
    activeId?: string | null;
    mode?: "panel" | "rail";
    /** Show minimize control on panel (vault). Rail expands via double-click. */
    showModeToggle?: boolean;
    onSelect: (id: string) => void;
    onToggleMode?: () => void;
    label?: string;
  }

  let {
    items,
    activeId = null,
    mode = "panel",
    showModeToggle = false,
    onSelect,
    onToggleMode,
    label = "On this page",
  }: Props = $props();

  /** Keep the rail bounded while retaining a useful neighborhood around the active heading. */
  const RAIL_WINDOW_SIZE = 21;
  const railItems = $derived.by(() => {
    if (items.length <= RAIL_WINDOW_SIZE) return items;
    const activeIndex = Math.max(
      0,
      activeId ? items.findIndex((item) => item.id === activeId) : 0,
    );
    const start = Math.min(
      Math.max(0, activeIndex - Math.floor(RAIL_WINDOW_SIZE / 2)),
      items.length - RAIL_WINDOW_SIZE,
    );
    return items.slice(start, start + RAIL_WINDOW_SIZE);
  });

  function onRailTickClick(id: string, event: MouseEvent) {
    // Ignore the synthetic follow-up click from a double-click pair.
    if (event.detail > 1) return;
    onSelect(id);
  }

  function onRailTickExpand(id: string, event: MouseEvent) {
    event.preventDefault();
    onSelect(id);
    onToggleMode?.();
  }
</script>

{#if items.length > 0}
  {#if mode === "panel"}
    <aside class="md-outline md-outline-panel" aria-label={label}>
      <div class="md-outline-panel-chrome">
        <p class="md-outline-label">{label}</p>
        {#if showModeToggle && onToggleMode}
          <button
            type="button"
            class="md-outline-mode-btn"
            aria-label="Minimize outline"
            title="Minimize"
            onclick={onToggleMode}
          >
            <PanelRightClose size={14} strokeWidth={2} />
          </button>
        {/if}
      </div>
      <ul class="md-outline-list">
        {#each items as item (item.id)}
          <li>
            <button
              type="button"
              class="md-outline-item"
              class:md-outline-item-h1={item.depth <= 1}
              class:md-outline-item-h3={item.depth >= 3}
              class:md-outline-item-active={item.id === activeId}
              title={item.text}
              onclick={() => onSelect(item.id)}
            >
              <span class="md-outline-item-text">{item.text}</span>
            </button>
          </li>
        {/each}
      </ul>
    </aside>
  {:else}
    <nav
      class="md-outline md-outline-rail"
      aria-label={onToggleMode ? `${label}. Double-click a mark to expand.` : label}
    >
      <div class="md-outline-rail-track">
        <ul class="md-outline-rail-list">
          {#each railItems as item (item.id)}
            <li class="md-outline-rail-row">
              <button
                type="button"
                class="md-outline-tick"
                class:md-outline-tick-h1={item.depth <= 1}
                class:md-outline-tick-h3={item.depth >= 3}
                class:md-outline-tick-active={item.id === activeId}
                aria-label={
                  onToggleMode
                    ? `${item.text}. Double-click to expand outline.`
                    : item.text
                }
                aria-current={item.id === activeId ? "true" : undefined}
                onclick={(event) => onRailTickClick(item.id, event)}
                ondblclick={(event) => onRailTickExpand(item.id, event)}
              >
                <span class="md-outline-tick-bar" aria-hidden="true"></span>
                <span class="md-outline-tick-label">
                  <span class="md-outline-tick-label-text">{item.text}</span>
                </span>
              </button>
            </li>
          {/each}
        </ul>
      </div>
    </nav>
  {/if}
{/if}

<style>
  .md-outline-panel {
    display: flex;
    flex-direction: column;
    min-height: 0;
    width: 15rem;
    flex-shrink: 0;
    overflow: hidden;
    border-left: 1px solid color-mix(in srgb, var(--color-surface-400) 22%, transparent);
    background: color-mix(in srgb, var(--color-surface-950) 72%, transparent);
    padding: 0.7rem 0.45rem 0.9rem 0.4rem;
  }

  .md-outline-panel-chrome {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.35rem;
    margin-bottom: 0.45rem;
    padding: 0 0.35rem 0.35rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-500) 22%, transparent);
  }

  .md-outline-label {
    margin: 0;
    font-size: 0.62rem;
    font-weight: 650;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-400));
  }

  .md-outline-mode-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.55rem;
    height: 1.55rem;
    margin: 0;
    padding: 0;
    border: 0;
    border-radius: 0.4rem;
    background: color-mix(in srgb, var(--color-surface-50) 5%, transparent);
    color: rgb(var(--color-surface-300));
    cursor: pointer;
  }

  .md-outline-mode-btn:hover {
    color: rgb(var(--color-surface-50));
    background: color-mix(in srgb, var(--color-surface-50) 12%, transparent);
  }

  .md-outline-list {
    margin: 0;
    padding: 0;
    list-style: none;
    overflow-x: hidden;
    overflow-y: auto;
    min-height: 0;
    flex: 1 1 auto;
    scrollbar-width: thin;
  }

  .md-outline-item {
    display: flex;
    align-items: center;
    width: 100%;
    min-width: 0;
    border: 0;
    border-radius: 0.4rem;
    background: transparent;
    padding: 0.32rem 0.4rem 0.32rem 0.45rem;
    text-align: left;
    font-size: 0.74rem;
    line-height: 1.3;
    color: rgb(var(--color-surface-400));
    cursor: pointer;
    border-left: 2px solid transparent;
  }

  .md-outline-item-text {
    display: block;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .md-outline-item-h1 {
    font-weight: 650;
    color: rgb(var(--color-surface-200));
    margin-top: 0.15rem;
  }

  .md-outline-item-h1:first-child {
    margin-top: 0;
  }

  .md-outline-item-h3 {
    padding-left: 0.95rem;
    font-size: 0.7rem;
    color: rgb(var(--color-surface-500));
  }

  .md-outline-item:hover {
    color: rgb(var(--color-surface-100));
    background: color-mix(in srgb, var(--color-surface-50) 6%, transparent);
  }

  .md-outline-item-active {
    color: rgb(var(--color-surface-50));
    background: color-mix(in srgb, var(--color-surface-50) 9%, transparent);
    border-left-color: rgb(var(--color-surface-50));
    font-weight: 600;
  }

  .md-outline-item-h3.md-outline-item-active {
    color: rgb(var(--color-surface-100));
  }

  /* —— Minimized tick rail —— */
  .md-outline-rail {
    position: absolute;
    top: 50%;
    right: 0.25rem;
    z-index: 5;
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 1.45rem;
    max-height: min(58vh, 22rem);
    box-sizing: border-box;
    transform: translateY(-50%);
    pointer-events: none;
  }

  .md-outline-rail-track {
    position: relative;
    width: 100%;
    pointer-events: auto;
    padding: 0.15rem 0.1rem;
    background: transparent;
    border: 0;
    box-shadow: none;
  }

  .md-outline-rail-list {
    position: relative;
    z-index: 1;
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.12rem;
  }

  .md-outline-rail-row {
    display: flex;
    justify-content: center;
    width: 100%;
    height: 0.48rem;
  }

  .md-outline-tick {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 0.48rem;
    margin: 0;
    padding: 0;
    border: 0;
    background: transparent;
    cursor: pointer;
  }

  .md-outline-tick-bar {
    display: block;
    width: 0.62rem;
    height: 1.5px;
    border-radius: 999px;
    background: rgb(var(--color-surface-600));
    transition:
      width 120ms ease,
      height 120ms ease,
      background 120ms ease;
  }

  .md-outline-tick-h1 .md-outline-tick-bar {
    width: 0.88rem;
    height: 2px;
    background: rgb(var(--color-surface-500));
  }

  .md-outline-tick-h3 .md-outline-tick-bar {
    width: 0.4rem;
    height: 1.5px;
    background: rgb(var(--color-surface-700));
  }

  .md-outline-tick:hover .md-outline-tick-bar {
    width: 1rem;
    height: 2px;
    background: rgb(var(--color-surface-200));
  }

  .md-outline-tick-active .md-outline-tick-bar,
  .md-outline-tick-active:hover .md-outline-tick-bar {
    width: 1.3rem;
    height: 2px;
    background: rgb(var(--color-surface-50));
  }

  .md-outline-tick-h3:hover .md-outline-tick-bar {
    width: 0.72rem;
    height: 2px;
  }

  .md-outline-tick-h3.md-outline-tick-active .md-outline-tick-bar,
  .md-outline-tick-h3.md-outline-tick-active:hover .md-outline-tick-bar {
    width: 1.3rem;
    height: 2px;
  }

  .md-outline-tick-label {
    position: absolute;
    right: 100%;
    top: 50%;
    transform: translateY(-50%) scale(0.98);
    display: flex;
    flex-direction: column;
    width: min(20rem, calc(100vw - 3rem));
    margin-right: 0.55rem;
    padding: 0.55rem 0.62rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-400) 18%, transparent);
    border-radius: 0.75rem;
    background: color-mix(in srgb, var(--color-surface-900) 97%, transparent);
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.42);
    color: rgb(var(--color-surface-50));
    font-size: 0.78rem;
    font-weight: 550;
    line-height: 1.3;
    opacity: 0;
    pointer-events: none;
    transition:
      opacity 100ms ease,
      transform 100ms ease;
  }

  .md-outline-tick-label-text {
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    white-space: normal;
  }

  /* Label is the natural click target on hover — keep it on the button. */
  .md-outline-tick:hover .md-outline-tick-label,
  .md-outline-tick:focus-visible .md-outline-tick-label,
  .md-outline-tick:focus-within .md-outline-tick-label {
    opacity: 1;
    transform: translateY(-50%) scale(1);
    pointer-events: auto;
  }

  @media (max-width: 640px) {
    .md-outline-rail {
      display: none;
    }
  }
</style>
