<script lang="ts">
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { humanPhaseLabel } from "$lib/forge";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { terminalCreate } from "$lib/terminal";

  const active = $derived(undertakings.active);

  function goDetail() {
    if (!active) return;
    undertakings.setWorkTab("undertakings");
    shellTabs.openSurface("work", { activate: true });
    void undertakings.select(active.workId);
  }

  function goReview() {
    if (!active) return;
    goDetail();
  }

  async function openWorktreeTerminal() {
    if (!active) return;
    try {
      const created = await terminalCreate({
        work_id: active.workId,
        lease_id: active.leaseId,
      });
      const sid =
        typeof created.session_id === "string"
          ? created.session_id
          : String((created as { id?: string }).id ?? "");
      if (sid) {
        undertakings.bindTerminal(sid);
        shellTabs.openTerminal(sid, {
          activate: true,
          title: `Terminal · ${active.title}`,
        });
      }
    } catch {
      /* surface via terminal pane */
    }
  }

  function detach() {
    if (chat.sessionId) undertakings.detachChat(chat.sessionId);
    undertakings.clearActive();
  }
</script>

{#if active}
  <div
    class="flex max-w-full items-center gap-1.5 rounded-full border border-primary-500/35 bg-surface-900/80 px-2.5 py-1 text-[11px] text-surface-100"
  >
    <span class="h-1.5 w-1.5 shrink-0 rounded-full bg-primary-400" aria-hidden="true"></span>
    <button type="button" class="truncate font-medium hover:underline" onclick={goDetail}>
      {active.title}
    </button>
    <span class="shrink-0 text-surface-400">·</span>
    <span class="shrink-0 text-surface-300">{humanPhaseLabel(active.humanPhase)}</span>
    {#if active.humanPhase === "review"}
      <button
        type="button"
        class="shrink-0 text-primary-300 hover:underline"
        onclick={goReview}
        title="Open ForgeLens review"
      >
        Review
      </button>
    {/if}
    <button
      type="button"
      class="shrink-0 text-surface-500 hover:text-surface-200"
      title="Open worktree terminal"
      onclick={() => void openWorktreeTerminal()}
    >
      ⌁
    </button>
    <button
      type="button"
      class="shrink-0 text-surface-500 hover:text-surface-200"
      title="Detach from undertaking"
      onclick={detach}
    >
      ×
    </button>
  </div>
{/if}
