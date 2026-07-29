<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    terminalAttach,
    terminalCreate,
    terminalDetach,
    terminalInterrupt,
    terminalKey,
    terminalSnapshot,
    type TerminalFrame,
  } from "$lib/terminal";

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

  let unlisten: UnlistenFn | null = null;
  let poll: ReturnType<typeof setInterval> | null = null;

  function focusInput() {
    inputEl?.focus();
  }

  async function connect() {
    connecting = true;
    error = null;
    try {
      let sid = (boundSessionId || sessionId).trim();
      if (!sid) {
        const created = (await terminalCreate({
          work_id: workId,
          cwd: null,
        })) as { session_id?: string };
        sid = created.session_id?.trim() ?? "";
        if (!sid) throw new Error("workshop did not return a session id");
        boundSessionId = sid;
      }
      const attach = await terminalAttach(sid);
      attachId = attach.attach_id;

      unlisten = await listen<TerminalFrame>("terminal-frame", (event) => {
        if (event.payload.attach_id !== attachId) return;
        cursorRow = event.payload.cursor_row;
        cursorCol = event.payload.cursor_col;
        void refreshSnapshot();
      });
      await refreshSnapshot();
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      connecting = false;
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

  onMount(() => {
    void connect().then(() => focusInput());
    poll = setInterval(() => void refreshSnapshot(), 1500);
  });

  onDestroy(() => {
    unlisten?.();
    if (poll) clearInterval(poll);
    if (attachId != null) void terminalDetach(attachId);
  });
</script>

<div
  class="terminal-pane flex h-full min-h-0 flex-col bg-[#0c0a09] text-[#e7e5e4]"
  role="application"
  aria-label={title}
>
  <div
    class="flex shrink-0 items-center justify-between gap-2 border-b border-white/10 px-3 py-1.5"
  >
    <div class="min-w-0 flex items-center gap-2 text-xs text-white/60">
      <span class="truncate">{title}</span>
      {#if boundSessionId}
        <span class="truncate font-mono text-[10px] text-white/35">
          {boundSessionId.slice(0, 8)}
        </span>
      {/if}
    </div>
    <div class="flex items-center gap-1">
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

  {#if error}
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
