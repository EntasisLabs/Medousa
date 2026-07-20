<script lang="ts">
  import { tick } from "svelte";
  import { Bot, ChevronDown, X } from "@lucide/svelte";
  import { activeAgent } from "$lib/stores/activeAgent.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { attachComposerMenuDismiss } from "$lib/utils/composerMenuDismiss";
  import { placeComposerPopover } from "$lib/utils/railPopover";

  interface Props {
    /** Show a dismissible footer chip when a specialist is active. */
    showChip?: boolean;
    /** External open control (e.g. from ComposerPlusMenu). */
    open?: boolean;
    /** Anchor used when the chip trigger is hidden. */
    anchorEl?: HTMLElement | null;
  }

  let {
    showChip = true,
    open = $bindable(false),
    anchorEl = null,
  }: Props = $props();

  let triggerEl = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);

  const isSpecialist = $derived(Boolean(activeAgent.selectedManuscriptId));
  const label = $derived.by(() => {
    const id = activeAgent.selectedManuscriptId;
    if (!id) return "Medousa";
    return catalog.manuscripts.find((entry) => entry.id === id)?.name ?? id;
  });
  const chipVisible = $derived(showChip && isSpecialist);

  $effect(() => {
    if (open && catalog.manuscripts.length === 0 && !catalog.loading) {
      void catalog.refresh();
    }
  });

  $effect(() => {
    if (!open || !menuEl) return;
    const anchor = (chipVisible ? triggerEl : null) ?? anchorEl ?? triggerEl;
    if (!anchor) return;

    let frame = 0;
    const place = () => {
      if (!menuEl) return;
      placeComposerPopover(anchor, menuEl);
      frame = window.requestAnimationFrame(() => {
        if (menuEl) placeComposerPopover(anchor, menuEl);
      });
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    window.visualViewport?.addEventListener("scroll", place);

    const detachDismiss = attachComposerMenuDismiss({
      isInside: (target) =>
        Boolean(
          menuEl?.contains(target) ||
            triggerEl?.contains(target) ||
            anchor.contains(target),
        ),
      onDismiss: () => {
        open = false;
      },
    });

    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("scroll", place);
      detachDismiss();
    };
  });

  function pickDefault() {
    activeAgent.clear();
    open = false;
  }

  function pickAgent(id: string) {
    activeAgent.setActive(id);
    open = false;
  }

  function clearAgent(event: MouseEvent) {
    event.stopPropagation();
    event.preventDefault();
    activeAgent.clear();
  }

  function openAgentsInWorkspace() {
    open = false;
    lmeWorkspace.setExplorerMode("agents");
    if (layout.isMobile) {
      // Mobile hosts specialists under More → Capabilities/Workshop.
      layout.openMore("workshop");
      return;
    }
    layout.setShellSidebarExpanded(true);
    layout.setShellSidebarMode("view");
    shellTabs.openSurface("library", { activate: true });
  }
</script>

{#if chipVisible}
  <div class="composer-footer-chip-wrap">
    <button
      bind:this={triggerEl}
      type="button"
      class="composer-footer-chip"
      aria-label="Active agent — {label}"
      aria-haspopup="dialog"
      aria-expanded={open}
      onclick={() => (open = !open)}
    >
      <Bot size={12} strokeWidth={1.75} class="shrink-0 opacity-70" />
      <span class="truncate font-medium">{label}</span>
      <ChevronDown size={12} class="shrink-0 opacity-50" strokeWidth={2} />
    </button>
    <button
      type="button"
      class="composer-footer-chip-dismiss"
      aria-label="Clear agent"
      onclick={clearAgent}
    >
      <X size={12} strokeWidth={2} />
    </button>
  </div>
{/if}

{#if open}
  <div
    bind:this={menuEl}
    class="composer-anchored-menu"
    role="dialog"
    aria-label="Choose agent"
  >
    <header class="composer-anchored-menu-header">
      <div class="min-w-0">
        <h2 class="text-sm font-semibold text-surface-50">Who runs this</h2>
        <p class="workshop-faint mt-0.5 text-xs">Default Medousa or a specialist</p>
      </div>
    </header>
    <div class="composer-anchored-menu-body space-y-1">
      <button
        type="button"
        class="settings-toggle-row w-full text-left {activeAgent.selectedManuscriptId === null
          ? 'workshop-list-row-active'
          : ''}"
        onclick={pickDefault}
      >
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Medousa</span>
          <span class="workshop-faint mt-0.5 block text-xs">Default — no specialist</span>
        </span>
      </button>
      {#each catalog.manuscripts as entry (entry.id)}
        <button
          type="button"
          class="settings-toggle-row w-full text-left {activeAgent.selectedManuscriptId ===
          entry.id
            ? 'workshop-list-row-active'
            : ''}"
          onclick={() => pickAgent(entry.id)}
        >
          <span class="min-w-0 flex-1">
            <span class="block truncate text-sm font-medium text-surface-100">{entry.name}</span>
            {#if entry.description}
              <span class="workshop-faint mt-0.5 block truncate text-xs">{entry.description}</span>
            {/if}
          </span>
        </button>
      {/each}
      <button
        type="button"
        class="workshop-text-action mt-2 text-xs"
        onclick={openAgentsInWorkspace}
      >
        Manage agents in Workspace…
      </button>
    </div>
  </div>
{/if}
