<script lang="ts">
  import { tick } from "svelte";
  import {
    Archive,
    ArchiveRestore,
    BookmarkPlus,
    ChevronDown,
    LoaderCircle,
    Trash2,
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
    mode?: string;
    model?: string;
  }

  let { disabled = false, mode, model }: Props = $props();

  let open = $state(false);
  let rootEl = $state<HTMLDivElement | null>(null);
  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);
  let stashes = $state<PromptStash[]>([]);
  let loading = $state(false);
  let saving = $state(false);
  let deletingId = $state<string | null>(null);
  let error = $state<string | null>(null);

  const canStash = $derived(
    !disabled && (chat.draft.trim().length > 0 || chat.pendingMediaRefs.length > 0),
  );

  $effect(() => {
    stashes.length;
    loading;
    error;
    if (!open || !menuEl || !triggerEl) return;
    let frame = 0;
    const place = () => {
      if (!menuEl || !triggerEl) return;
      placeComposerPopover(triggerEl, menuEl, { maxHeightRatio: 0.58 });
      frame = window.requestAnimationFrame(() => {
        if (menuEl && triggerEl) {
          placeComposerPopover(triggerEl, menuEl, { maxHeightRatio: 0.58 });
        }
      });
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    window.visualViewport?.addEventListener("scroll", place);
    const detachDismiss = attachComposerMenuDismiss({
      isInside: (target) => Boolean(rootEl?.contains(target) || menuEl?.contains(target)),
      onDismiss: () => (open = false),
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
    if (!open) return;
    void refresh();
  });

  async function refresh() {
    loading = true;
    error = null;
    try {
      stashes = await listPromptStashes();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
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

  function relativeAge(value: string): string {
    const then = Date.parse(value);
    if (!Number.isFinite(then)) return "Saved draft";
    const elapsed = Math.max(0, Date.now() - then);
    const minutes = Math.floor(elapsed / 60_000);
    if (minutes < 1) return "Just now";
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    if (days < 7) return `${days}d ago`;
    return new Date(then).toLocaleDateString(undefined, { month: "short", day: "numeric" });
  }

  function stashMeta(stash: PromptStash): string {
    const parts = [relativeAge(stash.updated_at)];
    const attachments = stash.draft.media_refs?.length ?? 0;
    if (attachments > 0) {
      parts.push(`${attachments} attachment${attachments === 1 ? "" : "s"}`);
    }
    return parts.join(" · ");
  }

  async function stashDraft() {
    if (!canStash || saving) return;
    saving = true;
    error = null;
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
      error = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  function applyStash(stash: PromptStash) {
    chat.prefillDraft(stash.draft.text);
    chat.pendingMediaRefs = [...(stash.draft.media_refs ?? [])];
    open = false;
  }

  async function removeStash(event: MouseEvent, stashId: string) {
    event.stopPropagation();
    if (deletingId) return;
    deletingId = stashId;
    error = null;
    try {
      await deletePromptStash(stashId);
      stashes = stashes.filter((stash) => stash.stash_id !== stashId);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      deletingId = null;
    }
  }
</script>

<div bind:this={rootEl} class="composer-drafts-control">
  <button
    bind:this={triggerEl}
    type="button"
    class="composer-turn-trigger"
    class:composer-turn-trigger-open={open}
    {disabled}
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label="Drafts"
    title="Drafts"
    onclick={() => (open = !open)}
  >
    <Archive size={13} strokeWidth={1.85} class="composer-turn-trigger-icon" />
    <span class="composer-turn-trigger-label">Drafts</span>
    <ChevronDown size={12} strokeWidth={2} class="composer-turn-trigger-chevron shrink-0" />
  </button>

  {#if open}
    <BodyPortal>
      <div
        bind:this={menuEl}
        class="composer-anchored-menu composer-turn-menu composer-drafts-menu"
        role="menu"
        aria-label="Drafts"
      >
        <header class="composer-anchored-menu-header composer-drafts-header">
          <div class="min-w-0">
            <h2 class="composer-turn-menu-title">Drafts</h2>
            <p class="composer-turn-menu-description">Park a prompt and return to it later</p>
          </div>
          {#if loading}<LoaderCircle size={12} class="shrink-0 animate-spin" />{/if}
        </header>
        <div class="composer-anchored-menu-body space-y-0.5">
          <button
            type="button"
            class="composer-turn-option"
            role="menuitem"
            disabled={!canStash || saving}
            onclick={() => void stashDraft()}
          >
            {#if saving}
              <LoaderCircle size={14} class="composer-turn-option-icon animate-spin" />
            {:else}
              <BookmarkPlus size={14} strokeWidth={1.8} class="composer-turn-option-icon" />
            {/if}
            <span class="composer-turn-option-copy">
              <span class="composer-turn-option-label">Stash current draft</span>
              <span class="composer-turn-option-description">
                {canStash ? "Save text, attachments, and settings" : "Write something first"}
              </span>
            </span>
          </button>

          {#if stashes.length > 0 || !loading}
            <div class="composer-drafts-divider" aria-hidden="true"></div>
          {/if}

          {#each stashes as stash (stash.stash_id)}
            <div class="composer-draft-row">
              <button
                type="button"
                class="composer-turn-option composer-draft-apply"
                role="menuitem"
                onclick={() => applyStash(stash)}
              >
                <ArchiveRestore size={14} strokeWidth={1.8} class="composer-turn-option-icon" />
                <span class="composer-turn-option-copy">
                  <span class="composer-turn-option-label">{stash.label || "Untitled draft"}</span>
                  <span class="composer-turn-option-description">{stashMeta(stash)}</span>
                </span>
              </button>
              <button
                type="button"
                class="composer-draft-delete"
                aria-label={`Delete ${stash.label || "draft"}`}
                disabled={deletingId !== null}
                onclick={(event) => void removeStash(event, stash.stash_id)}
              >
                {#if deletingId === stash.stash_id}
                  <LoaderCircle size={12} class="animate-spin" />
                {:else}
                  <Trash2 size={13} strokeWidth={1.8} />
                {/if}
              </button>
            </div>
          {/each}

          {#if !loading && stashes.length === 0 && !error}
            <p class="composer-drafts-empty">No saved drafts yet</p>
          {/if}
          {#if error}
            <p class="composer-stash-error" role="alert">Couldn’t update drafts.</p>
          {/if}
        </div>
      </div>
    </BodyPortal>
  {/if}
</div>
