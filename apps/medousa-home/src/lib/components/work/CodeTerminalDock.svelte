<script lang="ts">
  import { SquareTerminal } from "@lucide/svelte";
  import CodeOperationNotice from "$lib/components/code/CodeOperationNotice.svelte";
  import TerminalPane from "$lib/components/terminal/TerminalPane.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";

  interface Props {
    open: boolean;
    sessionId: string | null;
    busy?: boolean;
    error?: string | null;
    canCreateTerminal?: boolean;
    blockedReason?: string;
    onCreate?: () => void;
    workId: string;
    worktreeRoot?: string | null;
    title?: string;
    onClose: () => void;
    onPopOut?: () => void;
  }

  let {
    open,
    sessionId,
    busy = false,
    error = null,
    canCreateTerminal = false,
    blockedReason = "This project needs an available working folder on the workshop.",
    onCreate,
    workId,
    worktreeRoot = null,
    title = "Terminal",
    onClose,
    onPopOut,
  }: Props = $props();
</script>

{#if open}
  <div class="flex h-full min-h-0 flex-col bg-surface-950">
    <div class="min-h-0 flex-1">
      {#if sessionId}
        {#key sessionId}
          <TerminalPane
            {sessionId}
            {workId}
            executionRuntimeId={undertakings.active?.executionRuntimeId ?? null}
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
          {#if error}<CodeOperationNotice message={error} />{/if}
          <div class="px-4 py-4 text-chrome-sm text-content-secondary">
            {#if busy}<p role="status">Creating your workshop shell…</p>
            {:else}
              <p class="font-medium">{error ? "The shell could not be created." : "No shell is open for this project."}</p>
              <p class="mt-1 text-content-quiet">{canCreateTerminal ? "Open a shell in this project's working folder on the workshop." : blockedReason}</p>
              {#if onCreate}<button type="button" class="mt-3 rounded bg-primary-500/20 px-3 py-1.5 text-primary-100 disabled:opacity-40" disabled={!canCreateTerminal} onclick={onCreate}>{error ? "Retry opening terminal" : "Open terminal"}</button>{/if}
            {/if}
          </div>
        </div>
      {/if}
    </div>
  </div>
{/if}
