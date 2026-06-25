<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import StepIndicator from "./StepIndicator.svelte";
  import type { InstallerStep } from "../types";
  import markUrl from "../../assets/medousa-mark.png";

  interface Props {
    step: InstallerStep;
    version: string;
    footer?: import("svelte").Snippet;
  }

  let { step, version, children, footer }: Props & { children: import("svelte").Snippet } = $props();

  async function minimize() {
    await getCurrentWindow().minimize();
  }

  async function close() {
    await getCurrentWindow().close();
  }
</script>

<div class="installer-chrome">
  <header class="titlebar" data-tauri-drag-region>
    <div class="titlebar-left" data-tauri-drag-region>
      <img class="mark" src={markUrl} alt="" width="24" height="24" />
      <div class="titleblock" data-tauri-drag-region>
        <div class="product-name" data-tauri-drag-region>Medousa Installer</div>
        <div class="product-version" data-tauri-drag-region>v{version}</div>
      </div>
    </div>
    <div class="window-controls">
      <button type="button" class="win-btn" aria-label="Minimize" onclick={minimize}>—</button>
      <button type="button" class="win-btn close" aria-label="Close" onclick={close}>×</button>
    </div>
  </header>

  <div class="step-row">
    <StepIndicator {step} />
  </div>

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

  .titlebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.65rem 0.75rem 0.35rem 1rem;
    border-bottom: 1px solid var(--installer-border);
    background: var(--installer-canvas);
    user-select: none;
  }

  .titlebar-left {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    min-width: 0;
    flex: 1;
  }

  .mark {
    border-radius: 6px;
    flex-shrink: 0;
  }

  .product-name {
    font-size: 0.9rem;
    font-weight: 600;
    line-height: 1.2;
  }

  .product-version {
    font-size: 0.6875rem;
    color: var(--installer-muted);
  }

  .window-controls {
    display: flex;
    gap: 0.35rem;
    flex-shrink: 0;
  }

  .win-btn {
    width: 2rem;
    height: 1.75rem;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--installer-muted);
    font-size: 1rem;
    line-height: 1;
    transition:
      background var(--installer-motion),
      color var(--installer-motion);
  }

  .win-btn:hover {
    background: var(--installer-surface-raised);
    color: var(--installer-text);
  }

  .win-btn.close:hover {
    background: rgb(251 113 133 / 0.2);
    color: var(--installer-error);
  }

  .step-row {
    padding: 0 1rem 0.5rem;
    border-bottom: 1px solid var(--installer-border);
  }

  .content {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    padding: 0.85rem 1rem 0;
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
