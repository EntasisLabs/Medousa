<script lang="ts">
  import {
    componentRuntimeAppendEvents,
    componentRuntimeCompleteProbe,
    componentStoreDelete,
    componentStoreGet,
    componentStoreListKeys,
    componentStoreSet,
    deleteArtifact,
    fetchArtifact,
    fetchFeedTail,
    writeArtifact,
    type ComponentRuntimeEventInput,
  } from "$lib/daemon";
  import {
    DEFAULT_INLINE_ARTIFACT_CAP_PX,
    prepareArtifactHtml,
    type ArtifactEmbedMode,
  } from "$lib/utils/artifactPrepareHtml";
  import {
    isMedousaFeedTailRequest,
    isValidFeedId,
    type MedousaFeedTailResponse,
  } from "$lib/utils/medousaFeedClient";
  import {
    isMedousaStoreRequest,
    isValidStoreKey,
    type MedousaStoreResponse,
  } from "$lib/utils/medousaStoreClient";
  import {
    isMedousaArtifactProbeResult,
    isMedousaArtifactRuntimeEvent,
  } from "$lib/utils/medousaArtifactRuntimeClient";
  import { environment } from "$lib/stores/environment.svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { resolveEnvironmentTheme } from "$lib/utils/environmentTheme";
  import { friendlyUserError } from "$lib/utils/normieErrors";
  import PresentationArtifactMenu from "$lib/components/environment/PresentationArtifactMenu.svelte";
  import ArtifactSourceModal from "$lib/components/environment/ArtifactSourceModal.svelte";
  import ArtifactEditModal from "$lib/components/environment/ArtifactEditModal.svelte";

  const RUNTIME_FLUSH_MS = 500;
  const RUNTIME_MAX_BATCH = 20;

  let runtimeEventBuffer: ComponentRuntimeEventInput[] = [];
  let runtimeFlushTimer: ReturnType<typeof setTimeout> | null = null;
  let lastDispatchedProbeId = $state<string | null>(null);

  interface Props {
    sessionId: string;
    artifactId: string;
    label: string;
    componentId?: string | null;
    profileId?: string | null;
    mime?: string;
    heightPx?: number | null;
    compact?: boolean;
    bare?: boolean;
    mode?: ArtifactEmbedMode;
    feedState?: Record<string, unknown> | null;
    subscribedFeedIds?: string[];
    onOpenFull?: () => void;
    onArtifactDeleted?: () => void;
    manageable?: boolean;
    contentHeight?: number;
    truncated?: boolean;
  }

  let {
    sessionId,
    artifactId,
    label,
    componentId = null,
    profileId = null,
    mime = "text/html",
    heightPx = 360,
    compact = false,
    bare = false,
    mode = "inline",
    feedState = null,
    subscribedFeedIds = [],
    onOpenFull,
    onArtifactDeleted,
    manageable = false,
    contentHeight = $bindable(0),
    truncated = $bindable(false),
  }: Props = $props();

  let activeArtifactId = $state(artifactId);
  let sourceOpen = $state(false);
  let editOpen = $state(false);
  let editBusy = $state(false);
  let editError = $state<string | null>(null);
  let actionBusy = $state(false);

  let html = $state<string | null>(null);
  let rawHtml = $state<string | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(true);
  let frameHeight = $state(DEFAULT_INLINE_ARTIFACT_CAP_PX);
  let frameEl = $state<HTMLIFrameElement | null>(null);
  let frameReady = $state(false);
  let reportedContentHeight = $state(0);

  const inlineCapPx = $derived(heightPx ?? DEFAULT_INLINE_ARTIFACT_CAP_PX);
  const fillsParent = $derived(mode === "panel" || mode === "fullscreen");
  const storeScopeId = $derived(componentId?.trim() || null);
  const storeProfileId = $derived(profileId?.trim() || undefined);
  const pendingRuntimeProbe = $derived(
    storeScopeId ? environment.pendingRuntimeProbes.get(storeScopeId) ?? null : null,
  );
  const hostTheme = $derived(
    resolveEnvironmentTheme(
      environment.spec,
      workshops.activeColorThemeId ?? settings.colorTheme,
      workshops.activeBrandColor,
      isDarkTheme(),
    ),
  );

  $effect(() => {
    activeArtifactId = artifactId;
  });

  async function handleSaveEdit(nextHtml: string) {
    editBusy = true;
    editError = null;
    try {
      const response = await writeArtifact({
        session_id: sessionId,
        artifact_id: activeArtifactId,
        title: label,
        html: nextHtml,
        presentation: mode,
      });
      if (componentId) {
        await environment.updatePresentationArtifactId(
          componentId,
          response.artifact_id,
          storeProfileId ?? environment.spec?.profileId,
        );
      }
      activeArtifactId = response.artifact_id;
      editOpen = false;
    } catch (err) {
      editError = friendlyUserError(err instanceof Error ? err.message : String(err));
    } finally {
      editBusy = false;
    }
  }

  async function handleDeleteWidget() {
    const confirmed = window.confirm(
      `Delete “${label}”? This removes the widget from your view and deletes the HTML file.`,
    );
    if (!confirmed) return;
    actionBusy = true;
    try {
      const response = await deleteArtifact({
        session_id: sessionId,
        artifact_id: activeArtifactId,
      });
      if (componentId) {
        await environment.removePresentationComponent(
          componentId,
          storeProfileId ?? environment.spec?.profileId,
        );
      } else {
        await environment.unlinkComponentsForArtifacts(
          response.deleted_artifact_ids,
          storeProfileId ?? environment.spec?.profileId,
        );
      }
      onArtifactDeleted?.();
    } catch (err) {
      error = friendlyUserError(err instanceof Error ? err.message : String(err));
    } finally {
      actionBusy = false;
    }
  }

  async function flushRuntimeEvents() {
    if (runtimeFlushTimer) {
      clearTimeout(runtimeFlushTimer);
      runtimeFlushTimer = null;
    }
    const scope = storeScopeId;
    if (!scope || runtimeEventBuffer.length === 0) return;
    const batch = runtimeEventBuffer.splice(0, RUNTIME_MAX_BATCH);
    try {
      await componentRuntimeAppendEvents(scope, batch, {
        profileId: storeProfileId,
        sessionId,
      });
    } catch {
      // best-effort telemetry
    }
    if (runtimeEventBuffer.length > 0) {
      runtimeFlushTimer = setTimeout(() => void flushRuntimeEvents(), RUNTIME_FLUSH_MS);
    }
  }

  function queueRuntimeEvent(
    level: string,
    message: string,
    stack?: string,
    source?: string,
  ) {
    if (!storeScopeId) return;
    runtimeEventBuffer.push({
      level,
      message,
      stack,
      source,
      sessionId,
    });
    if (runtimeEventBuffer.length >= RUNTIME_MAX_BATCH) {
      void flushRuntimeEvents();
      return;
    }
    if (!runtimeFlushTimer) {
      runtimeFlushTimer = setTimeout(() => void flushRuntimeEvents(), RUNTIME_FLUSH_MS);
    }
  }

  async function handleRuntimeProbeResult(event: MessageEvent) {
    if (!isMedousaArtifactProbeResult(event.data)) return;
    const scope = storeScopeId;
    if (!scope) return;
    const { probeId, storeReady, storeRoundTripOk, errors } = event.data;
    try {
      await componentRuntimeCompleteProbe(scope, probeId, {
        probeId,
        componentId: scope,
        storeReady,
        storeRoundTripOk,
        errors,
        profileId: storeProfileId,
      });
    } catch {
      // probe is best-effort
    } finally {
      environment.clearRuntimeProbe(scope);
      lastDispatchedProbeId = null;
    }
  }

  function isDarkTheme(): boolean {
    if (typeof document === "undefined") return true;
    return document.documentElement.classList.contains("dark");
  }

  function applyFrameMetrics(reportedHeight?: number) {
    const measured = reportedHeight ?? reportedContentHeight;
    if (measured <= 0) return;
    contentHeight = measured;

    if (mode !== "inline") {
      truncated = false;
      return;
    }

    const cap = inlineCapPx;
    truncated = measured > cap + 4;
    frameHeight = Math.min(Math.max(measured, 120), cap);
  }

  function isArtifactHeightMessage(data: unknown): data is { type: string; height: number } {
    if (!data || typeof data !== "object") return false;
    const msg = data as Record<string, unknown>;
    return msg.type === "medousa:artifact:height" && typeof msg.height === "number";
  }

  function postFeedPatchToFrame(state: Record<string, unknown> | null) {
    const target = frameEl?.contentWindow;
    if (!target || !state) return;
    target.postMessage({ type: "medousa:feed:patch", feedState: state }, "*");
  }

  function feedAllowed(feedId: string): boolean {
    if (!isValidFeedId(feedId)) return false;
    if (subscribedFeedIds.length === 0) return false;
    return subscribedFeedIds.includes(feedId);
  }

  function respondStore(payload: MedousaStoreResponse) {
    frameEl?.contentWindow?.postMessage(payload, "*");
  }

  async function handleFeedTailRequest(event: MessageEvent) {
    if (!isMedousaFeedTailRequest(event.data)) return;

    const { requestId, feedId } = event.data;
    const limit = typeof event.data.limit === "number" ? event.data.limit : 10;
    const respond = (payload: MedousaFeedTailResponse) => {
      frameEl?.contentWindow?.postMessage(payload, "*");
    };

    if (!feedAllowed(feedId)) {
      respond({
        type: "medousa:feed:tail:response",
        requestId,
        feedId,
        ok: false,
        error: "feed not subscribed for this component",
      });
      return;
    }

    try {
      const tail = await fetchFeedTail(feedId, limit);
      respond({
        type: "medousa:feed:tail:response",
        requestId,
        feedId,
        ok: true,
        events: tail.events,
      });
    } catch (err) {
      respond({
        type: "medousa:feed:tail:response",
        requestId,
        feedId,
        ok: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }

  async function handleStoreRequest(event: MessageEvent) {
    if (!isMedousaStoreRequest(event.data)) return;

    const scope = storeScopeId;
    const { requestId, type } = event.data;
    if (!scope) {
      respondStore({
        type: "medousa:store:response",
        requestId,
        ok: false,
        error: "MedousaStore unavailable for chat-only artifacts",
      });
      return;
    }

    try {
      if (type === "medousa:store:get") {
        const key =
          typeof event.data.key === "string" && event.data.key.trim()
            ? event.data.key.trim()
            : undefined;
        if (key && !isValidStoreKey(key)) {
          throw new Error(`invalid store key '${key}'`);
        }
        const response = await componentStoreGet(scope, {
          key,
          profileId: storeProfileId,
        });
        if (key) {
          respondStore({
            type: "medousa:store:response",
            requestId,
            ok: true,
            value: response.entries[key] ?? null,
          });
          return;
        }
        respondStore({
          type: "medousa:store:response",
          requestId,
          ok: true,
          entries: response.entries,
        });
        return;
      }

      if (type === "medousa:store:set") {
        const key = event.data.key?.trim();
        if (!key || !isValidStoreKey(key)) {
          throw new Error("invalid store key");
        }
        await componentStoreSet(scope, key, event.data.value, storeProfileId);
        respondStore({ type: "medousa:store:response", requestId, ok: true });
        return;
      }

      if (type === "medousa:store:delete") {
        const key = event.data.key?.trim();
        if (!key || !isValidStoreKey(key)) {
          throw new Error("invalid store key");
        }
        await componentStoreDelete(scope, key, storeProfileId);
        respondStore({ type: "medousa:store:response", requestId, ok: true });
        return;
      }

      if (type === "medousa:store:list") {
        const response = await componentStoreListKeys(scope, storeProfileId);
        respondStore({
          type: "medousa:store:response",
          requestId,
          ok: true,
          keys: response.keys,
        });
      }
    } catch (err) {
      respondStore({
        type: "medousa:store:response",
        requestId,
        ok: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }

  async function handleWindowMessage(event: MessageEvent) {
    if (event.source !== frameEl?.contentWindow) return;
    if (isArtifactHeightMessage(event.data)) {
      reportedContentHeight = Math.max(0, Math.round(event.data.height));
      applyFrameMetrics(reportedContentHeight);
      return;
    }
    if (isMedousaArtifactRuntimeEvent(event.data)) {
      queueRuntimeEvent(
        event.data.level,
        event.data.message,
        event.data.stack,
        event.data.source,
      );
      return;
    }
    if (isMedousaArtifactProbeResult(event.data)) {
      await handleRuntimeProbeResult(event);
      return;
    }
    await handleFeedTailRequest(event);
    await handleStoreRequest(event);
  }

  $effect(() => {
    const sid = sessionId;
    const aid = activeArtifactId;
    const embedMode = mode;
    const scope = storeScopeId;
    html = null;
    error = null;
    loading = true;
    frameReady = false;
    truncated = false;
    contentHeight = 0;
    frameHeight = embedMode === "inline" ? inlineCapPx : DEFAULT_INLINE_ARTIFACT_CAP_PX;

    void (async () => {
      try {
        const response = await fetchArtifact(sid, aid);
        if (!response.mime.includes("html") && !mime.includes("html")) {
          error = "This artifact is not HTML.";
          return;
        }
        html = prepareArtifactHtml(
          response.body,
          embedMode,
          hostTheme.tokens,
          feedState,
          scope,
        );
        rawHtml = response.body;
      } catch (err) {
        error = friendlyUserError(err instanceof Error ? err.message : String(err));
      } finally {
        loading = false;
      }
    })();
  });

  $effect(() => {
    const body = rawHtml;
    const embedMode = mode;
    const tokens = hostTheme.tokens;
    const scope = storeScopeId;
    if (!body) return;
    html = prepareArtifactHtml(body, embedMode, tokens, feedState, scope);
  });

  $effect(() => {
    const state = feedState;
    if (!frameReady || !state) return;
    postFeedPatchToFrame(state);
  });

  $effect(() => {
    if (typeof window === "undefined") return;
    window.addEventListener("message", handleWindowMessage);
    return () => {
      window.removeEventListener("message", handleWindowMessage);
      void flushRuntimeEvents();
    };
  });

  $effect(() => {
    const probe = pendingRuntimeProbe;
    const ready = frameReady;
    const frame = frameEl;
    if (!probe || !ready || !frame?.contentWindow) return;
    if (lastDispatchedProbeId === probe.probeId) return;
    lastDispatchedProbeId = probe.probeId;
    frame.contentWindow.postMessage(
      { type: "medousa:artifact:probe", probeId: probe.probeId },
      "*",
    );
  });

  function handleFrameLoad(event: Event) {
    const frame = event.currentTarget as HTMLIFrameElement;
    frameReady = true;
    applyFrameMetrics();
    postFeedPatchToFrame(feedState);
  }
</script>

<div
  class="presentation-frame"
  class:presentation-frame-compact={compact}
  class:presentation-frame-bare={bare}
  class:presentation-frame-panel={mode === "panel"}
  class:presentation-frame-fullscreen={mode === "fullscreen"}
  class:presentation-frame-fill={fillsParent}
  class:presentation-frame-manageable={manageable}
>
  {#if manageable && !loading && !error}
    <div class="presentation-frame-toolbar">
      <PresentationArtifactMenu
        disabled={actionBusy || editBusy}
        onViewSource={() => {
          sourceOpen = true;
        }}
        onEdit={() => {
          editError = null;
          editOpen = true;
        }}
        onDelete={() => void handleDeleteWidget()}
      />
    </div>
  {/if}
  {#if loading}
    <div
      class="presentation-frame-skeleton"
      class:presentation-frame-skeleton-fill={fillsParent}
      aria-hidden="true"
    >
      <div class="presentation-frame-skeleton-shimmer"></div>
    </div>
    <p class="presentation-frame-status sr-only">Loading {label}…</p>
  {:else if error}
    <p class="presentation-frame-error">{error}</p>
  {:else if html}
    <iframe
      bind:this={frameEl}
      class="presentation-frame-embed"
      class:presentation-frame-embed-fill={fillsParent}
      title={label}
      sandbox="allow-scripts"
      scrolling={mode === "inline" ? "no" : "yes"}
      srcdoc={html}
      style={fillsParent ? undefined : `height: ${frameHeight}px`}
      onload={handleFrameLoad}
    ></iframe>
    {#if mode === "inline" && truncated && onOpenFull}
      <div class="presentation-frame-truncation">
        <button type="button" class="presentation-frame-truncation-btn" onclick={onOpenFull}>
          Open for full view
        </button>
      </div>
    {/if}
  {/if}
</div>

<ArtifactSourceModal
  open={sourceOpen}
  title={label}
  source={rawHtml ?? ""}
  onClose={() => {
    sourceOpen = false;
  }}
/>

<ArtifactEditModal
  open={editOpen}
  title={label}
  source={rawHtml ?? ""}
  busy={editBusy}
  error={editError}
  onClose={() => {
    if (!editBusy) editOpen = false;
  }}
  onSave={handleSaveEdit}
/>

<style>
  .presentation-frame-manageable {
    position: relative;
  }

  .presentation-frame-toolbar {
    position: absolute;
    top: 0.5rem;
    right: 0.5rem;
    z-index: 4;
  }

  .presentation-frame {
    overflow: hidden;
    border-radius: 0.875rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 40%, transparent);
    background: rgb(var(--color-surface-900));
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.12);
  }

  :global(html:not(.dark)) .presentation-frame {
    background: rgb(var(--color-surface-50));
    border-color: rgb(var(--color-surface-300) / 0.85);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.06);
  }

  .presentation-frame-compact {
    border-radius: 0.75rem;
  }

  .presentation-frame-bare {
    border: 0;
    border-radius: 0;
    background: transparent;
    box-shadow: none;
  }

  .presentation-frame-fill {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    width: 100%;
    overflow: hidden;
  }

  .presentation-frame-embed {
    display: block;
    width: 100%;
    border: 0;
    background: transparent;
  }

  .presentation-frame-embed-fill {
    flex: 1 1 auto;
    min-height: 0;
    height: 100%;
  }

  .presentation-frame-skeleton {
    position: relative;
    min-height: 12rem;
    overflow: hidden;
    background: color-mix(in srgb, var(--color-surface-800) 55%, var(--color-surface-900));
  }

  .presentation-frame-skeleton-fill {
    flex: 1 1 auto;
    min-height: 0;
    height: 100%;
  }

  .presentation-frame-skeleton-shimmer {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      105deg,
      transparent 40%,
      color-mix(in srgb, var(--color-surface-600) 35%, transparent) 50%,
      transparent 60%
    );
    background-size: 200% 100%;
    animation: presentation-shimmer 1.4s ease-in-out infinite;
  }

  @keyframes presentation-shimmer {
    from {
      background-position: 200% 0;
    }
    to {
      background-position: -200% 0;
    }
  }

  .presentation-frame-truncation {
    display: flex;
    justify-content: center;
    padding: 0.5rem 0.75rem 0.625rem;
    border-top: 1px solid color-mix(in srgb, var(--color-surface-600) 35%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 92%, transparent);
  }

  :global(html:not(.dark)) .presentation-frame-truncation {
    background: rgb(var(--color-surface-100));
    border-top-color: rgb(var(--color-surface-300) / 0.85);
  }

  .presentation-frame-truncation-btn {
    border: 0;
    border-radius: 999px;
    padding: 0.35rem 0.75rem;
    font-size: 0.6875rem;
    font-weight: 600;
    color: rgb(var(--color-primary-200));
    background: color-mix(in srgb, var(--color-primary-500) 16%, var(--color-surface-900));
    cursor: pointer;
  }

  :global(html:not(.dark)) .presentation-frame-truncation-btn {
    color: rgb(var(--color-primary-700));
    background: color-mix(in srgb, var(--color-primary-500) 12%, var(--color-surface-50));
  }

  .presentation-frame-status,
  .presentation-frame-error {
    margin: 0;
    padding: 0.875rem 1rem;
    font-size: 0.75rem;
    line-height: 1.4;
  }

  .presentation-frame-error {
    color: rgb(var(--color-warning-300));
  }
</style>
