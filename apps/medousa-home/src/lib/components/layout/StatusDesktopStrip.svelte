<script lang="ts">
  import { shellTabs } from "$lib/stores/shellTabs.svelte";

  const desktops = $derived(shellTabs.desktops);
  const activeId = $derived(shellTabs.activeDesktopId);
</script>

{#if desktops.length > 0}
  <div
    class="status-desktop-strip"
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
        onclick={() => void shellTabs.switchDesktop(desktop.id)}
      >
        <span class="status-desktop-mark-index" aria-hidden="true">{index + 1}</span>
      </button>
    {/each}
  </div>
{/if}
