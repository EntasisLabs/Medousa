<script lang="ts">
  import { resolveIcon } from "../iconMap";

  interface Props {
    title: string;
    description: string;
    sizeLabel: string;
    icon: string;
    selected?: boolean;
    onclick?: () => void;
  }

  let {
    title,
    description,
    sizeLabel,
    icon,
    selected = false,
    onclick,
  }: Props = $props();

  const Icon = $derived(resolveIcon(icon));
</script>

<button
  type="button"
  class="workload-card"
  class:selected
  role="radio"
  aria-checked={selected}
  {onclick}
>
  <div class="rail" aria-hidden="true"></div>
  <div class="card-top">
    <span class="icon-wrap" aria-hidden="true">
      <Icon size={22} strokeWidth={1.75} />
    </span>
    <span class="select-ring" aria-hidden="true"></span>
  </div>
  <div class="card-title">{title}</div>
  <div class="card-desc">{description}</div>
  <div class="card-meta">{sizeLabel}</div>
</button>

<style>
  .workload-card {
    position: relative;
    text-align: left;
    background: var(--installer-surface);
    border: 1px solid var(--installer-border);
    border-radius: var(--installer-radius-card);
    padding: 1rem 1rem 0.9rem 1.1rem;
    color: inherit;
    overflow: hidden;
    height: 100%;
    display: flex;
    flex-direction: column;
    transition:
      border-color var(--installer-motion),
      box-shadow var(--installer-motion),
      background var(--installer-motion);
  }

  .workload-card:hover {
    border-color: var(--installer-border-strong);
    background: var(--installer-surface-raised);
  }

  .workload-card.selected {
    border-color: var(--installer-accent);
    box-shadow: inset 0 0 0 1px var(--installer-accent-muted);
    background: var(--installer-surface);
  }

  .workload-card:focus-visible {
    outline: 2px solid var(--installer-accent);
    outline-offset: 2px;
  }

  .rail {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 3px;
    background: transparent;
    transition: background var(--installer-motion);
  }

  .workload-card.selected .rail {
    background: var(--installer-accent-rail);
  }

  .card-top {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 0.65rem;
  }

  .icon-wrap {
    color: var(--installer-accent);
    display: inline-flex;
  }

  .select-ring {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 1.5px solid var(--installer-border-strong);
    flex-shrink: 0;
    transition:
      border-color var(--installer-motion),
      background var(--installer-motion),
      box-shadow var(--installer-motion);
  }

  .workload-card.selected .select-ring {
    border-color: var(--installer-accent);
    background: var(--installer-accent);
    box-shadow: inset 0 0 0 3px var(--installer-surface);
  }

  .card-title {
    font-size: 1rem;
    font-weight: 600;
    margin-bottom: 0.25rem;
  }

  .card-desc {
    color: var(--installer-muted);
    font-size: var(--installer-body-size);
    line-height: 1.45;
    margin-bottom: 0.5rem;
    flex: 1;
  }

  .card-meta {
    color: var(--installer-faint);
    font-size: var(--installer-caption-size);
    font-weight: 500;
    margin-top: auto;
  }
</style>
