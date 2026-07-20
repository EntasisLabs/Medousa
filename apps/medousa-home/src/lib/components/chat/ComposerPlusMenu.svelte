<script lang="ts">
  import { tick } from "svelte";
  import { Bot, LoaderCircle, Paperclip, Plus, UserRound } from "@lucide/svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { attachComposerMenuDismiss } from "$lib/utils/composerMenuDismiss";
  import { placeComposerPopover } from "$lib/utils/railPopover";

  interface Props {
    disabled?: boolean;
    /** Optional workshop entry for mobile. */
    showWorkshop?: boolean;
    onProfile?: () => void;
    onAgent?: () => void;
    onWorkshop?: () => void;
  }

  let {
    disabled = false,
    showWorkshop = false,
    onProfile,
    onAgent,
    onWorkshop,
  }: Props = $props();

  let open = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);

  $effect(() => {
    if (!open || !triggerEl || !menuEl) return;
    let frame = 0;
    const place = () => {
      if (!triggerEl || !menuEl) return;
      placeComposerPopover(triggerEl, menuEl);
      frame = window.requestAnimationFrame(() => {
        if (triggerEl && menuEl) placeComposerPopover(triggerEl, menuEl);
      });
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    window.visualViewport?.addEventListener("scroll", place);

    const detachDismiss = attachComposerMenuDismiss({
      isInside: (target) =>
        Boolean(menuEl?.contains(target) || triggerEl?.contains(target)),
      onDismiss: () => {
        open = false;
      },
    });

    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("scroll", place);
      detachDismiss();
    };
  });

  function attach() {
    open = false;
    void chat.attachFilesFromPicker();
  }

  function pickProfile() {
    open = false;
    window.setTimeout(() => onProfile?.(), 0);
  }

  function pickAgent() {
    open = false;
    window.setTimeout(() => onAgent?.(), 0);
  }

  function pickWorkshop() {
    open = false;
    window.setTimeout(() => onWorkshop?.(), 0);
  }
</script>

<button
  bind:this={triggerEl}
  type="button"
  class="composer-bar-icon-btn"
  aria-label="Add attachments, profile, or agent"
  aria-haspopup="menu"
  aria-expanded={open}
  disabled={disabled || chat.pendingMediaUploading}
  onclick={() => (open = !open)}
>
  {#if chat.pendingMediaUploading}
    <LoaderCircle size={16} class="animate-spin" />
  {:else}
    <Plus size={18} strokeWidth={2} />
  {/if}
</button>

{#if open}
  <div
    bind:this={menuEl}
    class="composer-anchored-menu composer-plus-menu-panel"
    role="menu"
    aria-label="Composer actions"
  >
    <button type="button" class="composer-plus-menu-item" role="menuitem" onclick={attach}>
      <Paperclip size={15} strokeWidth={1.75} class="shrink-0 opacity-70" />
      <span>Attach files</span>
    </button>
    {#if showWorkshop && onWorkshop}
      <button
        type="button"
        class="composer-plus-menu-item"
        role="menuitem"
        onclick={pickWorkshop}
      >
        <span class="composer-plus-menu-dot" aria-hidden="true"></span>
        <span>Workshop…</span>
      </button>
    {/if}
    <button type="button" class="composer-plus-menu-item" role="menuitem" onclick={pickProfile}>
      <UserRound size={15} strokeWidth={1.75} class="shrink-0 opacity-70" />
      <span>Profile…</span>
    </button>
    <button type="button" class="composer-plus-menu-item" role="menuitem" onclick={pickAgent}>
      <Bot size={15} strokeWidth={1.75} class="shrink-0 opacity-70" />
      <span>Agent…</span>
    </button>
  </div>
{/if}
