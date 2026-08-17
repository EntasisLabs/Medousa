<script lang="ts">
  import "$lib/styles/work.postcss";
  import "$lib/styles/composer.postcss";
  import { tick } from "svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import AskComposer from "$lib/components/work/AskComposer.svelte";
  import { workAskDock } from "$lib/stores/workAskDock.svelte";
  import { placeDockPopover } from "$lib/utils/dockPopoverPlace";
  import { attachComposerMenuDismiss } from "$lib/utils/composerMenuDismiss";

  let panelEl = $state<HTMLDivElement | null>(null);
  let style = $state("");

  function place() {
    const anchor = workAskDock.anchorEl;
    if (!anchor || !panelEl) {
      // Centered fallback when opened without a button anchor.
      style = [
        "left:50%",
        "top:50%",
        "transform:translate(-50%,-50%)",
        "width:min(28rem,calc(100vw - 2rem))",
        "max-height:min(24rem,calc(100vh - 4rem))",
      ].join(";");
      return;
    }
    const placed = placeDockPopover(anchor, {
      preferUp: true,
      width: 420,
      maxHeight: 420,
      gap: 8,
    });
    style = [
      `left:${placed.left}px`,
      `top:${placed.top}px`,
      `transform:${placed.transform}`,
      `width:${placed.width}px`,
      `max-height:${placed.maxHeight}px`,
    ].join(";");
  }

  $effect(() => {
    if (!workAskDock.open) return;
    void tick().then(place);
    const onResize = () => place();
    window.addEventListener("resize", onResize);
    window.visualViewport?.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("resize", onResize);
      window.visualViewport?.removeEventListener("resize", onResize);
    };
  });

  $effect(() => {
    if (!workAskDock.open || !panelEl) return;
    const panel = panelEl;
    const anchor = workAskDock.anchorEl;
    return attachComposerMenuDismiss({
      isInside: (target) =>
        Boolean(
          target &&
            (panel.contains(target) || anchor?.contains(target)),
        ),
      onDismiss: () => workAskDock.closeDock(),
    });
  });
</script>

{#if workAskDock.open}
  <BodyPortal>
    <div
      bind:this={panelEl}
      class="work-ask-dock-popover"
      style={style}
      role="dialog"
      aria-label="New ask"
    >
      <header class="work-ask-dock-header">
        <p class="work-ask-dock-title">New ask</p>
        <button
          type="button"
          class="work-ask-dock-close"
          aria-label="Close ask"
          onclick={() => workAskDock.closeDock()}
        >
          Esc
        </button>
      </header>
      <AskComposer
        autofocus={true}
        onQueued={() => workAskDock.closeDock()}
      />
    </div>
  </BodyPortal>
{/if}
