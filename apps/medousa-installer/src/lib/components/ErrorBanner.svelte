<script lang="ts">
  interface Props {
    message: string;
    onRetry?: () => void;
  }

  let { message, onRetry }: Props = $props();
</script>

<div class="error-banner" role="alert">
  <p class="error-text">{message}</p>
  {#if onRetry}
    <button type="button" class="retry-btn" onclick={onRetry}>Try again</button>
  {/if}
</div>

<style>
  .error-banner {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    background: rgb(251 113 133 / 0.08);
    border: 1px solid rgb(251 113 133 / 0.35);
    border-radius: var(--installer-radius-control);
    padding: 0.75rem 0.85rem;
    margin-top: 1rem;
  }

  .error-text {
    margin: 0;
    color: var(--installer-error);
    font-size: var(--installer-body-size);
    line-height: 1.5;
    flex: 1;
  }

  .retry-btn {
    flex-shrink: 0;
    background: transparent;
    border: 1px solid var(--installer-error);
    color: var(--installer-error);
    border-radius: var(--installer-radius-control);
    padding: 0.35rem 0.75rem;
    font-size: var(--installer-caption-size);
    font-weight: 600;
    transition: background var(--installer-motion);
  }

  .retry-btn:hover {
    background: rgb(251 113 133 / 0.12);
  }

  .retry-btn:focus-visible {
    outline: 2px solid var(--installer-error);
    outline-offset: 2px;
  }
</style>
