<script lang="ts">
  import { ChevronDown, SquareTerminal, X } from "@lucide/svelte";
  import TerminalPane from "$lib/components/terminal/TerminalPane.svelte";

  interface Props {
    open: boolean;
    sessionId: string | null;
    workId: string;
    worktreeRoot?: string | null;
    title?: string;
    onClose: () => void;
    onPopOut?: () => void;
  }

  let {
    open,
    sessionId,
    workId,
    worktreeRoot = null,
    title = "Terminal",
    onClose,
    onPopOut,
  }: Props = $props();
</script>

{#if open}
  <div class="flex h-52 shrink-0 flex-col border-t border-surface-500/35 bg-[#0c0a09] sm:h-56">
    <div class="flex shrink-0 items-center justify-between gap-2 border-b border-white/10 px-2 py-0.5">
      <span class="flex items-center gap-1.5 text-[10px] text-white">
        <SquareTerminal size={11} />
        {title}
      </span>
      <div class="flex items-center gap-0.5">
        {#if onPopOut}
          <button
            type="button"
            class="rounded px-1.5 py-0.5 text-[9px] text-white hover:bg-white/10 hover:text-white"
            onclick={onPopOut}
            title="Open as a full Terminal tab"
          >Pop out</button>
        {/if}
        <button
          type="button"
          class="rounded p-1 text-white hover:bg-white/10 hover:text-white"
          aria-label="Collapse terminal"
          title="Collapse terminal"
          onclick={onClose}
        ><ChevronDown size={12} /></button>
        <button
          type="button"
          class="rounded p-1 text-white hover:bg-white/10 hover:text-white"
          aria-label="Close terminal"
          onclick={onClose}
        ><X size={12} /></button>
      </div>
    </div>
    <div class="min-h-0 flex-1">
      {#if sessionId}
        {#key sessionId}
          <TerminalPane {sessionId} {workId} {title} {worktreeRoot} compact />
        {/key}
      {:else}
        <p class="px-3 py-4 text-[10px] text-white">Opening workshop shell…</p>
      {/if}
    </div>
  </div>
{/if}
