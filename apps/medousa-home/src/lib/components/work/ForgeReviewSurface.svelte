<script lang="ts">
  import { FileCode2, LoaderCircle, RotateCcw, ShieldCheck } from "@lucide/svelte";
  import { getReviewFile, type ReviewFileDiff, type ReviewProjection } from "$lib/forge";

  interface Props {
    review: ReviewProjection;
    busy?: boolean;
    onOpenFile: (path: string, line?: number) => void | Promise<void>;
    onRestore: (comparison: ReviewFileDiff) => Promise<void>;
  }

  let { review, busy = false, onOpenFile, onRestore }: Props = $props();
  let selectedPath = $state("");
  let comparison = $state<ReviewFileDiff | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let mode = $state<"inline" | "side">("inline");

  const selectedFile = $derived(
    review.changed_files.find((file) => file.path === selectedPath) ?? null,
  );

  function statusLabel(status: string): string {
    if (status === "added") return "Added";
    if (status === "deleted") return "Deleted";
    if (status === "renamed") return "Renamed";
    if (status === "copied") return "Copied";
    if (status === "type_changed") return "Type changed";
    return "Changed";
  }

  function select(path: string) {
    selectedPath = path;
  }

  function sideRows(diff: ReviewFileDiff) {
    return diff.hunks.flatMap((hunk) => {
      const rows: Array<{
        key: string;
        oldNumber?: number | null;
        newNumber?: number | null;
        oldContent: string;
        newContent: string;
        kind: string;
      }> = [];
      for (let index = 0; index < hunk.lines.length; ) {
        const line = hunk.lines[index];
        if (line.kind === "context") {
          rows.push({
            key: `${hunk.old_start}:${hunk.new_start}:context:${index}`,
            oldNumber: line.old_line,
            newNumber: line.new_line,
            oldContent: line.content,
            newContent: line.content,
            kind: "context",
          });
          index += 1;
          continue;
        }
        const block = [];
        while (index < hunk.lines.length && hunk.lines[index].kind !== "context") {
          block.push(hunk.lines[index]);
          index += 1;
        }
        const deletions = block.filter((entry) => entry.kind === "deletion");
        const additions = block.filter((entry) => entry.kind === "addition");
        for (let offset = 0; offset < Math.max(deletions.length, additions.length); offset += 1) {
          const oldLine = deletions[offset];
          const newLine = additions[offset];
          rows.push({
            key: `${hunk.old_start}:${hunk.new_start}:change:${index}:${offset}`,
            oldNumber: oldLine?.old_line,
            newNumber: newLine?.new_line,
            oldContent: oldLine?.content ?? "",
            newContent: newLine?.content ?? "",
            kind: oldLine && newLine ? "replacement" : oldLine ? "deletion" : "addition",
          });
        }
      }
      return rows;
    });
  }

  async function restore() {
    if (!comparison || busy) return;
    const label = comparison.baseline.exists ? "restore its starting version" : "remove the added file";
    if (!window.confirm(`Reopen this project and ${label}? The reviewed revision stays saved as a recovery point.`)) return;
    await onRestore(comparison);
  }

  $effect(() => {
    const evidence = review.evidence_id;
    const paths = review.changed_files.map((file) => file.path);
    if (!selectedPath || !paths.includes(selectedPath)) selectedPath = paths[0] ?? "";
    if (!evidence || !selectedPath) {
      comparison = null;
      return;
    }
    const path = selectedPath;
    let cancelled = false;
    loading = true;
    error = null;
    void getReviewFile(review.work_id, path)
      .then((result) => {
        if (!cancelled) comparison = result;
      })
      .catch((err) => {
        if (!cancelled) {
          comparison = null;
          error = err instanceof Error ? err.message : String(err);
        }
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });
</script>

<section class="mt-3" aria-label="Change review">
  <div class="grid gap-2 sm:grid-cols-3">
    <div class="rounded-md border border-surface-500/25 bg-surface-950/30 p-2 sm:col-span-2">
      <p class="text-[9px] font-medium uppercase tracking-wider text-surface-500">Outcome</p>
      <p class="mt-1 text-[11px] leading-relaxed text-surface-200">{review.synthesis.outcome}</p>
      <p class="mt-1 text-[10px] text-surface-400">{review.synthesis.status_summary}</p>
    </div>
    <div class="rounded-md border p-2 {review.synthesis.risk === 'low' ? 'border-emerald-500/25 bg-emerald-950/15' : review.synthesis.risk === 'high' ? 'border-rose-500/30 bg-rose-950/20' : 'border-amber-500/30 bg-amber-950/20'}">
      <p class="flex items-center gap-1 text-[9px] font-medium uppercase tracking-wider text-surface-500"><ShieldCheck size={10} />Risk</p>
      <p class="mt-1 text-[10px] leading-relaxed text-surface-300">{review.synthesis.risk_summary}</p>
    </div>
  </div>

  <div class="mt-2 rounded-md border p-2 {review.synthesis.verification?.success ? 'border-emerald-500/25 bg-emerald-950/15' : review.synthesis.verification ? 'border-rose-500/30 bg-rose-950/20' : 'border-surface-500/25 bg-surface-950/30'}">
    <p class="text-[9px] font-medium uppercase tracking-wider text-surface-500">Verification</p>
    {#if review.synthesis.verification}
      <p class="mt-1 text-[11px] {review.synthesis.verification.success ? 'text-emerald-200' : 'text-rose-200'}">
        {review.synthesis.verification.success ? "Passed" : "Needs attention"} · {review.synthesis.verification.label}
      </p>
      <p class="mt-0.5 font-mono text-[9px] text-surface-500">
        {review.synthesis.verification.command.join(" ")}{review.synthesis.verification.duration_ms != null ? ` · ${(review.synthesis.verification.duration_ms / 1000).toFixed(1)}s` : ""}
      </p>
    {:else}
      <p class="mt-1 text-[10px] text-amber-200">No project check was recorded.</p>
    {/if}
  </div>

  {#if review.synthesis.unresolved_issues.length}
    <div class="mt-2 rounded-md border border-amber-500/25 bg-amber-950/15 p-2">
      <p class="text-[10px] font-medium text-amber-100">Before you finish</p>
      <ul class="mt-1 space-y-0.5 text-[10px] text-amber-100/75">
        {#each review.synthesis.unresolved_issues as issue}
          <li>• {issue}</li>
        {/each}
      </ul>
    </div>
  {/if}

  <div class="mt-3 grid min-h-56 overflow-hidden rounded-md border border-surface-500/30 bg-surface-950/25 md:grid-cols-[13rem_minmax(0,1fr)]">
    <div class="border-b border-surface-500/25 md:border-b-0 md:border-r">
      <p class="border-b border-surface-500/20 px-2 py-1.5 text-[9px] font-medium uppercase tracking-wider text-surface-500">
        {review.changed_files.length} changed {review.changed_files.length === 1 ? "file" : "files"}
      </p>
      <div class="max-h-52 overflow-auto md:max-h-80">
        {#each review.changed_files as file (file.path)}
          <button
            type="button"
            class="flex w-full items-center gap-2 border-b border-surface-500/10 px-2 py-1.5 text-left hover:bg-surface-800/50 {selectedPath === file.path ? 'bg-surface-800/70 text-surface-100' : 'text-surface-400'}"
            onclick={() => select(file.path)}
          >
            <FileCode2 size={11} class="shrink-0" />
            <span class="min-w-0 flex-1 truncate font-mono text-[9px]">{file.path}</span>
            <span class="shrink-0 text-[8px] text-surface-500">{statusLabel(file.status)}</span>
          </button>
        {/each}
      </div>
    </div>

    <div class="min-w-0">
      <header class="flex flex-wrap items-center justify-between gap-2 border-b border-surface-500/20 px-2 py-1.5">
        <div class="min-w-0">
          <p class="truncate font-mono text-[10px] text-surface-200">{selectedPath || "No changed files"}</p>
          {#if selectedFile?.old_path}<p class="truncate font-mono text-[8px] text-surface-500">from {selectedFile.old_path}</p>{/if}
        </div>
        <div class="flex items-center gap-1">
          <button type="button" class="rounded px-1.5 py-0.5 text-[9px] {mode === 'inline' ? 'bg-surface-700 text-surface-100' : 'text-surface-500 hover:text-surface-200'}" onclick={() => (mode = "inline")}>Inline</button>
          <button type="button" class="rounded px-1.5 py-0.5 text-[9px] {mode === 'side' ? 'bg-surface-700 text-surface-100' : 'text-surface-500 hover:text-surface-200'}" onclick={() => (mode = "side")}>Side by side</button>
          {#if selectedPath}
            <button type="button" class="rounded px-1.5 py-0.5 text-[9px] text-primary-300 hover:bg-surface-800" onclick={() => void onOpenFile(selectedPath, comparison?.hunks[0]?.new_start ?? 1)}>Open in Code</button>
          {/if}
        </div>
      </header>

      {#if loading}
        <div class="flex min-h-36 items-center justify-center text-[10px] text-surface-500"><LoaderCircle size={12} class="mr-1.5 animate-spin" />Reading exact revisions…</div>
      {:else if error}
        <p class="p-3 text-[10px] text-rose-200">{error}</p>
      {:else if comparison?.binary}
        <div class="p-4 text-center">
          <p class="text-xs text-surface-300">Binary file</p>
          <p class="mt-1 text-[10px] text-surface-500">Starting version: {Math.ceil(comparison.baseline.byte_size / 1024)} KB · Reviewed version: {Math.ceil(comparison.reviewed.byte_size / 1024)} KB</p>
          <p class="mt-2 text-[10px] text-surface-400">The exact versions remain in the saved Git revisions, but a text comparison is not meaningful.</p>
        </div>
      {:else if comparison && mode === "inline"}
        <div class="max-h-80 overflow-auto font-mono text-[9px] leading-4">
          {#each comparison.hunks as hunk (`${hunk.old_start}:${hunk.new_start}`)}
            <div class="sticky top-0 bg-surface-800/95 px-2 py-0.5 text-surface-500">−{hunk.old_start},{hunk.old_count} +{hunk.new_start},{hunk.new_count}</div>
            {#each hunk.lines as line, index (`${line.old_line ?? ""}:${line.new_line ?? ""}:${index}`)}
              <div class="grid grid-cols-[2rem_2rem_minmax(0,1fr)] {line.kind === 'addition' ? 'bg-emerald-950/35 text-emerald-100' : line.kind === 'deletion' ? 'bg-rose-950/35 text-rose-100' : 'text-surface-400'}">
                <span class="select-none px-1 text-right text-surface-600">{line.old_line ?? ""}</span><span class="select-none px-1 text-right text-surface-600">{line.new_line ?? ""}</span><span class="whitespace-pre px-2">{line.kind === "addition" ? "+" : line.kind === "deletion" ? "−" : " "}{line.content}</span>
              </div>
            {/each}
          {/each}
        </div>
      {:else if comparison}
        <div class="max-h-80 overflow-auto font-mono text-[9px] leading-4">
          <div class="sticky top-0 z-10 grid grid-cols-2 border-b border-surface-500/25 bg-surface-900 text-[8px] uppercase tracking-wider text-surface-500"><span class="px-2 py-1">Starting version</span><span class="border-l border-surface-500/25 px-2 py-1">Reviewed version</span></div>
          {#each sideRows(comparison) as row (row.key)}
            <div class="grid grid-cols-2">
              <div class="grid grid-cols-[2rem_minmax(0,1fr)] {row.kind === 'deletion' || row.kind === 'replacement' ? 'bg-rose-950/35 text-rose-100' : 'text-surface-400'}"><span class="select-none px-1 text-right text-surface-600">{row.oldNumber ?? ""}</span><span class="whitespace-pre border-r border-surface-500/20 px-2">{row.oldContent}</span></div>
              <div class="grid grid-cols-[2rem_minmax(0,1fr)] {row.kind === 'addition' || row.kind === 'replacement' ? 'bg-emerald-950/35 text-emerald-100' : 'text-surface-400'}"><span class="select-none px-1 text-right text-surface-600">{row.newNumber ?? ""}</span><span class="whitespace-pre px-2">{row.newContent}</span></div>
            </div>
          {/each}
        </div>
      {/if}

      {#if comparison}
        <footer class="flex flex-wrap items-center justify-between gap-2 border-t border-surface-500/20 px-2 py-1.5">
          <p class="min-w-40 flex-1 text-[9px] text-surface-500">Restoring reopens the project; this reviewed revision remains your recovery point.</p>
          <button type="button" class="flex items-center gap-1 rounded px-2 py-1 text-[9px] text-amber-200 hover:bg-amber-950/30 disabled:opacity-35" disabled={busy || (comparison.binary && comparison.baseline.exists)} onclick={() => void restore()}><RotateCcw size={10} />Restore starting version…</button>
        </footer>
      {/if}
    </div>
  </div>

  <details class="mt-2 rounded-md border border-surface-500/20 bg-surface-950/20 p-2">
    <summary class="cursor-pointer text-[10px] text-surface-400 hover:text-surface-200">Who contributed</summary>
    <div class="mt-2 flex flex-wrap gap-1.5">
      {#each review.attribution as source (source.id)}
        <span class="rounded-full border border-surface-500/25 bg-surface-900/60 px-2 py-0.5 text-[9px] text-surface-300">{source.label} · {source.state}</span>
      {/each}
    </div>
  </details>

  <details class="mt-2 rounded-md border border-surface-500/20 bg-surface-950/20 p-2">
    <summary class="cursor-pointer text-[10px] text-surface-400 hover:text-surface-200">Project timeline</summary>
    <ol class="mt-2 space-y-2 border-l border-surface-500/30 pl-3">
      {#each review.timeline as event (event.id)}
        <li>
          <div class="flex flex-wrap items-baseline justify-between gap-1"><p class="text-[10px] text-surface-200">{event.label}</p><time class="text-[8px] text-surface-600">{new Date(event.at).toLocaleString()}</time></div>
          <p class="text-[9px] text-surface-500">{event.actor_label}{event.detail ? ` · ${event.detail}` : ""}</p>
        </li>
      {/each}
    </ol>
  </details>
</section>
