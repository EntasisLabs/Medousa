<script lang="ts">
  interface Props {
    onRetry: () => void;
  }

  let { onRetry }: Props = $props();
</script>

<div class="shell-chunk-error" data-h09-shell-error role="alert">
  <p>Couldn't load this workspace layout.</p>
  <button type="button" onclick={onRetry}>Retry</button>
</div>

<style>
  .shell-chunk-error {
    display: grid;
    min-height: 100%;
    place-content: center;
    gap: 0.75rem;
    padding: 1.5rem;
    text-align: center;
  }
</style>
