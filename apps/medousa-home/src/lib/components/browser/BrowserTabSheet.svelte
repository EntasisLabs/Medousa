<script lang="ts">
  import { Plus } from "@lucide/svelte";
  import BrowserPopover from "$lib/components/browser/BrowserPopover.svelte";
  import BrowserTabList from "$lib/components/browser/BrowserTabList.svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import type { PopoverPlacement } from "$lib/utils/browserPopoverOverlay";

  interface Props {
    open?: boolean;
    onClose?: () => void;
    anchorRect?: DOMRect | null;
    mobile?: boolean;
  }

  let { open = false, onClose, anchorRect = null, mobile = false }: Props = $props();

  const tabCount = $derived(humanBrowser.tabs.length);
  const placement = $derived<PopoverPlacement>(mobile ? "above" : "panel");
</script>

<BrowserPopover
  {open}
  {onClose}
  {anchorRect}
  {placement}
  title="{tabCount} {tabCount === 1 ? 'Tab' : 'Tabs'}"
  ariaLabel="Browser tabs"
  width={mobile ? 340 : 380}
  maxHeight={mobile ? 280 : 400}
  hideNativeEmbed={true}
  backdrop={true}
>
  <BrowserTabList variant="sheet" onSelect={() => onClose?.()} />
  <div class="border-t border-surface-800/80 p-2">
    <button
      type="button"
      class="btn btn-sm variant-soft-surface w-full"
      onclick={() => {
        void humanBrowser.openTab("about:blank");
        onClose?.();
      }}
    >
      <Plus size={14} class="mr-1 inline" />
      New Tab
    </button>
  </div>
</BrowserPopover>
