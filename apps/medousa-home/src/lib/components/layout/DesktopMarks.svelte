<script lang="ts">
  import { shellTabs } from "$lib/stores/shellTabs.svelte";

  interface Props {
    /** Quiet marks for the titlebar notch; default matches the status bar. */
    density?: "default" | "compact" | "notch";
    /** Called after a desktop switch (e.g. keep drawer open). */
    onSwitch?: (desktopId: string) => void;
  }

  let { density = "default", onSwitch }: Props = $props();

  const desktops = $derived(shellTabs.desktops);
  const activeId = $derived(shellTabs.activeDesktopId);
</script>

{#if desktops.length > 0}
  <div
    class="status-desktop-strip"
    class:status-desktop-strip--compact={density === "compact" || density === "notch"}
    class:status-desktop-strip--notch={density === "notch"}
    role="group"
    aria-label="Virtual desktops"
  >
    {#each desktops as desktop, index (desktop.id)}
      {@const active = desktop.id === activeId}
      <button
        type="button"
        class="status-desktop-mark"
        class:status-desktop-mark--active={active}
        title="{desktop.name} · Desktop {index + 1}"
        aria-label="{desktop.name}, desktop {index + 1}{active
          ? ', current'
          : ''}"
        aria-current={active ? "true" : undefined}
        onclick={() => {
          void shellTabs.switchDesktop(desktop.id);
          onSwitch?.(desktop.id);
        }}
      >
        <span class="status-desktop-mark-index" aria-hidden="true">{index + 1}</span>
      </button>
    {/each}
  </div>
{/if}
