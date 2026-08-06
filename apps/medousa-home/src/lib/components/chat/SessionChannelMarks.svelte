<script lang="ts">
  import { GitBranch } from "@lucide/svelte";
  import {
    resolveSessionChannelMarks,
    sessionChannelTitle,
  } from "$lib/utils/sessionChannelMarks";

  interface Props {
    originSurface?: string | null;
    hasCodeWork?: boolean;
  }

  let { originSurface = null, hasCodeWork = false }: Props = $props();

  const marks = $derived(
    resolveSessionChannelMarks({
      origin_surface: originSurface,
      has_code_work: hasCodeWork,
    }),
  );
</script>

{#if marks.channel || marks.hasCodeWork}
  <span class="session-channel-marks" aria-hidden="true">
    {#if marks.channel === "vscode"}
      <span class="session-channel-mark" title={sessionChannelTitle("vscode")}>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 16 16"
          width="11"
          height="11"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M3.5 2.5 12.5 8 3.5 13.5Z" />
          <path d="M3.5 2.5 7 8 3.5 13.5" />
        </svg>
      </span>
    {:else if marks.channel === "neovim"}
      <span class="session-channel-mark" title={sessionChannelTitle("neovim")}>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 16 16"
          width="11"
          height="11"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M4 13V3l8 10V3" />
        </svg>
      </span>
    {:else if marks.channel === "obsidian"}
      <span class="session-channel-mark" title={sessionChannelTitle("obsidian")}>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 16 16"
          width="11"
          height="11"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M8 1.75 13.25 8 8 14.25 2.75 8Z" />
        </svg>
      </span>
    {:else if marks.channel === "browser"}
      <span class="session-channel-mark" title={sessionChannelTitle("browser")}>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 16 16"
          width="11"
          height="11"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="8" cy="8" r="5.25" />
          <path d="M2.75 8h10.5M8 2.75c1.6 1.7 1.6 8.8 0 10.5M8 2.75c-1.6 1.7-1.6 8.8 0 10.5" />
        </svg>
      </span>
    {/if}
    {#if marks.hasCodeWork}
      <span class="session-channel-mark" title="Code binding">
        <GitBranch size={11} strokeWidth={2} />
      </span>
    {/if}
  </span>
{/if}

<style>
  .session-channel-marks {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    flex-shrink: 0;
    color: color-mix(in oklab, var(--color-ink) 55%, transparent);
  }

  .session-channel-mark {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 0.75rem;
    height: 0.75rem;
  }

  .session-channel-mark :global(svg) {
    display: block;
  }
</style>
