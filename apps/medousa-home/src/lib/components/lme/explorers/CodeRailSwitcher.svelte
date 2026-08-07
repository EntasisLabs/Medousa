<script lang="ts">
  import { Check } from "@lucide/svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";

  export type CodeRailSwitcherItem = {
    id: string;
    label: string;
    detail?: string | null;
    active?: boolean;
  };

  interface Props {
    /** Trigger text (repo name or thread title). */
    value: string;
    items: CodeRailSwitcherItem[];
    /** Accessible name, e.g. "Project" / "Thread". */
    label: string;
    title?: string;
    emptyHint?: string;
    soft?: boolean;
    onSelect: (id: string) => void | Promise<void>;
  }

  let {
    value,
    items,
    label,
    title = label,
    emptyHint = "Nothing here yet",
    soft = false,
    onSelect,
  }: Props = $props();

  let menuOpen = $state(false);

  async function pick(id: string) {
    menuOpen = false;
    await onSelect(id);
  }
</script>

<OverflowMenu
  bind:open={menuOpen}
  {label}
  {title}
  align="left"
  class="min-w-0 shrink"
  panelClass="w-[min(18rem,calc(100vw-2rem))] rounded-lg border border-surface-500/40 bg-surface-900 p-1.5 shadow-xl"
>
  {#snippet trigger({ open, toggle })}
    <button
      type="button"
      class="vault-dock-branch"
      class:vault-dock-branch--active={open}
      class:code-dock-crumb--soft={soft}
      aria-expanded={open}
      aria-haspopup="menu"
      aria-label="{label}: {value}"
      title={title}
      onclick={toggle}
    >
      <span class="vault-dock-branch__label">{value}</span>
    </button>
  {/snippet}
  {#if items.length === 0}
    <p class="px-2 py-1.5 text-[10px] text-content-quiet">{emptyHint}</p>
  {:else}
    {#each items as item (item.id)}
      <button
        type="button"
        role="menuitem"
        class="code-dock-menu-item"
        class:code-dock-menu-item--active={item.active}
        aria-current={item.active ? "true" : undefined}
        onclick={() => void pick(item.id)}
      >
        <Check
          size={12}
          strokeWidth={2}
          class="code-dock-menu-check {item.active ? 'opacity-100' : 'opacity-0'}"
        />
        <span class="min-w-0 flex-1">
          <span class="code-dock-menu-label">{item.label}</span>
          {#if item.detail}
            <span class="code-dock-menu-detail">{item.detail}</span>
          {/if}
        </span>
      </button>
    {/each}
  {/if}
</OverflowMenu>

<style>
  .code-dock-crumb--soft :global(.vault-dock-branch__label) {
    color: color-mix(
      in srgb,
      rgb(var(--theme-text)) 72%,
      rgb(var(--theme-text-secondary))
    );
    font-weight: 500;
  }

  .code-dock-menu-item {
    display: flex;
    width: 100%;
    align-items: flex-start;
    gap: 0.4rem;
    border: 0;
    border-radius: 0.35rem;
    background: transparent;
    padding: 0.4rem 0.45rem;
    text-align: left;
    cursor: pointer;
  }

  .code-dock-menu-item:hover {
    background: rgb(var(--color-surface-800) / 0.7);
  }

  .code-dock-menu-item--active {
    background: rgb(var(--color-primary-500) / 0.1);
  }

  :global(.code-dock-menu-check) {
    margin-top: 0.15rem;
    flex-shrink: 0;
    color: rgb(var(--theme-link));
  }

  .code-dock-menu-label {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: rgb(var(--theme-text));
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    font-size: 13px;
    font-weight: 500;
    letter-spacing: 0;
  }

  .code-dock-menu-detail {
    display: block;
    margin-top: 0.15rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: color-mix(
      in srgb,
      rgb(var(--theme-text)) 55%,
      rgb(var(--theme-text-secondary))
    );
    font-family:
      -apple-system,
      BlinkMacSystemFont,
      "Segoe UI",
      system-ui,
      sans-serif;
    font-size: 11px;
    font-weight: 400;
  }
</style>
