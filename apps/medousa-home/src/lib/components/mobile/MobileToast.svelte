<script lang="ts">
  interface Props {
    message: string | null;
    actionLabel?: string;
    onAction?: () => void;
    onDismiss: () => void;
  }

  let { message, actionLabel, onAction, onDismiss }: Props = $props();
</script>

{#if message}
  <div class="mobile-toast" role="status">
    <span class="min-w-0 flex-1 truncate text-sm">{message}</span>
    {#if actionLabel && onAction}
      <button type="button" class="mobile-toast-action" onclick={onAction}>
        {actionLabel}
      </button>
    {/if}
    <button
      type="button"
      class="mobile-toast-dismiss"
      aria-label="Dismiss"
      onclick={onDismiss}
    >
      ✕
    </button>
  </div>
{/if}
