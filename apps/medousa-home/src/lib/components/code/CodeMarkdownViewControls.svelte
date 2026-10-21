<script lang="ts">
  import { ChevronDown, Code2, Columns2, Eye, FilePenLine } from "@lucide/svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import type { CodeMarkdownMode } from "$lib/code/codeMarkdownDocument";

  let { mode, disabled = false, onchange }: {
    mode: CodeMarkdownMode;
    disabled?: boolean;
    onchange: (mode: CodeMarkdownMode) => void | Promise<void>;
  } = $props();
  let compactOpen = $state(false);
  function choose(mode: CodeMarkdownMode) { compactOpen = false; void onchange(mode); }
  const views = [
    { value: "source", label: "Source", title: "Edit Markdown source", Icon: Code2 },
    { value: "markdown", label: "Write", title: "Open in Markdown editor", Icon: FilePenLine },
    { value: "preview", label: "Preview", title: "Preview the current draft", Icon: Eye },
  ] as const;
  const selected = $derived(views.find(view => view.value === mode) ?? views[0]);
</script>

<div class="markdown-views" role="group" aria-label="Markdown views">
  <div class="markdown-view-segments">
    {#each views as view (view.value)}
      <button type="button" {disabled} class="markdown-view" aria-label={view.title} title={view.title} aria-pressed={selected.value === view.value} onclick={() => void onchange(view.value)}>
        <view.Icon size={13} strokeWidth={1.75}/><span>{view.label}</span>
      </button>
    {/each}
  </div>
  <div class="markdown-view-compact">
    <OverflowMenu bind:open={compactOpen} label="Document view" panelWidth={160} panelClass="w-40 rounded-lg border border-surface-500/40 bg-surface-950 p-1 shadow-xl">
      {#snippet trigger({ open, toggle })}
        <button type="button" class="markdown-view" {disabled} aria-label={`Document view: ${selected.label}`} aria-haspopup="menu" aria-expanded={open} onclick={toggle}>
          <selected.Icon size={13} strokeWidth={1.75}/><span>{selected.label}</span><ChevronDown size={11}/>
        </button>
      {/snippet}
      {#each views as view (view.value)}
        <button type="button" role="menuitemcheckbox" class="code-chrome-menu-item" aria-checked={selected.value === view.value} onclick={() => choose(view.value)}>{view.label}</button>
      {/each}
    </OverflowMenu>
  </div>
  <button type="button" {disabled} class="markdown-view markdown-split" title="Source and live Markdown preview" aria-label="Source and live Markdown preview" aria-pressed={mode === "split"} onclick={() => void onchange(mode === "split" ? "source" : "split")}><Columns2 size={14} strokeWidth={1.75}/></button>
</div>

<style>
  .markdown-views { display:flex; align-items:center; gap:.25rem; flex-shrink:0; }
  .markdown-view-segments { display:flex; align-items:center; gap:1px; padding:2px; border-radius:6px; background:rgb(var(--theme-text)/.025); }
  .markdown-view { display:inline-flex; align-items:center; justify-content:center; gap:.35rem; min-height:27px; padding:.2rem .45rem; border:0; border-radius:4px; background:transparent; color:rgb(var(--theme-text-secondary)); font-size:12px; white-space:nowrap; cursor:pointer; }
  .markdown-view:hover:not(:disabled) { background:rgb(var(--theme-text)/.055); color:rgb(var(--theme-text)); }
  .markdown-view[aria-pressed="true"] { background:rgb(var(--theme-text)/.08); color:rgb(var(--theme-text)); }
  .markdown-view:disabled { opacity:.45; cursor:default; }
  .markdown-view-compact { display:none; }
  @container code-editor (max-width:1050px) { .markdown-view-segments .markdown-view :global(svg) { display:none; } }
  @container code-editor (max-width:800px) { .markdown-view-segments { display:none; } .markdown-view-compact { display:block; } }
</style>
