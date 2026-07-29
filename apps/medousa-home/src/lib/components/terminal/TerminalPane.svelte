<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    terminalAttach,
    terminalCreate,
    terminalDetach,
    terminalInfo,
    terminalInterrupt,
    terminalKey,
    terminalResize,
    terminalSessions,
    terminalSnapshot,
    type TerminalFrame,
    type TerminalSessionSummary,
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
  }

  let { sessionId, workId = null, title = "Terminal" }: Props = $props();

  let attachId = $state<number | null>(null);
  let boundSessionId = $state("");
  let lines = $state<string[]>([]);
  let cursorRow = $state(0);
  let cursorCol = $state(0);
  let error = $state<string | null>(null);
  let connecting = $state(true);
  let inputEl = $state<HTMLInputElement | null>(null);
  let sessions = $state<TerminalSessionSummary[]>([]);
  let sessionHostAvailable = $state(true);
  let hostMessage = $state("");
  let paneEl = $state<HTMLDivElement | null>(null);

  let unlisten: UnlistenFn | null = null;
  let poll: ReturnType<typeof setInterval> | null = null;
  let heartbeatTimer: ReturnType<typeof setInterval> | null = null;

  function focusInput() {
    inputEl?.focus();
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
    } catch (err) {
      sessionHostAvailable = false;
      hostMessage = err instanceof Error ? err.message : String(err);
    }
  }

  async function connect() {
    connecting = true;
    error = null;
    await checkHost();
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
        })) as { session_id?: string };
        sid = created.session_id?.trim() ?? "";
        if (!sid) throw new Error("workshop did not return a session id");
        boundSessionId = sid;
        if (undertakings.active) {
          undertakings.bindTerminal(sid);
        }
      }
      const attach = await terminalAttach(sid);
      attachId = attach.attach_id;
      await applyResize();

      unlisten = await listen<TerminalFrame>("terminal-frame", (event) => {
        if (event.payload.attach_id !== attachId) return;
        cursorRow = event.payload.cursor_row;
        cursorCol = event.payload.cursor_col;
        void refreshSnapshot();
      });
      await refreshSnapshot();
      await refreshSessionList();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      if (/session|sidecar|not found|unavailable|404|connection/i.test(error)) {
        sessionHostAvailable = false;
      }
    } finally {
      connecting = false;
    }
  }

  async function applyResize() {
    if (attachId == null || !paneEl) return;
    const cols = Math.max(40, Math.floor(paneEl.clientWidth / 7.2));
    const rows = Math.max(12, Math.floor(paneEl.clientHeight / 16));
    try {
      await terminalResize(attachId, cols, rows);
    } catch {
      /* ignore */
    }
  }

  async function refreshSnapshot() {
    if (attachId == null) return;
    try {
      lines = await terminalSnapshot(attachId);
    } catch {
      // attach may have been dropped
    }
  }

  async function sendKey(event: KeyboardEvent) {
    if (attachId == null) return;
    const key = normalizeKey(event);
    if (!key) return;
    event.preventDefault();
    try {
      await terminalKey(attachId, key, event.ctrlKey, event.altKey, event.shiftKey);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function normalizeKey(event: KeyboardEvent): string | null {
    const k = event.key;
    if (k === "Enter") return "enter";
    if (k === "Backspace") return "backspace";
    if (k === "Tab") return "tab";
    if (k === "Escape") return "escape";
    if (k === "ArrowUp") return "up";
    if (k === "ArrowDown") return "down";
    if (k === "ArrowLeft") return "left";
    if (k === "ArrowRight") return "right";
    if (k === "Home") return "home";
    if (k === "End") return "end";
    if (k === "Delete") return "delete";
    if (k === "PageUp") return "pageup";
    if (k === "PageDown") return "pagedown";
    if (k.length === 1) return k.toLowerCase();
    return null;
  }

  async function interrupt() {
    if (!boundSessionId) return;
    try {
      await terminalInterrupt(boundSessionId);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
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
      const created = await terminalCreate({ work_id: null, cwd: null, lease_id: null });
      const sid = created.session_id?.trim() ?? "";
      if (sid) shellTabs.openTerminal(sid, { activate: true, title: "Diagnostic Terminal" });
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  onMount(() => {
    void restoreUndertakingContext().then(connect).then(() => focusInput());
    poll = setInterval(() => void refreshSnapshot(), 1500);
    heartbeatTimer = setInterval(() => void sendHeartbeat(), 30_000);
    void sendHeartbeat();
  });

  onDestroy(() => {
    unlisten?.();
    if (poll) clearInterval(poll);
    if (heartbeatTimer) clearInterval(heartbeatTimer);
    if (attachId != null) void terminalDetach(attachId);
  });
</script>

<div
  bind:this={paneEl}
  class="terminal-pane flex h-full min-h-0 flex-col bg-[#0c0a09] text-[#e7e5e4]"
  role="application"
  aria-label={title}
>
  <div
    class="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-white/10 px-3 py-1.5"
  >
    <div class="min-w-0 flex flex-wrap items-center gap-2 text-xs text-white/60">
      <span class="truncate">{title}</span>
      {#if boundSessionId}
        <span class="truncate font-mono text-[10px] text-white/35">
          {boundSessionId.slice(0, 8)}
        </span>
      {/if}
      <UndertakingContextChip />
    </div>
    <div class="flex items-center gap-1">
      <select
        class="max-w-[140px] rounded bg-white/5 px-1 py-0.5 text-[10px] text-white/70"
        onchange={(e) => {
          const v = (e.currentTarget as HTMLSelectElement).value;
          if (!v) return;
          boundSessionId = v;
          void connect();
        }}
      >
        <option value="">Sessions…</option>
        {#if workId && sessions.some((session) => session.work_id === workId)}
          <optgroup label="This undertaking">
            {#each sessions.filter((session) => session.work_id === workId) as s (s.session_id)}
              <option value={s.session_id}>{s.session_id.slice(0, 8)} · {s.root_kind}</option>
            {/each}
          </optgroup>
        {/if}
        <optgroup label="Other sessions">
          {#each sessions.filter((session) => !workId || session.work_id !== workId) as s (s.session_id)}
            <option value={s.session_id}>
              {s.session_id.slice(0, 8)}{s.work_id ? " · tracked" : " · diagnostic"}
            </option>
          {/each}
        </optgroup>
      </select>
      <button
        type="button"
        class="rounded px-2 py-0.5 text-[11px] text-white/60 hover:bg-white/10 hover:text-white"
        onclick={() => void openDiagnosticSession()}
        title="Open an untracked diagnostic terminal"
      >
        Diagnostic
      </button>
      <button
        type="button"
        class="rounded px-2 py-0.5 text-[11px] text-white/60 hover:bg-white/10 hover:text-white"
        onclick={() => void applyResize()}
        title="Resize PTY to pane"
      >
        Resize
      </button>
      <button
        type="button"
        class="rounded px-2 py-0.5 text-[11px] text-white/60 hover:bg-white/10 hover:text-white"
        onclick={interrupt}
        title="Send SIGINT to the session"
      >
        Interrupt
      </button>
    </div>
  </div>

  {#if !sessionHostAvailable || (error && /session|sidecar|unavailable|404/i.test(error))}
    <div class="m-3 rounded border border-amber-500/40 bg-amber-950/40 p-3 text-xs text-amber-100">
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
    <div class="m-3 rounded border border-red-500/40 bg-red-500/10 p-2 text-xs text-red-200">
      {error}
    </div>
  {:else if connecting}
    <div class="flex flex-1 items-center justify-center text-xs text-white/50">
      Connecting to workshop session…
    </div>
  {:else}
    <div class="relative min-h-0 flex-1 overflow-hidden" role="presentation" onclick={focusInput}>
      <pre
        class="terminal-grid h-full w-full overflow-auto p-3 font-mono text-[12px] leading-[1.35] whitespace-pre"
        >{#each lines as line, i}{line}
          {#if i < lines.length - 1}{"\n"}{/if}{/each}</pre
      >
      <input
        bind:this={inputEl}
        class="absolute inset-0 h-full w-full cursor-text opacity-0"
        type="text"
        autocomplete="off"
        autocorrect="off"
        autocapitalize="off"
        spellcheck="false"
        aria-label="Terminal input"
        onkeydown={sendKey}
      />
    </div>
    <div
      class="flex shrink-0 items-center gap-2 border-t border-white/10 px-3 py-1 text-[10px] text-white/40"
    >
      <span>cursor {cursorRow}:{cursorCol}</span>
      <span class="text-white/25">shared session — agents see the same PTY</span>
    </div>
  {/if}
</div>

<style>
  .terminal-grid {
    caret-color: transparent;
    user-select: text;
  }
</style>
