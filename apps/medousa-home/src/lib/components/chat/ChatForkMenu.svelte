<script lang="ts">
  import { GitFork, LoaderCircle, TextCursorInput } from "@lucide/svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";

  interface Props {
    hasDraft?: boolean;
    busy?: boolean;
    mobile?: boolean;
    visible?: boolean;
    onFork: (includeDraft: boolean) => void | Promise<void>;
  }

  let {
    hasDraft = false,
    busy = false,
    mobile = false,
    visible = false,
    onFork,
  }: Props = $props();
  let open = $state(false);

  function run(includeDraft: boolean) {
    if (busy || (includeDraft && !hasDraft)) return;
    open = false;
    void onFork(includeDraft);
  }
</script>

<OverflowMenu
  bind:open
  align="right"
  panelWidth={220}
  panelClass="chat-fork-menu"
  class="chat-fork-menu-host"
>
  {#snippet trigger({ open: menuOpen, toggle })}
    <button
      type="button"
      class="chat-turn-action"
      class:chat-turn-action--visible={menuOpen || busy || mobile || visible}
      title="Fork conversation"
      aria-label="Fork conversation"
      aria-haspopup="menu"
      aria-expanded={menuOpen}
      disabled={busy}
      onclick={toggle}
    >
      {#if busy}
        <LoaderCircle size={14} strokeWidth={1.75} class="animate-spin" />
      {:else}
        <GitFork size={14} strokeWidth={1.75} />
      {/if}
    </button>
  {/snippet}

  <button
    type="button"
    role="menuitem"
    class="chat-fork-menu-item"
    onclick={() => run(false)}
  >
    <GitFork size={14} strokeWidth={1.75} />
    <span>
      <strong>Fork from here</strong>
      <small>Start from this committed point</small>
    </span>
  </button>
  <button
    type="button"
    role="menuitem"
    class="chat-fork-menu-item"
    disabled={!hasDraft}
    title={hasDraft ? undefined : "Write a draft first"}
    onclick={() => run(true)}
  >
    <TextCursorInput size={14} strokeWidth={1.75} />
    <span>
      <strong>Fork with draft</strong>
      <small>{hasDraft ? "Carry the current draft into the fork" : "Current composer is empty"}</small>
    </span>
  </button>
</OverflowMenu>

<style>
  :global(.chat-fork-menu) {
    width: 13.75rem;
    padding: 0.3rem;
    border: 1px solid rgb(var(--theme-border) / 0.56);
    border-radius: 0.65rem;
    background: rgb(var(--color-surface-900));
    box-shadow: 0 16px 42px rgb(0 0 0 / 0.24);
  }

  :global(.chat-fork-menu-item) {
    display: flex;
    width: 100%;
    align-items: flex-start;
    gap: 0.65rem;
    padding: 0.52rem 0.58rem;
    border: 0;
    border-radius: 0.45rem;
    background: transparent;
    color: rgb(var(--theme-text-secondary));
    text-align: left;
  }

  :global(.chat-fork-menu-item:hover:not(:disabled)),
  :global(.chat-fork-menu-item:focus-visible:not(:disabled)) {
    background: rgb(var(--theme-card-hover) / 0.72);
    color: rgb(var(--theme-text-primary));
    outline: none;
  }

  :global(.chat-fork-menu-item:disabled) {
    cursor: default;
    opacity: 0.42;
  }

  :global(.chat-fork-menu-item > svg) {
    flex: none;
    margin-top: 0.13rem;
  }

  :global(.chat-fork-menu-item > span) {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 0.1rem;
  }

  :global(.chat-fork-menu-item strong) {
    font-size: 0.75rem;
    font-weight: 500;
    line-height: 1.25;
  }

  :global(.chat-fork-menu-item small) {
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.65rem;
    line-height: 1.35;
  }
</style>
