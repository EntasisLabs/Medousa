<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    ChevronDown,
    ClipboardPaste,
    Globe,
    House,
    LayoutGrid,
    ListTodo,
    MessageSquare,
    MessageSquarePlus,
    Send,
    StickyNote,
    X,
  } from "@lucide/svelte";
  import MedousaCompanion from "$lib/components/brand/MedousaCompanion.svelte";
  import {
    applyCompanionStreamEvent,
    companionSpriteState,
    initialCompanionActivity,
    type CompanionFeedback,
  } from "$lib/companion/companionState";
  import { sendCompanionPrompt } from "$lib/companion/companionTurn";
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
  import { runtime } from "$lib/stores/runtime.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import { listenForMedousaMark } from "$lib/settings/companionAppearanceSync";
  import { medousaMarkOption } from "$lib/theme/medousaMarks";
  import { environmentIcon } from "$lib/utils/environmentIcons";
  import { readLastViewPopoutSurface } from "$lib/utils/viewPopout";
  import { homeChannelSurface } from "$lib/platform";
  import type { TurnStreamEnvelopeV3 } from "$lib/types/generated/daemon_api";
  import { isAskJobId } from "$lib/types/askJob";
  import { buildAskJobRequest } from "$lib/utils/askPrompt";
  import {
    hideDesktopToolbar,
    isTauri,
    showBrowser,
    showChatPopout,
    showMainWindow,
    showVaultSticky,
    showViewPopout,
  } from "$lib/window";
  import { connectWorkshop } from "$lib/workshopConnection";
  import { whenDocumentVisible } from "$lib/utils/whenDocumentVisible";

  type WindowMode = "pet" | "bubble" | "toolbelt";
  type ComposerMode = "chat" | "ask";

  const WINDOW_SIZES: Record<WindowMode, { width: number; height: number }> = {
    pet: { width: 112, height: 170 },
    bubble: { width: 350, height: 194 },
    toolbelt: { width: 390, height: 580 },
  };

  let expanded = $state(false);
  let renderMode = $state<WindowMode>("pet");
  let geometrySettling = $state(false);
  let viewsOpen = $state(false);
  let sending = $state(false);
  let approvalBusy = $state(false);
  let composerMode = $state<ComposerMode>("chat");
  let chatPrompt = $state("");
  let askPrompt = $state("");
  let health = $state<DaemonHealth | null>(null);
  let pendingApprovals = $state<TurnBudgetRequestRecord[]>([]);
  let activity = $state(initialCompanionActivity());
  let feedbackTimer: ReturnType<typeof setTimeout> | null = null;
  let resizeToken = 0;
  let renderedModeValue: WindowMode = "pet";
  let suppressPetClick = false;
  let petRestorePosition: { x: number; y: number } | null = null;

  const connected = $derived(health?.ok === true);
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
    expanded ? "toolbelt" : companionFeedback ? "bubble" : "pet",
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
  const sessionLabel = $derived(
    chat.sessions.find((session) => session.session_id === chat.sessionId)
      ?.display_name?.trim() || chat.currentSessionLabel() || "Current conversation",
  );
  const customViews = $derived(
    environment.navSurfaces().filter((surface) => surface.kind === "custom"),
  );
  const companionMark = $derived(medousaMarkOption(settings.medousaMark));
  const activePrompt = $derived(composerMode === "chat" ? chatPrompt : askPrompt);
  const activeAskCard = $derived.by(() => {
    const selected = workspace.cards.find(
      (card) => card.id === workspace.selectedCardId && isAskJobId(card.id),
    );
    return selected ?? workspace.railCards().find((card) => isAskJobId(card.id)) ?? null;
  });
  const toolbeltSubtitle = $derived(
    composerMode === "chat"
      ? connected
        ? sessionLabel
        : health?.message || "Connecting…"
      : activeAskCard
        ? `${activeAskCard.status_label} · ${activeAskCard.title}`
        : connected
          ? "Delegate background work"
          : health?.message || "Connecting…",
  );
  const latestAssistantReply = $derived.by(() => {
    for (let index = chat.messages.length - 1; index >= 0; index -= 1) {
      const message = chat.messages[index];
      if (message?.role === "assistant" && message.content.trim()) {
        return message.content.trim();
      }
    }
    return null;
  });

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
          detachInteractive = detach;
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
    if (transitioning) {
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
      if (nextMode === "toolbelt" && !petRestorePosition) {
        petRestorePosition = anchoredCompanionPosition({
          position,
          previousSize,
          targetSize: petSize,
          workArea,
        });
      }
      const restoreFromPet = nextMode !== "toolbelt" && petRestorePosition;
      const nextPosition = restoreFromPet
        ? anchoredCompanionPosition({
            position: restoreFromPet,
            previousSize: petSize,
            targetSize: { width: physicalWidth, height: physicalHeight },
            workArea,
          })
        : anchoredCompanionPosition({
            position,
            previousSize,
            targetSize: { width: physicalWidth, height: physicalHeight },
            workArea,
          });
      await current.setSize(new LogicalSize(target.width, target.height));
      if (token !== resizeToken) return;
      const actualSize = await current.outerSize();
      const clampedPosition = anchoredCompanionPosition({
        position: nextPosition,
        previousSize: actualSize,
        targetSize: actualSize,
        workArea,
      });
      await current.setPosition(
        new PhysicalPosition(clampedPosition.x, clampedPosition.y),
      );
      if (nextMode === "pet") petRestorePosition = null;
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
    if (!expanded) viewsOpen = false;
  }

  function beginPetGesture(event: PointerEvent) {
    if (!isTauri() || event.button !== 0) return;
    const startX = event.clientX;
    const startY = event.clientY;

    const cleanup = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", cleanup);
      window.removeEventListener("pointercancel", cleanup);
    };
    const move = (next: PointerEvent) => {
      if (Math.hypot(next.clientX - startX, next.clientY - startY) < 5) return;
      suppressPetClick = true;
      cleanup();
      void import("@tauri-apps/api/window").then(({ getCurrentWindow }) =>
        getCurrentWindow().startDragging(),
      );
    };

    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", cleanup, { once: true });
    window.addEventListener("pointercancel", cleanup, { once: true });
  }

  function handlePetClick() {
    if (suppressPetClick) {
      suppressPetClick = false;
      return;
    }
    toggleExpanded();
  }

  async function submitPrompt(event: SubmitEvent) {
    event.preventDefault();
    const submitMode = composerMode;
    const message = activePrompt.trim();
    if (!message || sending || !connected) return;
    sending = true;
    setFeedback(null, false);
    try {
      if (submitMode === "ask") {
        await workspace.submitAsk({
          ...buildAskJobRequest(message, [], []),
          modelHint: runtime.model,
        });
        askPrompt = "";
        setFeedback({
          tone: "success",
          message: "Ask queued. Medousa will keep working in the background.",
        });
        return;
      }
      const ticket = await sendCompanionPrompt(message);
      chatPrompt = "";
      activity = {
        activeTurnIds: new Set([...activity.activeTurnIds, ticket.turn_id]),
        feedback: null,
      };
    } catch (error) {
      setFeedback({
        tone: "error",
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      sending = false;
    }
  }

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

  async function switchSession(event: Event) {
    const sessionId = (event.currentTarget as HTMLSelectElement).value;
    if (!sessionId || sessionId === chat.sessionId) return;
    await chat.switchSession(sessionId);
  }

  async function startNewConversation() {
    try {
      await chat.newSession();
      chatPrompt = "";
      setFeedback({ tone: "success", message: "New conversation ready." });
    } catch (error) {
      setFeedback({
        tone: "error",
        message: error instanceof Error ? error.message : String(error),
      });
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
      if (composerMode === "chat") chatPrompt = nextPrompt;
      else askPrompt = nextPrompt;
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

  function updateActivePrompt(event: Event) {
    const value = (event.currentTarget as HTMLTextAreaElement).value;
    if (composerMode === "chat") chatPrompt = value;
    else askPrompt = value;
  }

  async function openChat() {
    expanded = false;
    if (isTauri()) await showChatPopout();
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

  async function openViews(event?: MouseEvent) {
    if (event?.shiftKey || event?.altKey) {
      viewsOpen = !viewsOpen;
      return;
    }
    const last = readLastViewPopoutSurface();
    if (last && customViews.some((view) => view.id === last)) {
      await showViewPopout(last);
      expanded = false;
      return;
    }
    if (customViews.length === 1) {
      await showViewPopout(customViews[0].id);
      expanded = false;
      return;
    }
    viewsOpen = !viewsOpen;
  }

  async function pickView(surfaceId: string) {
    await showViewPopout(surfaceId);
    expanded = false;
    viewsOpen = false;
  }

  async function dismissCompanion() {
    expanded = false;
    viewsOpen = false;
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
  {#if renderMode === "toolbelt"}
    <section class="companion-toolbelt" aria-label="Medousa companion toolbelt">
      <header class="companion-header" data-tauri-drag-region>
        <div class="companion-header-mark" data-tauri-drag-region>
          <MedousaCompanion
            state={spriteState}
            markId={settings.medousaMark}
            darkMode={settings.darkMode}
            size="2.1rem"
            label={null}
          />
        </div>
        <div class="companion-heading" data-tauri-drag-region>
          <strong>Medousa</strong>
          <span>{toolbeltSubtitle}</span>
        </div>
        <button
          type="button"
          class="companion-icon-btn"
          title="Collapse companion"
          aria-label="Collapse companion"
          onclick={toggleExpanded}
        >
          <ChevronDown size={17} strokeWidth={1.8} />
        </button>
        <button
          type="button"
          class="companion-icon-btn companion-icon-btn--quiet"
          title="Hide companion"
          aria-label="Hide companion"
          onclick={() => void dismissCompanion()}
        >
          <X size={16} strokeWidth={1.8} />
        </button>
      </header>

      {#if companionFeedback || activity.activeTurnIds.size > 0}
        <div
          class="companion-status companion-status--{companionFeedback?.tone ?? 'working'}"
          role="status"
          aria-live="polite"
        >
          <span class="companion-status-dot" aria-hidden="true"></span>
          <p>
            {companionFeedback?.message ??
              `Medousa is working on ${activity.activeTurnIds.size === 1 ? "a turn" : `${activity.activeTurnIds.size} turns`}…`}
          </p>
        </div>
      {/if}

      {#if pendingApproval}
        <section class="companion-approval" aria-label="Pending approval">
          <div>
            <strong>More tool rounds?</strong>
            <p>
              {pendingApproval.progress_summary || pendingApproval.reason}
            </p>
          </div>
          <div class="companion-approval-actions">
            <button
              type="button"
              class="companion-small-btn companion-small-btn--primary"
              disabled={approvalBusy}
              onclick={() => void resolveApproval(true)}
            >Approve</button>
            <button
              type="button"
              class="companion-small-btn"
              disabled={approvalBusy}
              onclick={() => void resolveApproval(false)}
            >Deny</button>
          </div>
        </section>
      {/if}

      <div class="companion-mode-switch" role="tablist" aria-label="Companion mode">
        <button
          type="button"
          role="tab"
          aria-selected={composerMode === "chat"}
          class:companion-mode-active={composerMode === "chat"}
          disabled={sending}
          onclick={() => (composerMode = "chat")}
        >
          <MessageSquare size={14} strokeWidth={1.8} />
          Chat
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={composerMode === "ask"}
          class:companion-mode-active={composerMode === "ask"}
          disabled={sending}
          onclick={() => (composerMode = "ask")}
        >
          <ListTodo size={14} strokeWidth={1.8} />
          Ask
        </button>
      </div>

      {#if composerMode === "chat" && latestAssistantReply && !companionFeedback && !pendingApproval}
        <section class="companion-recent" aria-label="Latest Medousa reply">
          <strong>Latest reply</strong>
          <p>{latestAssistantReply}</p>
        </section>
      {:else if composerMode === "ask" && !companionFeedback && !pendingApproval}
        <section class="companion-recent" aria-label="Background ask status">
          <strong>{activeAskCard ? "Current ask" : "Ask runs in Work"}</strong>
          <p>
            {activeAskCard
              ? `${activeAskCard.title} · ${activeAskCard.status_label}`
              : "Hand off something substantial, then switch back to Chat while Medousa keeps going."}
          </p>
        </section>
      {/if}

      <form class="companion-composer" onsubmit={submitPrompt}>
        <label for="companion-prompt">
          {composerMode === "chat" ? "Quick chat" : "Delegate to Medousa"}
        </label>
        <div class="companion-quick-actions" aria-label="Conversation actions">
          {#if composerMode === "chat"}
            <button type="button" onclick={() => void startNewConversation()}>
              <MessageSquarePlus size={14} strokeWidth={1.8} />
              New conversation
            </button>
          {/if}
          <button type="button" onclick={() => void useClipboard()}>
            <ClipboardPaste size={14} strokeWidth={1.8} />
            Use clipboard
          </button>
        </div>
        <textarea
          id="companion-prompt"
          value={activePrompt}
          rows="4"
          placeholder={connected
            ? composerMode === "chat"
              ? "Talk something through without leaving your flow…"
              : "What should Medousa go do in the background?"
            : "Waiting for the workshop…"}
          disabled={!connected || sending}
          oninput={updateActivePrompt}
          onkeydown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              event.currentTarget.form?.requestSubmit();
            }
          }}
        ></textarea>
        <div class="companion-composer-footer">
          {#if composerMode === "chat"}
            {#if chat.sessions.length > 0}
              <select
                aria-label="Conversation"
                value={chat.sessionId}
                onchange={(event) => void switchSession(event)}
              >
                {#if !chat.sessions.some((session) => session.session_id === chat.sessionId)}
                  <option value={chat.sessionId}>New conversation</option>
                {/if}
                {#each chat.sessions.slice(0, 12) as session (session.session_id)}
                  <option value={session.session_id}>
                    {session.display_name?.trim() || session.preview?.trim() || "Conversation"}
                  </option>
                {/each}
              </select>
            {:else}
              <span class="companion-session-label">{sessionLabel}</span>
            {/if}
          {:else}
            <span class="companion-session-label">Background · visible in Work</span>
          {/if}
          <button
            type="submit"
            class="companion-send-btn"
            disabled={!connected || sending || !activePrompt.trim()}
            aria-label={composerMode === "chat" ? "Send chat message" : "Queue background ask"}
          >
            <Send size={15} strokeWidth={1.9} />
            <span>{sending ? (composerMode === "chat" ? "Sending" : "Queuing") : (composerMode === "chat" ? "Send" : "Queue")}</span>
          </button>
        </div>
      </form>

      <nav class="companion-launchers" aria-label="Open Medousa tools">
        <button type="button" onclick={() => void openChat()}>
          <MessageSquare size={17} strokeWidth={1.75} />
          <span>Chat</span>
        </button>
        <button type="button" onclick={() => void openNote()}>
          <StickyNote size={17} strokeWidth={1.75} />
          <span>Note</span>
        </button>
        <button type="button" onclick={() => void openWeb()}>
          <Globe size={17} strokeWidth={1.75} />
          <span>Web</span>
        </button>
        <button
          type="button"
          class:companion-launcher-active={viewsOpen}
          title="Click last view; Shift-click the list"
          onclick={(event) => void openViews(event)}
        >
          <LayoutGrid size={17} strokeWidth={1.75} />
          <span>Views</span>
        </button>
        <button type="button" onclick={() => void openMain()}>
          <House size={17} strokeWidth={1.75} />
          <span>Main</span>
        </button>
      </nav>

      {#if viewsOpen}
        <div class="companion-views" role="listbox" aria-label="Custom views">
          {#if customViews.length === 0}
            <p>No custom views yet.</p>
          {:else}
            {#each customViews as surface (surface.id)}
              {@const SurfaceIcon = environmentIcon(surface.icon)}
              <button
                type="button"
                role="option"
                aria-selected={false}
                onclick={() => void pickView(surface.id)}
              >
                <SurfaceIcon size={14} strokeWidth={1.75} />
                <span>{surface.label}</span>
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </section>
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
      class:companion-pet--busy={activity.activeTurnIds.size > 0}
      onpointerdown={beginPetGesture}
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

  .companion-window--toolbelt {
    align-items: stretch;
    padding: 0.55rem;
  }

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
    background: radial-gradient(
      circle at 50% 45%,
      color-mix(in srgb, var(--companion-accent) 25%, var(--companion-stage) 15%),
      transparent 68%
    );
    filter: drop-shadow(0 13px 14px rgb(0 0 0 / 0.34));
    cursor: grab;
  }

  .companion-pet:active {
    cursor: grabbing;
  }

  .companion-pet:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--companion-accent) 90%, white 10%);
    outline-offset: -0.2rem;
  }

  .companion-pet--busy {
    filter: drop-shadow(0 13px 15px rgb(56 189 248 / 0.26));
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

  .companion-toolbelt {
    position: relative;
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
    gap: 0.65rem;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-500) / 0.34);
    border-radius: 1.3rem;
    background:
      radial-gradient(
        circle at 8% 0%,
        color-mix(in srgb, var(--companion-accent) 13%, transparent),
        transparent 34%
      ),
      rgb(var(--color-surface-950) / 0.95);
    box-shadow: 0 20px 52px rgb(0 0 0 / 0.44);
    padding: 0.75rem;
    backdrop-filter: blur(20px) saturate(1.12);
  }

  .companion-header {
    display: flex;
    min-height: 2.8rem;
    align-items: center;
    gap: 0.55rem;
  }

  .companion-header-mark {
    display: flex;
    width: 2.4rem;
    height: 2.7rem;
    align-items: center;
    justify-content: center;
    border: 1px solid color-mix(in srgb, var(--companion-accent) 18%, transparent);
    border-radius: 0.65rem;
    background: color-mix(in srgb, var(--companion-stage) 78%, transparent);
  }

  .companion-heading {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
  }

  .companion-heading strong {
    font-size: 0.83rem;
    letter-spacing: 0.01em;
  }

  .companion-heading span {
    overflow: hidden;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.68rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .companion-icon-btn {
    display: grid;
    width: 2rem;
    height: 2rem;
    flex: 0 0 auto;
    place-items: center;
    border: 0;
    border-radius: 0.65rem;
    background: transparent;
    color: rgb(var(--theme-text-secondary));
    cursor: pointer;
  }

  .companion-icon-btn:hover {
    background: rgb(var(--color-surface-800) / 0.85);
    color: rgb(var(--color-surface-50));
  }

  .companion-icon-btn--quiet {
    color: rgb(var(--theme-text-quiet));
  }

  .companion-status {
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
    border: 1px solid rgb(56 189 248 / 0.24);
    border-radius: 0.75rem;
    background: rgb(8 47 73 / 0.22);
    padding: 0.55rem 0.65rem;
  }

  .companion-status p {
    min-width: 0;
    margin: 0;
    color: rgb(var(--color-surface-200));
    font-size: 0.7rem;
    line-height: 1.4;
  }

  .companion-status-dot {
    width: 0.45rem;
    height: 0.45rem;
    flex: 0 0 auto;
    margin-top: 0.22rem;
    border-radius: 999px;
    background: rgb(56 189 248);
    box-shadow: 0 0 0 0.2rem rgb(56 189 248 / 0.12);
  }

  .companion-status--success {
    border-color: rgb(52 211 153 / 0.25);
    background: rgb(6 78 59 / 0.2);
  }

  .companion-status--success .companion-status-dot {
    background: rgb(52 211 153);
  }

  .companion-status--error {
    border-color: rgb(248 113 113 / 0.3);
    background: rgb(127 29 29 / 0.18);
  }

  .companion-status--error .companion-status-dot {
    background: rgb(248 113 113);
  }

  .companion-status--attention {
    border-color: rgb(251 191 36 / 0.32);
    background: rgb(120 53 15 / 0.2);
  }

  .companion-status--attention .companion-status-dot {
    background: rgb(251 191 36);
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

  .companion-mode-switch {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.25rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.25);
    border-radius: 0.72rem;
    background: rgb(var(--color-surface-900) / 0.58);
    padding: 0.22rem;
  }

  .companion-mode-switch button {
    display: inline-flex;
    height: 1.95rem;
    align-items: center;
    justify-content: center;
    gap: 0.38rem;
    border: 1px solid transparent;
    border-radius: 0.52rem;
    background: transparent;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.68rem;
    font-weight: 650;
    cursor: pointer;
  }

  .companion-mode-switch button:hover {
    color: rgb(var(--color-surface-100));
  }

  .companion-mode-switch button:disabled {
    cursor: wait;
    opacity: 0.65;
  }

  .companion-mode-switch .companion-mode-active {
    border-color: color-mix(in srgb, var(--companion-accent) 28%, transparent);
    background: color-mix(in srgb, var(--companion-accent) 13%, rgb(var(--color-surface-800)));
    color: rgb(var(--color-surface-50));
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.16);
  }

  .companion-recent {
    display: grid;
    gap: 0.2rem;
    max-height: 5.1rem;
    overflow: hidden;
    border-left: 2px solid color-mix(in srgb, var(--companion-accent) 58%, transparent);
    padding: 0.15rem 0.15rem 0.15rem 0.62rem;
  }

  .companion-recent strong {
    color: color-mix(in srgb, var(--companion-accent) 64%, rgb(var(--color-surface-100)));
    font-size: 0.65rem;
  }

  .companion-recent p {
    display: -webkit-box;
    margin: 0;
    overflow: hidden;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.69rem;
    line-height: 1.4;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
  }

  .companion-composer {
    display: flex;
    min-height: 0;
    flex: 1;
    flex-direction: column;
    gap: 0.4rem;
  }

  .companion-composer label {
    color: rgb(var(--theme-text-secondary));
    font-size: 0.7rem;
    font-weight: 650;
  }

  .companion-quick-actions {
    display: flex;
    gap: 0.38rem;
  }

  .companion-quick-actions button {
    display: inline-flex;
    align-items: center;
    gap: 0.32rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.28);
    border-radius: 0.55rem;
    background: rgb(var(--color-surface-900) / 0.54);
    padding: 0.34rem 0.5rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.64rem;
    cursor: pointer;
  }

  .companion-quick-actions button:hover {
    border-color: color-mix(in srgb, var(--companion-accent) 38%, transparent);
    color: rgb(var(--color-surface-100));
  }

  .companion-composer textarea {
    min-height: 5.2rem;
    flex: 1;
    resize: none;
    border: 1px solid rgb(var(--color-surface-500) / 0.34);
    border-radius: 0.85rem;
    outline: none;
    background: rgb(var(--color-surface-900) / 0.76);
    padding: 0.7rem 0.75rem;
    color: rgb(var(--color-surface-100));
    font: inherit;
    font-size: 0.78rem;
    line-height: 1.45;
  }

  .companion-composer textarea:focus {
    border-color: color-mix(in srgb, var(--companion-accent) 60%, transparent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--companion-accent) 10%, transparent);
  }

  .companion-composer textarea::placeholder {
    color: rgb(var(--theme-text-quiet));
  }

  .companion-composer-footer {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .companion-composer select {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    border: 0;
    outline: 0;
    background: transparent;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.66rem;
    text-overflow: ellipsis;
  }

  .companion-session-label {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.66rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .companion-send-btn {
    display: inline-flex;
    height: 2.05rem;
    align-items: center;
    gap: 0.38rem;
    border: 1px solid color-mix(in srgb, var(--companion-accent) 52%, transparent);
    border-radius: 0.65rem;
    background: linear-gradient(
      135deg,
      color-mix(in srgb, var(--companion-accent) 84%, #4c1d95),
      color-mix(in srgb, var(--companion-accent) 58%, #0369a1)
    );
    padding: 0 0.72rem;
    color: white;
    font-size: 0.7rem;
    font-weight: 650;
    cursor: pointer;
  }

  .companion-send-btn:disabled {
    cursor: default;
    filter: saturate(0.4);
    opacity: 0.48;
  }

  .companion-launchers {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 0.35rem;
  }

  .companion-launchers button {
    display: flex;
    min-width: 0;
    height: 3.15rem;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.25rem;
    border: 1px solid transparent;
    border-radius: 0.75rem;
    background: transparent;
    color: rgb(var(--theme-text-tertiary));
    cursor: pointer;
  }

  .companion-launchers button:hover,
  .companion-launchers .companion-launcher-active {
    border-color: rgb(var(--color-surface-500) / 0.25);
    background: rgb(var(--color-surface-800) / 0.66);
    color: rgb(var(--color-surface-100));
  }

  .companion-launchers span {
    font-size: 0.6rem;
  }

  .companion-views {
    position: absolute;
    right: 0.75rem;
    bottom: 4.45rem;
    left: 0.75rem;
    display: flex;
    max-height: 10rem;
    flex-direction: column;
    gap: 0.2rem;
    overflow-y: auto;
    border: 1px solid rgb(var(--color-surface-500) / 0.34);
    border-radius: 0.8rem;
    background: rgb(var(--color-surface-950) / 0.98);
    box-shadow: 0 14px 30px rgb(0 0 0 / 0.4);
    padding: 0.4rem;
  }

  .companion-views p {
    margin: 0;
    padding: 0.55rem;
    color: rgb(var(--theme-text-quiet));
    font-size: 0.68rem;
    text-align: center;
  }

  .companion-views button {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.45rem;
    border: 0;
    border-radius: 0.55rem;
    background: transparent;
    padding: 0.48rem 0.55rem;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.7rem;
    text-align: left;
    cursor: pointer;
  }

  .companion-views button:hover {
    background: rgb(var(--color-surface-800) / 0.78);
    color: rgb(var(--color-surface-50));
  }

  @media (prefers-reduced-motion: reduce) {
    .companion-window,
    .companion-pet,
    .companion-bubble,
    .companion-toolbelt {
      transition: none;
    }
  }
</style>
