<script lang="ts">
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import {
    controlComputerDriver,
    watchComputerDriver,
    type ComputerDriverReadiness,
    type ComputerWatchFrame,
    type ComputerWorldControlState,
  } from "$lib/daemon";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";
  import { LoaderCircle, Monitor, RefreshCw, X } from "@lucide/svelte";

  interface Props {
    open: boolean;
    readiness: ComputerDriverReadiness | null;
    executionRuntimeId?: string | null;
    workshopLabel?: string;
    onClose: () => void;
    onControlChange?: (control: ComputerWorldControlState) => void;
  }

  let {
    open,
    readiness,
    executionRuntimeId = null,
    workshopLabel = "Workshop",
    onClose,
    onControlChange,
  }: Props = $props();
  let sheetEl = $state<HTMLElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);
  let frame = $state<ComputerWatchFrame | null>(null);
  let frameUrl = $state<string | null>(null);
  let control = $state<ComputerWorldControlState | null>(null);
  let loading = $state(false);
  let controlBusy = $state(false);
  let error = $state<string | null>(null);
  let refreshGeneration = $state(0);

  const driverId = $derived(readiness?.driver.driver_id ?? "");
  const sessionId = $derived(readiness?.preflight?.session_id ?? "");
  const driverName = $derived(readiness?.driver.display_name || "Native computer");
  const canTakeControl = $derived(
    readiness?.driver.capabilities.includes("human_takeover") ?? false,
  );

  function close() {
    onClose();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!open || event.key !== "Escape") return;
    event.preventDefault();
    close();
  }

  function holderLabel(state: ComputerWorldControlState | null): string {
    if (!state || state.holder === "available") return "Ready";
    if (state.requester_has_control) return "You have control";
    if (state.holder === "human") return "Another person has control";
    return "Medousa is operating";
  }

  function holderHint(state: ComputerWorldControlState | null): string {
    if (state?.requester_has_control) {
      return "Medousa is fenced from new actions. Use the target computer directly, then hand it back.";
    }
    if (state?.holder === "human") {
      return "A different signed-in profile currently owns this desktop lease.";
    }
    return "Taking control interrupts queued agent work at the next governed action boundary.";
  }

  async function changeControl() {
    if (!driverId || !sessionId || !canTakeControl || controlBusy) return;
    controlBusy = true;
    error = null;
    try {
      const next = await controlComputerDriver(
        driverId,
        sessionId,
        control?.requester_has_control ? "return_to_medousa" : "take_control",
        executionRuntimeId,
      );
      control = next;
      if (frame) frame = { ...frame, control: next };
      onControlChange?.(next);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      controlBusy = false;
    }
  }

  $effect(() => {
    if (!open || !driverId || !sessionId) return;
    const generation = refreshGeneration;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let objectUrl: string | null = null;
    let hasFrame = false;
    frame = null;
    frameUrl = null;
    control = readiness?.control ?? null;
    error = null;
    loading = true;

    const schedule = (delay: number) => {
      if (!disposed) timer = setTimeout(load, delay);
    };
    const load = async () => {
      if (disposed) return;
      if (document.hidden) {
        schedule(1_500);
        return;
      }
      if (!hasFrame) loading = true;
      try {
        const next = await watchComputerDriver(
          driverId,
          sessionId,
          960,
          executionRuntimeId,
        );
        if (disposed || generation !== refreshGeneration) return;
        const binary = atob(next.capture.image_base64);
        const bytes = new Uint8Array(binary.length);
        for (let index = 0; index < binary.length; index += 1) {
          bytes[index] = binary.charCodeAt(index);
        }
        const nextUrl = URL.createObjectURL(new Blob([bytes], { type: next.capture.mime }));
        const previousUrl = objectUrl;
        objectUrl = nextUrl;
        frameUrl = nextUrl;
        frame = {
          ...next,
          capture: { ...next.capture, image_base64: "" },
        };
        if (previousUrl) queueMicrotask(() => URL.revokeObjectURL(previousUrl));
        control = next.control;
        error = null;
        hasFrame = true;
      } catch (cause) {
        if (disposed || generation !== refreshGeneration) return;
        const message = cause instanceof Error ? cause.message : String(cause);
        const transient =
          message.includes("between observations") || message.includes("stale_observation");
        if (!hasFrame || !transient) error = message;
      } finally {
        if (!disposed && generation === refreshGeneration) {
          loading = false;
          schedule(1_200);
        }
      }
    };
    void load();

    return () => {
      disposed = true;
      if (timer) clearTimeout(timer);
      if (objectUrl) URL.revokeObjectURL(objectUrl);
      frameUrl = null;
      frame = null;
      loading = false;
    };
  });

  $effect(() => {
    if (!open) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previous;
    };
  });

  $effect(() => {
    if (!open) return;
    return registerMobileBackHandler(() => {
      close();
      return true;
    });
  });

  $effect(() => {
    if (!open || !sheetEl || !headerEl || !window.matchMedia("(max-width: 767px)").matches) {
      return;
    }
    return attachMobileSheetGestures(sheetEl, headerEl, { onDismiss: close });
  });
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <BodyPortal>
    <div
      class="computer-watch-backdrop"
      role="presentation"
      onclick={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <div
        bind:this={sheetEl}
        class="computer-watch-sheet"
        role="dialog"
        aria-modal="true"
        aria-label={`Watch ${driverName}`}
      >
        <header bind:this={headerEl} class="computer-watch-header">
          <div class="computer-watch-grabber" aria-hidden="true"></div>
          <div class="computer-watch-heading">
            <span class="computer-watch-icon" aria-hidden="true">
              <Monitor size={17} strokeWidth={1.9} />
            </span>
            <div class="min-w-0 flex-1">
              <h2>{driverName}</h2>
              <p>{workshopLabel} · live focused window · {readiness?.driver.ownership ?? "attached"} desktop</p>
            </div>
            <button type="button" class="computer-watch-close" aria-label="Close computer view" onclick={close}>
              <X size={18} strokeWidth={2} />
            </button>
          </div>
        </header>

        <div class="computer-watch-body">
          <div class="computer-watch-frame" aria-live="polite">
            {#if frame && frameUrl}
              <img
                src={frameUrl}
                alt={`Focused window on ${driverName}`}
                draggable="false"
              />
              <span class="computer-watch-frame-meta">
                {frame.capture.image_width} × {frame.capture.image_height}
                {#if frame.capture.sensitive_regions_redacted > 0}
                  · {frame.capture.sensitive_regions_redacted} secure region{frame.capture
                    .sensitive_regions_redacted === 1
                    ? ""
                    : "s"} hidden
                {/if}
              </span>
            {:else if loading}
              <span class="computer-watch-placeholder">
                <LoaderCircle size={21} strokeWidth={1.8} class="animate-spin" />
                Connecting to the focused window…
              </span>
            {:else}
              <span class="computer-watch-placeholder">
                <Monitor size={22} strokeWidth={1.6} />
                No frame available
              </span>
            {/if}
          </div>

          {#if error}
            <div class="computer-watch-error" role="status">
              <span>{error}</span>
              <button type="button" onclick={() => (refreshGeneration += 1)}>
                <RefreshCw size={13} strokeWidth={2} />
                Retry now
              </button>
            </div>
          {/if}
        </div>

        <footer class="computer-watch-control">
          <div class="min-w-0 flex-1">
            <strong>{holderLabel(control)}</strong>
            <p>{holderHint(control)}</p>
          </div>
          {#if canTakeControl && (control?.holder !== "human" || control?.requester_has_control)}
            <button
              type="button"
              class:computer-watch-return={control?.requester_has_control}
              disabled={controlBusy}
              onclick={() => void changeControl()}
            >
              {#if controlBusy}
                <LoaderCircle size={14} strokeWidth={2} class="animate-spin" />
              {/if}
              {control?.requester_has_control ? "Return to Medousa" : "Take control"}
            </button>
          {/if}
        </footer>
      </div>
    </div>
  </BodyPortal>
{/if}

<style>
  .computer-watch-backdrop {
    position: fixed;
    inset: 0;
    z-index: 150;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1rem;
    background: rgb(var(--color-surface-950) / 0.76);
    backdrop-filter: blur(8px);
  }

  .computer-watch-sheet {
    display: flex;
    width: min(62rem, 100%);
    max-height: min(88dvh, 52rem);
    flex-direction: column;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-600) / 0.38);
    border-radius: 1.25rem;
    background: rgb(var(--color-surface-900));
    color: rgb(var(--theme-text-primary));
    box-shadow: 0 26px 80px rgb(0 0 0 / 0.48);
  }

  .computer-watch-header,
  .computer-watch-control {
    flex: none;
    padding: 0.9rem 1rem;
  }

  .computer-watch-header {
    border-bottom: 1px solid rgb(var(--color-surface-600) / 0.28);
  }

  .computer-watch-grabber {
    display: none;
  }

  .computer-watch-heading,
  .computer-watch-control {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .computer-watch-icon,
  .computer-watch-close {
    display: inline-flex;
    width: 2.15rem;
    height: 2.15rem;
    flex: none;
    align-items: center;
    justify-content: center;
    border-radius: 0.7rem;
  }

  .computer-watch-icon {
    background: rgb(var(--color-primary-500) / 0.12);
    color: rgb(var(--color-primary-300));
  }

  .computer-watch-close {
    border: 0;
    background: transparent;
    color: rgb(var(--theme-text-secondary));
  }

  .computer-watch-close:hover {
    background: rgb(var(--color-surface-700) / 0.5);
    color: rgb(var(--theme-text-primary));
  }

  .computer-watch-heading h2,
  .computer-watch-heading p,
  .computer-watch-control p {
    margin: 0;
  }

  .computer-watch-heading h2 {
    overflow: hidden;
    font-size: 0.92rem;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .computer-watch-heading p {
    margin-top: 0.12rem;
    font-size: 0.7rem;
    color: rgb(var(--theme-text-quiet));
  }

  .computer-watch-body {
    min-height: 0;
    flex: 1;
    overflow: auto;
    padding: 1rem;
    overscroll-behavior: contain;
  }

  .computer-watch-frame {
    position: relative;
    display: grid;
    min-height: 18rem;
    place-items: center;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-600) / 0.3);
    border-radius: 0.9rem;
    background:
      linear-gradient(45deg, rgb(var(--color-surface-950) / 0.36) 25%, transparent 25%),
      linear-gradient(-45deg, rgb(var(--color-surface-950) / 0.36) 25%, transparent 25%),
      rgb(var(--color-surface-950) / 0.74);
    background-position: 0 0, 8px 8px;
    background-size: 16px 16px;
  }

  .computer-watch-frame img {
    display: block;
    width: 100%;
    height: auto;
    max-height: calc(88dvh - 12rem);
    object-fit: contain;
    pointer-events: none;
    user-select: none;
  }

  .computer-watch-frame-meta {
    position: absolute;
    right: 0.55rem;
    bottom: 0.55rem;
    border-radius: 999px;
    padding: 0.25rem 0.48rem;
    background: rgb(0 0 0 / 0.72);
    font-size: 0.61rem;
    color: rgb(255 255 255 / 0.72);
  }

  .computer-watch-placeholder {
    display: inline-flex;
    align-items: center;
    gap: 0.55rem;
    font-size: 0.78rem;
    color: rgb(var(--theme-text-quiet));
  }

  .computer-watch-error {
    display: flex;
    margin-top: 0.65rem;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    border-radius: 0.7rem;
    background: rgb(var(--color-error-500) / 0.1);
    padding: 0.55rem 0.65rem;
    font-size: 0.68rem;
    color: rgb(var(--color-error-300));
  }

  .computer-watch-error button {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 0.3rem;
    color: inherit;
  }

  .computer-watch-control {
    border-top: 1px solid rgb(var(--color-surface-600) / 0.28);
  }

  .computer-watch-control strong {
    display: block;
    font-size: 0.78rem;
    font-weight: 620;
  }

  .computer-watch-control p {
    margin-top: 0.14rem;
    font-size: 0.66rem;
    line-height: 1.4;
    color: rgb(var(--theme-text-quiet));
  }

  .computer-watch-control > button {
    display: inline-flex;
    min-height: 2.25rem;
    flex: none;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    border-radius: 0.65rem;
    background: rgb(var(--color-primary-500));
    padding: 0.45rem 0.75rem;
    font-size: 0.72rem;
    font-weight: 620;
    color: white;
  }

  .computer-watch-control > button.computer-watch-return {
    background: rgb(var(--color-surface-700) / 0.72);
    color: rgb(var(--theme-text-primary));
  }

  .computer-watch-control > button:disabled {
    opacity: 0.55;
  }

  @media (max-width: 767px) {
    .computer-watch-backdrop {
      align-items: flex-end;
      padding: 0;
    }

    .computer-watch-sheet {
      width: 100%;
      max-height: 91dvh;
      border-right: 0;
      border-bottom: 0;
      border-left: 0;
      border-radius: 1.3rem 1.3rem 0 0;
    }

    .computer-watch-header {
      padding-top: 0.45rem;
    }

    .computer-watch-grabber {
      display: block;
      width: 2.6rem;
      height: 0.27rem;
      margin: 0 auto 0.45rem;
      border-radius: 999px;
      background: rgb(var(--color-surface-400) / 0.45);
    }

    .computer-watch-body {
      padding: 0.75rem;
    }

    .computer-watch-frame {
      min-height: 12rem;
    }

    .computer-watch-frame img {
      max-height: calc(91dvh - 13rem);
    }

    .computer-watch-control {
      align-items: flex-start;
      padding-bottom: max(0.9rem, env(safe-area-inset-bottom));
    }
  }
</style>
