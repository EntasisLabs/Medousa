<script lang="ts">
  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  const rows = [
    { keys: "Ctrl+B", action: "Toggle left rail" },
    { keys: "Ctrl+; %", action: "Split right" },
    { keys: 'Ctrl+; "', action: "Split down" },
    { keys: "Ctrl+; h/j/k/l", action: "Focus pane" },
    { keys: "Ctrl+; z", action: "Zoom pane" },
    { keys: "Ctrl+; x", action: "Close pane" },
    { keys: "Ctrl+; c", action: "Chat tab here" },
    { keys: "Ctrl+; n/p", action: "Next / prev tab" },
    { keys: "Ctrl+; w", action: "Show tabs" },
    { keys: "Ctrl+; 1–9", action: "Focus pane by index" },
    { keys: "Drag tab", action: "Move tab to another pane" },
  ];
</script>

<button
  type="button"
  class="shell-pane-cheatsheet-backdrop absolute inset-0 z-40 bg-black/40"
  aria-label="Close cheat sheet"
  onclick={onClose}
></button>
<div
  class="shell-pane-cheatsheet absolute left-1/2 top-1/2 z-50 w-[min(22rem,90vw)] -translate-x-1/2 -translate-y-1/2 rounded-lg border border-surface-500/40 bg-surface-900 p-4 shadow-xl"
  role="dialog"
  aria-modal="true"
  aria-label="Pane keyboard shortcuts"
>
  <div class="mb-3 flex items-center justify-between gap-2">
    <h2 class="text-sm font-semibold text-surface-50">Pane shortcuts</h2>
    <button type="button" class="btn btn-sm variant-ghost-surface" onclick={onClose}>
      Close
    </button>
  </div>
  <ul class="space-y-1.5">
    {#each rows as row}
      <li class="flex items-baseline justify-between gap-3 text-xs">
        <kbd class="shrink-0 rounded bg-surface-800 px-1.5 py-0.5 font-mono text-surface-200">
          {row.keys}
        </kbd>
        <span class="text-right text-surface-400">{row.action}</span>
      </li>
    {/each}
  </ul>
  <p class="mt-3 text-[11px] text-surface-500">
    Up to 4 live chats. Pane actions also live in Spotlight (Advanced).
  </p>
</div>
