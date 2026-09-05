<script lang="ts">
  import { Check, ChevronDown, Laptop, LoaderCircle, MonitorUp, Plus } from "@lucide/svelte";
  import BrowserPopover from "$lib/components/browser/BrowserPopover.svelte";
  import { governedBrowser } from "$lib/stores/governedBrowser.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();
  let open = $state(false);
  let trigger = $state<HTMLButtonElement | null>(null);
  let anchorRect = $state<DOMRect | null>(null);

  const label = $derived(
    governedBrowser.source.kind === "workshop"
      ? governedBrowser.selectedWorld?.driver.display_name || "Workshop"
      : "Device",
  );

  $effect(() => {
    const scopeId = workshops.activeWorkshopId;
    void governedBrowser.load(scopeId);
    const timer = window.setInterval(() => {
      void governedBrowser.refreshWorlds().catch(() => undefined);
    }, 2_000);
    return () => window.clearInterval(timer);
  });

  function toggle(event: MouseEvent) {
    event.stopPropagation();
    if (!open) anchorRect = trigger?.getBoundingClientRect() ?? null;
    open = !open;
  }

  function close() {
    open = false;
  }

  async function selectWorld(worldId: string) {
    close();
    await governedBrowser.selectWorld(worldId);
  }

  function selectDevice() {
    close();
    governedBrowser.chooseDevice();
  }

  async function createWorld(persistent: boolean) {
    await governedBrowser.createWorld({ persistent });
    if (!governedBrowser.error) close();
  }
</script>

<button
  bind:this={trigger}
  type="button"
  class:browser-surface-trigger-mobile={mobile}
  class="browser-surface-trigger"
  data-browser-popover-trigger
  aria-label="Choose browser world"
  aria-expanded={open}
  title="Choose browser world"
  onclick={toggle}
>
  {#if governedBrowser.source.kind === "workshop"}
    <MonitorUp size={14} strokeWidth={1.75} />
  {:else}
    <Laptop size={14} strokeWidth={1.75} />
  {/if}
  <span class="truncate">{label}</span>
  <ChevronDown size={12} strokeWidth={1.75} />
</button>

<BrowserPopover
  {open}
  onClose={close}
  {anchorRect}
  placement={mobile ? "panel" : "below"}
  title="Browser world"
  ariaLabel="Choose browser world"
  width={340}
  maxHeight={440}
  hideNativeEmbed={true}
>
  <p class="browser-popover-section-label">On this workshop</p>
  {#if governedBrowser.loadingWorlds}
    <div class="flex items-center gap-2 px-3 py-4 text-sm text-content-tertiary">
      <LoaderCircle class="animate-spin" size={16} />
      Loading browser worlds…
    </div>
  {:else}
    {#if governedBrowser.error}
      <div class="px-3 py-2 text-xs leading-relaxed text-pink-300" role="alert">
        {governedBrowser.error}
      </div>
    {/if}
    {#each governedBrowser.worlds as world (world.world_id)}
      <button
        type="button"
        class="browser-popover-row"
        onclick={() => void selectWorld(world.world_id)}
      >
        <MonitorUp size={16} class="shrink-0 text-content-tertiary" />
        <span>
          <span class="block truncate text-sm text-surface-50">
            {world.driver.display_name || world.title || "Workshop browser"}
          </span>
          <span class="block truncate text-xs text-content-tertiary">
            {world.run_state} · {world.profile.kind === "persistent" ? "saved profile" : "private profile"}
          </span>
        </span>
        {#if governedBrowser.source.kind === "workshop" && governedBrowser.source.worldId === world.world_id}
          <Check size={15} class="ml-auto shrink-0 text-primary-300" />
        {/if}
      </button>
    {/each}
  {/if}

  <button
    type="button"
    class="browser-popover-row"
    disabled={governedBrowser.creating}
    onclick={() => void createWorld(false)}
  >
    <Plus size={16} class="shrink-0 text-content-tertiary" />
    <span>
      <span class="block text-sm text-surface-50">New private browser</span>
      <span class="block text-xs text-content-tertiary">Identity is removed when its world is deleted</span>
    </span>
  </button>
  <button
    type="button"
    class="browser-popover-row"
    disabled={governedBrowser.creating}
    onclick={() => void createWorld(true)}
  >
    <Plus size={16} class="shrink-0 text-content-tertiary" />
    <span>
      <span class="block text-sm text-surface-50">New saved browser</span>
      <span class="block text-xs text-content-tertiary">Keeps cookies on this workshop</span>
    </span>
  </button>

  <p class="browser-popover-section-label">On this device</p>
  <button type="button" class="browser-popover-row" onclick={selectDevice}>
    <Laptop size={16} class="shrink-0 text-content-tertiary" />
    <span>
      <span class="block text-sm text-surface-50">Device browser</span>
      <span class="block text-xs text-content-tertiary">Uses this device’s cookies and passkeys</span>
    </span>
    {#if governedBrowser.source.kind === "device"}
      <Check size={15} class="ml-auto shrink-0 text-primary-300" />
    {/if}
  </button>
</BrowserPopover>

<style>
  .browser-surface-trigger {
    display: inline-flex;
    min-width: 0;
    max-width: 9.5rem;
    height: 1.55rem;
    flex-shrink: 0;
    align-items: center;
    gap: 0.3rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.3);
    border-radius: 999px;
    padding: 0 0.55rem;
    color: rgb(var(--theme-text-secondary));
    background: rgb(var(--shell-pane-muted-bg, var(--color-surface-800)) / 0.48);
    font-size: 0.7rem;
  }

  .browser-surface-trigger:hover {
    color: rgb(var(--theme-text));
    border-color: rgb(var(--theme-focus) / 0.35);
  }

  .browser-surface-trigger-mobile {
    max-width: 7.5rem;
    height: 2rem;
    padding-inline: 0.65rem;
    font-size: 0.75rem;
  }
</style>
