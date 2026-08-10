<script lang="ts">
  import { Check, MessageSquare, Trash2 } from "@lucide/svelte";
  import type { ReviewComment } from "$lib/forge";

  interface Props {
    comments: ReviewComment[];
    compose?: {
      path: string;
      side: "new" | "old" | string;
      line: number;
      content: string;
    } | null;
    draft?: string;
    busy?: boolean;
    onDraftChange?: (value: string) => void;
    onSubmit?: () => void | Promise<void>;
    onCancelCompose?: () => void;
    onResolve?: (commentId: string) => void | Promise<void>;
    onDelete?: (commentId: string) => void | Promise<void>;
    onJump?: (comment: ReviewComment) => void;
  }

  let {
    comments,
    compose = null,
    draft = "",
    busy = false,
    onDraftChange,
    onSubmit,
    onCancelCompose,
    onResolve,
    onDelete,
    onJump,
  }: Props = $props();

  const unresolved = $derived(comments.filter((comment) => !comment.resolved_at));
  const resolved = $derived(comments.filter((comment) => comment.resolved_at));
</script>

<aside class="review-comment-rail" aria-label="Review comments">
  <header>
    <MessageSquare size={13} />
    <span>Comments</span>
    <small>{unresolved.length} open</small>
  </header>

  {#if compose}
    <div class="review-comment-compose">
      <p>
        {compose.path}:{compose.line}
        {#if compose.content}
          <code>{compose.content.slice(0, 80)}</code>
        {/if}
      </p>
      <textarea
        rows="3"
        placeholder="What should change here?"
        value={draft}
        disabled={busy}
        oninput={(event) => onDraftChange?.(event.currentTarget.value)}
      ></textarea>
      <div class="review-comment-compose-actions">
        <button type="button" disabled={busy} onclick={() => onCancelCompose?.()}>Cancel</button>
        <button
          type="button"
          class="primary"
          disabled={busy || !draft.trim()}
          onclick={() => void onSubmit?.()}
        >Add comment</button>
      </div>
    </div>
  {/if}

  {#if unresolved.length === 0 && !compose}
    <p class="review-comment-empty">No open comments yet. Hover a diff line and click the comment icon, or press <kbd>.</kbd></p>
  {/if}

  <ul>
    {#each unresolved as comment (comment.id)}
      <li>
        <button type="button" class="review-comment-jump" onclick={() => onJump?.(comment)}>
          <strong>{comment.path}:{comment.start_line}</strong>
          <span>{comment.body}</span>
        </button>
        <div class="review-comment-actions">
          <button
            type="button"
            title="Resolve"
            disabled={busy}
            onclick={() => void onResolve?.(comment.id)}
          ><Check size={12} /></button>
          <button
            type="button"
            title="Delete"
            disabled={busy}
            onclick={() => void onDelete?.(comment.id)}
          ><Trash2 size={12} /></button>
        </div>
      </li>
    {/each}
  </ul>

  {#if resolved.length > 0}
    <details class="review-comment-resolved">
      <summary>{resolved.length} resolved</summary>
      <ul>
        {#each resolved as comment (comment.id)}
          <li class="resolved">
            <strong>{comment.path}:{comment.start_line}</strong>
            <span>{comment.body}</span>
          </li>
        {/each}
      </ul>
    </details>
  {/if}
</aside>

<style>
  .review-comment-rail {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
    min-width: 14rem;
    max-width: 18rem;
    border-left: 1px solid rgb(var(--color-surface-500) / 0.22);
    padding: 0.35rem 0 0.35rem 0.85rem;
    color: rgb(var(--color-surface-200));
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.6875rem;
    font-weight: 600;
  }

  header small {
    margin-left: auto;
    color: rgb(var(--theme-text-faint));
    font-weight: 500;
  }

  .review-comment-empty {
    font-size: 0.625rem;
    line-height: 1.45;
    color: rgb(var(--theme-text-quiet));
  }

  kbd {
    font-family: var(--font-mono);
    font-size: 0.5625rem;
  }

  .review-comment-compose {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.28);
    border-radius: 0.5rem;
    padding: 0.5rem;
    background: rgb(var(--color-surface-950) / 0.35);
  }

  .review-comment-compose p {
    font-size: 0.5625rem;
    color: rgb(var(--theme-text-quiet));
  }

  .review-comment-compose code {
    display: block;
    margin-top: 0.2rem;
    overflow: hidden;
    font-family: var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .review-comment-compose textarea {
    width: 100%;
    border: 1px solid rgb(var(--color-surface-500) / 0.3);
    border-radius: 0.35rem;
    background: rgb(var(--color-surface-900));
    padding: 0.35rem 0.45rem;
    color: rgb(var(--color-surface-100));
    font-size: 0.6875rem;
    resize: vertical;
  }

  .review-comment-compose-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.35rem;
  }

  .review-comment-compose-actions button,
  .review-comment-actions button {
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    padding: 0.2rem 0.4rem;
    color: rgb(var(--theme-text-quiet));
    font-size: 0.5625rem;
  }

  .review-comment-compose-actions .primary {
    background: rgb(var(--color-primary-500) / 0.18);
    color: rgb(var(--theme-link));
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.2);
    border-radius: 0.45rem;
    padding: 0.4rem 0.45rem;
  }

  li.resolved {
    opacity: 0.65;
  }

  .review-comment-jump {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    border: 0;
    background: transparent;
    padding: 0;
    text-align: left;
    color: inherit;
    cursor: pointer;
  }

  .review-comment-jump strong {
    font-family: var(--font-mono);
    font-size: 0.5625rem;
    color: rgb(var(--theme-link));
  }

  .review-comment-jump span,
  li.resolved span {
    font-size: 0.625rem;
    line-height: 1.4;
    color: rgb(var(--theme-text-secondary));
  }

  .review-comment-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.15rem;
  }

  .review-comment-resolved {
    font-size: 0.625rem;
    color: rgb(var(--theme-text-quiet));
  }

  .review-comment-resolved summary {
    cursor: pointer;
  }
</style>
