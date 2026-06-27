<script lang="ts">
  import ChatPanel from "$lib/components/chat/ChatPanel.svelte";
  import BrowserControlHandoff from "$lib/components/browser/BrowserControlHandoff.svelte";
  import { browserWorkshop } from "$lib/stores/browserWorkshop.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { haptic } from "$lib/haptics";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";

  interface Props {
    onOpenFullChat?: () => void;
  }

  let { onOpenFullChat }: Props = $props();

  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  function dismiss() {
    haptic("light");
    browserWorkshop.close();
  }

  $effect(() => {
    if (!browserWorkshop.open || browserWorkshop.minimized || !sheetEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, { onDismiss: dismiss });
  });
</script>

{#if browserWorkshop.open && !browserWorkshop.minimized}
  <div
    class="mobile-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) dismiss();
    }}
  >
    <div
      bind:this={sheetEl}
      class="mobile-sheet mobile-browser-workshop-sheet"
      role="dialog"
      aria-label="Browser workshop"
    >
      <header bind:this={headerEl} class="mobile-sheet-header scripts-workbench-sheet-header">
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <div class="flex w-full items-center justify-between gap-2">
          <div class="min-w-0 flex-1">
            <p class="truncate text-sm font-semibold text-surface-50">Ask Medousa</p>
            <p class="truncate text-[11px] text-surface-400">{browserWorkshop.scopeLabel}</p>
          </div>
          <BrowserControlHandoff compact={true} />
          <button type="button" class="btn btn-sm variant-ghost-surface shrink-0" onclick={dismiss}>
            Close
          </button>
        </div>
      </header>
      <div class="min-h-0 flex-1 overflow-hidden">
        <ChatPanel
          visible={true}
          embedded={true}
          workshop={true}
          mobile={true}
          showPopout={false}
          onOpenContext={() => onOpenFullChat?.()}
        />
      </div>
    </div>
  </div>
{/if}

{#if browserWorkshop.open && browserWorkshop.minimized}
  <button
    type="button"
    class="vault-note-workshop-dock mobile-browser-workshop-dock"
    aria-label="Restore browser workshop"
    onclick={() => (browserWorkshop.minimized = false)}
  >
    <span class="truncate">Medousa · {browserWorkshop.scopeLabel}</span>
  </button>
{/if}
