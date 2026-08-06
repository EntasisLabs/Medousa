<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { Terminal as XTerm, type IDisposable } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import "@xterm/xterm/css/xterm.css";
  import {
    terminalAttach,
    terminalCreate,
    terminalDetach,
    terminalInfo,
    terminalInterrupt,
    terminalReady,
    terminalResize,
    terminalSessions,
    terminalWrite,
    type TerminalOutput,
    type TerminalProtocolError,
    type TerminalResizeAck,
    type TerminalSessionSummary,
    type TerminalStatus,
  } from "$lib/terminal";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import UndertakingContextChip from "$lib/components/work/UndertakingContextChip.svelte";
  import { settingsNav } from "$lib/stores/settingsNav.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { getUndertaking, heartbeatLease } from "$lib/forge";

  interface Props {
    /** Workshop shell session id. Empty string = create a fresh session on mount. */
    sessionId: string;
    workId?: string | null;
    title?: string;
    /** Hide project chip / sessions chrome when docked under Code. */
    compact?: boolean;
  }

  let { sessionId, workId = null, title = "Terminal", compact = false }: Props = $props();

  let attachId = $state<number | null>(null);
  let boundSessionId = $state("");
  let error = $state<string | null>(null);
  let connecting = $state(true);
  let sessions = $state<TerminalSessionSummary[]>([]);
  let sessionHostAvailable = $state(true);
  let hostMessage = $state("");
  let terminalHost = $state<HTMLDivElement | null>(null);
  let connected = $state(false);
  let terminalCols = $state(0);
  let terminalRows = $state(0);
  let ptyCols = $state(0);
  let ptyRows = $state(0);

  let terminal: XTerm | null = null;
  let fitAddon: FitAddon | null = null;
  let outputUnlisten: UnlistenFn | null = null;
  let statusUnlisten: UnlistenFn | null = null;
  let resizeUnlisten: UnlistenFn | null = null;
  let errorUnlisten: UnlistenFn | null = null;
  let heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  let geometryTimer: ReturnType<typeof setTimeout> | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let resizeFrame: number | null = null;
  let connectGeneration = 0;
  let requestedCols = 0;
  let requestedRows = 0;
  let inputQueue = Promise.resolve();
  const terminalDisposables: IDisposable[] = [];

  function bytesToBase64(bytes: Uint8Array): string {
    let binary = "";
    const chunkSize = 0x8000;
    for (let offset = 0; offset < bytes.length; offset += chunkSize) {
      binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
    }
    return btoa(binary);
  }

  function base64ToBytes(data: string): Uint8Array {
    const binary = atob(data);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return bytes;
  }

  function textToBase64(data: string): string {
    return bytesToBase64(new TextEncoder().encode(data));
  }

  function binaryStringToBase64(data: string): string {
    const bytes = new Uint8Array(data.length);
    for (let index = 0; index < data.length; index += 1) {
      bytes[index] = data.charCodeAt(index) & 0xff;
    }
    return bytesToBase64(bytes);
  }

  function queueInput(data: string, binary = false) {
    const currentAttachId = attachId;
    if (currentAttachId == null) return;
    const encoded = binary ? binaryStringToBase64(data) : textToBase64(data);
    inputQueue = inputQueue
      .then(() => terminalWrite(currentAttachId, encoded))
      .catch((reason) => {
        if (attachId === currentAttachId) {
          error = reason instanceof Error ? reason.message : String(reason);
        }
      });
  }

  function focusTerminal() {
    terminal?.focus();
  }

  async function openExternalLink(uri: string) {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(uri);
    } catch {
      // Link activation should never make the terminal itself unusable.
    }
  }

  function initializeTerminal() {
    if (!terminalHost || terminal) return;
    terminal = new XTerm({
      allowTransparency: false,
      convertEol: false,
      cursorBlink: true,
      cursorStyle: "block",
      fontFamily:
        '"SFMono-Regular", "SF Mono", "Cascadia Code", "Roboto Mono", Menlo, monospace',
      fontSize: compact ? 11 : 12,
      letterSpacing: 0,
      lineHeight: 1.2,
      minimumContrastRatio: 4.5,
      rightClickSelectsWord: true,
      scrollback: 10_000,
      smoothScrollDuration: 80,
      theme: {
        background: "#0c0a09",
        foreground: "#e7e5e4",
        cursor: "#c4b5fd",
        cursorAccent: "#0c0a09",
        selectionBackground: "#6d28d966",
        black: "#1c1917",
        red: "#f87171",
        green: "#86efac",
        yellow: "#fde047",
        blue: "#93c5fd",
        magenta: "#c4b5fd",
        cyan: "#67e8f9",
        white: "#e7e5e4",
        brightBlack: "#78716c",
        brightRed: "#fca5a5",
        brightGreen: "#bbf7d0",
        brightYellow: "#fef08a",
        brightBlue: "#bfdbfe",
        brightMagenta: "#ddd6fe",
        brightCyan: "#a5f3fc",
        brightWhite: "#fafaf9",
      },
    });
    fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(new WebLinksAddon((_event, uri) => void openExternalLink(uri)));
    terminal.open(terminalHost);

    terminalDisposables.push(
      terminal.onData((data) => queueInput(data)),
      terminal.onBinary((data) => queueInput(data, true)),
      terminal.onResize(({ cols, rows }) => {
        terminalCols = cols;
        terminalRows = rows;
        void sendResize(cols, rows);
      }),
    );
    terminal.attachCustomKeyEventHandler((event) => {
      const copy =
        event.type === "keydown" &&
        (event.metaKey || event.ctrlKey) &&
        event.key.toLowerCase() === "c" &&
        terminal?.hasSelection();
      if (!copy || !terminal) return true;
      void navigator.clipboard.writeText(terminal.getSelection());
      return false;
    });
  }

  function scheduleFit() {
    if (resizeFrame != null) return;
    resizeFrame = requestAnimationFrame(() => {
      resizeFrame = null;
      fitTerminalNow();
    });
  }

  function fitTerminalNow() {
    if (!terminalHost || terminalHost.clientWidth === 0 || terminalHost.clientHeight === 0) {
      return;
    }
    try {
      fitAddon?.fit();
      if (terminal) {
        terminalCols = terminal.cols;
        terminalRows = terminal.rows;
        void sendResize(terminal.cols, terminal.rows);
      }
    } catch {
      // A pane can become hidden between ResizeObserver and this frame.
    }
  }

  function expectGeometry(cols: number, rows: number) {
    requestedCols = cols;
    requestedRows = rows;
    if (geometryTimer) clearTimeout(geometryTimer);
    geometryTimer = setTimeout(() => {
      geometryTimer = null;
      if (
        attachId != null &&
        (ptyCols !== requestedCols || ptyRows !== requestedRows)
      ) {
        error = `Terminal viewport is ${requestedCols}×${requestedRows}, but the PTY did not acknowledge that geometry.`;
      }
    }, 1_500);
  }

  async function sendResize(cols: number, rows: number) {
    const currentAttachId = attachId;
    if (currentAttachId == null || cols < 2 || rows < 1) return;
    if (cols === requestedCols && rows === requestedRows) return;
    expectGeometry(cols, rows);
    try {
      await terminalResize(currentAttachId, cols, rows);
    } catch (reason) {
      if (attachId === currentAttachId) {
        requestedCols = 0;
        requestedRows = 0;
        error = `Could not resize the terminal PTY: ${reason instanceof Error ? reason.message : String(reason)}`;
      }
    }
  }

  function openPackages() {
    settingsNav.openSection("packages");
    shellTabs.openDestination("settings");
    layout.openShellSidebarView("settings");
  }

  async function refreshSessionList() {
    try {
      sessions = await terminalSessions();
    } catch {
      sessions = [];
    }
  }

  async function checkHost() {
    try {
      const info = await terminalInfo();
      sessionHostAvailable = info.available;
      hostMessage = info.message ?? "";
    } catch (reason) {
      sessionHostAvailable = false;
      hostMessage = reason instanceof Error ? reason.message : String(reason);
    }
  }

  async function detachCurrent() {
    const currentAttachId = attachId;
    attachId = null;
    connected = false;
    ptyCols = 0;
    ptyRows = 0;
    requestedCols = 0;
    requestedRows = 0;
    if (geometryTimer) {
      clearTimeout(geometryTimer);
      geometryTimer = null;
    }
    if (currentAttachId != null) {
      try {
        await terminalDetach(currentAttachId);
      } catch {
        // Detach is idempotent from the pane's perspective.
      }
    }
  }

  async function connect() {
    const generation = ++connectGeneration;
    connecting = true;
    error = null;
    await detachCurrent();
    terminal?.reset();
    fitTerminalNow();
    await checkHost();
    if (generation !== connectGeneration) return;
    if (!sessionHostAvailable) {
      connecting = false;
      error = hostMessage || "Workshop session host unavailable";
      return;
    }

    try {
      let sid = (boundSessionId || sessionId).trim();
      if (!sid) {
        const leaseId = undertakings.active?.leaseId ?? null;
        const created = (await terminalCreate({
          work_id: workId ?? undertakings.active?.workId ?? null,
          cwd: null,
          lease_id: leaseId,
          cols: terminalCols || 80,
          rows: terminalRows || 24,
        })) as { session_id?: string };
        sid = created.session_id?.trim() ?? "";
        if (!sid) throw new Error("workshop did not return a session id");
        boundSessionId = sid;
        if (undertakings.active) undertakings.bindTerminal(sid);
      }

      const attach = await terminalAttach(
        sid,
        terminalCols || 80,
        terminalRows || 24,
      );
      if (generation !== connectGeneration) {
        await terminalDetach(attach.attach_id);
        return;
      }
      attachId = attach.attach_id;
      boundSessionId = attach.session_id;
      expectGeometry(terminalCols || 80, terminalRows || 24);
      await terminalReady(attach.attach_id);
      scheduleFit();
      await refreshSessionList();
      focusTerminal();
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
      if (/session|sidecar|not found|unavailable|404|connection/i.test(error)) {
        sessionHostAvailable = false;
      }
    } finally {
      if (generation === connectGeneration) connecting = false;
    }
  }

  async function switchSession(nextSessionId: string) {
    if (nextSessionId === boundSessionId) {
      focusTerminal();
      return;
    }
    boundSessionId = nextSessionId;
    await connect();
  }

  async function interrupt() {
    const sid = (boundSessionId || sessionId).trim();
    if (!sid) return;
    try {
      await terminalInterrupt(sid);
      focusTerminal();
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
    }
  }

  async function restoreUndertakingContext() {
    if (!workId) return;
    try {
      const item = await getUndertaking(workId);
      undertakings.setActiveFromItem(item);
      const sid = (boundSessionId || sessionId).trim();
      if (sid) undertakings.bindTerminal(sid);
    } catch {
      // A terminal remains usable even if its older undertaking no longer exists.
    }
  }

  async function sendHeartbeat() {
    const active = undertakings.active;
    if (!active?.leaseId || active.leaseGeneration == null) return;
    if (workId && active.workId !== workId) return;
    try {
      await heartbeatLease(active.leaseId, active.leaseGeneration);
    } catch {
      // Fresh projection polling will surface an expired or interrupted lease.
    }
  }

  async function openDiagnosticSession() {
    try {
      const created = await terminalCreate({
        work_id: null,
        cwd: null,
        lease_id: null,
        cols: terminalCols || 80,
        rows: terminalRows || 24,
      });
      const sid = created.session_id?.trim() ?? "";
      if (sid) shellTabs.openTerminal(sid, { activate: true, title: "Shell" });
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
    }
  }

  onMount(() => {
    let disposed = false;
    initializeTerminal();
    void (async () => {
      outputUnlisten = await listen<TerminalOutput>("terminal-output", (event) => {
        if (event.payload.attach_id !== attachId) return;
        try {
          terminal?.write(base64ToBytes(event.payload.data));
        } catch {
          error = "The terminal received malformed output from the session host.";
        }
      });
      statusUnlisten = await listen<TerminalStatus>("terminal-status", (event) => {
        if (event.payload.attach_id !== attachId) return;
        connected = event.payload.connected;
        if (!event.payload.connected && event.payload.message) {
          error = event.payload.message;
        }
      });
      resizeUnlisten = await listen<TerminalResizeAck>("terminal-resize", (event) => {
        if (event.payload.attach_id !== attachId) return;
        ptyCols = event.payload.cols;
        ptyRows = event.payload.rows;
        if (ptyCols === requestedCols && ptyRows === requestedRows) {
          if (geometryTimer) clearTimeout(geometryTimer);
          geometryTimer = null;
          if (error?.startsWith("Terminal viewport is ")) error = null;
        }
      });
      errorUnlisten = await listen<TerminalProtocolError>("terminal-error", (event) => {
        if (event.payload.attach_id !== attachId) return;
        error = `${event.payload.code}: ${event.payload.message}`;
      });
      if (disposed) {
        outputUnlisten?.();
        statusUnlisten?.();
        resizeUnlisten?.();
        errorUnlisten?.();
        outputUnlisten = null;
        statusUnlisten = null;
        resizeUnlisten = null;
        errorUnlisten = null;
        return;
      }
      await restoreUndertakingContext();
      await connect();
    })();

    if (!compact) heartbeatTimer = setInterval(() => void sendHeartbeat(), 30_000);
    if (terminalHost && typeof ResizeObserver !== "undefined") {
      resizeObserver = new ResizeObserver(scheduleFit);
      resizeObserver.observe(terminalHost);
    }
    if (!compact) void sendHeartbeat();

    return () => {
      disposed = true;
    };
  });

  onDestroy(() => {
    connectGeneration += 1;
    outputUnlisten?.();
    statusUnlisten?.();
    resizeUnlisten?.();
    errorUnlisten?.();
    if (heartbeatTimer) clearInterval(heartbeatTimer);
    if (geometryTimer) clearTimeout(geometryTimer);
    resizeObserver?.disconnect();
    if (resizeFrame != null) cancelAnimationFrame(resizeFrame);
    for (const disposable of terminalDisposables) disposable.dispose();
    terminalDisposables.length = 0;
    terminal?.dispose();
    terminal = null;
    fitAddon = null;
    void detachCurrent();
  });
</script>

<div
  class="terminal-pane flex h-full min-h-0 flex-col bg-[#0c0a09] text-[#e7e5e4]"
  role="application"
  aria-label={title}
>
  <div
    class="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-white/10 px-3 py-1.5"
  >
    <div class="min-w-0 flex flex-wrap items-center gap-2 text-xs text-white">
      {#if !compact}
        <span class="truncate">{title}</span>
        <UndertakingContextChip />
      {:else}
        <span class="truncate text-[10px] text-white">Shared with agents in this project</span>
      {/if}
    </div>
    <div class="flex items-center gap-1">
      {#if !compact}
        <details class="relative">
          <summary class="cursor-pointer list-none rounded px-2 py-0.5 text-[10px] text-white hover:bg-white/10 hover:text-white [&::-webkit-details-marker]:hidden">Sessions</summary>
          <div class="absolute right-0 top-full z-30 mt-1 w-56 rounded border border-white/15 bg-[#171312] p-1 shadow-xl">
            {#each sessions as session (session.session_id)}
              <button
                type="button"
                class="block w-full truncate rounded px-2 py-1 text-left text-[10px] text-white hover:bg-white/10 hover:text-white"
                title={session.cwd}
                onclick={() => void switchSession(session.session_id)}
              >
                {session.work_id === workId ? "Project shell" : session.work_id ? "Another project" : "Shell"} · {session.cwd.split(/[\\/]/).filter(Boolean).pop() ?? session.cwd}
              </button>
            {/each}
            {#if sessions.length === 0}
              <p class="px-2 py-1 text-[10px] text-white">No other sessions</p>
            {/if}
            <p class="mt-1 border-t border-white/10 px-2 pt-1 font-mono text-[8px] text-white">Current {boundSessionId.slice(0, 8)}</p>
          </div>
        </details>
        <button
          type="button"
          class="rounded px-2 py-0.5 text-[11px] text-white hover:bg-white/10 hover:text-white"
          onclick={() => void openDiagnosticSession()}
          title="Open a shell outside the current project"
        >
          New shell
        </button>
      {/if}
      <button
        type="button"
        class="rounded px-2 py-0.5 text-[11px] text-white hover:bg-white/10 hover:text-white"
        onclick={interrupt}
        title="Stop the running command"
      >
        Stop
      </button>
    </div>
  </div>

  <div class="relative min-h-0 flex-1 overflow-hidden">
    <div
      bind:this={terminalHost}
      class="terminal-host h-full w-full"
      role="presentation"
      onclick={focusTerminal}
    ></div>

    {#if connecting}
      <div class="pointer-events-none absolute inset-0 flex items-center justify-center bg-[#0c0a09]/80 text-xs text-white">
        Connecting to workshop session…
      </div>
    {:else if !sessionHostAvailable}
      <div class="absolute inset-x-3 top-3 z-10 rounded border border-amber-500/40 bg-amber-950/95 p-3 text-xs text-amber-100 shadow-xl">
        <p class="font-medium">Session host unavailable</p>
        <p class="mt-1 text-amber-100/80">{error || hostMessage}</p>
        <button
          type="button"
          class="mt-2 rounded bg-amber-500/30 px-2 py-1 text-[11px] hover:bg-amber-500/40"
          onclick={openPackages}
        >
          Open Settings → Packages
        </button>
      </div>
    {:else if error}
      <button
        type="button"
        class="absolute inset-x-3 top-3 z-10 rounded border border-red-500/40 bg-red-950/95 p-2 text-left text-xs text-red-100 shadow-xl"
        title="Click to reconnect"
        onclick={() => void connect()}
      >
        {error} <span class="ml-1 text-red-200/60">Click to reconnect.</span>
      </button>
    {/if}
  </div>

  <div
    class="flex shrink-0 items-center gap-2 border-t border-white/10 px-3 py-1 text-[10px] text-white"
  >
    <span class={connected ? "text-emerald-300/60" : "text-white"}>
      {connected ? "Connected" : connecting ? "Connecting" : "Disconnected"}
    </span>
    <span class="text-white">Shared with agents working in this project</span>
    <details class="ml-auto">
      <summary class="cursor-pointer">Technical details</summary>
      <span
        class="ml-2 font-mono"
        class:text-amber-300={connected && (ptyCols !== terminalCols || ptyRows !== terminalRows)}
      >
        view {terminalCols}×{terminalRows} · PTY {ptyCols || "—"}×{ptyRows || "—"} · {boundSessionId.slice(0, 8)}
      </span>
    </details>
  </div>
</div>

<style>
  .terminal-host {
    padding: 8px 10px;
  }

  .terminal-host :global(.xterm) {
    height: 100%;
  }

  .terminal-host :global(.xterm-viewport) {
    overscroll-behavior: contain;
  }
</style>
