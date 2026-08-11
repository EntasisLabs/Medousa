<script lang="ts">
  import { tick } from "svelte";
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
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import CodeProjectCreationFlow from "$lib/components/code/CodeProjectCreationFlow.svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import { attachComposerMenuDismiss } from "$lib/utils/composerMenuDismiss";
  import { placeToolbarPopover } from "$lib/utils/railPopover";

  interface Props {
    chatOnly?: boolean;
    header?: boolean;
  }

  let { chatOnly = false, header = false }: Props = $props();
  let chipMenuOpen = $state(false);
  const active = $derived(
    chatOnly && !undertakings.active?.boundChatSessionIds.includes(chat.sessionId)
      ? null
      : undertakings.active,
  );
  const review = $derived(
    active && undertakings.review?.work_id === active.workId ? undertakings.review : null,
  );
  let busy = $state(false);
  let error = $state<string | null>(null);
  let activeMode = $state<"general" | "coder">("general");
  let chooserOpen = $state(false);
  let chooserTriggerEl = $state<HTMLButtonElement | null>(null);
  let chooserPanelEl = $state<HTMLDivElement | null>(null);
  let creating = $state(false);
  let observedBindingWorkId: string | null | undefined = undefined;

  async function hydrateSharedBinding(sessionId: string) {
    if (!sessionId) return;
    try {
      const [binding, mode] = await Promise.all([
        getSessionCodeBinding(sessionId),
        getSessionAgentMode(sessionId),
      ]);
      if (chat.sessionId !== sessionId) return;
      activeMode = mode.effective_mode;
      const nextWorkId = binding.work_id?.trim() || null;
      if (
        (observedBindingWorkId !== undefined && observedBindingWorkId !== nextWorkId) ||
        (observedBindingWorkId === undefined && nextWorkId)
      ) {
        window.dispatchEvent(
          new CustomEvent("medousa-code-project-binding-changed", {
            detail: { sessionId, workId: nextWorkId },
          }),
        );
      }
      observedBindingWorkId = nextWorkId;
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
    observedBindingWorkId = undefined;
    chooserOpen = false;
    creating = false;
    void hydrateSharedBinding(sessionId);
    const interval = window.setInterval(() => void hydrateSharedBinding(sessionId), 2_000);
    return () => window.clearInterval(interval);
  });

  $effect(() => {
    if (!chooserOpen || !chooserTriggerEl || !chooserPanelEl) return;
    let frame = 0;
    const placeOnce = () => {
      if (!chooserTriggerEl || !chooserPanelEl) return;
      placeToolbarPopover(chooserTriggerEl, chooserPanelEl, {
        prefer: header ? "below" : "above",
        align: "start",
        width: (creating ? 23 : 18) * 16,
        maxHeightRatio: 0.72,
        gap: 6,
        pad: 8,
      });
      chooserPanelEl.style.overflowY = "auto";
    };
    const place = () => {
      placeOnce();
      frame = window.requestAnimationFrame(placeOnce);
    };
    void tick().then(place);
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    window.visualViewport?.addEventListener("scroll", place);
    const detachDismiss = attachComposerMenuDismiss({
      isInside: (target) =>
        Boolean(chooserPanelEl?.contains(target) || chooserTriggerEl?.contains(target)),
      onDismiss: () => (chooserOpen = false),
    });
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("scroll", place);
      detachDismiss();
    };
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
    if (active.humanPhase === "review") {
      void lmeWorkspace.openCodeReview(active.workId, `Review · ${active.title}`);
      return;
    }
    void lmeWorkspace.openCodeWorkspace(active.workId, active.title);
  }

  $effect(() => {
    if (!active || active.humanPhase !== "review") return;
    if (undertakings.review?.work_id === active.workId) return;
    void undertakings.select(active.workId);
  });

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
    const sessionId = chat.sessionId;
    if (sessionId) {
      undertakings.detachChat(sessionId);
      try {
        await clearSessionCodeBinding(sessionId);
        observedBindingWorkId = null;
        window.dispatchEvent(
          new CustomEvent("medousa-code-project-binding-changed", {
            detail: { sessionId, workId: null },
          }),
        );
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
    const sessionId = chat.sessionId;
    if (!sessionId || busy) return;
    busy = true;
    error = null;
    try {
      await setSessionCodeBinding(sessionId, item.id);
      if (chat.sessionId !== sessionId) return;
      undertakings.setActiveFromItem(item);
      undertakings.bindChat(sessionId);
      observedBindingWorkId = item.id;
      window.dispatchEvent(
        new CustomEvent("medousa-code-project-binding-changed", {
          detail: { sessionId, workId: item.id },
        }),
      );
      chooserOpen = false;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      busy = false;
    }
  }

  function finishSharedCreation(item: ItemProjection) {
    observedBindingWorkId = item.id;
    creating = false;
    chooserOpen = false;
  }
</script>

{#if active}
  <div class={header ? "flex min-w-0 max-w-full items-center gap-2" : "flex max-w-full flex-col gap-1.5"}>
  {#if header}<span class="shrink-0 text-content-faint" aria-hidden="true">/</span>{/if}
  <OverflowMenu
    bind:open={chipMenuOpen}
    align="left"
    class="max-w-full"
    panelClass="w-64 rounded-xl border border-surface-500/40 bg-surface-900/95 p-1.5 text-xs shadow-2xl backdrop-blur"
  >
    {#snippet trigger({ open, toggle })}
      <button
        type="button"
        class={header
          ? "flex min-w-0 max-w-full cursor-pointer items-center gap-1.5 rounded-md px-1.5 py-1 text-xs text-content-secondary transition hover:bg-surface-800/70 hover:text-surface-50"
          : "flex max-w-full cursor-pointer items-center gap-1.5 rounded-full border border-surface-500/35 bg-surface-900/75 px-2.5 py-1 text-chrome-md text-surface-200 transition hover:border-surface-400/60 hover:bg-surface-800/90"}
        aria-label={`Current project: ${active.title}`}
        aria-expanded={open}
        aria-haspopup="menu"
        onclick={toggle}
      >
        <CircleDot
          size={12}
          class={active.humanPhase === "review" ? "text-amber-300" : "text-primary-400"}
          aria-hidden="true"
        />
        <span class="truncate font-medium text-surface-100">{active.title}</span>
        <span class="shrink-0 text-content-quiet">·</span>
        <span class="shrink-0 text-content-tertiary">{humanPhaseLabel(active.humanPhase)}</span>
        {#if review && review.candidates.length > 1}
          <span class="hidden shrink-0 text-amber-300/90 sm:inline"
            >{review.candidates.length} candidates</span
          >
        {/if}
        {#if active.executorKind}
          <span class="hidden shrink-0 text-content-quiet sm:inline">{humanExecutorLabel(active.executorKind)}</span>
        {/if}
        <ChevronDown
          size={12}
          class="shrink-0 text-content-quiet transition {open ? 'rotate-180' : ''}"
          aria-hidden="true"
        />
      </button>
    {/snippet}

    <div class="px-2 py-1.5">
      <p class="truncate font-medium text-surface-100">{active.title}</p>
      <p class="mt-0.5 text-chrome-sm text-content-quiet">
        {humanPhaseGuidance(active.humanPhase)}
      </p>
    </div>

    <button type="button" role="menuitem" class="context-action" onclick={() => { chipMenuOpen = false; goDetail(); }}>
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
      role="menuitem"
      class="context-action"
      disabled={busy}
      onclick={() => { chipMenuOpen = false; void withItem("terminal"); }}
    >
      <SquareTerminal size={14} />
      Open Terminal here
    </button>

    {#if active.humanPhase === "work" || active.humanPhase === "prepare"}
      <div class="my-1 border-t border-surface-500/25" role="separator"></div>
      <button
        type="button"
        role="menuitem"
        class="context-action"
        disabled={busy}
        onclick={() => { chipMenuOpen = false; void withItem("codex"); }}
      >
        <Bot size={14} />
        Ask Codex to continue
      </button>
      <button
        type="button"
        role="menuitem"
        class="context-action"
        disabled={busy}
        onclick={() => { chipMenuOpen = false; void withItem("cursor"); }}
      >
        <Bot size={14} />
        Ask Cursor to continue
      </button>
    {/if}

    <div class="my-1 border-t border-surface-500/25" role="separator"></div>
    <button type="button" role="menuitem" class="context-action text-content-tertiary" onclick={() => { chipMenuOpen = false; void detach(); }}>
      <Link2Off size={14} />
      Stop following this project
    </button>

    {#if error}
      <p class="m-1.5 rounded-md bg-amber-950/60 px-2 py-1.5 text-chrome-sm text-amber-100">
        {humanizeForgeMessage(error)}
      </p>
    {/if}
  </OverflowMenu>
  </div>
{:else if activeMode === "coder"}
  <div class="flex min-w-0 max-w-full items-center gap-2">
    {#if header}<span class="shrink-0 text-content-faint" aria-hidden="true">/</span>{/if}
    <button
      bind:this={chooserTriggerEl}
      type="button"
      class={header
        ? "flex min-w-0 max-w-full items-center gap-1.5 rounded-md bg-primary-950/45 px-1.5 py-1 text-xs text-primary-100 transition hover:bg-primary-900/45"
        : "flex max-w-full items-center gap-1.5 rounded-full border border-primary-500/45 bg-primary-950/55 px-2.5 py-1 text-[11px] text-primary-100 transition hover:border-primary-400/70"}
      onclick={() => void openChooser()}
    >
      <FolderPlus size={12} aria-hidden="true" />
      <span>Choose or create project</span>
      <ChevronDown size={12} class={chooserOpen ? "rotate-180" : ""} aria-hidden="true" />
    </button>
    {#if chooserOpen}
      <BodyPortal>
      <div
        bind:this={chooserPanelEl}
        class="z-50 {creating ? 'w-[23rem]' : 'w-72'} overflow-hidden rounded-xl border border-surface-500/40 bg-surface-900/95 {creating ? 'p-0' : 'p-2'} text-xs shadow-2xl backdrop-blur"
        role="dialog"
        aria-label="Choose or create project"
      >
        {#if creating}
          <CodeProjectCreationFlow
            presentation="popover"
            sessionId={chat.sessionId}
            onCancel={() => (creating = false)}
            onCreated={finishSharedCreation}
            onContinue={finishSharedCreation}
          />
        {:else}
          <p class="px-1.5 pb-1.5 text-[10px] font-medium uppercase tracking-wide text-content-quiet">Continue a project</p>
          {#each undertakings.items.filter((item) => ["ready", "executing"].includes(item.state) && item.environment?.worktree).slice(0, 6) as item (item.id)}
            <button type="button" class="context-action" disabled={busy} onclick={() => void bindProject(item)}>
              <CircleDot size={13} />
              <span class="truncate">{item.title}</span>
            </button>
          {:else}
            <p class="px-1.5 py-2 text-content-quiet">No ready projects yet.</p>
          {/each}
          <div class="my-1 border-t border-surface-500/25"></div>
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
      </BodyPortal>
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
