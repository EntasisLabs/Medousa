<script lang="ts">
  import { GitFork } from "@lucide/svelte";

  interface Props {
    sourceLabel: string;
    onOpenSource: () => void | Promise<void>;
  }

  let { sourceLabel, onOpenSource }: Props = $props();
</script>

<button
  type="button"
  class="chat-derivation-notice"
  title="Open source conversation"
  onclick={() => void onOpenSource()}
>
  <GitFork size={12} strokeWidth={1.7} />
  <span>Derived from</span>
  <strong>{sourceLabel}</strong>
  <span aria-hidden="true">↗</span>
</button>

<style>
  .chat-derivation-notice {
    display: flex;
    width: fit-content;
    max-width: min(100%, 32rem);
    align-items: center;
    gap: 0.35rem;
    margin: 0 0 0.65rem;
    padding: 0.24rem 0.42rem;
    border: 0;
    border-radius: 0.4rem;
    background: transparent;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.6875rem;
    line-height: 1.35;
    text-align: left;
  }

  .chat-derivation-notice:hover,
  .chat-derivation-notice:focus-visible {
    background: rgb(var(--theme-card-hover) / 0.52);
    color: rgb(var(--theme-text-secondary));
    outline: none;
  }

  .chat-derivation-notice :global(svg),
  .chat-derivation-notice > span:last-child {
    flex: none;
  }

  .chat-derivation-notice strong {
    min-width: 0;
    overflow: hidden;
    color: inherit;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
