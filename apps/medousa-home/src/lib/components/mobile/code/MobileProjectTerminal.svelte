<script lang="ts">
  import {
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    ClipboardPaste,
    KeyboardOff,
    Square,
  } from "@lucide/svelte";
  import TerminalPane from "$lib/components/terminal/TerminalPane.svelte";
  import MobileCodeSurfaceSwitcher from "$lib/components/mobile/code/MobileCodeSurfaceSwitcher.svelte";
  import { haptic } from "$lib/haptics";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { mobileCodeWorkspaceState } from "$lib/stores/mobileCodeWorkspaceState.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { setMobileComposerFocus } from "$lib/utils/mobileKeyboardViewport";
  import { openMobileCodeFile } from "$lib/utils/mobileCodeOpen";
  import { openTrackedTerminal } from "$lib/utils/undertakingWorkspace";
  import { terminalInterrupt } from "$lib/terminal";

  interface Props {
    workId: string;
  }

  let { workId }: Props = $props();

  let pane = $state<{ sendInput: (data: string) => void } | null>(null);
  let error = $state<string | null>(null);
  let creating = $state(false);

  const sessionId = $derived(
    mobileCodeWorkspaceState.presentation?.terminalSessionId ?? "",
  );
  const ctrlLatch = $derived(Boolean(mobileCodeWorkspaceState.presentation?.ctrlLatch));
  const worktreeRoot = $derived(undertakings.detail?.environment?.worktree ?? null);

  $effect(() => {
    void workId;
    if (mobileCodeWorkspaceState.presentation?.terminalSessionId) return;
    void ensureSession();
  });

  $effect(() => {
    return () => setMobileComposerFocus(false);
  });

  $effect(() => {
    if (!mobileCodeWorkspaceState.sessionSheetOpen) return;
    return registerMobileBackHandler(() => {
      mobileCodeWorkspaceState.sessionSheetOpen = false;
      return true;
    });
  });

  async function ensureSession() {
    const detail = undertakings.detail?.id === workId ? undertakings.detail : null;
    if (!detail || creating) return;
    creating = true;
    error = null;
    try {
      const id = await openTrackedTerminal(detail, { activate: false });
      if (id) mobileCodeWorkspaceState.setTerminalSessionId(id);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      creating = false;
    }
  }

  function send(data: string) {
    haptic("light");
    if (ctrlLatch && data.length === 1) {
      const code = data.toUpperCase().charCodeAt(0);
      if (code >= 64 && code < 96) {
        pane?.sendInput(String.fromCharCode(code - 64));
        mobileCodeWorkspaceState.setCtrlLatch(false);
        return;
      }
    }
    pane?.sendInput(data);
  }

  async function paste() {
    haptic("light");
    try {
      const text = await navigator.clipboard.readText();
      if (text) pane?.sendInput(text);
    } catch {
      // Clipboard permission is optional; the PTY stays usable.
    }
  }

  function dismissKeyboard() {
    haptic("light");
    setMobileComposerFocus(false);
    (document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null)?.blur();
  }

  async function interrupt() {
    haptic("warning");
    if (sessionId) {
      await terminalInterrupt(sessionId, undertakings.active?.executionRuntimeId ?? null);
    }
  }
</script>

<div class="flex h-full min-h-0 flex-col">
  {#if error}
    <p class="shrink-0 border-b border-amber-500/30 bg-amber-950/30 px-3 py-1.5 text-[12px] text-amber-100">
      {error}
    </p>
  {/if}
  {#if sessionId}
    <div
      class="min-h-0 flex-1 overflow-hidden"
      onfocusin={() => setMobileComposerFocus(true)}
    >
      <TerminalPane
        bind:this={pane}
        {sessionId}
        {workId}
        executionRuntimeId={undertakings.active?.executionRuntimeId ?? null}
        title="Terminal"
        compact={false}
        mobile={true}
        worktreeRoot={worktreeRoot}
        onOpenPath={(path, line) => {
          void openMobileCodeFile(workId, path, {
            line,
            origin: "terminal",
          });
        }}
      />
    </div>
  {:else}
    <div class="flex flex-1 items-center justify-center px-6 text-sm text-content-quiet">
      {creating ? "Starting a project shell…" : "No shell session yet."}
    </div>
  {/if}

  <div class="mobile-code-key-row" role="toolbar" aria-label="Terminal keys">
    <MobileCodeSurfaceSwitcher variant="icons" />
    <span class="mobile-code-key-split" aria-hidden="true"></span>
    <button type="button" class="mobile-code-key" onclick={() => send("\u001b")}>Esc</button>
    <button type="button" class="mobile-code-key" onclick={() => send("\t")}>Tab</button>
    <button
      type="button"
      class="mobile-code-key"
      class:mobile-code-key-active={ctrlLatch}
      aria-pressed={ctrlLatch}
      onclick={() => {
        haptic("medium");
        mobileCodeWorkspaceState.toggleCtrlLatch();
      }}
    >Ctrl</button>
    <button type="button" class="mobile-code-key" aria-label="Up" onclick={() => send("\u001b[A")}><ChevronUp size={16} /></button>
    <button type="button" class="mobile-code-key" aria-label="Down" onclick={() => send("\u001b[B")}><ChevronDown size={16} /></button>
    <button type="button" class="mobile-code-key" aria-label="Left" onclick={() => send("\u001b[D")}><ChevronLeft size={16} /></button>
    <button type="button" class="mobile-code-key" aria-label="Right" onclick={() => send("\u001b[C")}><ChevronRight size={16} /></button>
    <button type="button" class="mobile-code-key" onclick={() => send("\r")}>Enter</button>
    <button type="button" class="mobile-code-key" aria-label="Paste" onclick={() => void paste()}><ClipboardPaste size={15} /></button>
    <button type="button" class="mobile-code-key" aria-label="Hide keyboard" onclick={dismissKeyboard}><KeyboardOff size={15} /></button>
    <button type="button" class="mobile-code-key mobile-code-key-danger" aria-label="Interrupt" onclick={() => void interrupt()}><Square size={13} /></button>
  </div>
</div>

<style>
  .mobile-code-key-row {
    display: flex;
    flex-shrink: 0;
    gap: 0.25rem;
    overflow-x: auto;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.35);
    background: rgb(var(--color-surface-900) / 0.95);
    padding: 0.35rem 0.4rem calc(0.35rem + var(--mobile-keyboard-inset, 0px));
  }

  .mobile-code-key {
    display: inline-flex;
    min-width: 2.75rem;
    min-height: 2.75rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.5rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.35);
    background: rgb(var(--color-surface-800) / 0.7);
    color: rgb(var(--color-content-secondary));
    font-size: 11px;
    font-weight: 600;
  }

  .mobile-code-key-active {
    border-color: rgb(var(--color-content-link) / 0.55);
    color: rgb(var(--color-content-link));
  }

  .mobile-code-key-danger {
    color: rgb(248 113 113);
  }

  .mobile-code-key-split {
    width: 1px;
    align-self: stretch;
    flex-shrink: 0;
    margin: 0.35rem 0.15rem;
    background: rgb(var(--color-surface-500) / 0.4);
  }
</style>
