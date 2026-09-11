<script lang="ts">
  import { Check, MessageSquareWarning, Pencil } from "@lucide/svelte";

  interface Props {
    attached: boolean; busy: boolean; canApprove: boolean;
    allowContinue: boolean; allowReview: boolean; allowApply: boolean;
    sourceBranch: string; reviewedBranch: string; actionLabel: string;
    integrationStrategy: "fast_forward_only" | "preserve_branch";
    onContinue: () => void; onRequestChanges: () => void;
    onApprove: () => void; onApply: () => void;
  }

  let { attached, busy, canApprove, allowContinue, allowReview, allowApply,
    sourceBranch, reviewedBranch, actionLabel,
    integrationStrategy = $bindable(), onContinue, onRequestChanges,
    onApprove, onApply }: Props = $props();
</script>

{#if allowReview}
  {#if allowContinue}
    <button type="button" class="scripts-workbench-toolbar-btn" disabled={busy} title="Continue editing" aria-label="Continue editing" onclick={onContinue}>
      <Pencil size={14} strokeWidth={1.75} />
    </button>
  {/if}
  <button type="button" class="scripts-workbench-toolbar-btn" disabled={busy} title="Request changes" aria-label="Request changes" onclick={onRequestChanges}>
    <MessageSquareWarning size={14} strokeWidth={1.75} />
  </button>
  {#if !attached}
    <label class="sr-only" for="review-integration-strategy">After approval</label>
    <select id="review-integration-strategy" class="max-w-[min(13rem,32vw)] rounded-md border border-surface-500/35 bg-surface-900 px-2 py-1 text-[11px] text-content-secondary focus:border-primary-400/60 focus:ring-primary-400/35" bind:value={integrationStrategy} disabled={busy} title="Choose what approval will do">
      <option value="fast_forward_only">Apply to {sourceBranch}</option>
      <option value="preserve_branch">Keep {reviewedBranch}</option>
    </select>
  {/if}
  <button type="button" class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary flex items-center gap-1.5 px-2" disabled={!canApprove} title={`Approve changes · ${actionLabel}`} aria-label={`Approve changes · ${actionLabel}`} onclick={onApprove}>
    <Check size={14} strokeWidth={1.75} /><span class="hidden sm:inline">Approve</span>
  </button>
{:else if allowApply}
  <button type="button" class="scripts-workbench-toolbar-btn scripts-workbench-toolbar-btn-primary flex items-center gap-1.5 px-2" disabled={busy} title={actionLabel} aria-label={actionLabel} onclick={onApply}>
    <Check size={14} strokeWidth={1.75} /><span>{actionLabel}</span>
  </button>
{/if}
