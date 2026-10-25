<script lang="ts">
  import { onMount } from "svelte";
  import TerminalPane from "./TerminalPane.svelte";
  import { terminalSessions, type TerminalSessionSummary } from "$lib/terminal";
  import { openDesktopWindow } from "$lib/desktop/windows";
  import { showMainWindow } from "$lib/window";

  let { sessionId, workshopId, title, onClose }: {
    sessionId: string; workshopId: string; title: string; onClose: () => void;
  } = $props();
  let session = $state<TerminalSessionSummary | null>(null);
  let error = $state<string | null>(null);
  let disposed = false;
  async function load() {
    error = null;
    try {
      const sessions = await terminalSessions(null);
      if (disposed) return;
      const found = sessions.find(entry => entry.session_id === sessionId);
      if (!found || found.exited) throw new Error("This terminal session has ended. Open a new terminal from Spotlight.");
      session = found;
    } catch (cause) { if (!disposed) error = cause instanceof Error ? cause.message : String(cause); }
  }
  function openSession(id: string) {
    return openDesktopWindow({ kind: "terminal", workshopId, resourceId: id });
  }
  function openPath(path: string) {
    if (!session?.work_id) return;
    void openDesktopWindow({ kind: "code", workshopId, resourceId: session.work_id, path }).catch(cause => {
      error = cause instanceof Error ? cause.message : String(cause);
    });
  }
  function manageHost() {
    void showMainWindow().catch(cause => { error = cause instanceof Error ? cause.message : String(cause); });
  }
  onMount(() => { void load(); return () => { disposed = true; }; });
</script>

<div class="flex min-h-0 min-w-0 flex-1 flex-col">
  {#if error}<div class="p-4 text-sm text-content-quiet" role="alert">{error} <button type="button" class="underline" onclick={() => void load()}>Try again</button></div>{/if}
  {#if session}
    <TerminalPane {sessionId} {title} workId={session.work_id} worktreeRoot={session.cwd} executionRuntimeId={null} onOpenSession={openSession} onOpenPath={openPath} onManageHost={manageHost} {onClose}/>
  {:else if !error}<p class="m-auto p-8 text-content-quiet" role="status">Opening your terminal…</p>{/if}
</div>
