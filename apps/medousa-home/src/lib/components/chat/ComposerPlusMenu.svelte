<script lang="ts">
  import { tick } from "svelte";
  import {
    ArchiveRestore,
    Bot,
    BookmarkPlus,
    LoaderCircle,
    Paperclip,
    Plus,
    Trash2,
    UserRound,
  } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import {
    createPromptStash,
    deletePromptStash,
    listPromptStashes,
  } from "$lib/daemon/session";
  import { chat } from "$lib/stores/chat.svelte";
  import type { PromptStash, SessionRef } from "$lib/types/generated/daemon_api";
  import { attachComposerMenuDismiss } from "$lib/utils/composerMenuDismiss";
  import { placeComposerPopover } from "$lib/utils/railPopover";

  interface Props {
    disabled?: boolean;
    /** Optional workshop entry for mobile. */
    showWorkshop?: boolean;
    /** Keep drafts in the compact add menu on mobile layouts. */
    showStashes?: boolean;
    onProfile?: () => void;
    onAgent?: () => void;
    onWorkshop?: () => void;
    mode?: string;
    model?: string;
  }

  let {
    disabled = false,
    showWorkshop = false,
    showStashes = true,
    onProfile,
    onAgent,
    onWorkshop,
    mode,
    model,
  }: Props = $props();

  let open = $state(false);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);
  let stashes = $state<PromptStash[]>([]);
  let stashesLoading = $state(false);
  let stashSaving = $state(false);
  let deletingStashId = $state<string | null>(null);
  let stashError = $state<string | null>(null);

  const canStash = $derived(
    !disabled && (chat.draft.trim().length > 0 || chat.pendingMediaRefs.length > 0),
  );

  $effect(() => {
    stashes.length;
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

  $effect(() => {
    if (!open || !showStashes) return;
    void refreshStashes();
  });

  async function refreshStashes() {
    stashesLoading = true;
    stashError = null;
    try {
      stashes = await listPromptStashes();
    } catch (err) {
      stashError = err instanceof Error ? err.message : String(err);
    } finally {
      stashesLoading = false;
    }
  }

  function sourceSession(): SessionRef | undefined {
    for (let index = chat.messages.length - 1; index >= 0; index -= 1) {
      const coordinate = chat.messages[index]?.transcript;
      if (coordinate?.sessionId === chat.sessionId) {
        return {
          authority_id: coordinate.authorityId,
          session_id: coordinate.sessionId,
        };
      }
    }
    return undefined;
  }

  function draftLabel(text: string): string | undefined {
    const firstLine = text
      .split(/\r?\n/, 1)[0]
      ?.replace(/\s+/g, " ")
      .trim();
    if (!firstLine) return undefined;
    return firstLine.length > 54 ? `${firstLine.slice(0, 53)}…` : firstLine;
  }

  async function stashDraft() {
    if (!canStash || stashSaving) return;
    stashSaving = true;
    stashError = null;
    try {
      const stash = await createPromptStash({
        label:
          draftLabel(chat.draft) ??
          chat.pendingMediaRefs[0]?.label?.trim() ??
          "Attachment draft",
        draft: {
          text: chat.draft,
          media_refs: [...chat.pendingMediaRefs],
          mode: mode?.trim() || undefined,
          model: model?.trim() || undefined,
        },
        source_session: sourceSession(),
      });
      stashes = [
        stash,
        ...stashes.filter((candidate) => candidate.stash_id !== stash.stash_id),
      ];
      chat.prefillDraft("");
      chat.clearPendingMedia();
    } catch (err) {
      stashError = err instanceof Error ? err.message : String(err);
    } finally {
      stashSaving = false;
    }
  }

  function applyStash(stash: PromptStash) {
    chat.prefillDraft(stash.draft.text);
    chat.pendingMediaRefs = [...(stash.draft.media_refs ?? [])];
    open = false;
  }

  async function removeStash(event: MouseEvent, stashId: string) {
    event.stopPropagation();
    if (deletingStashId) return;
    deletingStashId = stashId;
    stashError = null;
    try {
      await deletePromptStash(stashId);
      stashes = stashes.filter((stash) => stash.stash_id !== stashId);
    } catch (err) {
      stashError = err instanceof Error ? err.message : String(err);
    } finally {
      deletingStashId = null;
    }
  }

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
  aria-label="Add attachments and composer options"
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
  <BodyPortal>
    <div
      bind:this={menuEl}
      class="composer-anchored-menu composer-plus-menu-panel"
      role="menu"
      aria-label="Composer actions"
    >
      <button type="button" class="composer-plus-menu-item" role="menuitem" onclick={attach}>
        <Paperclip size={15} strokeWidth={1.75} class="shrink-0 opacity-70" />
        <span>Attach</span>
      </button>
      {#if showStashes}
        <button
          type="button"
          class="composer-plus-menu-item"
          role="menuitem"
          disabled={!canStash || stashSaving}
          onclick={() => void stashDraft()}
        >
          {#if stashSaving}
            <LoaderCircle size={15} class="shrink-0 animate-spin opacity-70" />
          {:else}
            <BookmarkPlus size={15} strokeWidth={1.75} class="shrink-0 opacity-70" />
          {/if}
          <span>Stash draft</span>
        </button>
      {/if}
      {#if showWorkshop && onWorkshop}
        <button
          type="button"
          class="composer-plus-menu-item"
          role="menuitem"
          onclick={pickWorkshop}
        >
          <span class="composer-plus-menu-dot" aria-hidden="true"></span>
          <span>Workshop</span>
        </button>
      {/if}
      <button type="button" class="composer-plus-menu-item" role="menuitem" onclick={pickProfile}>
        <UserRound size={15} strokeWidth={1.75} class="shrink-0 opacity-70" />
        <span>Profile</span>
      </button>
      <button type="button" class="composer-plus-menu-item" role="menuitem" onclick={pickAgent}>
        <Bot size={15} strokeWidth={1.75} class="shrink-0 opacity-70" />
        <span>Agent</span>
      </button>

      {#if showStashes}
        {#if stashesLoading || stashes.length > 0 || stashError}
          <div class="composer-plus-menu-divider" aria-hidden="true"></div>
          <div class="composer-stash-heading">
            <span>Prompt stashes</span>
            {#if stashesLoading}<LoaderCircle size={12} class="animate-spin" />{/if}
          </div>
        {/if}

        {#each stashes as stash (stash.stash_id)}
          <div class="composer-stash-row">
            <button
              type="button"
              class="composer-stash-apply"
              role="menuitem"
              onclick={() => applyStash(stash)}
            >
              <ArchiveRestore size={14} strokeWidth={1.75} class="shrink-0 opacity-65" />
              <span class="composer-stash-copy">
                <span class="composer-stash-label">{stash.label || "Untitled draft"}</span>
                <span class="composer-stash-meta">
                  {stash.draft.media_refs?.length
                    ? `${stash.draft.media_refs.length} attachment${stash.draft.media_refs.length === 1 ? "" : "s"}`
                    : "Saved prompt"}
                </span>
              </span>
            </button>
            <button
              type="button"
              class="composer-stash-delete"
              aria-label={`Delete ${stash.label || "prompt stash"}`}
              disabled={deletingStashId !== null}
              onclick={(event) => void removeStash(event, stash.stash_id)}
            >
              {#if deletingStashId === stash.stash_id}
                <LoaderCircle size={12} class="animate-spin" />
              {:else}
                <Trash2 size={13} strokeWidth={1.8} />
              {/if}
            </button>
          </div>
        {/each}

        {#if stashError}
          <p class="composer-stash-error" role="alert">Couldn’t update prompt stashes.</p>
        {/if}
      {/if}
    </div>
  </BodyPortal>
{/if}
