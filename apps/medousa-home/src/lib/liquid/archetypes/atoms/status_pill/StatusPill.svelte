<script lang="ts">
  /** `status_pill` atom — the "Searching the web…" affordance. */
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  type PillState = "loading" | "ok" | "warn" | "error";
  const STATES: PillState[] = ["loading", "ok", "warn", "error"];

  let { node }: ArchetypeProps = $props();

  const label = $derived(typeof node.props.label === "string" ? node.props.label : "");
  const state = $derived<PillState>(
    STATES.includes(node.props.state as PillState) ? (node.props.state as PillState) : "loading",
  );
</script>

<span class="liquid-status-pill" data-state={state}>
  {#if state === "loading"}
    <span class="liquid-status-dot" aria-hidden="true"></span>
  {/if}
  <span class="liquid-status-label">{label}</span>
</span>

<style>
  .liquid-status-pill {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.25rem 0.7rem;
    border-radius: 999px;
    font-size: 0.75rem;
    font-weight: 500;
    line-height: 1;
    border: 1px solid transparent;
  }

  .liquid-status-pill[data-state="loading"] {
    color: rgb(var(--color-surface-200));
    background: color-mix(in srgb, var(--color-surface-700) 60%, transparent);
    border-color: color-mix(in srgb, var(--color-surface-500) 45%, transparent);
  }

  .liquid-status-pill[data-state="ok"] {
    color: rgb(var(--color-success-200));
    background: color-mix(in srgb, var(--color-success-500) 16%, transparent);
    border-color: color-mix(in srgb, var(--color-success-500) 40%, transparent);
  }

  .liquid-status-pill[data-state="warn"] {
    color: rgb(var(--color-warning-200));
    background: color-mix(in srgb, var(--color-warning-500) 16%, transparent);
    border-color: color-mix(in srgb, var(--color-warning-500) 40%, transparent);
  }

  .liquid-status-pill[data-state="error"] {
    color: rgb(var(--color-error-200));
    background: color-mix(in srgb, var(--color-error-500) 16%, transparent);
    border-color: color-mix(in srgb, var(--color-error-500) 40%, transparent);
  }

  .liquid-status-dot {
    width: 0.45rem;
    height: 0.45rem;
    border-radius: 999px;
    background: currentColor;
    animation: liquid-pulse 1.1s ease-in-out infinite;
  }

  @keyframes liquid-pulse {
    0%,
    100% {
      opacity: 0.35;
      transform: scale(0.85);
    }
    50% {
      opacity: 1;
      transform: scale(1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .liquid-status-dot {
      animation: none;
    }
  }
</style>
