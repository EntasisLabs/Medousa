<script lang="ts">
  import { Keyboard, LoaderCircle, MousePointer2, RefreshCw } from "@lucide/svelte";
  import { pointInCssViewport } from "$lib/browser/worldPresentation";
  import { governedBrowser } from "$lib/stores/governedBrowser.svelte";

  interface Props {
    mobile?: boolean;
    visible?: boolean;
  }

  type PointerStart = {
    pointerId: number;
    clientX: number;
    clientY: number;
  };

  let { mobile = false, visible = true }: Props = $props();
  let viewportEl = $state<HTMLElement | null>(null);
  let keyboardInput = $state<HTMLTextAreaElement | null>(null);
  let pointerStart = $state<PointerStart | null>(null);
  let wheelDeltaX = 0;
  let wheelDeltaY = 0;
  let wheelPoint: { x: number; y: number } | null = null;
  let wheelTimer: ReturnType<typeof setTimeout> | null = null;

  const world = $derived(governedBrowser.selectedWorld);
  const presentationWorldId = $derived(
    visible && world?.run_state === "running" ? world.world_id : null,
  );
  const canInteract = $derived(
    Boolean(
      world?.run_state === "running" &&
        governedBrowser.observation &&
        governedBrowser.frameDataUrl,
    ),
  );

  $effect(() => {
    const worldId = presentationWorldId;
    if (!worldId) return;
    return governedBrowser.startPresentation(worldId, () => {
      const cssWidth = viewportEl?.clientWidth ?? 960;
      const scale = typeof devicePixelRatio === "number" ? devicePixelRatio : 1;
      return cssWidth * scale;
    });
  });

  $effect(() => {
    return () => {
      if (wheelTimer) clearTimeout(wheelTimer);
    };
  });

  function mappedPoint(clientX: number, clientY: number) {
    const observation = governedBrowser.observation;
    const bounds = viewportEl?.getBoundingClientRect();
    if (!observation || !bounds) return null;
    return pointInCssViewport(clientX, clientY, bounds, observation.viewport);
  }

  function viewportScale() {
    const observation = governedBrowser.observation;
    const bounds = viewportEl?.getBoundingClientRect();
    if (!observation || !bounds || bounds.width <= 0 || bounds.height <= 0) {
      return { x: 1, y: 1 };
    }
    const imageRatio = observation.viewport.width / observation.viewport.height;
    const boundsRatio = bounds.width / bounds.height;
    const renderedWidth = boundsRatio > imageRatio ? bounds.height * imageRatio : bounds.width;
    const renderedHeight = boundsRatio > imageRatio ? bounds.height : bounds.width / imageRatio;
    return {
      x: observation.viewport.width / renderedWidth,
      y: observation.viewport.height / renderedHeight,
    };
  }

  function handlePointerDown(event: PointerEvent) {
    if (!canInteract || !viewportEl || event.button > 2) return;
    event.preventDefault();
    viewportEl.focus({ preventScroll: true });
    viewportEl.setPointerCapture(event.pointerId);
    pointerStart = {
      pointerId: event.pointerId,
      clientX: event.clientX,
      clientY: event.clientY,
    };
  }

  function handlePointerUp(event: PointerEvent) {
    const start = pointerStart;
    pointerStart = null;
    if (!start || start.pointerId !== event.pointerId || !canInteract) return;
    event.preventDefault();
    const startPoint = mappedPoint(start.clientX, start.clientY);
    const endPoint = mappedPoint(event.clientX, event.clientY);
    if (!startPoint || !endPoint) return;
    const distance = Math.hypot(event.clientX - start.clientX, event.clientY - start.clientY);
    if (distance <= 8) {
      const button = event.button === 1 ? "middle" : event.button === 2 ? "right" : "left";
      void governedBrowser.sendInput({ action: "click", ...endPoint, button });
      return;
    }
    void governedBrowser.sendInput({
      action: "scroll",
      x: startPoint.x,
      y: startPoint.y,
      delta_x: startPoint.x - endPoint.x,
      delta_y: startPoint.y - endPoint.y,
    });
  }

  function handleWheel(event: WheelEvent) {
    if (!canInteract) return;
    const point = mappedPoint(event.clientX, event.clientY);
    if (!point) return;
    event.preventDefault();
    const scale = viewportScale();
    wheelDeltaX += event.deltaX * scale.x;
    wheelDeltaY += event.deltaY * scale.y;
    wheelPoint = point;
    governedBrowser.markInteractive();
    if (wheelTimer) return;
    wheelTimer = setTimeout(() => {
      wheelTimer = null;
      const deltaX = Math.max(-10_000, Math.min(10_000, wheelDeltaX));
      const deltaY = Math.max(-10_000, Math.min(10_000, wheelDeltaY));
      const at = wheelPoint;
      wheelDeltaX = 0;
      wheelDeltaY = 0;
      wheelPoint = null;
      if (!at || (!deltaX && !deltaY)) return;
      void governedBrowser.sendInput({
        action: "scroll",
        x: at.x,
        y: at.y,
        delta_x: deltaX,
        delta_y: deltaY,
      });
    }, 48);
  }

  function keyModifiers(event: KeyboardEvent): string[] {
    const modifiers: string[] = [];
    if (event.altKey) modifiers.push("alt");
    if (event.ctrlKey) modifiers.push("control");
    if (event.metaKey) modifiers.push("meta");
    if (event.shiftKey) modifiers.push("shift");
    return modifiers;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!canInteract || event.isComposing) return;
    if (event.key.length === 1 && !event.altKey && !event.ctrlKey && !event.metaKey) {
      event.preventDefault();
      void governedBrowser.sendInput({ action: "text", text: event.key });
      return;
    }
    const supported = new Set([
      "Backspace",
      "Tab",
      "Enter",
      "Escape",
      "PageUp",
      "PageDown",
      "End",
      "Home",
      "ArrowLeft",
      "ArrowUp",
      "ArrowRight",
      "ArrowDown",
      "Delete",
    ]);
    if (!supported.has(event.key) && !event.altKey && !event.ctrlKey && !event.metaKey) return;
    event.preventDefault();
    void governedBrowser.sendInput({
      action: "key",
      key: event.key,
      modifiers: keyModifiers(event),
    });
  }

  function handleMobileInput(event: Event) {
    if (!canInteract) return;
    const inputEvent = event as InputEvent;
    if (inputEvent.data) {
      void governedBrowser.sendInput({ action: "text", text: inputEvent.data });
    }
    if (keyboardInput) keyboardInput.value = "";
  }

  function handleMobileKeydown(event: KeyboardEvent) {
    if (!["Backspace", "Enter", "Tab", "Escape"].includes(event.key)) return;
    event.preventDefault();
    void governedBrowser.sendInput({ action: "key", key: event.key });
  }

  function handlePaste(event: ClipboardEvent) {
    const text = event.clipboardData?.getData("text/plain");
    if (!text || !canInteract) return;
    event.preventDefault();
    void governedBrowser.sendInput({ action: "text", text });
  }
</script>

<div class="isolated-browser-stage">
  {#if governedBrowser.frameDataUrl}
  <button
    bind:this={viewportEl}
    type="button"
    class:isolated-browser-viewport-mobile={mobile}
    class="isolated-browser-viewport isolated-browser-viewport-ready"
    aria-label="Interactive workshop browser viewport"
    onpointerdown={handlePointerDown}
    onpointerup={handlePointerUp}
    onpointercancel={() => (pointerStart = null)}
    onwheel={handleWheel}
    onkeydown={handleKeydown}
    onpaste={handlePaste}
    oncontextmenu={(event) => canInteract && event.preventDefault()}
  >
    <img
      class="isolated-browser-frame"
      src={governedBrowser.frameDataUrl}
      alt={governedBrowser.activeTitle}
      draggable="false"
    />
    {#if governedBrowser.frameLoading}
      <span class="isolated-browser-frame-status" aria-label="Refreshing browser view">
        <LoaderCircle class="animate-spin" size={13} />
      </span>
    {/if}
  </button>
  {:else}
  <div
    bind:this={viewportEl}
    class:isolated-browser-viewport-mobile={mobile}
    class="isolated-browser-viewport"
  >
    {#if world?.run_state === "failed"}
      <div class="isolated-browser-empty">
        <RefreshCw size={22} />
        <strong>Workshop browser stopped</strong>
        <span>{world.failure || "The browser process exited."}</span>
        <button type="button" class="btn btn-sm variant-soft-primary" onclick={() => void governedBrowser.selectWorld(world.world_id)}>
          Start again
        </button>
      </div>
    {:else if world && world.run_state !== "running"}
      <div class="isolated-browser-empty">
        <RefreshCw size={22} />
        <strong>Workshop browser is {world.run_state}</strong>
        <button type="button" class="btn btn-sm variant-soft-primary" onclick={() => void governedBrowser.selectWorld(world.world_id)}>
          Resume
        </button>
      </div>
    {:else}
      <div class="isolated-browser-empty" role="status">
        <LoaderCircle class="animate-spin" size={22} />
        <strong>Connecting to the workshop browser…</strong>
        <span>The browser keeps running on the workshop if this view closes.</span>
      </div>
    {/if}

  </div>
  {/if}

  {#if mobile}
    <textarea
      bind:this={keyboardInput}
      class="isolated-browser-keyboard-capture"
      aria-label="Type into workshop browser"
      autocapitalize="sentences"
      autocomplete="off"
      onkeydown={handleMobileKeydown}
      oninput={handleMobileInput}
      onpaste={handlePaste}
    ></textarea>
    <button
      type="button"
      class="isolated-browser-keyboard-button"
      aria-label="Open keyboard for browser page"
      disabled={!canInteract}
      onclick={() => keyboardInput?.focus()}
    >
      <Keyboard size={17} />
      Type
    </button>
  {:else if canInteract}
    <div class="isolated-browser-input-hint">
      <MousePointer2 size={13} />
      Click, scroll, and type here
    </div>
  {/if}
</div>

<style>
  .isolated-browser-stage {
    position: relative;
    display: flex;
    min-height: 0;
    min-width: 0;
    flex: 1 1 auto;
    overflow: hidden;
    background: rgb(14 14 18);
  }

  .isolated-browser-viewport {
    position: relative;
    display: flex;
    min-height: 0;
    min-width: 0;
    flex: 1 1 auto;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    border: 0;
    padding: 0;
    background: transparent;
    outline: none;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
  }

  .isolated-browser-viewport-ready {
    cursor: default;
  }

  .isolated-browser-viewport:focus-visible {
    box-shadow: inset 0 0 0 2px rgb(var(--theme-focus) / 0.55);
  }

  .isolated-browser-frame {
    pointer-events: none;
    display: block;
    height: 100%;
    width: 100%;
    object-fit: contain;
    object-position: center;
  }

  .isolated-browser-empty {
    display: flex;
    max-width: 24rem;
    flex-direction: column;
    align-items: center;
    gap: 0.65rem;
    padding: 2rem;
    text-align: center;
    color: rgb(var(--theme-text-secondary));
  }

  .isolated-browser-empty strong {
    color: rgb(var(--theme-text));
    font-size: 0.9rem;
  }

  .isolated-browser-empty span {
    font-size: 0.75rem;
    line-height: 1.45;
  }

  .isolated-browser-frame-status,
  .isolated-browser-input-hint,
  .isolated-browser-keyboard-button {
    position: absolute;
    z-index: 2;
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.3);
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.82);
    color: rgb(var(--theme-text-secondary));
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.2);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
  }

  .isolated-browser-frame-status {
    top: 0.65rem;
    right: 0.65rem;
    border-radius: 999px;
    padding: 0.35rem;
  }

  .isolated-browser-input-hint {
    right: 0.75rem;
    bottom: 0.75rem;
    border-radius: 999px;
    padding: 0.35rem 0.6rem;
    font-size: 0.68rem;
    opacity: 0;
    transition: opacity 120ms ease;
    pointer-events: none;
  }

  .isolated-browser-stage:hover .isolated-browser-input-hint,
  .isolated-browser-viewport:focus-within + .isolated-browser-input-hint {
    opacity: 1;
  }

  .isolated-browser-keyboard-capture {
    position: absolute;
    left: 50%;
    bottom: 0;
    height: 1px;
    width: 1px;
    border: 0;
    padding: 0;
    opacity: 0.01;
    font-size: 16px;
  }

  .isolated-browser-keyboard-button {
    right: 0.65rem;
    bottom: 0.65rem;
    min-height: 2.25rem;
    border-radius: 999px;
    padding: 0 0.75rem;
    font-size: 0.75rem;
    font-weight: 600;
  }

  .isolated-browser-keyboard-button:disabled {
    opacity: 0.45;
  }

  .isolated-browser-viewport-mobile {
    padding-bottom: 0;
  }
</style>
