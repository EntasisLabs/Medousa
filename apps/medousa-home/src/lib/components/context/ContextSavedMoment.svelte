<script lang="ts">
  import { ArrowUpRight, MessageSquare, GitBranch, Folder, Link2 } from "@lucide/svelte";
  import ContextRelatedNotes from "$lib/components/context/ContextRelatedNotes.svelte";
  import ContextMemoryValue from "$lib/components/context/ContextMemoryValue.svelte";
  import type { LocusNodeDetailResponse } from "$lib/types/locus";
  import { formatContextWhen, sessionDisplayName } from "$lib/utils/contextHuman";
  import { contextMemoryPresentation } from "$lib/utils/contextBrowse";
  import { memoryView, memorySourceSession, memoryRelationTarget, memoryEvidenceLabel, memoryFieldHasValue, memoryLabel } from "$lib/utils/contextMemory";
  import { contextThreads } from "$lib/context/contextThreads.svelte";
  import { contextShell } from "$lib/context/contextShell.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";

  let { detail, sessionLabels = {}, compact = false }: {
    detail: LocusNodeDetailResponse;
    sessionLabels?: Record<string, string>;
    compact?: boolean;
  } = $props();
  const view = $derived(memoryView(detail));
  const sourceSession = $derived(memorySourceSession(detail, new Set(chat.sessions.map((session) => session.session_id))));
  const sourceLabel = $derived(sessionDisplayName(sourceSession || detail.node.session_id, sessionLabels));
  const keys = $derived(new Set(contextThreads.nodes.map((node) => node.sync_key)));
  const relations = $derived(view.relations.map((relation) => {
    const key = memoryRelationTarget(relation.target, keys);
    const node = key ? contextThreads.nodes.find((node) => node.sync_key === key) : null;
    return { ...relation, key, title: node ? contextMemoryPresentation(node).title : null };
  }));
  const sections = $derived(view.sections.filter(memoryFieldHasValue));
  function openRelatedMemory(key: string) {
    contextShell.selectMapNode(`thread:${key}`);
    void contextThreads.loadDetail(key);
  }
  async function openSource() {
    if (!sourceSession) return;
    shellTabs.openChat(sourceSession, { activate: true });
    await chat.switchSession(sourceSession);
  }
</script>

<article class="context-saved-moment" class:compact>
  <p class="memory-eyebrow">{view.kind}</p>
  <h2 class="context-memory-heading">{view.title}</h2>
  <div class="context-record-meta">
    <span class="inline-flex items-center gap-1.5" aria-label="Source session"><MessageSquare size={13} aria-hidden="true" />{sourceLabel}</span>
    <time>Saved {formatContextWhen(detail.node.timestamp)}</time>
    {#if sourceSession}<button type="button" class="memory-source-link" onclick={() => void openSource()}>Open source session <ArrowUpRight size={12} /></button>{/if}
  </div>

  {#if view.summary && (view.summary !== view.title || (!sections.length && !view.execution))}
    <section class="memory-section" aria-label="Saved summary">
      <h3>{view.kind === "Decision" ? "Decision" : view.kind === "Handoff" ? "Handoff" : "What was saved"}</h3>
      <ContextMemoryValue value={view.summary} />
    </section>
  {/if}

  {#if view.execution}
    <section class="memory-section memory-execution" aria-label="Recorded activity">
      <h3>Recorded activity</h3>
      <p class="text-sm text-surface-100">{view.execution.tool === "cognition_coder_shell_run" ? "Shell tool execution" : memoryLabel(view.execution.tool)} {view.execution.outcome}.</p>
      {#if view.execution.error}<div class="mt-3 text-sm text-content-warning"><ContextMemoryValue value={view.execution.error} /></div>{/if}
      {#if view.kind === "Verification" && view.execution.outcome === "completed"}<p class="mt-2 text-xs text-content-secondary">This checkpoint records completion of the tool call. A check verdict is not included in its saved details.</p>{/if}
    </section>
  {/if}

  {#each sections as section (section.name)}
    <section class="memory-section" aria-label={section.label}>
      <h3>{section.label}</h3>
      <ContextMemoryValue value={section.value} />
    </section>
  {/each}

  {#if view.project || view.branch}
    <section class="memory-section" aria-label="Working context">
      <h3>Working context</h3>
      <div class="memory-context">
        {#if view.project}<span><Folder size={14} />{view.project}</span>{/if}
        {#if view.branch}<span><GitBranch size={14} /><span class="break-all">{view.branch}</span></span>{/if}
      </div>
      {#if view.head}<p class="mt-2 text-xs text-content-quiet">Recorded against commit <code title={view.head}>{view.head.slice(0, 12)}</code></p>{/if}
    </section>
  {/if}

  {#if view.evidence.length || view.paths.length || view.symbols.length}
    <section class="memory-section" aria-label="Evidence and references">
      <h3>Evidence and references</h3>
      {#if view.evidence.length}
        <p class="mb-3 text-xs text-content-secondary">References saved with this memory</p>
        <ul class="memory-references">{#each view.evidence as reference}<li><Link2 size={14} /><details><summary>{memoryEvidenceLabel(reference)} <span class="text-content-quiet">· {reference.split(":").pop()?.slice(-8)}</span></summary><code class="mt-2 block break-all text-xs text-content-secondary">{reference}</code></details></li>{/each}</ul>
      {/if}
      {#if view.paths.length}<h4 class="mb-2 mt-4 text-xs text-content-secondary">Referenced files</h4><ul class="space-y-2">{#each view.paths as path}<li class="break-all font-mono text-xs text-surface-100">{path}</li>{/each}</ul>{/if}
      {#if view.symbols.length}<h4 class="mb-2 mt-4 text-xs text-content-secondary">Referenced symbols</h4><ul class="space-y-2">{#each view.symbols as symbol}<li class="break-all font-mono text-xs text-surface-100">{symbol}</li>{/each}</ul>{/if}
    </section>
  {/if}

  {#if relations.length}
    <section class="memory-section" aria-label="Connected memory">
      <h3>Connected memory</h3>
      <ul class="space-y-3">{#each relations as relation}
        <li><span class="mb-1 block text-xs text-content-secondary">{memoryLabel(relation.relation)}</span>
          {#if relation.key}<button type="button" class="memory-connection" onclick={() => openRelatedMemory(relation.key!)}>{relation.title}<ArrowUpRight size={13} /></button>
          {:else}<details class="text-xs text-content-quiet"><summary>Referenced memory outside the loaded list</summary><code class="mt-2 block break-all">{relation.target}</code></details>{/if}
        </li>
      {/each}</ul>
    </section>
  {/if}

  <ContextRelatedNotes sessionId={sourceSession || detail.node.session_id} />
  {#key detail.node.sync_key}
    <details class="context-record-details">
      <summary>Record details</summary>
      <dl class="mt-4 space-y-3">
        <div><dt>Memory session</dt><dd class="mt-1 break-all font-mono text-[11px]">{detail.node.session_id}</dd></div>
        <div><dt>Record ID</dt><dd class="mt-1 break-all font-mono text-[11px]">{detail.node.sync_key}</dd></div>
      </dl>
      {#if view.compressionDepth !== null}<p class="mt-3">Compression depth: {view.compressionDepth}</p>{/if}
      {#if view.parentNodeId}<p class="mt-3 break-all">Parent record: {view.parentNodeId}</p>{/if}
      {#if !sourceSession}<p class="mt-3">The source session is not in the loaded chat list.</p>{/if}
      {#if view.technicalFields.length}
        <dl class="mt-4 space-y-3">{#each view.technicalFields as field}<div><dt>{memoryLabel(field.name)}</dt><dd class="mt-1 break-all text-[11px]"><ContextMemoryValue value={field.value} /></dd></div>{/each}</dl>
      {/if}
      <p class="mb-2 mt-5">Original saved record</p>
      <pre class="max-h-96 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-surface-900 p-3 text-[11px] leading-relaxed">{detail.raw || view.original || "No additional content was stored."}</pre>
    </details>
  {/key}
</article>

<style>
  .memory-eyebrow { font-size: 11px; text-transform: uppercase; letter-spacing: .1em; color: rgb(var(--theme-text-quiet)); }
  .context-memory-heading { margin: .8rem 0 1rem; font-size: clamp(1.45rem, 2.4vw, 2rem); line-height: 1.25; letter-spacing: -.035em; font-weight: 500; color: rgb(var(--color-surface-100)); overflow-wrap: anywhere; }
  .compact .context-memory-heading { font-size: 1.15rem; }
  .context-record-meta { display: flex; align-items: center; flex-wrap: wrap; gap: .5rem 1rem; font-size: 12px; color: rgb(var(--theme-text-secondary)); }
  .memory-source-link, .memory-connection { display: inline-flex; align-items: center; gap: .4rem; color: rgb(var(--color-surface-100)); }
  .memory-source-link:hover, .memory-connection:hover { text-decoration: underline; }
  .memory-section { margin-top: 1.75rem; color: rgb(var(--color-surface-100)); font-size: 14px; }
  .memory-section h3 { margin-bottom: .65rem; font-size: 12px; font-weight: 500; color: rgb(var(--theme-text-secondary)); }
  .memory-execution { padding: 1rem 1.15rem; background: rgb(var(--color-surface-800) / .35); border: 1px solid rgb(var(--color-surface-700) / .3); border-radius: .65rem; }
  .memory-context { display: flex; flex-wrap: wrap; gap: .6rem 1.2rem; font-size: 13px; }
  .memory-context > span { display: inline-flex; align-items: center; gap: .45rem; min-width: 0; }
  .memory-context :global(svg) { flex-shrink: 0; color: rgb(var(--theme-text-quiet)); }
  .memory-references { display: grid; gap: .75rem; }
  .memory-references li { display: flex; align-items: baseline; gap: .6rem; font-size: 12px; }
  .memory-references :global(svg) { flex-shrink: 0; color: rgb(var(--theme-text-quiet)); }
  .context-record-details { margin-top: 2.5rem; padding-top: 1.15rem; border-top: 1px solid rgb(var(--color-surface-700) / .3); font-size: 12px; color: rgb(var(--theme-text-secondary)); }
</style>
