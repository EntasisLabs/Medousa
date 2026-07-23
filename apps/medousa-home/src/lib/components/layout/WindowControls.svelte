<script lang="ts">
  import { Minus, Square, X } from "@lucide/svelte";
  import { titlebarMode } from "$lib/platform";

  let maximized = $state(false);

  const show = $derived(titlebarMode() === "custom-winlinux");

  async function appWindow() {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    return getCurrentWindow();
  }

  $effect(() => {
    if (!show) return;
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const win = await appWindow();
        maximized = await win.isMaximized();
        unlisten = await win.onResized(async () => {
          maximized = await win.isMaximized();
        });
      } catch {
        /* ignore */
      }
    })();
    return () => unlisten?.();
  });

  async function minimize() {
    try {
      await (await appWindow()).minimize();
    } catch {
      /* ignore */
    }
  }

  async function toggleMaximize() {
    try {
      await (await appWindow()).toggleMaximize();
      maximized = await (await appWindow()).isMaximized();
    } catch {
      /* ignore */
    }
  }

  async function close() {
    try {
      await (await appWindow()).close();
    } catch {
      /* ignore */
    }
  }
</script>

{#if show}
  <div class="window-controls" role="group" aria-label="Window">
    <button
      type="button"
      class="window-controls-btn"
      title="Minimize"
      aria-label="Minimize"
      onclick={() => void minimize()}
    >
      <Minus size={12} strokeWidth={2} />
    </button>
    <button
      type="button"
      class="window-controls-btn"
      title={maximized ? "Restore" : "Maximize"}
      aria-label={maximized ? "Restore" : "Maximize"}
      onclick={() => void toggleMaximize()}
    >
      <Square size={10} strokeWidth={2} />
    </button>
    <button
      type="button"
      class="window-controls-btn window-controls-btn--close"
      title="Close"
      aria-label="Close"
      onclick={() => void close()}
    >
      <X size={12} strokeWidth={2} />
    </button>
  </div>
{/if}

<style>
  .window-controls {
    display: inline-flex;
    flex-shrink: 0;
    align-items: stretch;
    height: 100%;
    margin-left: 0.15rem;
  }

  .window-controls-btn {
    display: inline-flex;
    width: 2.75rem;
    align-items: center;
    justify-content: center;
    border: 0;
    background: transparent;
    color: rgb(var(--color-surface-400));
    transition:
      background-color 120ms ease,
      color 120ms ease;
  }

  .window-controls-btn:hover {
    background: rgb(var(--color-surface-700) / 0.65);
    color: rgb(var(--color-surface-100));
  }

  .window-controls-btn--close:hover {
    background: rgb(var(--color-error-500) / 0.9);
    color: white;
  }
</style>
