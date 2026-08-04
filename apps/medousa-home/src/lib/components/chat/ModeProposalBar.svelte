<script lang="ts">
  import { haptic } from "$lib/haptics";
  import {
    decideSessionAgentModeProposal,
    getSessionCodeBinding,
    listSessionAgentModeProposals,
  } from "$lib/daemon";
  import type { AgentModeProposalResponse } from "$lib/types/generated/daemon_api";

  interface Props {
    sessionId: string;
    coderContextAvailable?: boolean;
    mobile?: boolean;
  }

  let { sessionId, coderContextAvailable = false, mobile = false }: Props = $props();
  let proposals = $state<AgentModeProposalResponse[]>([]);
  let busy = $state(false);
  let feedback = $state<string | null>(null);
  let now = $state(Date.now());
  let initialized = false;
  let daemonCodeBindingAvailable = $state(false);
  const seenResolved = new Set<string>();

  const pending = $derived(
    proposals.find((proposal) => proposal.status === "pending") ?? null,
  );
  const remainingSeconds = $derived(
    pending
      ? Math.max(0, Math.ceil((Date.parse(pending.expires_at_utc) - now) / 1000))
      : 0,
  );
  const acceptBlocked = $derived(
    pending?.to_mode === "coder" && !coderContextAvailable && !daemonCodeBindingAvailable,
  );

  async function refresh(expectedSessionId: string) {
    if (!expectedSessionId) {
      proposals = [];
      return;
    }
    try {
      const [response, binding] = await Promise.all([
        listSessionAgentModeProposals(expectedSessionId),
        getSessionCodeBinding(expectedSessionId),
      ]);
      if (expectedSessionId !== sessionId.trim()) return;
      proposals = response.proposals;
      daemonCodeBindingAvailable = Boolean(binding.work_id);
      if (initialized) {
        const newlyApplied = response.proposals.some(
          (proposal) =>
            proposal.status === "accepted" &&
            !seenResolved.has(proposal.proposal_id),
        );
        if (newlyApplied) notifyModeChanged();
      }
      for (const proposal of response.proposals) {
        if (proposal.status !== "pending") seenResolved.add(proposal.proposal_id);
      }
      initialized = true;
    } catch {
      // Connection state owns offline/error presentation; polling stays quiet.
    }
  }

  function notifyModeChanged() {
    window.dispatchEvent(new CustomEvent("medousa-agent-mode-changed", {
      detail: { sessionId: sessionId.trim() },
    }));
  }

  async function decide(accept: boolean) {
    if (!pending || busy || (accept && acceptBlocked)) return;
    busy = true;
    feedback = null;
    try {
      const result = await decideSessionAgentModeProposal(
        sessionId,
        pending.proposal_id,
        accept,
      );
      seenResolved.add(result.proposal_id);
      feedback = accept ? "Mode change accepted" : "Mode change denied";
      if (accept && result.status === "accepted") notifyModeChanged();
      haptic(accept ? "success" : "light");
      await refresh(sessionId.trim());
    } catch (err) {
      feedback = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    const activeSessionId = sessionId.trim();
    initialized = false;
    seenResolved.clear();
    void refresh(activeSessionId);
    const interval = window.setInterval(() => {
      now = Date.now();
      void refresh(activeSessionId);
    }, 1_000);
    return () => window.clearInterval(interval);
  });
</script>

{#if pending}
  <div
    class="{mobile
      ? 'mx-3 mb-2 rounded-xl border border-primary-500/30 bg-primary-950/40 p-3'
      : 'mx-4 mb-2 rounded-lg border border-primary-500/25 bg-primary-950/35 px-3 py-2.5'}"
    role="status"
    aria-live="polite"
  >
    <div class="flex flex-wrap items-start justify-between gap-2">
      <div class="min-w-0 flex-1">
        <p class="text-xs font-medium text-primary-200">
          Switch to {pending.to_mode === "coder" ? "Coder" : "General"} mode?
        </p>
        <p class="mt-0.5 text-sm text-surface-100">{pending.reason}</p>
        <p class="workshop-faint mt-1 text-xs">
          {#if acceptBlocked}
            Bind this chat to a Forge undertaking to accept.
          {:else}
            Expires in {remainingSeconds}s · applies next turn
          {/if}
        </p>
        {#if feedback}
          <p class="mt-1 text-xs text-surface-400">{feedback}</p>
        {/if}
      </div>
      <div class="flex shrink-0 flex-wrap gap-1.5">
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          disabled={busy || acceptBlocked}
          onclick={() => void decide(true)}
        >
          Accept
        </button>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          disabled={busy}
          onclick={() => void decide(false)}
        >
          Not now
        </button>
      </div>
    </div>
  </div>
{/if}
