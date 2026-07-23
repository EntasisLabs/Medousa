<script lang="ts">
  import { AlertTriangle, CheckCircle2, MessageSquarePlus, RefreshCw, Zap } from "@lucide/svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { partitionWorkHub } from "$lib/utils/workHub";
  import {
    dispatchWorkFocusAsk,
    dispatchWorkOpenTray,
  } from "$lib/utils/workChromeEvents";

  interface Props {
    onAction?: () => void;
  }

  let { onAction }: Props = $props();

  const partition = $derived(partitionWorkHub(workspace.visibleCards()));
  const primary = $derived(workspace.primaryInMotionCard());

  function focusAsk() {
    onAction?.();
    dispatchWorkFocusAsk();
  }

  async function jumpPrimary() {
    if (!primary) return;
    await workspace.selectCard(primary.id);
    onAction?.();
  }

  function openTray(tray: "settled" | "failed" | "stuck") {
    onAction?.();
    dispatchWorkOpenTray(tray);
  }
</script>

<div class="lme-dock-leading-ghost min-w-0 flex-1" aria-hidden="true"></div>

<button
  type="button"
  class="vault-dock-icon-btn"
  title="New ask"
  aria-label="Focus ask composer"
  onclick={focusAsk}
>
  <MessageSquarePlus size={15} strokeWidth={1.75} />
</button>

<div class="lme-dock-chrome-secondary flex shrink-0 items-center gap-0.5">
  <button
    type="button"
    class="vault-dock-icon-btn"
    title={primary ? `Open ${primary.title}` : "No living cards"}
    aria-label="Jump to primary living card"
    disabled={!primary}
    onclick={() => void jumpPrimary()}
  >
    <Zap size={15} strokeWidth={1.75} />
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn relative"
    title="Settled tray"
    aria-label="Open settled tray"
    onclick={() => openTray("settled")}
  >
    <CheckCircle2 size={15} strokeWidth={1.75} />
    {#if partition.settled.length > 0}
      <span class="absolute right-0.5 top-0.5 size-1.5 rounded-full bg-primary-400"></span>
    {/if}
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn relative"
    title="Failed tray"
    aria-label="Open failed tray"
    onclick={() => openTray("failed")}
  >
    <AlertTriangle size={15} strokeWidth={1.75} />
    {#if partition.failed.length > 0}
      <span class="absolute right-0.5 top-0.5 size-1.5 rounded-full bg-warning-400"></span>
    {/if}
  </button>
  <button
    type="button"
    class="vault-dock-icon-btn relative"
    title="Stuck tray"
    aria-label="Open stuck tray"
    onclick={() => openTray("stuck")}
  >
    <span class="text-[10px] font-bold tracking-tight">!</span>
    {#if partition.stuck.length > 0}
      <span class="absolute right-0.5 top-0.5 size-1.5 rounded-full bg-warning-400"></span>
    {/if}
  </button>
</div>

<button
  type="button"
  class="vault-dock-icon-btn"
  title="Refresh"
  aria-label="Refresh work cards"
  onclick={() => void workspace.reconcileCardsFromSnapshot()}
>
  <RefreshCw size={15} strokeWidth={1.75} />
</button>
