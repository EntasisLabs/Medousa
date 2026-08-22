<script lang="ts">
  import { Copy, RotateCcw, SquareTerminal, Trash2, X } from "@lucide/svelte";
  import type { CodeBottomPanel } from "$lib/code/codeWorkbenchState.svelte";
  import type { CodeProblemsController } from "$lib/code/codeProblemsController.svelte";
  import type { CodeTasksController } from "$lib/code/codeTasksController.svelte";
  import CodeProblemsPanel from "$lib/components/code/CodeProblemsPanel.svelte";
  import CodeTasksOutput from "$lib/components/code/CodeTasksOutput.svelte";
  import CodeTerminalDock from "$lib/components/work/CodeTerminalDock.svelte";

  interface Props {
    active: CodeBottomPanel;
    problems: CodeProblemsController;
    tasks: CodeTasksController;
    workId: string;
    terminalSessionId: string | null;
    workspaceRoot: string | null;
    terminalTitle: string;
    onSelect: (panel: Exclude<CodeBottomPanel, null>) => void | Promise<void>;
    onClose: () => void;
    onOpenLocation: (path: string, line: number) => void;
    onPopOutTerminal: () => void | Promise<void>;
  }

  let {
    active,
    problems,
    tasks,
    workId,
    terminalSessionId,
    workspaceRoot,
    terminalTitle,
    onSelect,
    onClose,
    onOpenLocation,
    onPopOutTerminal,
  }: Props = $props();

  let commandRevealed = $state(false);
  const command = $derived(tasks.run?.task.argv.join(" ") ?? tasks.selectedTask?.argv.join(" ") ?? "");

  async function copyOutput() {
    const output = [tasks.liveStdout, tasks.liveStderr].filter(Boolean).join("\n");
    if (!output || typeof navigator === "undefined" || !navigator.clipboard) return;
    await navigator.clipboard.writeText(output);
  }
</script>

{#if active}
  <section class="flex max-h-72 shrink-0 flex-col border-t border-surface-500/35 bg-surface-950/90" aria-label="Code feedback">
    <div class="flex shrink-0 items-center justify-between gap-2 border-b border-surface-500/25 px-2 py-1">
      <div class="flex min-w-0 items-center gap-0.5" role="tablist" aria-label="Code feedback channels">
        {#each ["problems", "output", "tests", "terminal"] as panel (panel)}
          <button type="button" role="tab" aria-selected={active === panel} class="rounded px-2 py-0.5 text-chrome-xs capitalize {active === panel ? 'bg-surface-700 text-surface-50' : 'text-content-quiet hover:bg-surface-800 hover:text-content-secondary'}" onclick={() => void onSelect(panel as Exclude<CodeBottomPanel, null>)}>
            {panel}{#if panel === "problems" && problems.counts.total > 0} <span class="text-rose-300">{problems.counts.total}</span>{/if}
          </button>
        {/each}
      </div>
      <div class="flex items-center gap-0.5">
        {#if active === "output"}
          {#if command}<button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800" aria-pressed={commandRevealed} onclick={() => (commandRevealed = !commandRevealed)}>Command</button>{/if}
          <button type="button" class="rounded p-1 text-content-quiet hover:bg-surface-800 hover:text-content-secondary" aria-label="Copy output" title="Copy output" onclick={() => void copyOutput()}><Copy size={11} /></button>
          <button type="button" class="rounded p-1 text-content-quiet hover:bg-surface-800 hover:text-content-secondary" aria-label="Clear output" title="Clear output" onclick={() => tasks.clearOutput()}><Trash2 size={11} /></button>
          {#if tasks.running}<button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-rose-200 hover:bg-rose-500/10" onclick={() => void tasks.stopDetected()}>{tasks.run?.state === "stopping" ? "Force stop" : "Stop"}</button>
          {:else if tasks.result}<button type="button" class="rounded p-1 text-content-quiet hover:bg-surface-800 hover:text-content-secondary" aria-label="Rerun task" title="Rerun task" onclick={() => void tasks.rerunLast()}><RotateCcw size={11} /></button>{/if}
        {:else if active === "terminal"}
          <SquareTerminal size={11} class="text-content-quiet" />
          <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800" onclick={() => void onPopOutTerminal()}>Pop out</button>
        {/if}
        <button type="button" class="rounded p-1 text-content-quiet hover:bg-surface-800 hover:text-content-secondary" aria-label="Close feedback panel" onclick={onClose}><X size={11} /></button>
      </div>
    </div>
    {#if active === "output" && commandRevealed && command}
      <code class="shrink-0 border-b border-surface-500/20 bg-surface-900/70 px-3 py-1 font-mono text-chrome-xs text-content-tertiary">{command}</code>
    {/if}
    <div class="min-h-0 overflow-y-auto">
      {#if active === "problems"}
        <CodeProblemsPanel {problems} />
      {:else if active === "output"}
        <CodeTasksOutput {tasks} mode="output" {onOpenLocation} />
      {:else if active === "tests"}
        <CodeTasksOutput {tasks} mode="tests" {onOpenLocation} />
      {:else}
        <CodeTerminalDock open={true} sessionId={terminalSessionId} {workId} worktreeRoot={workspaceRoot} title={terminalTitle} onClose={onClose} onPopOut={() => void onPopOutTerminal()} />
      {/if}
    </div>
  </section>
{/if}
