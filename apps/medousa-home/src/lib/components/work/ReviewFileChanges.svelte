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
    onRestore?: (comparison: ReviewFileDiff) => Promise<void>;
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

  let expandedPaths = $state<Set<string>>(new Set());
  let expandedScopeKeys = $state<Set<string>>(new Set());
  let fullDiffPaths = $state<Set<string>>(new Set());
  let pinnedIntentsPath = $state<string | null>(null);
  let hoverIntentsPath = $state<string | null>(null);
  let fileDiffs = $state<Record<string, ReviewFileDiff>>({});
  let fileErrors = $state<Record<string, string>>({});
  let loadingPaths = $state<Set<string>>(new Set());
  let scopesByPath = $state<Record<string, ReviewSymbolScope[]>>({});
  let scopesLoading = $state<Record<string, boolean>>({});
  let density = $state<"comfortable" | "compact">("compact");
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

  const allExpanded = $derived(
    files.length > 0 && files.every((file) => expandedPaths.has(file.path)),
  );

  const anyExpanded = $derived(files.some((file) => expandedPaths.has(file.path)));

  $effect(() => {
    const evidenceId = review.evidence_id ?? null;
    if (evidenceId === resetForEvidenceId) return;
    resetForEvidenceId = evidenceId;
    untrack(() => {
      expandedPaths = new Set();
      expandedScopeKeys = new Set();
      fullDiffPaths = new Set();
      pinnedIntentsPath = null;
      hoverIntentsPath = null;
      fileDiffs = {};
      fileErrors = {};
      loadingPaths = new Set();
      scopesByPath = {};
      scopesLoading = {};
    });
  });

  function basename(path: string): string {
    const parts = path.replaceAll("\\", "/").split("/");
    return parts[parts.length - 1] || path;
  }

  function parentDir(path: string): string {
    const normalized = path.replaceAll("\\", "/");
    const idx = normalized.lastIndexOf("/");
    return idx > 0 ? normalized.slice(0, idx) : "";
  }

  function scopeKey(path: string, scopeId: string): string {
    return `${path}\0${scopeId}`;
  }

  function symbolLabel(file: ReviewFileChange): string | null {
    const count = scopesByPath[file.path]?.length ?? file.symbol_count ?? 0;
    if (count <= 0) {
      if (scopesLoading[file.path]) return "…";
      return null;
    }
    return `${count}`;
  }

  function shortKind(kind: string | null | undefined): string {
    const raw = (kind ?? "").toLowerCase();
    if (raw.includes("fn") || raw.includes("function") || raw.includes("method")) return "fn";
    if (raw.includes("struct") || raw.includes("class") || raw.includes("type")) return "type";
    if (raw.includes("trait") || raw.includes("interface")) return "trait";
    if (raw.includes("mod") || raw.includes("module")) return "mod";
    if (raw.includes("enum")) return "enum";
    if (raw.includes("const")) return "const";
    if (raw.length <= 6 && raw) return raw;
    return "sym";
  }

  function symbolName(label: string): string {
    const trimmed = label.trim();
    if (!trimmed) return label;
    const parts = trimmed.split("::");
    return parts[parts.length - 1] || trimmed;
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
    loadingPaths = new Set(loadingPaths).add(path);
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
      const next = new Set(loadingPaths);
      next.delete(path);
      loadingPaths = next;
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

  async function openFile(path: string) {
    const next = new Set(expandedPaths);
    next.add(path);
    expandedPaths = next;
    const diff = await ensureFileDiff(path);
    if (diff) void enrichScopes(path, diff);
  }

  async function toggleFile(path: string) {
    if (expandedPaths.has(path)) {
      const next = new Set(expandedPaths);
      next.delete(path);
      expandedPaths = next;
      const nextScopes = new Set(
        [...expandedScopeKeys].filter((key) => !key.startsWith(`${path}\0`)),
      );
      expandedScopeKeys = nextScopes;
      const nextFull = new Set(fullDiffPaths);
      nextFull.delete(path);
      fullDiffPaths = nextFull;
      return;
    }
    await openFile(path);
  }

  async function expandAll() {
    expandedPaths = new Set(files.map((file) => file.path));
    await Promise.all(
      files.map(async (file) => {
        const diff = await ensureFileDiff(file.path);
        if (diff) void enrichScopes(file.path, diff);
      }),
    );
  }

  function collapseAll() {
    expandedPaths = new Set();
    expandedScopeKeys = new Set();
    fullDiffPaths = new Set();
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

  function toggleScope(path: string, scopeId: string) {
    const key = scopeKey(path, scopeId);
    const next = new Set(expandedScopeKeys);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    expandedScopeKeys = next;
    if (!expandedPaths.has(path)) {
      void openFile(path);
    }
  }

  function toggleFullDiff(path: string) {
    const next = new Set(fullDiffPaths);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    fullDiffPaths = next;
  }
</script>

{#if files.length === 0}
  <p class="file-empty">No file changes in this revision.</p>
{:else}
  <div class="file-skim-chrome">
    <div class="file-skim-toolbar">
      {#if allExpanded}
        <button type="button" class="file-skim-action" onclick={collapseAll}>Collapse all</button>
      {:else}
        <button type="button" class="file-skim-action" onclick={() => void expandAll()}>
          Expand all
        </button>
        {#if anyExpanded}
          <button type="button" class="file-skim-action" onclick={collapseAll}>Collapse all</button>
        {/if}
      {/if}
    </div>

    <ul class="file-skim" aria-label="Changed files">
      {#each files as file (file.path)}
        {@const intents = intentsFor(file)}
        {@const symbols = symbolLabel(file)}
        {@const open = expandedPaths.has(file.path)}
        {@const showIntents = pinnedIntentsPath === file.path || hoverIntentsPath === file.path}
        {@const dir = parentDir(file.path)}
        <li class="file-row" class:file-row--open={open} class:file-row--risk={riskPaths.has(file.path)}>
          <div class="file-row-main">
            <button
              type="button"
              class="file-expand"
              aria-expanded={open}
              title={intents.length ? intents.join(" · ") : file.path}
              onclick={() => void toggleFile(file.path)}
            >
              <ChevronRight size={12} class="file-chevron {open ? 'file-chevron--open' : ''}" />
              <span class="file-identity">
                <span class="file-path">{basename(file.path)}</span>
                {#if dir}
                  <span class="file-dir">{dir}</span>
                {/if}
              </span>
            </button>

            <div class="file-meta">
              {#if intents.length === 1}
                <span class="file-intent-quiet" title={intents[0]}>{intents[0]}</span>
              {:else if intents.length > 1}
                <button
                  type="button"
                  class="file-intent-quiet file-intent-quiet--btn"
                  aria-expanded={pinnedIntentsPath === file.path}
                  title={intents.join(" · ")}
                  onmouseenter={() => (hoverIntentsPath = file.path)}
                  onmouseleave={() => {
                    if (hoverIntentsPath === file.path) hoverIntentsPath = null;
                  }}
                  onclick={(event) => toggleIntents(file.path, event)}
                >
                  <MessageSquareText size={11} strokeWidth={1.8} />
                  {intents.length}
                </button>
              {/if}
              {#if symbols}
                <span class="file-stat" title="{symbols} {Number(symbols) === 1 ? 'symbol' : 'symbols'}">
                  {symbols}
                  <span class="file-stat-unit">{Number(symbols) === 1 ? "symbol" : "symbols"}</span>
                </span>
              {/if}
              {#if (file.lines_added ?? 0) > 0 || (file.lines_removed ?? 0) > 0}
                <span class="file-add tabular-nums">+{file.lines_added ?? 0}</span>
                <span class="file-del tabular-nums">−{file.lines_removed ?? 0}</span>
              {/if}
            </div>
          </div>

          {#if showIntents && intents.length > 1}
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
              {#if loadingPaths.has(file.path) && !fileDiffs[file.path]}
                <p class="file-loading">Loading changes…</p>
              {:else if fileErrors[file.path]}
                <p class="file-error">{fileErrors[file.path]}</p>
              {:else if fileDiffs[file.path]}
                {@const diff = fileDiffs[file.path]!}
                {@const scopes = scopesByPath[file.path] ?? []}
                {#if scopes.length > 0}
                  <ul class="scope-list" aria-label="Changed symbols">
                    {#each scopes as scope (scope.id)}
                      {@const scopeOpen = expandedScopeKeys.has(scopeKey(file.path, scope.id))}
                      <li class="scope-row">
                        <button
                          type="button"
                          class="scope-expand"
                          aria-expanded={scopeOpen}
                          title={scope.label}
                          onclick={() => toggleScope(file.path, scope.id)}
                        >
                          <ChevronRight
                            size={11}
                            class="file-chevron {scopeOpen ? 'file-chevron--open' : ''}"
                          />
                          <span class="scope-kind">{shortKind(scope.kind)}</span>
                          <span class="scope-label">{symbolName(scope.label)}</span>
                          <span class="file-add tabular-nums">+{scope.lines_added}</span>
                          <span class="file-del tabular-nums">−{scope.lines_removed}</span>
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
                              onRestoreFile={onRestore
                                ? async (path) => {
                                    const comparison = fileDiffs[path];
                                    if (comparison) await onRestore(comparison);
                                  }
                                : undefined}
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
                    onclick={() => toggleFullDiff(file.path)}
                  >
                    {fullDiffPaths.has(file.path) ? "Hide full file diff" : "Show full file diff"}
                  </button>
                  {#if fullDiffPaths.has(file.path)}
                    <DiffStack
                      files={[toStackFile(diff)]}
                      {mode}
                      {density}
                      {wrap}
                      chrome="prefs"
                      collapsedPaths={[]}
                      onToggleCollapsed={() => {}}
                      onOpenFile={(path, line) => onOpenFile(path, line)}
                      onRestoreFile={onRestore
                        ? async (path) => {
                            const comparison = fileDiffs[path];
                            if (comparison) await onRestore(comparison);
                          }
                        : undefined}
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
                    onRestoreFile={onRestore
                      ? async (path) => {
                          const comparison = fileDiffs[path];
                          if (comparison) await onRestore(comparison);
                        }
                      : undefined}
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
  </div>
{/if}

<style>
  .file-empty {
    margin: 0;
    font-size: 0.8rem;
    color: rgb(var(--theme-text-quiet));
  }

  .file-skim-chrome {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .file-skim-toolbar {
    display: flex;
    justify-content: flex-end;
    gap: 0.55rem;
    padding: 0 0.1rem;
  }

  .file-skim-action {
    border: 0;
    background: transparent;
    padding: 0;
    color: rgb(var(--theme-text-secondary));
    font-size: 0.6875rem;
    font-weight: 500;
    cursor: pointer;
  }

  .file-skim-action:hover {
    color: rgb(var(--theme-link));
  }

  .file-skim {
    margin: 0;
    padding: 0;
    list-style: none;
    border: 1px solid rgb(var(--theme-border) / 0.22);
    border-radius: var(--theme-container-radius, 0.55rem);
    background: rgb(var(--theme-card) / calc(var(--theme-pane-alpha, 0.82) * 0.55));
    overflow: hidden;
  }

  .file-row {
    position: relative;
    border-bottom: 1px solid rgb(var(--theme-border) / 0.16);
    color: rgb(var(--theme-text));
    transition: background-color 140ms ease;
  }

  .file-row:last-child {
    border-bottom: 0;
  }

  .file-row:hover {
    background: rgb(var(--color-surface-800) / 0.18);
  }

  .file-row--open {
    background: rgb(var(--color-surface-800) / 0.14);
  }

  .file-row--risk {
    box-shadow: inset 2px 0 0 rgb(var(--theme-warning) / 0.7);
  }

  .file-row-main {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.55rem;
    min-height: 1.7rem;
    padding: 0.22rem 0.55rem;
  }

  .file-expand {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.3rem;
    border: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
    text-align: left;
  }

  :global(.file-chevron) {
    flex-shrink: 0;
    opacity: 0.62;
    transition: transform 140ms ease, opacity 140ms ease;
  }

  :global(.file-chevron--open) {
    transform: rotate(90deg);
    opacity: 0.85;
  }

  .file-identity {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: baseline;
    gap: 0.4rem;
  }

  .file-path {
    flex-shrink: 0;
    font-size: 0.75rem;
    font-weight: 600;
    color: rgb(var(--theme-text));
  }

  .file-dir {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.625rem;
    color: rgb(var(--theme-text-secondary));
  }

  .file-meta {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 0.4rem;
    max-width: 42%;
    font-variant-numeric: tabular-nums;
  }

  .file-intent-quiet {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.625rem;
    color: rgb(var(--theme-text-secondary));
  }

  .file-intent-quiet--btn {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    max-width: none;
    border: 0;
    background: transparent;
    padding: 0;
    color: rgb(var(--theme-text-secondary));
    cursor: pointer;
  }

  .file-intent-quiet--btn:hover {
    color: rgb(var(--theme-text));
  }

  .file-stat {
    font-size: 0.625rem;
    font-weight: 600;
    color: rgb(var(--theme-text-secondary));
  }

  .file-stat-unit {
    margin-left: 0.12rem;
    font-weight: 500;
    opacity: 0.85;
  }

  .file-add {
    font-size: 0.625rem;
    font-weight: 600;
    color: color-mix(
      in srgb,
      rgb(var(--syn-addition-fg)) 72%,
      rgb(var(--theme-text-secondary))
    );
  }

  .file-del {
    font-size: 0.625rem;
    font-weight: 600;
    color: color-mix(
      in srgb,
      rgb(var(--syn-deletion-fg)) 72%,
      rgb(var(--theme-text-secondary))
    );
  }

  .intent-popover {
    position: absolute;
    z-index: 5;
    top: calc(100% - 0.1rem);
    left: 1.35rem;
    width: min(22rem, calc(100% - 2rem));
    padding: 0.55rem 0.65rem;
    border-radius: var(--theme-control-radius, 0.5rem);
    border: 1px solid rgb(var(--theme-border) / 0.4);
    background: rgb(var(--theme-card) / 0.98);
    box-shadow: 0 10px 30px rgb(var(--theme-shadow) / 0.22);
    color: rgb(var(--theme-text));
    animation: intent-in 140ms ease;
  }

  @keyframes intent-in {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .intent-popover-title {
    margin: 0 0 0.35rem;
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--theme-text-faint));
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
    color: rgb(var(--theme-text));
  }

  .file-body {
    padding: 0 0.45rem 0.45rem 1.35rem;
    border-top: 1px solid rgb(var(--theme-border) / 0.12);
    animation: body-in 160ms ease;
  }

  @keyframes body-in {
    from {
      opacity: 0.4;
    }
    to {
      opacity: 1;
    }
  }

  .file-loading,
  .file-error {
    margin: 0.4rem 0.15rem;
    font-size: 0.75rem;
    color: rgb(var(--theme-text-secondary));
  }

  .file-error {
    color: rgb(var(--theme-error));
  }

  .scope-list {
    margin: 0.25rem 0 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 0.05rem;
  }

  .scope-expand {
    display: grid;
    width: 100%;
    grid-template-columns: auto auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 0.35rem;
    padding: 0.18rem 0.2rem;
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    color: inherit;
    cursor: pointer;
    text-align: left;
    transition: background-color 120ms ease;
  }

  .scope-expand:hover {
    background: rgb(var(--color-surface-800) / 0.28);
  }

  .scope-kind {
    font-size: 0.5625rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: rgb(var(--theme-text-quiet));
  }

  .scope-label {
    min-width: 0;
    overflow: hidden;
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 0.6875rem;
    font-weight: 550;
    color: rgb(var(--theme-text));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .scope-diff {
    margin: 0.1rem 0 0.25rem;
  }

  .file-full-diff {
    margin: 0.3rem 0 0.1rem;
    border: 0;
    background: transparent;
    color: color-mix(
      in srgb,
      rgb(var(--theme-link)) 55%,
      rgb(var(--theme-text-secondary))
    );
    font-size: 0.6875rem;
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
  }

  .file-full-diff:hover {
    color: rgb(var(--theme-link));
  }
</style>
