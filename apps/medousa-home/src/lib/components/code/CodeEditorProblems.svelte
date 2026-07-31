<script lang="ts">
  /**
   * Problems list chrome for script editor diagnostics.
   */
  export type CodeEditorProblem = {
    message: string;
    severity: "error" | "warning" | "info" | "hint";
    line: number;
    source?: string;
  };

  interface Props {
    problems?: CodeEditorProblem[];
    onSelect?: (line: number) => void;
    onClose?: () => void;
  }

  let { problems = [], onSelect, onClose }: Props = $props();

  const errors = $derived(problems.filter((p) => p.severity === "error").length);
  const warnings = $derived(problems.filter((p) => p.severity === "warning").length);
</script>

<section
  class="code-editor-problems flex max-h-40 min-h-0 shrink-0 flex-col border-t border-surface-500/40 bg-surface-900/50"
  aria-label="Problems"
>
  <div
    class="flex items-center justify-between border-b border-surface-500/30 px-2 py-1 text-[11px] text-surface-300"
  >
    <span>
      Problems
      {#if problems.length}
        <span class="text-surface-500">
          · {errors} error{errors === 1 ? "" : "s"}, {warnings} warning{warnings === 1
            ? ""
            : "s"}
        </span>
      {/if}
    </span>
    {#if onClose}
      <button
        type="button"
        class="rounded px-1 text-surface-400 hover:bg-surface-700/60"
        onclick={() => onClose?.()}
      >
        Hide
      </button>
    {/if}
  </div>
  <ul class="min-h-0 flex-1 overflow-y-auto text-[12px]">
    {#if problems.length === 0}
      <li class="px-3 py-2 text-surface-400">No problems</li>
    {:else}
      {#each problems as p, i (i + p.message + p.line)}
        <li>
          <button
            type="button"
            class="flex w-full gap-2 px-3 py-1 text-left hover:bg-surface-700/40"
            onclick={() => onSelect?.(p.line)}
          >
            <span
              class="shrink-0 uppercase {p.severity === 'error'
                ? 'text-error-400'
                : p.severity === 'warning'
                  ? 'text-warning-400'
                  : 'text-surface-400'}"
            >
              {p.severity}
            </span>
            <span class="min-w-0 flex-1 truncate text-surface-100">{p.message}</span>
            <span class="shrink-0 text-surface-500">:{p.line}</span>
          </button>
        </li>
      {/each}
    {/if}
  </ul>
</section>
