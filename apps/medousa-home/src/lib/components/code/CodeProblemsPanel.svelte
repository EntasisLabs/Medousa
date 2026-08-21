<script lang="ts">
  import { CircleAlert, FileCode2, LoaderCircle, RotateCcw, Search } from "@lucide/svelte";
  import {
    PROBLEM_SEVERITY_OPTIONS,
    type CodeProblemsController,
  } from "$lib/code/codeProblemsController.svelte";

  interface Props {
    problems: CodeProblemsController;
  }

  let { problems }: Props = $props();
</script>

<div class="flex items-center gap-1.5 border-b border-surface-500/20 px-2 py-1.5">
  <label class="flex min-w-28 flex-1 items-center gap-1.5 rounded border border-surface-500/35 bg-surface-900/70 px-1.5 py-1 focus-within:border-primary-400/60">
    <Search size={11} class="shrink-0 text-content-quiet" />
    <input class="min-w-0 flex-1 bg-transparent text-chrome-sm text-content-secondary outline-none placeholder:text-content-faint" aria-label="Filter project problems" placeholder="Filter problems" bind:value={problems.query} />
  </label>
  <div class="flex items-center gap-0.5" aria-label="Problem severity filter">
    {#each PROBLEM_SEVERITY_OPTIONS as option (option.value)}
      <button type="button" class="rounded px-1.5 py-1 text-chrome-xs {problems.severity === option.value ? 'bg-primary-500/20 text-primary-100' : 'text-content-quiet hover:bg-surface-800 hover:text-content-secondary'}" aria-pressed={problems.severity === option.value} onclick={() => (problems.severity = option.value)}>{option.label}</button>
    {/each}
  </div>
  <span class="text-chrome-xs text-rose-300" title="Errors">{problems.counts.errors}</span>
  <span class="text-chrome-xs text-amber-300" title="Warnings">{problems.counts.warnings}</span>
  <button type="button" class="rounded p-0.5 text-content-quiet hover:bg-surface-800 hover:text-surface-200 disabled:opacity-50" aria-label="Refresh project problems" title="Refresh project problems" disabled={problems.loading} onclick={() => void problems.refresh()}><RotateCcw size={11} class={problems.loading ? "animate-spin" : ""} /></button>
</div>

{#if problems.error}
  <div class="flex items-start justify-between gap-3 border-b border-rose-400/20 bg-rose-500/5 px-3 py-2 text-chrome-sm text-rose-200">
    <span>Could not refresh project problems: {problems.error}</span>
    <button type="button" class="shrink-0 underline underline-offset-2" onclick={() => void problems.refresh()}>Retry</button>
  </div>
{/if}
{#if problems.unavailableLanguages.length > 0}
  <p class="border-b border-amber-400/20 bg-amber-500/5 px-3 py-1.5 text-chrome-sm text-amber-200">Results are incomplete for {problems.unavailableLanguages.join(", ")}.</p>
{/if}
{#if problems.loading && !problems.loaded}
  <p class="flex items-center px-3 py-3 text-chrome-sm text-content-quiet"><LoaderCircle size={11} class="mr-1.5 animate-spin" />Loading project problems…</p>
{:else if problems.counts.total === 0}
  <p class="px-3 py-3 text-chrome-sm text-content-quiet">No problems found in this project.</p>
{:else if problems.groups.length === 0}
  <p class="px-3 py-3 text-chrome-sm text-content-quiet">No problems match the current filters.</p>
{:else}
  {#each problems.groups as group (group.path)}
    <div class="flex items-center gap-2 border-b border-surface-500/20 bg-surface-900/70 px-3 py-1 text-chrome-xs text-content-tertiary">
      <FileCode2 size={10} class="shrink-0 text-content-link/70" />
      <span class="min-w-0 flex-1 truncate font-mono">{group.path}</span>
      {#if group.counts.errors > 0}<span class="text-rose-300">{group.counts.errors}E</span>{/if}
      {#if group.counts.warnings > 0}<span class="text-amber-300">{group.counts.warnings}W</span>{/if}
    </div>
    {#each group.problems as problem (problem.id)}
      <button type="button" class="flex w-full items-start gap-2 border-b border-surface-500/15 px-3 py-1.5 text-left hover:bg-surface-800/60" title={`${problem.path}:${problem.line}:${problem.character}`} onclick={() => void problems.openProblem(problem)}>
        <CircleAlert size={11} class={problem.severity === "error" ? "mt-0.5 shrink-0 text-rose-300" : problem.severity === "warning" ? "mt-0.5 shrink-0 text-amber-300" : "mt-0.5 shrink-0 text-sky-300"} />
        <span class="min-w-0 flex-1 text-chrome-sm text-content-secondary">
          <span class="break-words">{problem.message}</span>
          {#if problem.origin === "task"}<span class="ml-1 rounded bg-surface-800 px-1 text-chrome-xs text-content-quiet">{problem.taskLabel} · current run</span>{/if}
          {#if problem.source || problem.code}<span class="ml-1 text-chrome-xs text-content-faint">{[problem.source, problem.code].filter(Boolean).join(" · ")}</span>{/if}
        </span>
        <span class="shrink-0 font-mono text-chrome-xs text-content-quiet">{problem.line}:{problem.character}</span>
      </button>
    {/each}
  {/each}
{/if}
