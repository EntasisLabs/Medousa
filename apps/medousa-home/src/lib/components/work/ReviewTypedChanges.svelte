<script lang="ts">
  import { Code2, FlaskConical, Workflow, BookOpen, File } from "@lucide/svelte";
  import { typedChanges, type ChangeFile, type ReviewChangeKind } from "$lib/code/reviewChangeKinds";
  let { files, total = files.length, loading = false, onselect }: { files: ChangeFile[]; total?: number; loading?: boolean; onselect: (kind: ReviewChangeKind | null) => void } = $props();
  const groups = $derived(typedChanges(files));
  const icons = { code: Code2, tests: FlaskConical, ci: Workflow, docs: BookOpen, other: File };
</script>
<div class="typed-changes">
  <div class="heading"><h2>Changes</h2><button type="button" onclick={() => onselect(null)} aria-label="Browse all changed files">{total} {total === 1 ? "file" : "files"}</button></div>
  {#each groups as group (group.kind)}
    {@const Icon = icons[group.kind]}
    <button type="button" class="change-row" onclick={() => onselect(group.kind)} aria-label={`Browse ${group.label.toLowerCase()} changes`}>
      <Icon size={14}/><span class="name">{group.label}</span><span class="count">{group.count}</span>
      {#if group.known}<span class="added">+{group.additions.toLocaleString()}</span><span class="removed">−{group.deletions.toLocaleString()}</span>{:else}<span class="unknown">Stats unavailable</span>{/if}
    </button>
  {/each}
  {#if files.length < total}<p class="quiet">{loading ? "Loading file breakdown…" : `Breakdown covers ${files.length} of ${total} files`}</p>{:else if !total}<p class="quiet">No changed files</p>{/if}
</div>
<style>
  .typed-changes { display:grid; grid-template-columns:14px minmax(0,1fr) 1.25rem auto auto; column-gap:.45rem; }
  .heading { grid-column:1/-1; display:flex; align-items:center; justify-content:space-between; gap:.5rem; margin-bottom:.5rem; }
  h2 { margin:0; font-size:.6875rem; font-weight:500; color:rgb(var(--theme-text-tertiary)); }
  button { border:0; background:transparent; color:inherit; cursor:pointer; font-size:.6875rem; padding:0; }
  .heading button,.count { color:rgb(var(--theme-text-tertiary)); }
  .change-row { display:grid; grid-template-columns:subgrid; grid-column:1/-1; align-items:center; column-gap:.45rem; width:100%; min-height:1.85rem; text-align:left; font-size:.75rem; border-radius:.25rem; font-variant-numeric:tabular-nums; }
  .change-row:hover { background:rgb(var(--theme-text)/.035); }
  .change-row :global(svg) { flex-shrink:0; color:rgb(var(--theme-text-tertiary)); }
  .name { min-width:0; } .count { text-align:right; font-size:.6875rem; } .added { text-align:right; color:rgb(var(--theme-success)); } .removed { text-align:right; color:rgb(var(--theme-error)); }
  .quiet,.unknown { color:rgb(var(--theme-text-tertiary)); font-size:.6875rem; line-height:1.5; } .unknown { grid-column:4/-1; text-align:right; } .quiet { grid-column:1/-1; margin:.4rem 0 0; }
  button:focus-visible { outline:2px solid rgb(var(--theme-link)); outline-offset:3px; }
</style>
