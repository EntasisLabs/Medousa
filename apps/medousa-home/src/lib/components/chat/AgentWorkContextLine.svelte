<script lang="ts">
  import { ChevronRight, GitBranch } from "@lucide/svelte";
  import type { Snippet } from "svelte";

  let { title, status, attention = false, children }: {
    title: string;
    status: string;
    attention?: boolean;
    children: Snippet;
  } = $props();
</script>

<details class="agent-work-line">
  <summary class:needs-attention={attention}>
    <GitBranch size={14} aria-hidden="true" />
    <span class="work-title" title={title}>{title}</span>
    <span class="work-status" aria-live="polite">{status}</span>
    <ChevronRight size={13} class="work-chevron" aria-hidden="true" />
  </summary>
  <div class="work-details">{@render children()}</div>
</details>

<style>
  .agent-work-line { min-width: 0; }
  summary {
    display: flex; align-items: center; gap: 8px; min-width: 0; min-height: 30px;
    padding: 4px 8px; border-radius: 6px; list-style: none; cursor: pointer;
    font-size: 12px; color: rgb(var(--theme-text-tertiary));
  }
  summary::-webkit-details-marker { display: none; }
  summary:hover { background: rgb(var(--theme-card-hover) / .6); }
  summary :global(svg) { flex-shrink: 0; }
  .work-title { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; color: rgb(var(--theme-text-secondary)); }
  .work-status { flex-shrink: 0; }
  .work-status::before { content: '·'; padding-right: 8px; }
  .needs-attention .work-status { color: rgb(var(--theme-warning)); }
  summary :global(.work-chevron) { margin-left: auto; transition: transform 150ms; }
  details[open] > summary :global(.work-chevron) { transform: rotate(90deg); }
  .work-details { margin: 6px 8px 0; padding: 12px 0 4px; border-top: 1px solid rgb(var(--theme-border) / .6); }
  @media (max-width: 480px) {
    summary { flex-wrap: wrap; }
    .work-title { flex: 1; }
    .work-status { order: 4; width: calc(100% - 22px); margin-left: 22px; }
    .work-status::before { display: none; }
  }
  @media (pointer: coarse) { summary { min-height: 44px; } }
  @media (prefers-reduced-motion: reduce) { summary :global(.work-chevron) { transition: none; } }
</style>
