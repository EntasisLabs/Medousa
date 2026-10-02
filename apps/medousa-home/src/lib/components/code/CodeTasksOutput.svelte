<script lang="ts">
  import CodeTestsExplorer from "./CodeTestsExplorer.svelte";
  /**
   * Task output dock, last-run banner, and discovered tests list.
   */
  import type { CodeTasksController } from "$lib/code/codeTasksController.svelte";

  interface Props {
    tasks: CodeTasksController;
    activePath?: string;
    onOpenLocation: (path: string, line: number) => void;
    mode?: "output" | "tests";
  }

  let { tasks, onOpenLocation, mode = "output", activePath = "" }: Props = $props();
</script>

{#if mode === "output"}
  <div class="flex min-h-0 flex-1 flex-col bg-surface-950/80">
    <div class="flex items-center justify-between gap-2 border-b border-surface-500/20 px-2.5 py-1">
      <span class="text-chrome-xs font-medium uppercase tracking-[0.06em] text-content-quiet">
        {#if tasks.preparing}Saving before run…
        {:else if tasks.run}{tasks.run.task.label}
        {:else}Output{/if}
        {#if tasks.run?.state === "ready"}<span class="normal-case tracking-normal text-emerald-300/90"> · ready</span>
        {:else if tasks.running}<span class="normal-case tracking-normal text-content-link"> · {tasks.run?.state === "stopping" ? "stopping" : "running"}</span>
        {:else if tasks.run}<span class="normal-case tracking-normal {tasks.run.state === 'failed' ? 'text-rose-200' : 'text-content-secondary'}"> · {tasks.run.state === "passed" ? "completed" : tasks.run.state === "failed" ? "needs attention" : tasks.run.state}</span>{/if}
        {#if tasks.outputTruncated}<span class="normal-case tracking-normal text-amber-200/80"> · truncated</span>{/if}
        {#if tasks.runHistoryTruncated}<span class="normal-case tracking-normal text-amber-200/80"> · more runs retained</span>{/if}
      </span>
      <div class="flex items-center gap-1">
        {#if tasks.recentRuns.length > 1}
          <select
            class="max-w-36 rounded border border-surface-500/30 bg-surface-900 px-1 py-0.5 text-chrome-xs text-content-secondary"
            aria-label="Recent project runs"
            value={tasks.run?.run_id ?? ""}
            disabled={tasks.running || tasks.preparing || tasks.testQueueActive}
            onchange={(event) => void tasks.openRun(event.currentTarget.value)}
          >
            {#each tasks.recentRuns as recent (recent.run_id)}
              <option value={recent.run_id}>{recent.task.label} · {recent.state}</option>
            {/each}
          </select>
        {/if}
        {#if (tasks.readyUrl || tasks.run?.ready_url) && (tasks.run?.state === "ready" || tasks.running)}
          <button
            type="button"
            class="rounded px-1.5 py-0.5 text-chrome-xs text-emerald-200/90 hover:bg-emerald-500/10 disabled:opacity-40"
            disabled={tasks.previewOpening}
            onclick={() => void tasks.openPreview()}
          >{tasks.previewOpening ? "Opening…" : "Open Preview"}</button>
          <button
            type="button"
            class="rounded px-1.5 py-0.5 text-chrome-xs text-emerald-200/90 hover:bg-emerald-500/10 disabled:opacity-40"
            disabled={tasks.previewOpening}
            onclick={() => void tasks.openPreview(true)}
          >Open Beside Code</button>
        {/if}
        {#if tasks.running && (tasks.run?.state === "running" || tasks.run?.state === "ready" || tasks.run?.state === "stopping")}
          <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-rose-200/90 hover:bg-rose-500/10" onclick={() => void tasks.stopDetected()}>{tasks.run?.state === "stopping" ? "Force stop" : "Stop"}</button>
        {/if}
        <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800 hover:text-content-secondary" onclick={() => tasks.toggleOutput(false)}>Hide</button>
      </div>
    </div>
    {#if tasks.run}
      <p class="shrink-0 border-b border-surface-500/15 px-3 py-2 text-chrome-sm text-content-secondary">{tasks.run.state === "ready" ? "Your application is ready. Open its workshop preview to continue." : tasks.running ? "This command stays attached to its original project, even when you change files." : tasks.run.state === "failed" ? "Review the reported locations and output, then rerun this exact command." : tasks.run.state === "cancelled" ? "This invocation was cancelled; it does not verify your current edits." : "This recorded invocation completed. Its result applies to the code it ran against."}
        <span class="ml-2 text-chrome-xs text-content-quiet">{tasks.run.task.root ?? "."}{tasks.run.started_at ? ` · ${new Date(tasks.run.started_at).toLocaleString()}` : ""}</span>
      </p>
    {/if}
    {#if !tasks.liveStdout && !tasks.liveStderr && !tasks.running && !tasks.run}
      <p class="px-2.5 py-2 text-chrome-sm text-content-quiet">Run a project check to stream output here.</p>
    {:else}
      <pre class="min-h-0 flex-1 overflow-auto px-2.5 py-1.5 font-mono text-chrome-xs leading-relaxed text-content-tertiary whitespace-pre-wrap break-words" aria-label="Task output">{tasks.liveStdout}{#if tasks.liveStdout && tasks.liveStderr}{"\n\n"}{/if}{#if tasks.liveStderr}<span class="text-rose-200/90">{tasks.liveStderr}</span>{/if}{#if !tasks.liveStdout && !tasks.liveStderr && tasks.running}<span class="text-content-quiet">Waiting for output…</span>{/if}</pre>
      {#if tasks.liveLocations.length}
        <div class="max-h-20 shrink-0 overflow-y-auto border-t border-surface-500/20">
          {#each tasks.liveLocations.slice(0, 8) as location (`${location.path}:${location.line}:${location.column}:${location.message}`)}
            <button type="button" class="flex w-full items-center gap-2 border-b border-surface-500/10 px-2.5 py-1 text-left text-chrome-xs text-content-secondary hover:bg-surface-800/60" onclick={() => onOpenLocation(location.path, location.line)}>
              <span class="min-w-0 flex-1 truncate">{location.message || location.path}</span>
              <span class="shrink-0 font-mono text-content-quiet">{location.path}:{location.line}</span>
            </button>
          {/each}
        </div>
      {/if}
    {/if}
  </div>
{/if}
{#if mode === "output" && tasks.result}
  <div class="shrink-0 border-t {tasks.result.success ? 'border-emerald-500/25 bg-emerald-950/20 text-emerald-200' : 'border-rose-500/30 bg-rose-950/25 text-rose-200'}">
    <button type="button" class="flex w-full items-center justify-between gap-2 px-2.5 py-1 text-left text-chrome-xs" title="Repeat this exact command" onclick={() => void tasks.rerunLast()}>
      <span>{tasks.result.success ? "Passed" : "Needs attention"} · {tasks.result.task.label}</span>
      <span class="text-current">Rerun · {(tasks.result.duration_ms / 1000).toFixed(1)}s{tasks.result.exit_code != null ? ` · exit ${tasks.result.exit_code}` : ""}</span>
    </button>
    {#each tasks.result.locations.slice(0, 5) as location (`${location.path}:${location.line}:${location.column}`)}
      <button type="button" class="flex w-full items-center gap-2 border-t border-current/10 px-2.5 py-1 text-left text-chrome-xs hover:bg-white/5" onclick={() => onOpenLocation(location.path, location.line)}>
        <span class="min-w-0 flex-1 truncate">{location.message || location.path}</span>
        <span class="shrink-0 font-mono">{location.path}:{location.line}</span>
      </button>
    {/each}
  </div>
{/if}
{#if mode === "tests"}
  <CodeTestsExplorer {tasks} {activePath} {onOpenLocation} />
{/if}
