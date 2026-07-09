<script lang="ts">
  import type { InstallerStep } from "../types";

  interface Props {
    step: InstallerStep;
  }

  let { step }: Props = $props();

  const steps: { id: InstallerStep; label: string }[] = [
    { id: "progress", label: "Downloading" },
    { id: "complete", label: "Done" },
  ];

  const activeIndex = $derived.by(() => {
    if (step === "complete") return 1;
    if (step === "progress") return 0;
    return -1;
  });
</script>

<nav class="step-indicator" aria-label="Installation progress">
  {#each steps as s, i (s.id)}
    <div class="step" class:active={i === activeIndex} class:done={i < activeIndex}>
      <span class="dot" aria-hidden="true"></span>
      <span class="label">{s.label}</span>
    </div>
    {#if i < steps.length - 1}
      <span class="connector" class:done={i < activeIndex} aria-hidden="true"></span>
    {/if}
  {/each}
</nav>

<style>
  .step-indicator {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    margin-top: 0.65rem;
  }

  .step {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    color: var(--installer-faint);
    font-size: 0.6875rem;
    font-weight: 500;
  }

  .step.active {
    color: var(--installer-text-secondary);
  }

  .step.done {
    color: var(--installer-muted);
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--installer-border-strong);
    transition: background var(--installer-motion), transform var(--installer-motion);
  }

  .step.active .dot {
    background: var(--installer-accent);
    transform: scale(1.15);
  }

  .step.done .dot {
    background: var(--installer-accent-muted);
    border: 1px solid var(--installer-accent);
  }

  .connector {
    width: 1.25rem;
    height: 1px;
    background: var(--installer-border);
  }

  .connector.done {
    background: var(--installer-accent-muted);
  }

  @media (max-width: 700px) {
    .label {
      display: none;
    }
  }
</style>
