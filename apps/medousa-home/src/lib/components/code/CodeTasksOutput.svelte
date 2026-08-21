<script lang="ts">
  /**
   * Task output dock, last-run banner, and discovered tests list.
   */
  import type { CodeTasksController } from "$lib/code/codeTasksController.svelte";

  interface Props {
    tasks: CodeTasksController;
    onOpenLocation: (path: string, line: number) => void;
  }

  let { tasks, onOpenLocation }: Props = $props();
</script>

{#if tasks.outputOpen}
  <div class="flex max-h-52 shrink-0 flex-col border-t border-surface-500/30 bg-surface-950/80">
    <div class="flex items-center justify-between gap-2 border-b border-surface-500/20 px-2.5 py-1">
      <span class="text-chrome-xs font-medium uppercase tracking-[0.06em] text-content-quiet">
        {#if tasks.run}Task: {tasks.run.task.label}{:else}Output{/if}
        {#if tasks.run?.state === "ready"}<span class="normal-case tracking-normal text-emerald-300/90"> · ready</span>
        {:else if tasks.running}<span class="normal-case tracking-normal text-content-link"> · running</span>{/if}
        {#if tasks.outputTruncated}<span class="normal-case tracking-normal text-amber-200/80"> · truncated</span>{/if}
      </span>
      <div class="flex items-center gap-1">
        {#if (tasks.readyUrl || tasks.run?.ready_url) && (tasks.run?.state === "ready" || tasks.running)}
          <button
            type="button"
            class="rounded px-1.5 py-0.5 text-chrome-xs text-emerald-200/90 hover:bg-emerald-500/10 disabled:opacity-40"
            disabled={tasks.previewOpening}
            onclick={() => void tasks.openPreview()}
          >{tasks.previewOpening ? "Opening…" : "Open in Browser"}</button>
        {/if}
        {#if tasks.running && (tasks.run?.state === "running" || tasks.run?.state === "ready")}
          <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-rose-200/90 hover:bg-rose-500/10" onclick={() => void tasks.stopDetected()}>Stop</button>
        {/if}
        <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800 hover:text-content-secondary" onclick={() => tasks.toggleOutput(false)}>Hide</button>
      </div>
    </div>
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
{#if tasks.result}
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
{#if tasks.testsOpen}
  <div class="max-h-44 shrink-0 overflow-y-auto border-t border-surface-500/25 bg-surface-950/90">
    <div class="sticky top-0 flex items-center justify-between bg-surface-950 px-2.5 py-1 text-chrome-xs uppercase tracking-wider text-content-quiet"><span>Project tests</span><span>{tasks.projectTests.length}</span></div>
    {#if tasks.projectTests.length === 0}
      <p class="px-3 py-3 text-chrome-sm text-content-quiet">No individual tests were discovered. The project test command still works.</p>
    {:else}
      {#each tasks.projectTests as test (test.id)}
        <div class="flex items-center border-t border-surface-500/15">
          <button type="button" class="min-w-0 flex-1 truncate px-3 py-1.5 text-left text-chrome-sm text-content-secondary hover:bg-surface-800/60" onclick={() => onOpenLocation(test.path, test.line)}>{test.label}<span class="ml-2 font-mono text-chrome-xs text-content-faint">{test.path}:{test.line}</span></button>
          <button type="button" class="mr-2 rounded px-1.5 py-0.5 text-chrome-xs text-content-link hover:bg-surface-800 disabled:opacity-40" disabled={tasks.running} onclick={() => void tasks.runDetected(test)}>Run</button>
        </div>
      {/each}
    {/if}
  </div>
{/if}
