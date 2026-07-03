<script lang="ts">
  import { fetchArtifact, fetchFeedTail } from "$lib/daemon";
  import {
    DEFAULT_INLINE_ARTIFACT_CAP_PX,
    measureIframeContentHeight,
    prepareArtifactHtml,
    type ArtifactEmbedMode,
  } from "$lib/utils/artifactPrepareHtml";
  import {
    isMedousaFeedTailRequest,
    isValidFeedId,
    type MedousaFeedTailResponse,
  } from "$lib/utils/medousaFeedClient";
  import { friendlyUserError } from "$lib/utils/normieErrors";

  interface Props {
    sessionId: string;
    artifactId: string;
    label: string;
    mime?: string;
    heightPx?: number | null;
    compact?: boolean;
    bare?: boolean;
    mode?: ArtifactEmbedMode;
    feedState?: Record<string, unknown> | null;
    subscribedFeedIds?: string[];
    onOpenFull?: () => void;
    contentHeight?: number;
    truncated?: boolean;
  }

  let {
    sessionId,
    artifactId,
    label,
    mime = "text/html",
    heightPx = 360,
    compact = false,
    bare = false,
    mode = "inline",
    feedState = null,
    subscribedFeedIds = [],
    onOpenFull,
    contentHeight = $bindable(0),
    truncated = $bindable(false),
  }: Props = $props();

  let html = $state<string | null>(null);
  let rawHtml = $state<string | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(true);
  let frameHeight = $state(DEFAULT_INLINE_ARTIFACT_CAP_PX);
  let frameEl = $state<HTMLIFrameElement | null>(null);
  let frameReady = $state(false);

  const inlineCapPx = $derived(heightPx ?? DEFAULT_INLINE_ARTIFACT_CAP_PX);
  const fillsParent = $derived(mode === "panel" || mode === "fullscreen");

  function isDarkTheme(): boolean {
    if (typeof document === "undefined") return true;
    return document.documentElement.classList.contains("dark");
  }

  function applyFrameMetrics(frame: HTMLIFrameElement) {
    const measured = measureIframeContentHeight(frame);
    contentHeight = measured;

    if (mode !== "inline") {
      truncated = false;
      return;
    }

    const cap = inlineCapPx;
    truncated = measured > cap + 4;
    frameHeight = Math.min(Math.max(measured, 120), cap);
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

  async function handleWindowMessage(event: MessageEvent) {
    if (event.source !== frameEl?.contentWindow) return;
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

  $effect(() => {
    const sid = sessionId;
    const aid = artifactId;
    const embedMode = mode;
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
        html = prepareArtifactHtml(response.body, embedMode, isDarkTheme(), feedState);
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
    const isDark = isDarkTheme();
    if (!body) return;
    html = prepareArtifactHtml(body, embedMode, isDark, feedState);
  });

  $effect(() => {
    const state = feedState;
    if (!frameReady || !state) return;
    postFeedPatchToFrame(state);
  });

  $effect(() => {
    if (typeof window === "undefined") return;
    window.addEventListener("message", handleWindowMessage);
    return () => window.removeEventListener("message", handleWindowMessage);
  });

  function handleFrameLoad(event: Event) {
    const frame = event.currentTarget as HTMLIFrameElement;
    frameReady = true;
    applyFrameMetrics(frame);
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
>
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

<style>
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
