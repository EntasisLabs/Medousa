<script lang="ts">
  import type { BrowserCompositorState } from "$lib/utils/browserCompositor";

  interface Props {
    state?: BrowserCompositorState | null;
  }

  let { state = null }: Props = $props();

  const showDebug =
    import.meta.env.DEV &&
    typeof localStorage !== "undefined" &&
    localStorage.getItem("medousa-browser-compositor-debug") === "1";

  const nativeDelta = $derived.by(() => {
    if (!state?.lastBounds || !state.nativeReadback) return null;
    return {
      x: Math.round(state.nativeReadback.x - state.lastBounds.x),
      y: Math.round(state.nativeReadback.y - state.lastBounds.y),
      w: Math.round(state.nativeReadback.width - state.lastBounds.width),
      h: Math.round(state.nativeReadback.height - state.lastBounds.height),
    };
  });
</script>

{#if showDebug && state}
  <div
    class="pointer-events-none absolute inset-0 z-[100] border border-dashed border-emerald-400/60"
    aria-hidden="true"
    title="DOM embed slot — native WKWebView is positioned separately by Rust"
  >
    <div
      class="absolute left-2 top-2 max-w-[min(100%-1rem,28rem)] rounded bg-black/75 px-2 py-1 font-mono text-[10px] leading-relaxed text-emerald-300"
    >
      DOM slot {state.visible ? "visible" : "hidden"}
      {#if state.hideReasons.length > 0}
        — {state.hideReasons.join(", ")}
      {/if}
      {#if state.lastBounds}
        — dom {Math.round(state.lastBounds.x)},{Math.round(state.lastBounds.y)}
        {Math.round(state.lastBounds.width)}×{Math.round(state.lastBounds.height)}
      {/if}
      {#if state.nativeReadback}
        — native {Math.round(state.nativeReadback.x)},{Math.round(state.nativeReadback.y)}
        {Math.round(state.nativeReadback.width)}×{Math.round(state.nativeReadback.height)}
        {#if nativeDelta}
          — Δ {nativeDelta.x},{nativeDelta.y} {nativeDelta.w}×{nativeDelta.h}
        {/if}
        {#if state.nativeReadback.shellOriginX !== 0 || state.nativeReadback.shellOriginY !== 0}
          — shell {Math.round(state.nativeReadback.shellOriginX)},{Math.round(
            state.nativeReadback.shellOriginY,
          )}
        {/if}
      {:else if state.readbackError}
        — readback err: {state.readbackError}
      {:else}
        — readback pending…
      {/if}
    </div>
  </div>
{/if}
