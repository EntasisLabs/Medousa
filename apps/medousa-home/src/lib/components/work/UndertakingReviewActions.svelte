<script lang="ts">
  import { Check, ChevronDown } from "@lucide/svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";

  interface Props {
    attached: boolean; busy: boolean; canApprove: boolean;
    allowReview: boolean; allowApply: boolean;
    sourceBranch: string; reviewedBranch: string; actionLabel: string;
    integrationStrategy: "fast_forward_only" | "preserve_branch";
    onRequestChanges: () => void; onApprove: () => void; onApply: () => void;
  }

  let { attached, busy, canApprove, allowReview, allowApply,
    sourceBranch, reviewedBranch, actionLabel,
    integrationStrategy = $bindable(), onRequestChanges,
    onApprove, onApply }: Props = $props();
  let optionsOpen = $state(false);
  function chooseStrategy(strategy: Props["integrationStrategy"]) {
    integrationStrategy = strategy;
    optionsOpen = false;
  }
</script>

{#if allowApply}
  <button type="button" class="review-primary-action" disabled={busy} title={actionLabel} aria-label={actionLabel} onclick={onApply}>
    <Check size={14} strokeWidth={1.8} /><span>{actionLabel}</span>
  </button>
{:else if allowReview}
  <OverflowMenu bind:open={optionsOpen} label="Forge review actions" mobileTitle="Review candidate" panelClass="review-approval-menu w-64 rounded-lg border border-surface-500/40 bg-surface-900 p-1.5 shadow-xl">
    {#snippet trigger({ open, toggle })}
      <button type="button" class="review-primary-action" aria-label="Forge review actions" disabled={busy} aria-haspopup="menu" aria-expanded={open} onclick={toggle}>Review<ChevronDown size={13} /></button>
    {/snippet}
    <button type="button" role="menuitem" class="approval-choice" disabled={busy || !canApprove} onclick={() => { optionsOpen = false; onApprove(); }}><span>Approve changes</span><Check size={13} /></button>
    <button type="button" role="menuitem" class="approval-choice" disabled={busy} onclick={() => { optionsOpen = false; onRequestChanges(); }}>Request changes</button>
    {#if !attached}
      <p class="approval-menu-label">After approval</p>
      <button type="button" role="menuitemcheckbox" aria-checked={integrationStrategy === "fast_forward_only"} class="approval-choice" disabled={busy} onclick={() => chooseStrategy("fast_forward_only")}><span>Apply to {sourceBranch}</span>{#if integrationStrategy === "fast_forward_only"}<Check size={13} />{/if}</button>
      <button type="button" role="menuitemcheckbox" aria-checked={integrationStrategy === "preserve_branch"} class="approval-choice" disabled={busy} onclick={() => chooseStrategy("preserve_branch")}><span>Keep {reviewedBranch}</span>{#if integrationStrategy === "preserve_branch"}<Check size={13} />{/if}</button>
      <p class="approval-menu-hint">Approve first, then apply or keep the reviewed branch.</p>
    {/if}
  </OverflowMenu>
{/if}

<style>
  button { cursor:pointer; }
  button:disabled { opacity:.4; cursor:default; }
  .review-primary-action { display:inline-flex; align-items:center; justify-content:center; gap:.4rem; min-height:1.9rem; padding:.4rem .75rem; background:rgb(var(--theme-text)); color:rgb(var(--theme-canvas)); border:0; border-radius:.45rem; font-size:.75rem; font-weight:550; white-space:nowrap; }
  .review-primary-action:hover { filter:brightness(.92); }
  .approval-menu-label { padding:.35rem .5rem; font-size:.6875rem; color:rgb(var(--theme-text-tertiary)); }
  .approval-choice { display:flex; align-items:center; justify-content:space-between; gap:.75rem; width:100%; padding:.5rem; text-align:left; border:0; background:transparent; color:rgb(var(--theme-text)); border-radius:.35rem; font-size:.75rem; }
  .approval-choice span { min-width:0; overflow-wrap:anywhere; }
  .approval-choice:hover { background:rgb(var(--theme-text)/.06); }
  .approval-menu-hint { border-top:1px solid rgb(var(--theme-border)/.4); margin:.4rem .5rem .2rem; padding-top:.5rem; color:rgb(var(--theme-text-tertiary)); font-size:.6875rem; line-height:1.5; }
</style>
