<script lang="ts">
  import CodeOperationNotice from "./CodeOperationNotice.svelte";
  import { tick } from "svelte";
  import { Check, ChevronDown, Play, Search, Square, X } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { placeToolbarPopover } from "$lib/utils/railPopover";
  import { commandKindLabel, commandRoot, currentCommandRoot, filterCommands } from "$lib/code/codeCommandContext";
  import type { CodeTasksController } from "$lib/code/codeTasksController.svelte";
  let { tasks, path = "", variant = "picker", runDisabled = false, runTitle = "" }: {
    tasks: CodeTasksController; path?: string; variant?: "picker" | "run"; runDisabled?: boolean; runTitle?: string;
  } = $props();
  let open = $state(false);
  let query = $state("");
  let all = $state(false);
  let limit = $state(60);
  let trigger = $state<HTMLButtonElement | null>(null);
  let panel = $state<HTMLDivElement | null>(null);
  let input = $state<HTMLInputElement | null>(null);
  const root = $derived(currentCommandRoot(tasks.projectTasks, path));
  const matches = $derived(filterCommands(tasks.projectTasks.filter((task) => all || root === null || commandRoot(task) === root), query));
  const visible = $derived(matches.slice(0, limit));
  const selected = $derived(tasks.selectedTask);
  function close() { open = false; trigger?.focus(); }
  function select(id: string) { tasks.selectTask(id); close(); }
  function keydown(event: KeyboardEvent) {
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(); return; }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp" && event.key !== "Enter") return;
    const rows = Array.from(panel?.querySelectorAll<HTMLButtonElement>("[data-command]") ?? []);
    const index = rows.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "Enter" && document.activeElement === input) { event.preventDefault(); if (rows[0]) rows[0].click(); return; }
    if (event.key === "Enter") return;
    event.preventDefault();
    const next = event.key === "ArrowDown" ? index + 1 : index < 0 ? rows.length - 1 : index - 1;
    if (next < 0) input?.focus(); else rows[Math.min(next, rows.length - 1)]?.focus();
  }
  $effect(() => {
    if (!open) return;
    query = ""; limit = 60; all = root === null;
    const place = () => { if (trigger && panel) placeToolbarPopover(trigger, panel, { prefer: "below", align: "end", width: 440, maxHeight: 400, gap: 6, pad: 8 }); };
    void tick().then(() => { place(); input?.focus(); });
    window.addEventListener("resize", place);
    window.visualViewport?.addEventListener("resize", place);
    return () => {
      window.removeEventListener("resize", place);
      window.visualViewport?.removeEventListener("resize", place);
    };
  });
</script>
{#if variant === "run"}
  <div class="command-run" role="group" aria-label="Project command">
    {#if tasks.running}
      <button type="button" class="command-run-button command-stop" aria-label={tasks.run ? `Stop ${tasks.run.task.label}` : "Stop project command"} title={tasks.run ? `Stop ${tasks.run.task.label}` : "Stop project command"} onclick={() => void tasks.stopDetected()}><Square size={12}/><span>Stop</span></button>
    {:else if selected}
      <button type="button" class="command-run-button" disabled={runDisabled || tasks.preparing || tasks.testQueueActive || !tasks.taskAvailable(selected)} aria-label={`Run ${selected.label}`} title={runTitle || `${selected.label}: ${selected.argv.join(" ")}`} onclick={() => void tasks.runDetected()}><Play size={13}/><span>{tasks.preparing ? "Saving…" : selected.label}</span></button>
    {/if}
    {#if !tasks.running}
      <button bind:this={trigger} type="button" class="command-run-button" class:command-chevron={!!selected} aria-haspopup="dialog" aria-expanded={open} aria-label="Choose project command" title={selected ? "Choose project command" : "Choose a command to run"} disabled={tasks.preparing || tasks.testQueueActive} onclick={() => (open = !open)}>
        {#if selected}<ChevronDown size={12}/>{:else}<Play size={13}/><span>Run…</span>{/if}
      </button>
    {/if}
  </div>
{:else}
<button bind:this={trigger} type="button" class="flex max-w-56 items-center gap-2 px-2 text-chrome-sm text-content-secondary hover:bg-primary-500/10" aria-haspopup="dialog" aria-expanded={open} aria-label="Choose project command" disabled={tasks.running || tasks.preparing || tasks.testQueueActive} onclick={() => (open = !open)}>
  <span class="truncate">{selected?.label ?? "Choose command…"}{#if selected && commandRoot(selected) !== "."}<span class="ml-1 text-content-quiet">· {commandRoot(selected)}</span>{/if}</span><ChevronDown size={12} />
</button>
{/if}
{#if open}
  <BodyPortal>
    <button type="button" tabindex="-1" class="fixed inset-0 z-[120] cursor-default" aria-label="Dismiss command picker" onclick={close}></button>
    <div bind:this={panel} class="command-picker z-[121] flex flex-col overflow-hidden" role="dialog" aria-label="Choose project command" tabindex="-1" onkeydown={keydown}>
      <div class="command-picker-search">
        <Search size={14} />
        <input bind:this={input} bind:value={query} oninput={() => (limit = 60)} class="command-picker-input" type="search" autocomplete="off" spellcheck="false" aria-label="Search project commands" placeholder="Search commands…" />
        <button type="button" class="command-picker-close" aria-label="Close command picker" onclick={close}><X size={13} /></button>
      </div>
      <div class="command-picker-scopes">
        <button type="button" disabled={root === null} aria-pressed={!all} onclick={() => { all = false; limit = 60; }}>Current package</button>
        <button type="button" aria-pressed={all} onclick={() => { all = true; limit = 60; }}>Whole project</button>
        {#if !all && root && root !== "."}<span title={root}>{root}</span>{/if}
      </div>
      {#if tasks.catalogError}<CodeOperationNotice message={tasks.catalogError} />{/if}
      <div class="command-picker-list min-h-0 flex-1 overflow-y-auto">
        {#each visible as task, index (task.id)}
          {#if index === 0 || visible[index - 1].kind !== task.kind}<p class="command-picker-heading">{commandKindLabel(task.kind)}</p>{/if}
          <button type="button" data-command title={task.argv.join(" ")} class="command-picker-row" aria-pressed={tasks.selectedTaskId === task.id} onclick={() => select(task.id)}>
            <span class="command-picker-row-title"><span>{task.label}</span>{#if tasks.selectedTaskId === task.id}<Check size={13} />{/if}</span>
            <span class="command-picker-row-detail">{#if all && commandRoot(task) !== "."}{commandRoot(task)} · {/if}{task.argv.join(" ")}{#if task.long_running} · background{/if}</span>
            {#if task.available === false}<span class="command-picker-unavailable">{tasks.taskRepair(task) ?? "Required tool is unavailable on this workshop."}</span>{/if}
          </button>
        {/each}
        {#if matches.length === 0}<p class="px-2 py-5 text-chrome-sm text-content-secondary">{query ? "No commands match this search." : "No commands were discovered in this scope. Try Whole project."}</p>{/if}
        {#if matches.length > limit}<button type="button" class="m-2 rounded px-2 py-1 text-chrome-sm text-content-link" onclick={() => (limit += 60)}>Show more · {matches.length - limit} remaining</button>{/if}
      </div>
      <p class="command-picker-footer">Select, then Run · ↑↓ navigate · Esc closes</p>
    </div>
  </BodyPortal>
{/if}

<style>
  .command-run { display:flex; align-items:center; gap:0; min-width:0; flex-shrink:0; }
  .command-run-button { display:inline-flex; align-items:center; gap:.35rem; min-height:28px; border:0; border-radius:4px; padding:.2rem .45rem; background:transparent; color:rgb(var(--theme-text-secondary)); font-size:12px; cursor:pointer; }
  .command-run-button span { max-width:10rem; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .command-run-button:hover:not(:disabled) { background:rgb(var(--theme-text)/.055); color:rgb(var(--theme-text)); }
  .command-run-button:disabled { opacity:.45; cursor:default; }
  .command-chevron { padding-inline:.25rem; }
  .command-stop { color:rgb(var(--theme-error)); }
  @container code-editor (max-width:1050px) { .command-run-button span { max-width:7rem; } }
  @container code-editor (max-width:600px) { .command-run-button span { max-width:5rem; } }
  .command-picker {
    max-height: min(400px, calc(100dvh - 16px));
    border: 1px solid rgb(var(--theme-border) / .5);
    border-radius: 10px;
    background: rgb(var(--theme-pane));
    color: rgb(var(--theme-text));
    font-size: 12px; line-height: 1.4;
    box-shadow: 0 12px 36px #0004;
  }
  .command-picker-search {
    display: flex; flex-shrink: 0; align-items: center; gap: .5rem;
    margin: .5rem .5rem .25rem; padding: .35rem .5rem;
    border: 1px solid rgb(var(--theme-border) / .45); border-radius: 6px;
    background: rgb(var(--theme-text) / .025); color: rgb(var(--theme-text-tertiary));
  }
  .command-picker-search:focus-within { border-color: rgb(var(--theme-text) / .22); }
  .command-picker-input, .command-picker-input:focus {
    min-width: 0; width: 100%; flex: 1; appearance: none; -webkit-appearance: none;
    padding: .15rem 0; border: 0; border-radius: 0; background: transparent;
    box-shadow: none; outline: none; --tw-ring-shadow: 0 0 #0000;
    color: rgb(var(--theme-text)); font: inherit; font-size: 13px; line-height: 1.5;
  }
  .command-picker-input::placeholder { color: rgb(var(--theme-text-tertiary)); }
  .command-picker-input::-webkit-search-cancel-button, .command-picker-input::-webkit-search-decoration { display: none; }
  .command-picker-close { display: inline-flex; align-items: center; justify-content: center; width: 22px; height: 22px; flex-shrink: 0; border: 0; border-radius: 4px; background: transparent; color: inherit; }
  .command-picker-close:hover { color: rgb(var(--theme-text)); background: rgb(var(--theme-text) / .06); }
  .command-picker-scopes { display: flex; align-items: center; gap: .25rem; padding: .25rem .5rem .5rem; flex-shrink: 0; border-bottom: 1px solid rgb(var(--theme-border) / .3); }
  .command-picker-scopes button { padding: .25rem .5rem; border: 0; border-radius: 4px; background: transparent; color: rgb(var(--theme-text-tertiary)); font-size: 11px; white-space: nowrap; }
  .command-picker-scopes button[aria-pressed="true"], .command-picker-scopes button:hover:not(:disabled) { background: rgb(var(--theme-text) / .07); color: rgb(var(--theme-text)); }
  .command-picker-scopes button:disabled { opacity: .45; }
  .command-picker-scopes > span { min-width: 0; margin-left: auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: rgb(var(--theme-text-tertiary)); font-size: 11px; }
  .command-picker-list { padding: .25rem; overscroll-behavior: contain; scrollbar-width: thin; }
  .command-picker-heading { margin: 0; line-height: 1.4; padding: .5rem .5rem .25rem; color: rgb(var(--theme-text-tertiary)); font-size: 11px; }
  .command-picker-row { display: flex; width: 100%; min-width: 0; flex-direction: column; gap: .2rem; padding: .35rem .5rem; border: 0; border-radius: 5px; text-align: left; background: transparent; font-size: 12px; line-height: 1.4; }
  .command-picker-row:hover, .command-picker-row:focus-visible { background: rgb(var(--theme-text) / .06); }
  .command-picker-row:focus-visible { outline: 1px solid rgb(var(--theme-text) / .25); outline-offset: -1px; }
  .command-picker-row-title { display: flex; align-items: center; gap: .5rem; width: 100%; color: rgb(var(--theme-text)); font-size: 12px; }
  .command-picker-row-title > span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .command-picker-row-detail { width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: rgb(var(--theme-text-tertiary)); font: 11px/1.4 var(--font-family-mono, monospace); }
  .command-picker-unavailable { color: rgb(var(--theme-warning)); font-size: 11px; }
  .command-picker-footer { margin: 0; line-height: 1.4; flex-shrink: 0; padding: .45rem .75rem; border-top: 1px solid rgb(var(--theme-border) / .3); color: rgb(var(--theme-text-tertiary)); font-size: 11px; }
</style>
