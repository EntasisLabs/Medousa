<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import {
    approveTurnBudgetRequest,
    denyTurnBudgetRequest,
  } from "$lib/daemon";
  import { homeChannelSurface } from "$lib/platform";
  import { haptic } from "$lib/haptics";

  interface Props {
    mobile?: boolean;
    onOpenWork?: () => void;
  }

  let { mobile = false, onOpenWork }: Props = $props();

  let busy = $state(false);
  let feedback = $state<string | null>(null);

  const pending = $derived(
    chat.budgetAlert ?? chat.pendingBudgetApprovals[0] ?? null,
  );

  async function approve() {
    if (!pending || busy) return;
    busy = true;
    feedback = null;
    try {
      const response = await approveTurnBudgetRequest(
        pending.requestId,
        pending.requestedRounds ?? undefined,
        homeChannelSurface(),
      );
      chat.noteBudgetResolved(pending.requestId);
      chat.clearBudgetAlert();
      feedback = response.message || "Approved";
      haptic("success");
    } catch (err) {
      feedback = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function deny() {
    if (!pending || busy) return;
    busy = true;
    feedback = null;
    try {
      const response = await denyTurnBudgetRequest(
        pending.requestId,
        homeChannelSurface(),
      );
      chat.noteBudgetResolved(pending.requestId);
      chat.clearBudgetAlert();
      feedback = response.message || "Denied";
      haptic("light");
    } catch (err) {
      feedback = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function openWorkCard() {
    if (!pending) return;
    onOpenWork?.();
    await workspace.selectCard(pending.workCardId);
  }
</script>

{#if pending}
  <div
    class="{mobile
      ? 'mx-3 mb-2 rounded-xl border border-warning-500/30 bg-warning-950/40 p-3'
      : 'chat-budget-bar mx-4 mb-2 rounded-lg border border-warning-500/25 bg-warning-950/35 px-3 py-2.5'}"
    role="status"
    aria-live="polite"
  >
    <div class="flex flex-wrap items-start justify-between gap-2">
      <div class="min-w-0 flex-1">
        <p class="text-xs font-medium text-warning-200">Needs your approval</p>
        <p class="mt-0.5 text-sm text-surface-100">
          {pending.message}
        </p>
        {#if pending.requestedRounds}
          <p class="workshop-faint mt-1 text-xs">
            Requesting +{pending.requestedRounds} tool round{pending.requestedRounds === 1
              ? ""
              : "s"}
          </p>
        {/if}
        {#if feedback}
          <p class="mt-1 text-xs text-surface-400">{feedback}</p>
        {/if}
      </div>
      <div class="flex shrink-0 flex-wrap gap-1.5">
        <button
          type="button"
          class="btn btn-sm variant-filled-warning"
          disabled={busy}
          onclick={() => void approve()}
        >
          Approve
        </button>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          disabled={busy}
          onclick={() => void deny()}
        >
          Deny
        </button>
        {#if onOpenWork}
          <button
            type="button"
            class="btn btn-sm variant-ghost-surface"
            disabled={busy}
            onclick={() => void openWorkCard()}
          >
            Work
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
