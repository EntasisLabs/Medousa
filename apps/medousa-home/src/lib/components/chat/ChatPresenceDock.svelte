<script lang="ts">
  import { tick, type Snippet } from "svelte";

  interface Props {
    showEmpty: boolean;
    showInlineComposer: boolean;
    presenceAsk: string;
    showContinue: boolean;
    onContinue: () => void;
    composerCentered?: boolean;
    runBlurp?: () => Promise<void> | void;
    children?: Snippet;
  }

  let {
    showEmpty,
    showInlineComposer,
    presenceAsk,
    showContinue,
    onContinue,
    composerCentered = $bindable(false),
    runBlurp = $bindable(() => {}),
    children,
  }: Props = $props();

  let presenceDockMode = $state<"center" | "docking" | "docked">("docked");
  let presenceDockEl = $state<HTMLDivElement | undefined>(undefined);
  let presenceEmptyEl = $state<HTMLDivElement | undefined>(undefined);
  let presenceAskEl = $state<HTMLParagraphElement | undefined>(undefined);
  let presenceContinueEl = $state<HTMLButtonElement | undefined>(undefined);
  let presenceBlurpToken = 0;
  let presenceDockLocked = $state(false);
  let presenceCenterOffset = $state(0);
  let presenceCenterPlaced = $state(false);

  const centered = $derived(
    showEmpty &&
      showInlineComposer &&
      (presenceDockMode === "center" || presenceDockMode === "docking"),
  );

  $effect(() => {
    composerCentered = centered;
  });

  function clearPresenceDockInlineStyles() {
    const el = presenceDockEl;
    if (!el) return;
    el.getAnimations().forEach((animation) => animation.cancel());
    el.style.transition = "";
    el.style.transform = "";
    el.style.transformOrigin = "";
    el.style.willChange = "";
    el.style.backfaceVisibility = "";
  }

  $effect(() => {
    if (showEmpty && showInlineComposer && !presenceDockLocked && presenceDockMode === "docked") {
      presenceDockMode = "center";
    }
  });

  $effect(() => {
    if (showEmpty && showInlineComposer) return;
    presenceBlurpToken += 1;
    presenceDockLocked = false;
    presenceCenterOffset = 0;
    presenceCenterPlaced = false;
    presenceDockMode = "docked";
    clearPresenceDockInlineStyles();
  });

  async function placePresenceSeams() {
    presenceCenterPlaced = false;
    await tick();
    await tick();
    const dock = presenceDockEl;
    const parent = dock?.parentElement ?? presenceEmptyEl?.parentElement;
    if (!parent || presenceDockMode !== "center") return;

    const parentRect = parent.getBoundingClientRect();
    const seam2 = parentRect.top + (parentRect.height * 2) / 3;
    const empty = presenceEmptyEl;
    const ask = presenceAskEl;
    const cont = presenceContinueEl;
    if (empty && ask) {
      empty.style.left = "50%";
      empty.style.transform = "translateX(-50%)";
      const askHeight = ask.offsetHeight;
      const emptyTop = parentRect.height / 3 - askHeight / 2;
      empty.style.top = `${Math.max(0, emptyTop)}px`;
      if (cont) {
        const titleCenterY = parentRect.height / 3;
        const inputCenterY = (parentRect.height * 2) / 3;
        const midY = (titleCenterY + inputCenterY) / 2;
        const continueCenterY = midY - parentRect.height * 0.08;
        const contHeight = cont.offsetHeight || 18;
        const margin = continueCenterY - emptyTop - askHeight - contHeight / 2;
        cont.style.marginTop = `${Math.max(10, margin)}px`;
      }
    }
    if (dock) {
      dock.style.transition = "none";
      dock.style.transform = "translate3d(0, 0, 0)";
      void dock.offsetHeight;
      const dockRect = dock.getBoundingClientRect();
      const dockCenter = dockRect.top + dockRect.height / 2;
      const offset = seam2 - dockCenter;
      presenceCenterOffset = offset;
      dock.style.transform = `translate3d(0, ${offset}px, 0)`;
    }
    presenceCenterPlaced = true;
  }

  $effect(() => {
    if (presenceDockMode !== "center" || presenceDockLocked) return;
    void presenceDockEl;
    void presenceEmptyEl;
    void presenceAskEl;
    void presenceContinueEl;
    void placePresenceSeams();
  });

  $effect(() => {
    if (presenceDockMode !== "center" || presenceDockLocked) return;
    const parent = presenceDockEl?.parentElement ?? presenceEmptyEl?.parentElement;
    if (!parent || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => {
      void placePresenceSeams();
    });
    ro.observe(parent);
    if (presenceDockEl) ro.observe(presenceDockEl);
    return () => ro.disconnect();
  });

  function prefersReducedMotion(): boolean {
    return (
      typeof window !== "undefined" &&
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches === true
    );
  }

  async function runPresenceDockBlurp() {
    presenceDockLocked = true;
    const token = ++presenceBlurpToken;
    const el = presenceDockEl;
    const y = presenceCenterOffset;

    if (!el || prefersReducedMotion()) {
      presenceDockMode = "docked";
      presenceCenterOffset = 0;
      presenceCenterPlaced = false;
      clearPresenceDockInlineStyles();
      return;
    }

    el.getAnimations().forEach((animation) => animation.cancel());
    presenceDockMode = "docking";
    el.style.transition = "none";
    el.style.transformOrigin = "50% 50%";
    el.style.willChange = "transform";
    el.style.backfaceVisibility = "hidden";
    el.style.transform = `translate3d(0, ${y}px, 0) scale3d(1, 1, 1)`;

    const smootherstep = (t: number) => t * t * t * (t * (t * 6 - 15) + 10);
    const mix = (a: number, b: number, t: number) => a + (b - a) * t;
    const STEPS = 20;
    const NECK = 0.5;
    const FALL_START = 0.16;
    const NECK_AT = 0.58;
    const keyframes: Keyframe[] = [];
    for (let i = 0; i <= STEPS; i += 1) {
      const t = i / STEPS;
      const fallT =
        t <= FALL_START ? 0 : smootherstep((t - FALL_START) / (1 - FALL_START));
      const yPos = y * (1 - fallT);
      let scaleX: number;
      if (t <= NECK_AT) {
        scaleX = mix(1, NECK, smootherstep(t / NECK_AT));
      } else {
        scaleX = mix(NECK, 1, smootherstep((t - NECK_AT) / (1 - NECK_AT)));
      }
      const pinch = 1 - scaleX;
      const scaleY = 1 + pinch * 0.35;
      keyframes.push({
        transform: `translate3d(0, ${yPos}px, 0) scale3d(${scaleX}, ${scaleY}, 1)`,
        offset: t,
      });
    }

    const drop = el.animate(keyframes, {
      duration: 1080,
      easing: "linear",
      fill: "forwards",
    });
    try {
      await drop.finished;
    } catch {
      /* aborted */
    }
    if (token !== presenceBlurpToken) return;
    el.style.transform = "translate3d(0, 0, 0) scale3d(1, 1, 1)";
    drop.cancel();
    await tick();
    if (token !== presenceBlurpToken) return;
    el.style.transition = "";
    el.style.transform = "";
    el.style.transformOrigin = "";
    el.style.willChange = "";
    el.style.backfaceVisibility = "";
    presenceCenterOffset = 0;
    presenceDockMode = "docked";
  }

  $effect(() => {
    runBlurp = runPresenceDockBlurp;
  });
</script>

{#if showEmpty && (presenceDockMode === "center" || presenceDockMode === "docking")}
  <div
    bind:this={presenceEmptyEl}
    class="chat-presence-empty {presenceDockMode === 'docking'
      ? 'chat-presence-empty--exiting'
      : ''} {presenceCenterPlaced || presenceDockMode === 'docking'
      ? 'chat-presence-empty--placed'
      : ''}"
  >
    <p bind:this={presenceAskEl} class="chat-presence-ask">{presenceAsk}</p>
    {#if showContinue}
      <button
        bind:this={presenceContinueEl}
        type="button"
        class="chat-presence-continue"
        onclick={onContinue}
      >
        Continue where we left off
      </button>
    {/if}
  </div>
{/if}

{#if showInlineComposer}
  <div
    bind:this={presenceDockEl}
    class="chat-presence-dock chat-presence-dock--{presenceDockMode}"
    class:chat-presence-dock--placed={presenceCenterPlaced ||
      presenceDockMode === "docking" ||
      presenceDockMode === "docked"}
  >
    {@render children?.()}
  </div>
{/if}

<style>
  .chat-presence-dock--center,
  .chat-presence-dock--docking {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 6;
    display: flex;
    width: 100%;
    flex-direction: column;
    align-items: stretch;
    padding: 0;
  }

  .chat-presence-dock--center:not(.chat-presence-dock--placed) {
    visibility: hidden;
  }

  .chat-presence-dock--docked {
    position: relative;
    z-index: 10;
    display: flex;
    width: 100%;
    flex-shrink: 0;
    flex-direction: column;
    align-items: stretch;
    padding: 0;
  }

  .chat-presence-empty {
    position: absolute;
    left: 50%;
    top: 0;
    z-index: 5;
    display: flex;
    width: max-content;
    max-width: min(28rem, calc(100% - 2rem));
    flex-direction: column;
    align-items: center;
    pointer-events: none;
  }

  .chat-presence-empty:not(.chat-presence-empty--placed):not(.chat-presence-empty--exiting) {
    visibility: hidden;
  }

  .chat-presence-ask {
    margin: 0;
    font-size: 1.125rem;
    font-weight: 600;
    letter-spacing: -0.02em;
    white-space: nowrap;
    color: rgb(var(--color-surface-50));
  }

  .chat-presence-continue {
    border: 0;
    background: transparent;
    font-size: 0.8125rem;
    color: rgb(var(--theme-text-tertiary));
    text-decoration: underline;
    text-decoration-color: rgb(var(--color-surface-500) / 0.5);
    text-underline-offset: 0.18em;
    cursor: pointer;
    pointer-events: auto;
    transition:
      color 150ms ease,
      text-decoration-color 150ms ease;
  }

  .chat-presence-continue:hover {
    color: rgb(var(--color-surface-200));
    text-decoration-color: rgb(var(--color-surface-400) / 0.7);
  }

  .chat-presence-empty--exiting {
    opacity: 0;
    transition: opacity 420ms cubic-bezier(0.22, 1, 0.36, 1);
    pointer-events: none;
  }
</style>
