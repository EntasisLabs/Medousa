<script lang="ts">
  import { onMount, tick } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { createPetGesture } from "$lib/companion/petGesture";
  import MedousaCompanion from "$lib/components/brand/MedousaCompanion.svelte";
  import {
    applyCompanionStreamEvent,
    companionSpriteState,
    initialCompanionActivity,
    type CompanionFeedback,
  } from "$lib/companion/companionState";
  import CompanionConversationView from "$lib/components/brand/CompanionConversation.svelte";
  import { CompanionConversation } from "$lib/companion/companionConversation.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { setActiveWorkshopIdPort } from "$lib/utils/workshopLocality";
  import {
    anchoredCompanionPosition,
    companionWorkAreaForWindow,
  } from "$lib/companion/windowGeometry";
  import {
    approveTurnBudgetRequest,
    denyTurnBudgetRequest,
    listTurnBudgetRequests,
    onInteractiveEvent,
    type DaemonHealth,
    type TurnBudgetRequestRecord,
  } from "$lib/daemon";
  import { environment } from "$lib/stores/environment.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { listenForMedousaMark } from "$lib/settings/companionAppearanceSync";
  import { medousaMarkOption } from "$lib/theme/medousaMarks";
  import { homeChannelSurface } from "$lib/platform";
  import type { TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";
  import {
    hideDesktopToolbar,
    isTauri,
    showBrowser,
    showChatPopout,
    showMainWindow,
    showVaultSticky,
    showViewPopout,
  } from "$lib/window";
  import { activateWorkshopScope, connectWorkshop } from "$lib/workshopConnection";
  import { whenDocumentVisible } from "$lib/utils/whenDocumentVisible";

  type WindowMode = "pet" | "bubble" | "bar" | "spotlight";
  const isFloating = (mode: WindowMode) => mode === "bar" || mode === "spotlight";

  const WINDOW_SIZES: Record<WindowMode, { width: number; height: number }> = {
    pet: { width: 112, height: 170 },
    bubble: { width: 350, height: 194 },
    bar: { width: 620, height: 82 },
    spotlight: { width: 620, height: 422 },
  };

  let expanded = $state(false);
  let renderMode = $state<WindowMode>("pet");
  let geometrySettling = $state(false);
  let bodyOpen = $state(false);
  let approvalBusy = $state(false);
  let health = $state<DaemonHealth | null>(null);
  let pendingApprovals = $state<TurnBudgetRequestRecord[]>([]);
  let activity = $state(initialCompanionActivity());
  let feedbackTimer: ReturnType<typeof setTimeout> | null = null;
  let resizeToken = 0;
  let renderedModeValue: WindowMode = "pet";
  let lastFloatingPosition: { x: number; y: number } | null = null;
  let petRestorePosition: { x: number; y: number } | null = null;

  const petGesture = createPetGesture(
    () => getCurrentWindow().startDragging(),
    cause => setFeedback({ tone: "error", message: `Could not move the companion: ${cause instanceof Error ? cause.message : String(cause)}` }),
  );
  setActiveWorkshopIdPort(() => workshops.activeWorkshopId);
  const connected = $derived(health?.ok === true);
  const addressed = new CompanionConversation(() => expanded && connected, () => connected);
  const sending = $derived(addressed.conversation.sending);
  const pendingApproval = $derived(pendingApprovals[0] ?? null);
  const companionFeedback = $derived(
    pendingApproval
      ? ({
          tone: "attention",
          message:
            pendingApproval.progress_summary?.trim() ||
            pendingApproval.reason?.trim() ||
            "Medousa needs approval to continue.",
        } satisfies CompanionFeedback)
      : activity.feedback,
  );
  const mode = $derived<WindowMode>(
    expanded ? (bodyOpen ? "spotlight" : "bar") : companionFeedback ? "bubble" : "pet",
  );
  const spriteState = $derived(
    companionSpriteState({
      connected,
      expanded,
      sending,
      activeTurnCount: activity.activeTurnIds.size,
      pendingApproval: pendingApproval != null,
      feedbackTone: companionFeedback?.tone ?? null,
    }),
  );
  const customViews = $derived(
    environment.navSurfaces().filter((surface) => surface.kind === "custom"),
  );
  const companionMark = $derived(medousaMarkOption(settings.medousaMark));
  const actions = $derived([
    { id: "chat", label: "Open Chat", description: "Full conversation window", keywords: "conversation session", icon: "chat" as const, run: openChat },
    { id: "note", label: "New note", description: "Open a sticky note", keywords: "notes vault sticky", icon: "note" as const, run: openNote },
    { id: "web", label: "Open browser", description: "Browse in Medousa", keywords: "web url website", icon: "web" as const, run: openWeb },
    { id: "main", label: "Open Medousa", description: "Main window", keywords: "home main workshop", icon: "main" as const, run: openMain },
    ...customViews.map(view => ({ id: `view:${view.id}`, label: `Open ${view.label}`, description: "Custom view", keywords: "view", icon: "view" as const, run: () => pickView(view.id) })),
  ]);
  const hasNotice = $derived(Boolean(pendingApproval || companionFeedback?.tone === "attention" || companionFeedback?.tone === "error"));

  $effect(() => {
    const nextMode = mode;
    void syncWindowSize(nextMode);
  });

  onMount(() => {
    document.documentElement.classList.add("desktop-toolbar-shell");
    document.body.classList.add("desktop-toolbar-shell");

    let detachInteractive: (() => void) | null = null;
    let detachAppearance: (() => void) | null = null;
    let mounted = true;
    const detachWorkshop = isTauri()
      ? whenDocumentVisible(() =>
          connectWorkshop({
            onHealthChange: (nextHealth) => {
              if (nextHealth?.ok) {
                activateWorkshopScope(workshops.activeWorkshopId);
                if (!health?.ok) void environment.load();
              }
              health = nextHealth;
              if (nextHealth?.ok) void refreshApprovals();
            },
            mode: "observer",
          }),
        )
      : () => {};
    if (isTauri()) {
      void onInteractiveEvent<TurnStreamEnvelopeV3>(
        handleInteractiveEvent,
      ).then(
        (detach) => {
          if (mounted) detachInteractive = detach; else detach();
        },
      );
    }
    void listenForMedousaMark((mark) =>
      settings.setMedousaMark(mark, { broadcast: false }),
    ).then((detach) => {
      if (mounted) detachAppearance = detach;
      else detach();
    });
    const approvalPoll = setInterval(() => {
      if (document.visibilityState === "visible" && health?.ok) {
        void refreshApprovals();
      }
    }, 6_000);

    return () => {
      mounted = false;
      document.documentElement.classList.remove("desktop-toolbar-shell");
      document.body.classList.remove("desktop-toolbar-shell");
      detachWorkshop();
      detachInteractive?.();
      detachAppearance?.();
      clearInterval(approvalPoll);
      if (feedbackTimer) clearTimeout(feedbackTimer);
      petGesture.destroy();
    };
  });

  function handleInteractiveEvent(
    payload: TurnStreamEnvelopeV3,
  ) {
    const result = applyCompanionStreamEvent(activity, payload);
    activity = {
      activeTurnIds: result.activeTurnIds,
      feedback: result.feedback,
    };
    if (result.approvalChanged) void refreshApprovals();
    if (result.feedback?.tone !== "attention") scheduleFeedbackClear();
  }

  function setFeedback(next: CompanionFeedback | null, autoClear = true) {
    activity = { ...activity, feedback: next };
    if (autoClear && next) scheduleFeedbackClear();
  }

  function scheduleFeedbackClear() {
    if (feedbackTimer) clearTimeout(feedbackTimer);
    feedbackTimer = setTimeout(() => {
      feedbackTimer = null;
      activity = { ...activity, feedback: null };
    }, 6_000);
  }

  async function refreshApprovals() {
    try {
      pendingApprovals = await listTurnBudgetRequests(true);
    } catch {
      // Health state already communicates connection trouble.
    }
  }

  async function syncWindowSize(nextMode: WindowMode) {
    const token = ++resizeToken;
    const transitioning = renderedModeValue !== nextMode;
    if (!isTauri()) {
      renderedModeValue = nextMode;
      renderMode = nextMode;
      return;
    }
    if (transitioning && !(isFloating(renderedModeValue) && isFloating(nextMode))) {
      geometrySettling = true;
      await tick();
      await new Promise<void>((resolve) => setTimeout(resolve, 65));
      if (token !== resizeToken) return;
    }
    try {
      const {
        getCurrentWindow,
        LogicalSize,
        PhysicalPosition,
        availableMonitors,
      } = await import("@tauri-apps/api/window");
      const current = getCurrentWindow();
      const target = WINDOW_SIZES[nextMode];
      const [position, previousSize, scaleFactor, monitors] = await Promise.all([
        current.outerPosition(),
        current.outerSize(),
        current.scaleFactor(),
        availableMonitors(),
      ]);
      if (token !== resizeToken) return;
      const physicalWidth = Math.round(target.width * scaleFactor);
      const physicalHeight = Math.round(target.height * scaleFactor);
      const petSize = {
        width: Math.round(WINDOW_SIZES.pet.width * scaleFactor),
        height: Math.round(WINDOW_SIZES.pet.height * scaleFactor),
      };
      const workArea = companionWorkAreaForWindow({
        position,
        size: previousSize,
        workAreas: monitors.map((monitor) => monitor.workArea),
      });
      const floating = isFloating(nextMode);
      const wasFloating = isFloating(renderedModeValue);
      const targetSize = { width: physicalWidth, height: physicalHeight };
      if (wasFloating && petRestorePosition && lastFloatingPosition) {
        petRestorePosition = { x: petRestorePosition.x + position.x - lastFloatingPosition.x, y: petRestorePosition.y + position.y - lastFloatingPosition.y };
      }
      // Reserve enough room when opening the slim bar so adding results below
      // does not have to move its input near the bottom of the screen.
      const reserveSize = floating ? {
        width: Math.round(WINDOW_SIZES.spotlight.width * scaleFactor),
        height: Math.round(WINDOW_SIZES.spotlight.height * scaleFactor),
      } : undefined;
      if (floating && !wasFloating && !petRestorePosition) {
        petRestorePosition = anchoredCompanionPosition({ position, previousSize, targetSize: petSize, workArea });
      }
      const petOrigin = petRestorePosition ?? position;
      const openingOrigin = {
        x: petOrigin.x + (petSize.width - 64 * scaleFactor) / 2 - 8 * scaleFactor,
        y: petOrigin.y + (petSize.height - 64 * scaleFactor) / 2 - 8 * scaleFactor,
      };
      const nextPosition = floating
        ? anchoredCompanionPosition({ position: wasFloating ? position : openingOrigin, previousSize, targetSize, workArea, anchor: "top-left", reserveSize })
        : anchoredCompanionPosition({ position: petOrigin, previousSize: petRestorePosition ? petSize : previousSize, targetSize, workArea });
      await current.setSize(new LogicalSize(target.width, target.height));
      if (token !== resizeToken) return;
      const actualSize = await current.outerSize();
      const clampedPosition = anchoredCompanionPosition({
        position: nextPosition,
        previousSize: actualSize,
        targetSize: actualSize,
        workArea,
        anchor: "top-left", reserveSize,
      });
      await current.setPosition(
        new PhysicalPosition(clampedPosition.x, clampedPosition.y),
      );
      lastFloatingPosition = floating ? clampedPosition : null;
      if (!floating) petRestorePosition = null;
      if (token !== resizeToken) return;
      renderedModeValue = nextMode;
      renderMode = nextMode;
      await tick();
      requestAnimationFrame(() => {
        if (token === resizeToken) geometrySettling = false;
      });
    } catch {
      // Window resizing is best-effort in browser development mode.
      if (token === resizeToken) {
        renderedModeValue = nextMode;
        renderMode = nextMode;
        geometrySettling = false;
      }
    }
  }

  function toggleExpanded() {
    expanded = !expanded;
    bodyOpen = expanded && Boolean(addressed.query || addressed.conversation.target || hasNotice);
  }

  function beginPetGesture(event: PointerEvent) {
    if (isTauri()) petGesture.pointerdown(event);
  }

  function handlePetClick(event: MouseEvent) {
    if (petGesture.click(event)) toggleExpanded();
  }

  function suppressImageDrag(event: DragEvent) { event.preventDefault(); }

  async function resolveApproval(approved: boolean) {
    const pending = pendingApproval;
    if (!pending || approvalBusy) return;
    approvalBusy = true;
    try {
      const response = approved
        ? await approveTurnBudgetRequest(
            pending.request_id,
            pending.requested_rounds,
            homeChannelSurface(),
          )
        : await denyTurnBudgetRequest(pending.request_id, homeChannelSurface());
      setFeedback({
        tone: approved ? "success" : "attention",
        message: response.message || (approved ? "Approved." : "Denied."),
      });
      await refreshApprovals();
    } catch (error) {
      setFeedback({
        tone: "error",
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      approvalBusy = false;
    }
  }

  async function useClipboard() {
    try {
      const clipboard = (await navigator.clipboard.readText()).trim();
      if (!clipboard) {
        setFeedback({ tone: "attention", message: "The clipboard is empty." });
        return;
      }
      const nextPrompt = `Use this clipboard context:\n\n${clipboard}`;
      addressed.query = nextPrompt;
      setFeedback({
        tone: "success",
        message: "Clipboard added. Review it before sending.",
      });
    } catch {
      setFeedback({
        tone: "error",
        message: "Medousa could not read the clipboard. Check clipboard permission.",
      });
    }
  }

  async function openChat() {
    expanded = false;
    if (isTauri()) await showChatPopout(addressed.conversation.sessionId ?? undefined, chat.workshopScopeId ?? undefined);
  }

  async function openNote() {
    expanded = false;
    if (isTauri()) await showVaultSticky();
  }

  async function openWeb() {
    expanded = false;
    if (isTauri()) await showBrowser();
  }

  async function openMain() {
    expanded = false;
    if (isTauri()) await showMainWindow();
  }

  async function pickView(surfaceId: string) {
    await showViewPopout(surfaceId);
    expanded = false;
    bodyOpen = false;
  }

  async function dismissCompanion() {
    expanded = false;
    bodyOpen = false;
    if (isTauri()) await hideDesktopToolbar();
  }
</script>

<main
  class="companion-window companion-window--{renderMode}"
  class:companion-window--settling={geometrySettling}
  data-mode={renderMode}
  style:--companion-accent={settings.darkMode ? companionMark.darkColor : companionMark.lightColor}
  style:--companion-stage={settings.darkMode ? companionMark.darkPreviewBackground : companionMark.lightPreviewBackground}
>
  {#if isFloating(renderMode)}
    <div class="companion-floating">
      <button type="button" class="companion-handle" onpointerdown={beginPetGesture} ondragstart={suppressImageDrag} onclick={handlePetClick} aria-label="Collapse companion" title="Drag to move; click to collapse">
        <MedousaCompanion state={spriteState} markId={settings.medousaMark} darkMode={settings.darkMode} size="3.5rem" label={null}/>
        {#if pendingApproval}<span class="companion-handle-badge" aria-hidden="true">!</span>{/if}
      </button>
      {#snippet notice()}
        {#if pendingApproval}
          <section class="companion-approval" aria-label="Pending approval">
            <div><strong>More tool rounds?</strong><p>{pendingApproval.progress_summary || pendingApproval.reason}</p></div>
            <div class="companion-approval-actions">
              <button type="button" class="companion-small-btn companion-small-btn--primary" disabled={approvalBusy} onclick={() => void resolveApproval(true)}>Approve</button>
              <button type="button" class="companion-small-btn" disabled={approvalBusy} onclick={() => void resolveApproval(false)}>Deny</button>
            </div>
          </section>
        {:else if companionFeedback && hasNotice}
          <p class="companion-notice" role="status">{companionFeedback.message}</p>
        {/if}
      {/snippet}
      <CompanionConversationView model={addressed} {connected} {actions} {notice} {hasNotice} scopeLabel={workshops.activeLabel}
        onOpen={() => void openChat()} onClipboard={() => void useClipboard()} onCollapse={toggleExpanded}
        onMain={() => void openMain()} onHide={() => void dismissCompanion()} onBodyChange={value => bodyOpen = value}/>
    </div>
  {:else}
    {#if renderMode === "bubble" && companionFeedback}
      <button
        type="button"
        class="companion-bubble companion-bubble--{companionFeedback.tone}"
        onclick={toggleExpanded}
        aria-label="Open companion: {companionFeedback.message}"
      >
        <strong>
          {companionFeedback.tone === "success"
            ? "Done"
            : companionFeedback.tone === "error"
              ? "Something went wrong"
              : "Needs you"}
        </strong>
        <span>{companionFeedback.message}</span>
      </button>
    {/if}
    <button
      type="button"
      class="companion-pet"
      onpointerdown={beginPetGesture}
      ondragstart={suppressImageDrag}
      onclick={handlePetClick}
      aria-label="Open Medousa companion"
      title="Medousa companion"
    >
      <MedousaCompanion
        state={spriteState}
        markId={settings.medousaMark}
        darkMode={settings.darkMode}
        size="4.35rem"
      />
      {#if activity.activeTurnIds.size > 0 || pendingApproval}
        <span class="companion-pet-badge" aria-hidden="true">
          {pendingApproval ? "!" : activity.activeTurnIds.size}
        </span>
      {/if}
    </button>
  {/if}
</main>

<style>
  :global(html.desktop-toolbar-shell),
  :global(body.desktop-toolbar-shell),
  :global(html.desktop-toolbar-shell body),
  :global(.desktop-toolbar-shell .h-full) {
    background: transparent !important;
    background-color: transparent !important;
    overflow: hidden !important;
  }

  .companion-window {
    display: flex;
    width: 100vw;
    height: 100vh;
    box-sizing: border-box;
    align-items: flex-end;
    justify-content: flex-end;
    gap: 0.45rem;
    padding: 0.45rem;
    color: rgb(var(--color-surface-50));
    background: transparent;
    opacity: 1;
    transition: opacity 65ms ease-out;
    -webkit-user-select: none;
    user-select: none;
  }

  .companion-window--settling {
    opacity: 0;
    pointer-events: none;
  }

  .companion-window--bar, .companion-window--spotlight { align-items: stretch; padding: .5rem; }
  .companion-floating { display: flex; align-items: flex-start; gap: .5rem; width: 100%; min-width: 0; height: 100%; }
  .companion-handle { position: relative; display: grid; place-items: center; width: 4rem; height: 3.65rem; flex-shrink: 0; border: 0; border-radius: 50%; padding: 0; background: transparent; cursor: grab;  }
  .companion-handle:active { cursor: grabbing; }
  .companion-handle-badge { position: absolute; right: .1rem; top: .1rem; display: grid; place-items: center; width: 1rem; height: 1rem; border-radius: 50%; background: rgb(245 158 11); color: rgb(30 20 8); font-size: .7rem; }
  .companion-notice { margin: 0; color: rgb(var(--theme-text-secondary)); font-size: .76rem; }

  .companion-pet {
    position: relative;
    display: flex;
    width: 6rem;
    height: 9.2rem;
    flex: 0 0 auto;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 2rem;
    background: transparent;
    cursor: grab;
  }

  .companion-pet:active {
    cursor: grabbing;
  }

  .companion-pet:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--companion-accent) 90%, white 10%);
    outline-offset: -0.2rem;
  }

  .companion-pet :global(*), .companion-handle :global(*) {
    pointer-events: none;
    -webkit-user-drag: none;
    user-select: none;
    -webkit-user-select: none;
  }

  .companion-pet-badge {
    position: absolute;
    top: 1.05rem;
    right: 0.45rem;
    display: grid;
    min-width: 1.35rem;
    height: 1.35rem;
    place-items: center;
    border: 2px solid rgb(var(--color-surface-950));
    border-radius: 999px;
    background: rgb(245 158 11);
    color: rgb(30 20 8);
    font-size: 0.68rem;
    font-weight: 800;
  }

  .companion-bubble {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 0.2rem;
    align-self: center;
    margin-bottom: 1rem;
    padding: 0.75rem 0.85rem;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    border-radius: 1rem 1rem 0.35rem 1rem;
    background: rgb(var(--color-surface-950) / 0.94);
    box-shadow: 0 14px 34px rgb(0 0 0 / 0.38);
    backdrop-filter: blur(18px) saturate(1.15);
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .companion-bubble strong {
    color: rgb(var(--color-surface-100));
    font-size: 0.72rem;
  }

  .companion-bubble span {
    display: -webkit-box;
    overflow: hidden;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.72rem;
    line-height: 1.35;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
  }

  .companion-bubble--success {
    border-color: rgb(52 211 153 / 0.36);
  }

  .companion-bubble--error {
    border-color: rgb(248 113 113 / 0.42);
  }

  .companion-bubble--attention {
    border-color: rgb(251 191 36 / 0.44);
  }

  .companion-approval {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    border: 1px solid rgb(251 191 36 / 0.28);
    border-radius: 0.8rem;
    background: rgb(120 53 15 / 0.18);
    padding: 0.65rem;
  }

  .companion-approval > div:first-child {
    min-width: 0;
    flex: 1;
  }

  .companion-approval strong {
    color: rgb(254 243 199);
    font-size: 0.72rem;
  }

  .companion-approval p {
    display: -webkit-box;
    margin: 0.15rem 0 0;
    overflow: hidden;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.66rem;
    line-height: 1.35;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  .companion-approval-actions {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .companion-small-btn {
    min-width: 4.3rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    border-radius: 0.5rem;
    background: rgb(var(--color-surface-900) / 0.72);
    padding: 0.3rem 0.55rem;
    color: rgb(var(--color-surface-200));
    font-size: 0.65rem;
    cursor: pointer;
  }

  .companion-small-btn--primary {
    border-color: rgb(251 191 36 / 0.42);
    background: rgb(245 158 11 / 0.2);
    color: rgb(254 243 199);
  }

  .companion-small-btn:disabled {
    cursor: wait;
    opacity: 0.55;
  }

  @media (prefers-reduced-motion: reduce) {
    .companion-window,
    .companion-pet,
    .companion-bubble,
    .companion-floating {
      transition: none;
    }
  }
</style>
