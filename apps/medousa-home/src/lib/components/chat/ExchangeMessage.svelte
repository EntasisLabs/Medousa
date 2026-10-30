<script lang="ts">
  import { renderMarkdown } from '$lib/markdown';
  let { label, text, timestamp, sender = false }: { label: string; text: string; timestamp?: string; sender?: boolean } = $props();
</script>
<article class:sender>
  <div class="byline"><span>{label}</span>{#if timestamp}<time datetime={timestamp}>{new Date(timestamp).toLocaleString()}</time>{/if}</div>
  <div class="message markdown-body">{@html renderMarkdown(text)}</div>
</article>
<style>
  article { margin-bottom: 24px; min-width: 0; }
  .byline { display: flex; flex-wrap: wrap; gap: 10px; align-items: baseline; margin: 0 14px 7px; color: rgb(var(--theme-text-secondary)); font-size: 11px; }
  time { color: rgb(var(--theme-text-tertiary)); font-size: 10px; }
  .message { padding: 16px 18px; border-radius: 20px; background: rgb(var(--theme-card-hover) / .45); font-size: 14px; line-height: 1.7; overflow-wrap: anywhere; }
  .sender { margin-left: 32px; }
  .sender .message { background: rgb(var(--theme-border) / .2); }
  @media (max-width: 600px) { .sender { margin-left: 16px; } .message { padding: 14px 16px; } }
</style>
