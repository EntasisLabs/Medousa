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
  <button type="button" class="review-secondary-action" disabled={busy} onclick={onRequestChanges}>Request changes</button>
  <div class="review-approval-group">
    <button type="button" class="review-primary-action" disabled={busy || !canApprove} title={`Approve changes · ${actionLabel}`} aria-label={`Approve changes · ${actionLabel}`} onclick={onApprove}>
      <Check size={14} strokeWidth={1.8} /><span>Approve</span>
    </button>
    {#if !attached}
      <OverflowMenu bind:open={optionsOpen} label="Approval options" panelClass="review-approval-menu w-64 rounded-lg border border-surface-500/40 bg-surface-900 p-1.5 shadow-xl">
        {#snippet trigger({ open, toggle })}
          <button type="button" class="review-approval-options" disabled={busy} aria-label="Approval options" aria-expanded={open} aria-haspopup="menu" title={`After approval: ${actionLabel}`} onclick={toggle}><ChevronDown size={13} /></button>
        {/snippet}
        <p class="approval-menu-label">After approval</p>
        <button type="button" role="menuitemcheckbox" aria-checked={integrationStrategy === "fast_forward_only"} class="approval-choice" disabled={busy} onclick={() => chooseStrategy("fast_forward_only")}><span>Apply to {sourceBranch}</span>{#if integrationStrategy === "fast_forward_only"}<Check size={13} />{/if}</button>
        <button type="button" role="menuitemcheckbox" aria-checked={integrationStrategy === "preserve_branch"} class="approval-choice" disabled={busy} onclick={() => chooseStrategy("preserve_branch")}><span>Keep {reviewedBranch}</span>{#if integrationStrategy === "preserve_branch"}<Check size={13} />{/if}</button>
        <p class="approval-menu-hint">Approve first, then apply or keep the reviewed branch.</p>
      </OverflowMenu>
    {/if}
  </div>
{/if}

<style>
  button { cursor:pointer; }
  button:disabled { opacity:.4; cursor:default; }
  .review-secondary-action { background:transparent; border:0; border-radius:.4rem; padding:.45rem .6rem; font-size:.75rem; color:rgb(var(--theme-text-secondary)); white-space:nowrap; }
  .review-secondary-action:hover { background:rgb(var(--theme-text)/.05); color:rgb(var(--theme-text)); }
  .review-primary-action,.review-approval-options { display:inline-flex; align-items:center; justify-content:center; gap:.4rem; min-height:1.9rem; padding:.4rem .75rem; background:rgb(var(--theme-text)); color:rgb(var(--theme-canvas)); border:0; border-radius:.45rem; font-size:.75rem; font-weight:550; white-space:nowrap; }
  .review-primary-action:hover,.review-approval-options:hover { filter:brightness(.92); }
  .review-approval-group { display:flex; align-items:stretch; }
  .review-approval-group:has(.review-approval-options) > .review-primary-action { border-radius:.45rem 0 0 .45rem; }
  .review-approval-options { height:100%; padding-inline:.4rem; border-left:1px solid rgb(var(--theme-canvas)/.2); border-radius:0 .45rem .45rem 0; }
  .approval-menu-label { padding:.35rem .5rem; font-size:.6875rem; color:rgb(var(--theme-text-tertiary)); }
  .approval-choice { display:flex; align-items:center; justify-content:space-between; gap:.75rem; width:100%; padding:.5rem; text-align:left; border:0; background:transparent; color:rgb(var(--theme-text)); border-radius:.35rem; font-size:.75rem; }
  .approval-choice span { min-width:0; overflow-wrap:anywhere; }
  .approval-choice:hover { background:rgb(var(--theme-text)/.06); }
  .approval-menu-hint { border-top:1px solid rgb(var(--theme-border)/.4); margin:.4rem .5rem .2rem; padding-top:.5rem; color:rgb(var(--theme-text-tertiary)); font-size:.6875rem; line-height:1.5; }
</style>
