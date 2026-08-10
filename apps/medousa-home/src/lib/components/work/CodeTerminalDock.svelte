<script lang="ts">
  import { SquareTerminal } from "@lucide/svelte";
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
    <div class="min-h-0 flex-1">
      {#if sessionId}
        {#key sessionId}
          <TerminalPane
            {sessionId}
            {workId}
            {title}
            {worktreeRoot}
            compact
            onPopOut={onPopOut}
            onCollapse={onClose}
          />
        {/key}
      {:else}
        <div class="flex h-full flex-col">
          <div class="flex shrink-0 items-center gap-1.5 border-b border-white/10 px-2 py-0.5">
            <SquareTerminal size={11} class="text-white/70" />
            <span class="truncate text-chrome-sm text-white">{title}</span>
          </div>
          <p class="px-3 py-4 text-chrome-sm text-white">Opening workshop shell…</p>
        </div>
      {/if}
    </div>
  </div>
{/if}
