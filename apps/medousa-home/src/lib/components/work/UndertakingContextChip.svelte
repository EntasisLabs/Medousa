<script lang="ts">
  import {
    Bot,
    ChevronDown,
    CircleDot,
    ExternalLink,
    GitPullRequestArrow,
    Link2Off,
    SquareTerminal,
  } from "@lucide/svelte";
  import { getUndertaking, humanPhaseLabel } from "$lib/forge";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import {
    openTrackedTerminal,
    startTrackedAgent,
  } from "$lib/utils/undertakingWorkspace";

  const active = $derived(undertakings.active);
  let busy = $state(false);
  let error = $state<string | null>(null);

  function goDetail() {
    if (!active) return;
    undertakings.setWorkTab("undertakings");
    shellTabs.openSurface("work", { activate: true });
    void undertakings.select(active.workId);
  }

  async function withItem(action: "terminal" | "codex" | "cursor") {
    if (!active || busy) return;
    busy = true;
    error = null;
    try {
      const item = await getUndertaking(active.workId);
      if (action === "terminal") await openTrackedTerminal(item);
      else await startTrackedAgent(item, action);
      await undertakings.select(item.id);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  function detach() {
    if (chat.sessionId) undertakings.detachChat(chat.sessionId);
    undertakings.clearActive();
  }
</script>

{#if active}
  <details class="group relative max-w-full">
    <summary
      class="flex max-w-full cursor-pointer list-none items-center gap-1.5 rounded-full border border-surface-500/35 bg-surface-900/75 px-2.5 py-1 text-[11px] text-surface-200 transition hover:border-surface-400/60 hover:bg-surface-800/90 [&::-webkit-details-marker]:hidden"
      aria-label={`Undertaking context: ${active.title}`}
    >
      <CircleDot
        size={12}
        class={active.humanPhase === "review" ? "text-amber-300" : "text-primary-400"}
        aria-hidden="true"
      />
      <span class="truncate font-medium text-surface-100">{active.title}</span>
      <span class="shrink-0 text-surface-500">·</span>
      <span class="shrink-0 text-surface-400">{humanPhaseLabel(active.humanPhase)}</span>
      {#if active.executorKind}
        <span class="hidden shrink-0 text-surface-500 sm:inline">{active.executorKind}</span>
      {/if}
      <ChevronDown
        size={12}
        class="shrink-0 text-surface-500 transition group-open:rotate-180"
        aria-hidden="true"
      />
    </summary>

    <div
      class="absolute left-0 top-full z-50 mt-1.5 w-64 rounded-xl border border-surface-500/40 bg-surface-900/95 p-1.5 text-xs shadow-2xl backdrop-blur"
    >
      <div class="px-2 py-1.5">
        <p class="truncate font-medium text-surface-100">{active.title}</p>
        <p class="mt-0.5 text-[10px] text-surface-500">
          {humanPhaseLabel(active.humanPhase)}
          {#if active.attemptSeq} · attempt {active.attemptSeq}{/if}
          {#if active.executorKind} · {active.executorKind}{/if}
        </p>
      </div>

      <button type="button" class="context-action" onclick={goDetail}>
        {#if active.humanPhase === "review"}
          <GitPullRequestArrow size={14} />
          Open ForgeLens review
        {:else}
          <ExternalLink size={14} />
          Open undertaking
        {/if}
      </button>
      <button
        type="button"
        class="context-action"
        disabled={busy}
        onclick={() => void withItem("terminal")}
      >
        <SquareTerminal size={14} />
        Continue in Terminal
      </button>

      {#if active.humanPhase === "work" || active.humanPhase === "prepare"}
        <div class="my-1 border-t border-surface-500/25"></div>
        <button
          type="button"
          class="context-action"
          disabled={busy}
          onclick={() => void withItem("codex")}
        >
          <Bot size={14} />
          Continue with Codex
        </button>
        <button
          type="button"
          class="context-action"
          disabled={busy}
          onclick={() => void withItem("cursor")}
        >
          <Bot size={14} />
          Continue with Cursor
        </button>
      {/if}

      <div class="my-1 border-t border-surface-500/25"></div>
      <button type="button" class="context-action text-surface-400" onclick={detach}>
        <Link2Off size={14} />
        Detach this pane
      </button>

      {#if error}
        <p class="m-1.5 rounded-md bg-amber-950/60 px-2 py-1.5 text-[10px] text-amber-100">
          {error}
        </p>
      {/if}
    </div>
  </details>
{/if}

<style>
  .context-action {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.5rem;
    border-radius: 0.5rem;
    padding: 0.45rem 0.5rem;
    color: rgb(var(--color-surface-200));
    text-align: left;
  }

  .context-action:hover:not(:disabled) {
    background: rgb(var(--color-surface-700) / 0.65);
    color: rgb(var(--color-surface-50));
  }

  .context-action:disabled {
    opacity: 0.4;
  }
</style>
