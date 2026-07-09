<script lang="ts">
  import StepIndicator from "./StepIndicator.svelte";
  import type { InstallerStep } from "../types";

  interface Props {
    step: InstallerStep;
    showProgress?: boolean;
    footer?: import("svelte").Snippet;
  }

  let {
    step,
    showProgress = false,
    children,
    footer,
  }: Props & { children: import("svelte").Snippet } = $props();
</script>

<div class="installer-chrome">
  {#if showProgress}
    <div class="step-row">
      <StepIndicator {step} />
    </div>
  {/if}

  <main class="content screen-transition">
    {@render children()}
  </main>

  {#if footer}
    <footer class="chrome-footer">
      {@render footer()}
    </footer>
  {/if}
</div>

<style>
  .installer-chrome {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--installer-canvas);
    color: var(--installer-text);
    overflow: hidden;
  }

  .step-row {
    padding: 0.75rem 1.25rem 0.5rem;
    border-bottom: 1px solid var(--installer-border);
    flex-shrink: 0;
  }

  .content {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .content > :global(*) {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .chrome-footer {
    flex-shrink: 0;
    border-top: 1px solid var(--installer-border);
    background: var(--installer-surface);
  }
</style>
