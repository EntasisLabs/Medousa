<script lang="ts">
  import { tick } from "svelte";
  import { ChevronDown, Search, X } from "@lucide/svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import { placeToolbarPopover } from "$lib/utils/railPopover";
  import { commandKindLabel, commandRoot, currentCommandRoot, filterCommands } from "$lib/code/codeCommandContext";
  import type { CodeTasksController } from "$lib/code/codeTasksController.svelte";
  let { tasks, path = "" }: { tasks: CodeTasksController; path?: string } = $props();
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
    const place = () => { if (trigger && panel) placeToolbarPopover(trigger, panel, { prefer: "below", align: "end", width: 440, gap: 6, pad: 8 }); };
    void tick().then(() => { place(); input?.focus(); });
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  });
</script>
<button bind:this={trigger} type="button" class="flex max-w-56 items-center gap-2 px-2 text-chrome-sm text-content-secondary hover:bg-primary-500/10" aria-haspopup="dialog" aria-expanded={open} aria-label="Choose project command" disabled={tasks.running || tasks.preparing} onclick={() => (open = !open)}>
  <span class="truncate">{selected?.label ?? "Choose command…"}{#if selected && commandRoot(selected) !== "."}<span class="ml-1 text-content-quiet">· {commandRoot(selected)}</span>{/if}</span><ChevronDown size={12} />
</button>
{#if open}
  <BodyPortal>
    <button type="button" tabindex="-1" class="fixed inset-0 z-[120] cursor-default" aria-label="Dismiss command picker" onclick={close}></button>
    <div bind:this={panel} class="z-[121] flex max-h-[min(36rem,80vh)] flex-col overflow-hidden rounded-lg border border-surface-500/40 bg-surface-950 shadow-2xl" role="dialog" aria-label="Choose project command" tabindex="-1" onkeydown={keydown}>
      <div class="flex items-center gap-2 border-b border-surface-500/25 p-3"><Search size={14} /><input bind:this={input} bind:value={query} oninput={() => (limit = 60)} class="min-w-0 flex-1 bg-transparent text-chrome-md outline-none" aria-label="Search project commands" placeholder="Search commands, packages, or tools…" /><button type="button" aria-label="Close command picker" onclick={close}><X size={14} /></button></div>
      <div class="flex flex-wrap items-center gap-2 border-b border-surface-500/20 px-3 py-2 text-chrome-xs">
        <button type="button" class="rounded px-2 py-1 {all ? 'text-content-quiet' : 'bg-primary-500/20 text-primary-100'}" disabled={root === null} aria-pressed={!all} onclick={() => { all = false; limit = 60; }}>Current package</button>
        <button type="button" class="rounded px-2 py-1 {all ? 'bg-primary-500/20 text-primary-100' : 'text-content-quiet'}" aria-pressed={all} onclick={() => { all = true; limit = 60; }}>Whole project</button>
        <span class="min-w-0 truncate text-content-quiet">{all ? "All discovered packages" : root === "." ? "Project root" : root}</span>
      </div>
      <div class="min-h-0 flex-1 overflow-y-auto p-2">
        {#each visible as task, index (task.id)}
          {#if index === 0 || visible[index - 1].kind !== task.kind}<p class="px-2 pb-1 pt-2 text-chrome-xs font-medium text-content-quiet">{commandKindLabel(task.kind)}</p>{/if}
          <button type="button" data-command class="flex w-full flex-col gap-1 rounded px-2 py-2 text-left hover:bg-surface-800 focus:bg-surface-800 focus:outline focus:outline-1 focus:outline-primary-400" aria-pressed={tasks.selectedTaskId === task.id} onclick={() => select(task.id)}>
            <span class="text-chrome-sm text-content-secondary">{task.label}{#if task.available === false}<span class="ml-2 text-amber-200">Unavailable</span>{/if}</span>
            <span class="text-chrome-xs text-content-quiet">{commandRoot(task) === "." ? "Project root" : commandRoot(task)} · {task.provider}{task.long_running ? " · background" : ""}</span>
            <code class="break-all text-chrome-xs text-content-faint">{task.argv.join(" ")}</code>
            {#if task.available === false}<span class="text-chrome-xs text-amber-200">{tasks.taskRepair(task) ?? "Required tool is unavailable on this workshop."}</span>{/if}
          </button>
        {/each}
        {#if matches.length === 0}<p class="px-2 py-5 text-chrome-sm text-content-quiet">{query ? "No commands match this search." : "No commands were discovered in this scope. Try Whole project."}</p>{/if}
        {#if matches.length > limit}<button type="button" class="m-2 rounded px-2 py-1 text-chrome-sm text-content-link" onclick={() => (limit += 60)}>Show more · {matches.length - limit} remaining</button>{/if}
      </div>
      <p class="border-t border-surface-500/20 px-3 py-2 text-chrome-xs text-content-quiet">Select a command, then Run. Arrow keys navigate; Escape returns to the editor toolbar.</p>
    </div>
  </BodyPortal>
{/if}
