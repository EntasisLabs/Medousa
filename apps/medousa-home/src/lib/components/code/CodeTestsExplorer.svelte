<script lang="ts">
  import { codeTestTargetSupported, distinctCodeTestTargets } from "$lib/code/codeTestTargets";
  import type { CodeTasksController } from "$lib/code/codeTasksController.svelte";
  import { commandRoot, currentCommandRoot } from "$lib/code/codeCommandContext";
  import CodeOperationNotice from "./CodeOperationNotice.svelte";
  let { tasks, activePath, onOpenLocation }: { tasks: CodeTasksController; activePath: string; onOpenLocation: (path: string, line: number) => void } = $props();
  let query = $state("");
  let scope = $state("project");
  let failedOnly = $state(false);
  let limit = $state(120);
  const root = $derived(currentCommandRoot(tasks.projectTasks, activePath));
  const recentByTest = $derived.by(() => {
    const map = new Map<string, typeof tasks.recentRuns[number]>();
    for (const run of [...tasks.recentRuns].sort((a, b) => b.started_at.localeCompare(a.started_at))) if (run.test_id && !map.has(run.test_id)) map.set(run.test_id, run);
    return map;
  });
  const filtered = $derived(tasks.projectTests.filter((test) => {
    if (scope === "file" && test.path !== activePath) return false;
    if (scope === "package" && !tasks.projectTasks.some((task) => task.id === test.task_id && commandRoot(task) === root)) return false;
    if (failedOnly && recentByTest.get(test.id)?.state !== "failed") return false;
    return `${test.label} ${test.path} ${test.provider ?? ""}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase());
  }).sort((a, b) => a.path.localeCompare(b.path) || a.line - b.line));
  const queueTargets = $derived(distinctCodeTestTargets(filtered));
  const groups = $derived.by(() => {
    const map = new Map<string, typeof tasks.projectTests>();
    for (const test of filtered.slice(0, limit)) map.set(test.path, [...(map.get(test.path) ?? []), test]);
    return [...map].map(([path, tests]) => ({ path, tests }));
  });
</script>
<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex shrink-0 flex-wrap items-center gap-2 border-b border-surface-500/20 px-3 py-2">
    <input class="min-w-32 flex-1 rounded border border-surface-500/35 bg-surface-900 px-2 py-1.5 text-chrome-sm" aria-label="Filter tests" placeholder="Filter by test, file, or provider…" bind:value={query} oninput={() => (limit = 120)} />
    <select class="rounded bg-surface-900 px-2 py-1.5 text-chrome-sm" aria-label="Test scope" bind:value={scope} onchange={() => (limit = 120)}><option value="project">Whole project</option><option value="package" disabled={root === null}>Current package</option><option value="file" disabled={!activePath}>Current file</option></select>
    <button type="button" class="rounded px-2 py-1 text-chrome-sm {failedOnly ? 'bg-rose-500/15 text-rose-200' : 'text-content-quiet'}" aria-pressed={failedOnly} onclick={() => { failedOnly = !failedOnly; limit = 120; }}>Failed invocations</button>
    <button type="button" class="rounded bg-primary-500/15 px-2 py-1 text-chrome-sm text-content-link disabled:opacity-40" disabled={tasks.running || tasks.preparing || tasks.testQueueActive || !queueTargets.length || queueTargets.some((test) => !codeTestTargetSupported(test))} onclick={() => void tasks.runTests(queueTargets)}>{failedOnly ? "Rerun failed invocations" : "Run filtered targets"} · {queueTargets.length}</button>
    <button type="button" class="rounded px-2 py-1 text-chrome-sm text-content-quiet disabled:opacity-40" disabled={tasks.testsLoading} onclick={() => void tasks.refreshTests()}>Refresh</button>
  </div>
  <p class="shrink-0 border-b border-surface-500/15 px-3 py-1.5 text-chrome-xs text-content-quiet">{filtered.length} of {tasks.projectTests.length} discovered tests · results show prior invocations, not verification of current edits.</p>
  {#if tasks.testQueue}<div class="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-surface-500/20 px-3 py-2 text-chrome-sm text-content-secondary" role="status"><span>Queue {tasks.testQueue.phase} · {tasks.testQueue.completed}/{tasks.testQueue.total} invocations · {tasks.testQueue.passed} passed · {tasks.testQueue.failed} failed</span>{#if tasks.testQueueActive}<button type="button" class="text-chrome-xs text-content-link" onclick={() => tasks.cancelTestQueue()}>Clear remaining queue</button>{/if}</div>{/if}
  {#if tasks.projectTests.length >= 2000}<p class="border-b border-amber-500/20 px-3 py-2 text-chrome-xs text-amber-200">Discovery reached the workshop’s 2,000-test limit. More tests may exist; package Test commands remain available.</p>{/if}
  {#if tasks.testsError}<CodeOperationNotice message={tasks.testsError} />{/if}
  <div class="min-h-0 flex-1 overflow-y-auto">
    {#if tasks.testsLoading}<p class="px-4 py-4 text-chrome-sm text-content-quiet" role="status">Discovering tests on the workshop…</p>
    {:else if !tasks.testsLoaded}<p class="px-4 py-4 text-chrome-sm text-content-quiet">Test discovery is unavailable. Retry with Refresh.</p>
    {:else if tasks.projectTests.length === 0}<p class="px-4 py-4 text-chrome-sm text-content-quiet">No individually addressable tests were discovered. Choose a package’s Test command from the command picker.</p>
    {:else if filtered.length === 0}<p class="px-4 py-4 text-chrome-sm text-content-quiet">No tests match this scope and filter.</p>
    {:else}
      {#each groups as group (group.path)}
        <details open class="border-b border-surface-500/20"><summary class="sticky top-0 cursor-pointer bg-surface-900 px-3 py-2 font-mono text-chrome-xs text-content-secondary">{group.path} · {group.tests.length}</summary>
          {#each group.tests as test (test.id)}
            {@const recent = recentByTest.get(test.id)}
            {@const supported = codeTestTargetSupported(test)}
            <div class="flex flex-wrap items-center gap-2 border-t border-surface-500/10 px-3 py-2">
              <button type="button" class="min-w-0 flex-1 text-left text-chrome-sm text-content-secondary hover:text-content-link" onclick={() => onOpenLocation(test.path, test.line)}><span class="break-words">{test.label}</span><span class="ml-2 text-chrome-xs text-content-quiet">line {test.line}</span></button>
              <span class="text-chrome-xs text-content-quiet">{tasks.run?.test_id === test.id && tasks.running ? "Running" : recent ? `Last invocation: ${recent.state}` : "Not run"}</span>
              {#if recent}<button type="button" class="text-chrome-xs text-content-link disabled:opacity-40" disabled={tasks.running || tasks.preparing || tasks.testQueueActive} onclick={() => void tasks.openRun(recent.run_id)}>View output · {new Date(recent.started_at).toLocaleString()}</button>{/if}
              {#if supported}<button type="button" class="rounded bg-primary-500/10 px-2 py-1 text-chrome-xs text-content-link disabled:opacity-40" disabled={tasks.running || tasks.preparing || tasks.testQueueActive} onclick={() => void tasks.runDetected(test)}>{test.target_kind === "file" ? "Run file" : recent ? "Rerun test" : "Run test"}</button>
              {:else}<span class="text-chrome-xs text-content-quiet">Individual targeting unavailable</span><button type="button" class="text-chrome-xs text-content-link disabled:opacity-40" disabled={tasks.running || tasks.preparing || tasks.testQueueActive} onclick={() => void tasks.runTask(test.task_id)}>Run package</button>{/if}
            </div>
          {/each}
        </details>
      {/each}
      {#if filtered.length > limit}<button type="button" class="m-3 rounded px-2 py-1 text-chrome-sm text-content-link" onclick={() => (limit += 120)}>Show more · {filtered.length - limit} remaining</button>{/if}
    {/if}
  </div>
</div>
