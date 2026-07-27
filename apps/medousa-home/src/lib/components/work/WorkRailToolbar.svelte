<script lang="ts">
  import {
    AlertTriangle,
    CheckCircle2,
    CircleOff,
    MessageSquarePlus,
    RefreshCw,
    Zap,
  } from "@lucide/svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { workAskDock } from "$lib/stores/workAskDock.svelte";
  import { partitionWorkHub, type WorkHubLayer } from "$lib/utils/workHub";

  interface Props {
    onAction?: () => void;
    variant?: "popover" | "rail-row";
  }

  let { onAction, variant = "popover" }: Props = $props();

  const partition = $derived(partitionWorkHub(workspace.visibleCards()));
  const active = $derived(workspace.workRailFilter);

  function openAsk(event: MouseEvent) {
    const trigger = event.currentTarget as HTMLElement;
    workAskDock.openDock(trigger);
    onAction?.();
  }

  function setFilter(layer: WorkHubLayer) {
    workspace.setWorkRailFilter(layer);
    onAction?.();
  }

  function filterBtnClass(layer: WorkHubLayer): string {
    return active === layer
      ? "vault-dock-icon-btn vault-dock-icon-btn-active relative"
      : "vault-dock-icon-btn relative";
  }
</script>

{#if variant === "popover"}
  <div class="lme-dock-leading-ghost min-w-0 flex-1" aria-hidden="true"></div>
{/if}

<button
  type="button"
  class="vault-dock-icon-btn"
  data-work-ask-trigger="true"
  title="New ask"
  aria-label="New ask"
  aria-expanded={workAskDock.open}
  onclick={openAsk}
>
  <MessageSquarePlus size={15} strokeWidth={1.75} />
</button>

{#if variant === "popover"}
  <div class="lme-dock-chrome-secondary flex shrink-0 items-center gap-0.5">
    <button
      type="button"
      class={filterBtnClass("living")}
      title="In motion"
      aria-label="Show work in motion"
      aria-pressed={active === "living"}
      onclick={() => setFilter("living")}
    >
      <Zap size={15} strokeWidth={1.75} />
      {#if partition.living.length > 0}
        <span class="absolute right-0.5 top-0.5 size-1.5 rounded-full bg-primary-400"></span>
      {/if}
    </button>
    <button
      type="button"
      class={filterBtnClass("settled")}
      title="Settled"
      aria-label="Show settled work"
      aria-pressed={active === "settled"}
      onclick={() => setFilter("settled")}
    >
      <CheckCircle2 size={15} strokeWidth={1.75} />
      {#if partition.settled.length > 0}
        <span class="absolute right-0.5 top-0.5 size-1.5 rounded-full bg-primary-400"></span>
      {/if}
    </button>
    <button
      type="button"
      class={filterBtnClass("failed")}
      title="Failed"
      aria-label="Show failed work"
      aria-pressed={active === "failed"}
      onclick={() => setFilter("failed")}
    >
      <AlertTriangle size={15} strokeWidth={1.75} />
      {#if partition.failed.length > 0}
        <span class="absolute right-0.5 top-0.5 size-1.5 rounded-full bg-warning-400"></span>
      {/if}
    </button>
    <button
      type="button"
      class={filterBtnClass("stuck")}
      title="Stuck"
      aria-label="Show stuck work"
      aria-pressed={active === "stuck"}
      onclick={() => setFilter("stuck")}
    >
      <span class="text-[10px] font-bold tracking-tight">!</span>
      {#if partition.stuck.length > 0}
        <span class="absolute right-0.5 top-0.5 size-1.5 rounded-full bg-warning-400"></span>
      {/if}
    </button>
    <button
      type="button"
      class={filterBtnClass("stopped")}
      title="Stopped"
      aria-label="Show stopped work"
      aria-pressed={active === "stopped"}
      onclick={() => setFilter("stopped")}
    >
      <CircleOff size={15} strokeWidth={1.75} />
      {#if partition.stopped.length > 0}
        <span class="absolute right-0.5 top-0.5 size-1.5 rounded-full bg-surface-400"></span>
      {/if}
    </button>
  </div>
{/if}

<button
  type="button"
  class="vault-dock-icon-btn"
  title="Refresh"
  aria-label="Refresh work cards"
  onclick={() => void workspace.reconcileCardsFromSnapshot()}
>
  <RefreshCw size={15} strokeWidth={1.75} />
</button>
