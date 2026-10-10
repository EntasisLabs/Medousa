<script lang="ts">
  import { ChevronRight, CircleCheck, CircleAlert, LoaderCircle } from "@lucide/svelte";
  import type { Snippet } from "svelte";

  let { title, status, activity = null, state = "working", attention = false, children }: {
    title: string;
    status: string;
    activity?: string | null;
    state?: "working" | "review" | "complete" | "attention";
    attention?: boolean;
    children: Snippet;
  } = $props();
</script>

<details class="agent-work-line" data-state={state}>
  <summary class:needs-attention={attention}>
    <span class="state-icon">
      {#if state === "attention"}<CircleAlert size={15} aria-hidden="true" />
      {:else if state === "working"}<LoaderCircle size={15} aria-hidden="true" />
      {:else}<CircleCheck size={15} aria-hidden="true" />{/if}
    </span>
    <span class="work-heading"><span class="work-title" title={title}>{title}</span>{#if activity}<span class="work-activity" title={activity}>{activity}</span>{/if}</span>
    <span class="work-status" aria-live="polite">{status}</span>
    <ChevronRight size={13} class="work-chevron" aria-hidden="true" />
  </summary>
  <div class="work-details">{@render children()}</div>
</details>

<style>
  .agent-work-line { min-width: 0; }
  summary {
    display: grid; grid-template-columns: 16px minmax(0, 1fr) auto 14px; align-items: center; gap: 10px; min-width: 0; min-height: 58px;
    padding: 10px 14px; list-style: none; cursor: pointer;
    font-size: 12px; color: rgb(var(--theme-text-secondary));
  }
  summary::-webkit-details-marker { display: none; }
  summary:hover { background: rgb(var(--theme-card-hover) / .6); }
  summary :global(svg) { flex-shrink: 0; }
  .state-icon { display: flex; color: rgb(var(--theme-link)); }
  [data-state="review"] .state-icon, [data-state="review"] .work-status { color: rgb(var(--theme-warning)); }
  [data-state="complete"] .state-icon { color: rgb(var(--theme-success)); }
  [data-state="attention"] .state-icon { color: rgb(var(--theme-warning)); }
  .work-heading { min-width: 0; }
  .work-title { display: block; overflow-wrap: anywhere; font-size: 13px; font-weight: 500; color: rgb(var(--theme-text-primary)); }
  .work-activity { display: block; margin-top: 2px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: rgb(var(--theme-text-secondary)); }
  .work-status { flex-shrink: 0; }
  .needs-attention .work-status { color: rgb(var(--theme-warning)); }
  summary :global(.work-chevron) { margin-left: auto; transition: transform 150ms; }
  details[open] > summary :global(.work-chevron) { transform: rotate(90deg); }
  .work-details { margin: 0; padding: 2px 14px 14px 40px; }
  @media (max-width: 480px) {
    summary { grid-template-columns: 16px minmax(0, 1fr) 14px; gap: 8px; }
    .work-status { grid-column: 2; grid-row: 2; }
    summary :global(.work-chevron) { grid-column: 3; grid-row: 1; }
  }
  @media (pointer: coarse) { summary { min-height: 44px; } }
  @media (prefers-reduced-motion: reduce) { summary :global(.work-chevron) { transition: none; } }
</style>
