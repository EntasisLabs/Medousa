<script lang="ts">
  import { chat } from "$lib/stores/chat.svelte";
  import {
    approveAgentPermission,
    denyAgentPermission,
  } from "$lib/daemon";
  import { homeChannelSurface } from "$lib/platform";
  import { haptic } from "$lib/haptics";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  let busy = $state(false);
  let feedback = $state<string | null>(null);

  const pending = $derived(chat.permissionAlert);

  async function approve() {
    if (!pending || busy) return;
    busy = true;
    feedback = null;
    try {
      await approveAgentPermission(pending.requestId, homeChannelSurface());
      chat.notePermissionResolved(pending.requestId);
      feedback = "Approved";
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
      await denyAgentPermission(pending.requestId, homeChannelSurface());
      chat.notePermissionResolved(pending.requestId);
      feedback = "Denied";
      haptic("light");
    } catch (err) {
      feedback = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

{#if pending}
  <div
    class="{mobile
      ? 'mx-3 mb-2 rounded-xl border border-amber-500/30 bg-amber-950/40 p-3'
      : 'chat-permission-bar mx-4 mb-2 rounded-lg border border-amber-500/25 bg-amber-950/35 px-3 py-2.5'}"
    role="status"
    aria-live="polite"
  >
    <div class="flex flex-wrap items-start justify-between gap-2">
      <div class="min-w-0 flex-1">
        <p class="text-xs font-medium text-amber-200">Agent needs permission</p>
        <p class="mt-0.5 text-sm text-surface-100">
          {pending.message}
        </p>
        {#if pending.agentRuntime}
          <p class="workshop-faint mt-1 text-xs">
            Runtime: {pending.agentRuntime}
          </p>
        {/if}
        {#if feedback}
          <p class="mt-1 text-xs text-content-tertiary">{feedback}</p>
        {/if}
      </div>
      <div class="flex shrink-0 flex-wrap gap-1.5">
        <button
          type="button"
          class="btn btn-sm variant-filled-warning"
          disabled={busy}
          onclick={() => void approve()}
        >
          Allow
        </button>
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface"
          disabled={busy}
          onclick={() => void deny()}
        >
          Deny
        </button>
      </div>
    </div>
  </div>
{/if}
