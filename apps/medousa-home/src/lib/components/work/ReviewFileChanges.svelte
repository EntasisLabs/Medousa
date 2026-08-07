<script lang="ts">
  import { ChevronRight, MessageSquareText } from "@lucide/svelte";
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import { countDiffStats, type DiffFileSection } from "$lib/diff/diffTypes";
  import {
    getReviewFile,
    getWorldAtLocation,
    type ReviewFileChange,
    type ReviewFileDiff,
    type ReviewProjection,
    type ReviewSymbolScope,
  } from "$lib/forge";
  import { untrack } from "svelte";

  interface Props {
    review: ReviewProjection;
    busy?: boolean;
    onOpenFile: (path: string, line?: number) => void | Promise<void>;
    onRestore: (comparison: ReviewFileDiff) => Promise<void>;
    onComment?: (input: {
      path: string;
      side: "new" | "old";
      line: number;
      content: string;
    }) => void;
  }

  let {
    review,
    busy = false,
    onOpenFile,
    onRestore,
    onComment,
  }: Props = $props();

  let expandedPath = $state<string | null>(null);
  let expandedScopeId = $state<string | null>(null);
  let pinnedIntentsPath = $state<string | null>(null);
  let hoverIntentsPath = $state<string | null>(null);
  let fileDiffs = $state<Record<string, ReviewFileDiff>>({});
  let fileErrors = $state<Record<string, string>>({});
  let loadingPath = $state<string | null>(null);
  let scopesByPath = $state<Record<string, ReviewSymbolScope[]>>({});
  let scopesLoading = $state<Record<string, boolean>>({});
  let density = $state<"comfortable" | "compact">("comfortable");
  let wrap = $state(false);
  let mode = $state<"inline" | "side">("inline");
  /** Non-reactive latch so seal changes reset local UI without an effect loop. */
  let resetForEvidenceId: string | null | undefined = undefined;

  const riskPaths = $derived.by(() => {
    const paths = new Set<string>();
    for (const violation of review.policy?.violations ?? []) {
      if (violation.path) paths.add(violation.path);
    }
    for (const risk of review.policy?.capture_risks ?? []) {
      if ("path" in risk && typeof risk.path === "string") paths.add(risk.path);
    }
    return paths;
  });

  const files = $derived.by(() => {
    const list = [...review.changed_files];
    return list.sort((a, b) => {
      const ar = riskPaths.has(a.path) ? 0 : 1;
      const br = riskPaths.has(b.path) ? 0 : 1;
      if (ar !== br) return ar - br;
      return a.path.localeCompare(b.path);
    });
  });

  $effect(() => {
    const evidenceId = review.evidence_id ?? null;
    if (evidenceId === resetForEvidenceId) return;
    resetForEvidenceId = evidenceId;
    untrack(() => {
      expandedPath = null;
      expandedScopeId = null;
      pinnedIntentsPath = null;
      hoverIntentsPath = null;
      fileDiffs = {};
      fileErrors = {};
      scopesByPath = {};
      scopesLoading = {};
    });
  });

  function basename(path: string): string {
    const parts = path.replaceAll("\\", "/").split("/");
    return parts[parts.length - 1] || path;
  }

  function symbolLabel(file: ReviewFileChange): string | null {
    const count = scopesByPath[file.path]?.length ?? file.symbol_count ?? 0;
    if (count <= 0) {
      if (scopesLoading[file.path]) return "indexing…";
      return null;
    }
    return `${count} ${count === 1 ? "symbol" : "symbols"}`;
  }

  function intentsFor(file: ReviewFileChange): string[] {
    return file.intents?.filter(Boolean) ?? [];
  }

  function toggleIntents(path: string, event: MouseEvent) {
    event.stopPropagation();
    pinnedIntentsPath = pinnedIntentsPath === path ? null : path;
    hoverIntentsPath = null;
  }

  async function ensureFileDiff(path: string): Promise<ReviewFileDiff | null> {
    if (fileDiffs[path]) return fileDiffs[path]!;
    loadingPath = path;
    try {
      const diff = await getReviewFile(review.work_id, path, review.attempt_id ?? undefined);
      fileDiffs = { ...fileDiffs, [path]: diff };
      const nextErrors = { ...fileErrors };
      delete nextErrors[path];
      fileErrors = nextErrors;
      return diff;
    } catch (err) {
      fileErrors = {
        ...fileErrors,
        [path]: err instanceof Error ? err.message : String(err),
      };
      return null;
    } finally {
      if (loadingPath === path) loadingPath = null;
    }
  }

  async function enrichScopes(path: string, diff: ReviewFileDiff) {
    if (diff.binary) return;
    const already = untrack(() => scopesByPath[path]?.length ?? 0);
    if (already > 0) return;
    const existing = review.changed_files.find((file) => file.path === path)?.scopes;
    if (existing && existing.length > 0) {
      scopesByPath = { ...untrack(() => scopesByPath), [path]: existing };
      return;
    }
    scopesLoading = { ...untrack(() => scopesLoading), [path]: true };
    const sealed = review.world?.sealed;
    const snapshot =
      sealed?.world && sealed.version
        ? { world: sealed.world, version: sealed.version }
        : null;
    try {
      const probeLines = new Set<number>();
      for (const hunk of diff.hunks) {
        for (const line of hunk.lines) {
          if (line.kind === "addition" && line.new_line != null) {
            probeLines.add(line.new_line);
          } else if (line.kind === "deletion" && line.old_line != null) {
            probeLines.add(line.old_line);
          }
        }
      }
      const samples = [...probeLines].sort((a, b) => a - b).slice(0, 24);
      const byKey = new Map<string, ReviewSymbolScope>();
      await Promise.all(
        samples.map(async (line) => {
          try {
            const result = await getWorldAtLocation(review.work_id, path, line, snapshot);
            const entity = result.entity;
            if (!entity?.label || entity.line_start == null || entity.line_end == null) return;
            const id = entity.id || `${entity.label}:${entity.line_start}-${entity.line_end}`;
            if (byKey.has(id)) return;
            const scopeHunks = diff.hunks.filter((hunk) =>
              hunk.lines.some((entry) => {
                const n = entry.new_line ?? entry.old_line;
                return (
                  n != null &&
                  n >= (entity.line_start ?? 0) &&
                  n <= (entity.line_end ?? 0)
                );
              }),
            );
            if (scopeHunks.length === 0) return;
            const stats = countDiffStats(scopeHunks);
            byKey.set(id, {
              id,
              label: entity.label,
              kind: entity.kind,
              line_start: entity.line_start,
              line_end: entity.line_end,
              entity_id: entity.id,
              lines_added: stats.additions,
              lines_removed: stats.deletions,
              intents: [],
            });
          } catch {
            /* World may be indexing or unavailable */
          }
        }),
      );
      scopesByPath = { ...untrack(() => scopesByPath), [path]: [...byKey.values()] };
    } finally {
      const next = { ...untrack(() => scopesLoading) };
      delete next[path];
      scopesLoading = next;
    }
  }

  async function toggleFile(path: string) {
    if (expandedPath === path) {
      expandedPath = null;
      expandedScopeId = null;
      return;
    }
    expandedPath = path;
    expandedScopeId = null;
    const diff = await ensureFileDiff(path);
    if (diff) void enrichScopes(path, diff);
  }

  function toStackFile(diff: ReviewFileDiff, scope?: ReviewSymbolScope | null): DiffFileSection {
    const stats = countDiffStats(diff.hunks);
    let hunks = diff.hunks;
    if (scope) {
      hunks = diff.hunks.filter((hunk) =>
        hunk.lines.some((line) => {
          const n = line.new_line ?? line.old_line;
          return n != null && n >= scope.line_start && n <= scope.line_end;
        }),
      );
    }
    const scoped = scope ? countDiffStats(hunks) : stats;
    return {
      id: scope ? `${diff.path}::${scope.id}` : diff.path,
      path: diff.path,
      oldPath: diff.old_path,
      status: diff.status,
      binary: diff.binary,
      additions: scoped.additions,
      deletions: scoped.deletions,
      hunks,
      baselineBytes: diff.baseline.byte_size,
      reviewedBytes: diff.reviewed.byte_size,
      baselineExists: diff.baseline.exists,
      reviewedExists: diff.reviewed.exists,
      beforeText: diff.baseline.content ?? null,
      afterText: diff.reviewed.content ?? null,
    };
  }

  function openScope(path: string, scopeId: string) {
    expandedScopeId = expandedScopeId === scopeId ? null : scopeId;
    expandedPath = path;
  }
</script>

{#if files.length === 0}
  <p class="file-empty">No file changes in this revision.</p>
{:else}
  <ul class="file-skim" aria-label="Changed files">
    {#each files as file (file.path)}
      {@const intents = intentsFor(file)}
      {@const symbols = symbolLabel(file)}
      {@const open = expandedPath === file.path}
      {@const showIntents = pinnedIntentsPath === file.path || hoverIntentsPath === file.path}
      <li class="file-row" class:file-row--open={open} class:file-row--risk={riskPaths.has(file.path)}>
        <div class="file-row-main">
          <button
            type="button"
            class="file-expand"
            aria-expanded={open}
            onclick={() => void toggleFile(file.path)}
          >
            <ChevronRight size={13} class="file-chevron {open ? 'file-chevron--open' : ''}" />
            <span class="file-path" title={file.path}>{basename(file.path)}</span>
            <span class="file-dir">{file.path.includes("/") ? file.path.slice(0, file.path.lastIndexOf("/")) : ""}</span>
          </button>
          <div class="file-stats">
            {#if symbols}
              <span class="file-stat">{symbols}</span>
            {/if}
            {#if (file.lines_added ?? 0) > 0 || (file.lines_removed ?? 0) > 0}
              <span class="file-add">+{file.lines_added ?? 0}</span>
              <span class="file-del">−{file.lines_removed ?? 0}</span>
            {/if}
            {#if intents.length > 0}
              <button
                type="button"
                class="file-intents"
                aria-expanded={pinnedIntentsPath === file.path}
                aria-label={intents.length === 1 ? "Show intent" : `Show ${intents.length} intents`}
                onmouseenter={() => (hoverIntentsPath = file.path)}
                onmouseleave={() => {
                  if (hoverIntentsPath === file.path) hoverIntentsPath = null;
                }}
                onclick={(event) => toggleIntents(file.path, event)}
              >
                <MessageSquareText size={11} strokeWidth={1.8} />
                {#if intents.length === 1}
                  <span class="file-intent-peek">{intents[0]}</span>
                {:else}
                  <span>{intents.length} intents</span>
                {/if}
              </button>
            {/if}
          </div>
        </div>

        {#if showIntents && intents.length}
          <div
            class="intent-popover"
            role="dialog"
            tabindex="-1"
            aria-label="Edit intents"
            onmouseenter={() => (hoverIntentsPath = file.path)}
            onmouseleave={() => {
              if (hoverIntentsPath === file.path && pinnedIntentsPath !== file.path) {
                hoverIntentsPath = null;
              }
            }}
          >
            <p class="intent-popover-title">Why this file changed</p>
            <ol>
              {#each intents as intent, index (`${file.path}:${index}`)}
                <li>{intent}</li>
              {/each}
            </ol>
          </div>
        {/if}

        {#if open}
          <div class="file-body">
            {#if loadingPath === file.path && !fileDiffs[file.path]}
              <p class="file-loading">Loading changes…</p>
            {:else if fileErrors[file.path]}
              <p class="file-error">{fileErrors[file.path]}</p>
            {:else if fileDiffs[file.path]}
              {@const diff = fileDiffs[file.path]!}
              {@const scopes = scopesByPath[file.path] ?? []}
              {#if scopes.length > 0}
                <ul class="scope-list" aria-label="Changed symbols">
                  {#each scopes as scope (scope.id)}
                    {@const scopeOpen = expandedScopeId === scope.id}
                    <li class="scope-row">
                      <button
                        type="button"
                        class="scope-expand"
                        aria-expanded={scopeOpen}
                        onclick={() => openScope(file.path, scope.id)}
                      >
                        <ChevronRight
                          size={12}
                          class="file-chevron {scopeOpen ? 'file-chevron--open' : ''}"
                        />
                        <span class="scope-kind">{scope.kind}</span>
                        <span class="scope-label">{scope.label}</span>
                        <span class="file-add">+{scope.lines_added}</span>
                        <span class="file-del">−{scope.lines_removed}</span>
                      </button>
                      {#if scopeOpen}
                        <div class="scope-diff">
                          <DiffStack
                            files={[toStackFile(diff, scope)]}
                            {mode}
                            {density}
                            {wrap}
                            chrome="none"
                            collapsedPaths={[]}
                            onToggleCollapsed={() => {}}
                            onOpenFile={(path, line) => onOpenFile(path, line)}
                            onRestoreFile={async (path) => {
                              const comparison = fileDiffs[path];
                              if (comparison) await onRestore(comparison);
                            }}
                            onComment={onComment
                              ? (input) =>
                                  onComment({
                                    path: input.path,
                                    side: input.side,
                                    line: input.line,
                                    content: input.content,
                                  })
                              : undefined}
                          />
                        </div>
                      {/if}
                    </li>
                  {/each}
                </ul>
                <button
                  type="button"
                  class="file-full-diff"
                  disabled={busy}
                  onclick={() => (expandedScopeId = expandedScopeId === "__full__" ? null : "__full__")}
                >
                  {expandedScopeId === "__full__" ? "Hide full file diff" : "Show full file diff"}
                </button>
                {#if expandedScopeId === "__full__"}
                  <DiffStack
                    files={[toStackFile(diff)]}
                    {mode}
                    {density}
                    {wrap}
                    chrome="prefs"
                    collapsedPaths={[]}
                    onToggleCollapsed={() => {}}
                    onOpenFile={(path, line) => onOpenFile(path, line)}
                    onRestoreFile={async (path) => {
                              const comparison = fileDiffs[path];
                              if (comparison) await onRestore(comparison);
                            }}
                    onComment={onComment
                      ? (input) =>
                          onComment({
                            path: input.path,
                            side: input.side,
                            line: input.line,
                            content: input.content,
                          })
                      : undefined}
                  />
                {/if}
              {:else}
                <DiffStack
                  files={[toStackFile(diff)]}
                  {mode}
                  {density}
                  {wrap}
                  chrome="prefs"
                  collapsedPaths={[]}
                  onToggleCollapsed={() => {}}
                  onOpenFile={(path, line) => onOpenFile(path, line)}
                  onRestoreFile={async (path) => {
                              const comparison = fileDiffs[path];
                              if (comparison) await onRestore(comparison);
                            }}
                  onComment={onComment
                    ? (input) =>
                        onComment({
                          path: input.path,
                          side: input.side,
                          line: input.line,
                          content: input.content,
                        })
                    : undefined}
                />
              {/if}
            {/if}
          </div>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .file-empty {
    margin: 0;
    font-size: 0.8rem;
    color: var(--color-content-quiet, #8a8580);
  }

  .file-skim {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .file-row {
    position: relative;
    border: 1px solid color-mix(in oklab, var(--syn-border, #333) 85%, transparent);
    border-radius: 0.55rem;
    background: color-mix(in oklab, var(--syn-bg-elevated, #161616) 55%, transparent);
  }

  .file-row--risk {
    border-color: color-mix(in oklab, var(--color-warning-600, #c9893a) 45%, transparent);
  }

  .file-row-main {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.35rem 0.75rem;
    padding: 0.45rem 0.55rem;
  }

  .file-expand {
    display: flex;
    min-width: 0;
    flex: 1 1 12rem;
    align-items: baseline;
    gap: 0.35rem;
    border: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
    text-align: left;
  }

  :global(.file-chevron) {
    flex-shrink: 0;
    opacity: 0.55;
    transition: transform 120ms ease;
  }

  :global(.file-chevron--open) {
    transform: rotate(90deg);
  }

  .file-path {
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--color-content-primary, #eee);
  }

  .file-dir {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.68rem;
    color: var(--color-content-quiet, #8a8580);
  }

  .file-stats {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem 0.5rem;
  }

  .file-stat {
    font-size: 0.68rem;
    color: var(--color-content-secondary, #b8b4ae);
  }

  .file-add {
    font-size: 0.68rem;
    font-weight: 600;
    color: color-mix(in oklab, var(--syn-addition-fg, #3f9c6b) 90%, white);
  }

  .file-del {
    font-size: 0.68rem;
    font-weight: 600;
    color: color-mix(in oklab, var(--syn-deletion-fg, #c45c5c) 90%, white);
  }

  .file-intents {
    display: inline-flex;
    max-width: 16rem;
    align-items: center;
    gap: 0.3rem;
    padding: 0.15rem 0.45rem;
    border-radius: 999px;
    border: 1px solid color-mix(in oklab, var(--syn-border, #333) 80%, transparent);
    background: color-mix(in oklab, var(--syn-bg, #111) 70%, transparent);
    color: var(--color-content-secondary, #b8b4ae);
    font-size: 0.68rem;
    cursor: pointer;
  }

  .file-intent-peek {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .intent-popover {
    position: absolute;
    z-index: 5;
    top: calc(100% - 0.15rem);
    right: 0.55rem;
    left: auto;
    width: min(22rem, calc(100% - 1rem));
    padding: 0.55rem 0.65rem;
    border-radius: 0.5rem;
    border: 1px solid color-mix(in oklab, var(--syn-border, #444) 90%, transparent);
    background: color-mix(in oklab, var(--syn-bg-elevated, #1a1a1a) 96%, black);
    box-shadow: 0 10px 30px rgb(0 0 0 / 0.35);
  }

  .intent-popover-title {
    margin: 0 0 0.35rem;
    font-size: 0.65rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--color-content-quiet, #8a8580);
  }

  .intent-popover ol {
    margin: 0;
    padding-left: 1.1rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .intent-popover li {
    font-size: 0.75rem;
    line-height: 1.4;
    color: var(--color-content-primary, #eee);
  }

  .file-body {
    padding: 0 0.45rem 0.55rem;
    border-top: 1px solid color-mix(in oklab, var(--syn-border, #333) 70%, transparent);
  }

  .file-loading,
  .file-error {
    margin: 0.55rem 0.2rem;
    font-size: 0.75rem;
    color: var(--color-content-quiet, #8a8580);
  }

  .file-error {
    color: var(--color-error-600, #c45c5c);
  }

  .scope-list {
    margin: 0.45rem 0 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .scope-expand {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.4rem;
    padding: 0.35rem 0.3rem;
    border: 0;
    border-radius: 0.35rem;
    background: transparent;
    color: inherit;
    cursor: pointer;
    text-align: left;
  }

  .scope-expand:hover {
    background: color-mix(in oklab, var(--syn-bg, #111) 50%, transparent);
  }

  .scope-kind {
    font-size: 0.62rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-content-quiet, #8a8580);
  }

  .scope-label {
    flex: 1;
    min-width: 0;
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 0.75rem;
    font-weight: 550;
  }

  .scope-diff {
    margin: 0.15rem 0 0.35rem;
  }

  .file-full-diff {
    margin: 0.45rem 0 0.25rem;
    border: 0;
    background: transparent;
    color: var(--color-content-secondary, #b8b4ae);
    font-size: 0.7rem;
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
  }
</style>
