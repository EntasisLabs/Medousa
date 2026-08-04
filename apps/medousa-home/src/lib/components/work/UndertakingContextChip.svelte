<script lang="ts">
  import {
    Bot,
    ChevronDown,
    CircleDot,
    ExternalLink,
    FolderPlus,
    GitPullRequestArrow,
    Link2Off,
    SquareTerminal,
  } from "@lucide/svelte";
  import {
    clearSessionCodeBinding,
    getSessionAgentMode,
    getSessionCodeBinding,
    setSessionCodeBinding,
    startSessionCodeProject,
  } from "$lib/daemon";
  import {
    getUndertaking,
    humanExecutorLabel,
    humanPhaseGuidance,
    humanPhaseLabel,
    humanizeForgeMessage,
    type ItemProjection,
  } from "$lib/forge";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import {
    openTrackedTerminal,
    startTrackedAgent,
  } from "$lib/utils/undertakingWorkspace";

  interface Props {
    chatOnly?: boolean;
  }

  let { chatOnly = false }: Props = $props();
  const active = $derived(
    chatOnly && !undertakings.active?.boundChatSessionIds.includes(chat.sessionId)
      ? null
      : undertakings.active,
  );
  let busy = $state(false);
  let error = $state<string | null>(null);
  let activeMode = $state<"general" | "coder">("general");
  let chooserOpen = $state(false);
  let creating = $state(false);
  let newTitle = $state("");
  let newBrief = $state("");

  async function hydrateSharedBinding(sessionId: string) {
    if (!sessionId) return;
    try {
      const [binding, mode] = await Promise.all([
        getSessionCodeBinding(sessionId),
        getSessionAgentMode(sessionId),
      ]);
      if (chat.sessionId !== sessionId) return;
      activeMode = mode.effective_mode;
      if (!binding.work_id) {
        undertakings.detachChat(sessionId);
        return;
      }
      if (undertakings.active?.workId === binding.work_id) {
        undertakings.bindChat(sessionId);
        return;
      }
      const item = await getUndertaking(binding.work_id);
      if (chat.sessionId !== sessionId) return;
      undertakings.setActiveFromItem(item);
      undertakings.bindChat(sessionId);
    } catch {
      // The chat connection UI owns transport errors; hydrate opportunistically.
    }
  }

  $effect(() => {
    if (!chatOnly) return;
    const sessionId = chat.sessionId;
    activeMode = "general";
    chooserOpen = false;
    creating = false;
    void hydrateSharedBinding(sessionId);
    const interval = window.setInterval(() => void hydrateSharedBinding(sessionId), 2_000);
    return () => window.clearInterval(interval);
  });

  $effect(() => {
    if (!chatOnly) return;
    const open = () => {
      chooserOpen = true;
      void undertakings.refreshList();
    };
    window.addEventListener("medousa-open-code-project-chooser", open);
    const refreshMode = () => void hydrateSharedBinding(chat.sessionId);
    window.addEventListener("medousa-agent-mode-changed", refreshMode);
    return () => {
      window.removeEventListener("medousa-open-code-project-chooser", open);
      window.removeEventListener("medousa-agent-mode-changed", refreshMode);
    };
  });

  function goDetail() {
    if (!active) return;
    void lmeWorkspace.openCodeWorkspace(active.workId, active.title);
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

  async function detach() {
    if (chat.sessionId) {
      undertakings.detachChat(chat.sessionId);
      try {
        await clearSessionCodeBinding(chat.sessionId);
      } catch {
        // Clearing the binding is independent from changing the active mode.
      }
    }
    undertakings.clearActive();
  }

  async function openChooser() {
    chooserOpen = !chooserOpen;
    if (chooserOpen) await undertakings.refreshList();
  }

  async function bindProject(item: ItemProjection) {
    if (!chat.sessionId || busy) return;
    busy = true;
    error = null;
    try {
      await setSessionCodeBinding(chat.sessionId, item.id);
      undertakings.setActiveFromItem(item);
      undertakings.bindChat(chat.sessionId);
      chooserOpen = false;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  async function createProject() {
    if (!chat.sessionId || !newTitle.trim() || busy) return;
    busy = true;
    error = null;
    try {
      const created = await startSessionCodeProject(chat.sessionId, {
        title: newTitle.trim(),
        brief: newBrief.trim() || newTitle.trim(),
        source: "blank",
      });
      const item = await getUndertaking(created.work_id);
      undertakings.setActiveFromItem(item);
      undertakings.bindChat(chat.sessionId);
      newTitle = "";
      newBrief = "";
      creating = false;
      chooserOpen = false;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }
</script>

{#if active}
  <details class="group relative max-w-full">
    <summary
      class="flex max-w-full cursor-pointer list-none items-center gap-1.5 rounded-full border border-surface-500/35 bg-surface-900/75 px-2.5 py-1 text-[11px] text-surface-200 transition hover:border-surface-400/60 hover:bg-surface-800/90 [&::-webkit-details-marker]:hidden"
      aria-label={`Current project: ${active.title}`}
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
        <span class="hidden shrink-0 text-surface-500 sm:inline">{humanExecutorLabel(active.executorKind)}</span>
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
          {humanPhaseGuidance(active.humanPhase)}
        </p>
      </div>

      <button type="button" class="context-action" onclick={goDetail}>
        {#if active.humanPhase === "review"}
          <GitPullRequestArrow size={14} />
          Review changes
        {:else}
          <ExternalLink size={14} />
          Open project
        {/if}
      </button>
      <button
        type="button"
        class="context-action"
        disabled={busy}
        onclick={() => void withItem("terminal")}
      >
        <SquareTerminal size={14} />
        Open Terminal here
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
          Ask Codex to continue
        </button>
        <button
          type="button"
          class="context-action"
          disabled={busy}
          onclick={() => void withItem("cursor")}
        >
          <Bot size={14} />
          Ask Cursor to continue
        </button>
      {/if}

      <div class="my-1 border-t border-surface-500/25"></div>
      <button type="button" class="context-action text-surface-400" onclick={() => void detach()}>
        <Link2Off size={14} />
        Stop following this project
      </button>

      {#if error}
        <p class="m-1.5 rounded-md bg-amber-950/60 px-2 py-1.5 text-[10px] text-amber-100">
          {humanizeForgeMessage(error)}
        </p>
      {/if}
    </div>
  </details>
{:else if activeMode === "coder"}
  <div class="relative max-w-full">
    <button
      type="button"
      class="flex max-w-full items-center gap-1.5 rounded-full border border-primary-500/45 bg-primary-950/55 px-2.5 py-1 text-[11px] text-primary-100 transition hover:border-primary-400/70"
      onclick={() => void openChooser()}
    >
      <FolderPlus size={12} aria-hidden="true" />
      <span>Choose or create project</span>
      <ChevronDown size={12} class={chooserOpen ? "rotate-180" : ""} aria-hidden="true" />
    </button>
    {#if chooserOpen}
      <div class="absolute left-0 top-full z-50 mt-1.5 w-72 rounded-xl border border-surface-500/40 bg-surface-900/95 p-2 text-xs shadow-2xl backdrop-blur">
        <p class="px-1.5 pb-1.5 text-[10px] font-medium uppercase tracking-wide text-surface-500">Continue a project</p>
        {#each undertakings.items.filter((item) => ["ready", "executing"].includes(item.state) && item.environment?.worktree).slice(0, 6) as item (item.id)}
          <button type="button" class="context-action" disabled={busy} onclick={() => void bindProject(item)}>
            <CircleDot size={13} />
            <span class="truncate">{item.title}</span>
          </button>
        {:else}
          <p class="px-1.5 py-2 text-surface-500">No ready projects yet.</p>
        {/each}
        <div class="my-1 border-t border-surface-500/25"></div>
        {#if creating}
          <form class="space-y-1.5 p-1" onsubmit={(event) => { event.preventDefault(); void createProject(); }}>
            <input class="w-full rounded-md border border-surface-500/40 bg-surface-950 px-2 py-1.5 text-surface-100" placeholder="Project name" bind:value={newTitle} />
            <textarea class="w-full resize-none rounded-md border border-surface-500/40 bg-surface-950 px-2 py-1.5 text-surface-100" rows="2" placeholder="What should Medousa build?" bind:value={newBrief}></textarea>
            <div class="flex justify-end gap-1.5">
              <button type="button" class="btn btn-sm variant-ghost-surface" onclick={() => (creating = false)}>Cancel</button>
              <button type="submit" class="btn btn-sm variant-filled-primary" disabled={busy || !newTitle.trim()}>Create and bind</button>
            </div>
          </form>
        {:else}
          <button type="button" class="context-action" onclick={() => (creating = true)}>
            <FolderPlus size={14} />
            Create a new project
          </button>
          <button
            type="button"
            class="context-action text-primary-200"
            onclick={() => {
              chooserOpen = false;
              window.dispatchEvent(new CustomEvent("medousa-code-project-agent-setup"));
            }}
          >
            <Bot size={14} />
            Let Medousa choose or create it
          </button>
        {/if}
        {#if error}
          <p class="m-1.5 rounded-md bg-amber-950/60 px-2 py-1.5 text-[10px] text-amber-100">{humanizeForgeMessage(error)}</p>
        {/if}
      </div>
    {/if}
  </div>
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
