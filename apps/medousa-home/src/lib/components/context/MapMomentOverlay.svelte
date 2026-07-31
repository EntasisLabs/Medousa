<script lang="ts">
  import type { LocusNodeDetailResponse } from "$lib/types/locus";
  import {
    formatContextWhen,
    humanMomentTitle,
    momentHeadline,
    momentKeptProse,
  } from "$lib/utils/contextHuman";
  import { AVEC_DIMENSIONS } from "$lib/utils/contextPosture";
  import { X } from "@lucide/svelte";

  interface Props {
    detail: LocusNodeDetailResponse;
    chatSessionAvailable?: boolean;
    onOpenChat?: () => void;
    onClear?: () => void;
  }

  let {
    detail,
    chatSessionAvailable = false,
    onOpenChat,
    onClear,
  }: Props = $props();

  const when = $derived(formatContextWhen(detail.node.timestamp));
  const title = $derived(humanMomentTitle(detail.node));
  const kept = $derived(
    momentKeptProse(detail.raw, detail.node.context_summary, title, 220),
  );
  const headline = $derived(momentHeadline(detail.node.user_avec, kept, title));
  const userAvec = $derived(detail.node.user_avec ?? null);
  const showKept = $derived(
    Boolean(kept && kept !== headline && !headline.startsWith(kept.slice(0, 24))),
  );
</script>

<article
  class="map-moment-card"
  role="region"
  aria-label="Moment"
  onpointerdown={(event) => event.stopPropagation()}
>
  {#if onClear}
    <button
      type="button"
      class="map-moment-card-dismiss"
      aria-label="Clear focus"
      onclick={onClear}
    >
      <X size={14} strokeWidth={1.75} />
    </button>
  {/if}

  <p class="map-moment-card-when">{when}</p>
  <h2 class="map-moment-card-headline">{headline}</h2>

  {#if showKept}
    <p class="map-moment-card-kept">{kept}</p>
  {/if}

  {#if userAvec}
    <div class="map-moment-pulse" aria-label="How you showed up">
      {#each AVEC_DIMENSIONS as dim (dim.key)}
        {@const value = userAvec[dim.key]}
        <div class="map-moment-pulse-row">
          <span class="map-moment-pulse-label">{dim.label}</span>
          <span class="map-moment-pulse-track" aria-hidden="true">
            <span
              class="map-moment-pulse-fill"
              style="width: {Math.min(100, Math.max(0, value * 100))}%"
            ></span>
          </span>
        </div>
      {/each}
    </div>
  {/if}

  {#if chatSessionAvailable && onOpenChat}
    <button type="button" class="map-moment-card-chat" onclick={onOpenChat}>
      Open chat
    </button>
  {/if}
</article>

<style>
  .map-moment-card {
    position: relative;
    width: 100%;
    padding: 1.1rem 1.15rem 1rem;
    border-radius: 1rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.22);
    background: rgb(var(--color-surface-950, var(--color-surface-900)) / 0.72);
    backdrop-filter: blur(16px) saturate(1.1);
    box-shadow: 0 18px 40px rgb(0 0 0 / 0.35);
  }

  .map-moment-card-dismiss {
    position: absolute;
    top: 0.65rem;
    right: 0.65rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.6rem;
    height: 1.6rem;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: rgb(var(--color-surface-500));
    cursor: pointer;
  }

  .map-moment-card-dismiss:hover {
    color: rgb(var(--color-surface-200));
    background: rgb(var(--color-surface-800) / 0.65);
  }

  .map-moment-card-when {
    margin: 0;
    padding-right: 1.75rem;
    font-size: 11px;
    letter-spacing: 0.02em;
    color: rgb(var(--color-surface-500));
  }

  .map-moment-card-headline {
    margin: 0.45rem 0 0;
    padding-right: 1.5rem;
    font-size: 1.15rem;
    font-weight: 560;
    letter-spacing: -0.025em;
    line-height: 1.25;
    color: rgb(var(--color-surface-50));
  }

  .map-moment-card-kept {
    margin: 0.7rem 0 0;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    overflow: hidden;
    font-size: 0.8125rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-300) / 0.92);
  }

  .map-moment-pulse {
    margin-top: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .map-moment-pulse-row {
    display: grid;
    grid-template-columns: 4.25rem 1fr;
    align-items: center;
    gap: 0.55rem;
  }

  .map-moment-pulse-label {
    font-size: 10px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-500));
  }

  .map-moment-pulse-track {
    height: 2px;
    overflow: hidden;
    border-radius: 999px;
    background: rgb(var(--color-surface-700) / 0.55);
  }

  .map-moment-pulse-fill {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: rgb(var(--color-surface-200) / 0.85);
  }

  .map-moment-card-chat {
    margin-top: 0.95rem;
    border: 0;
    background: transparent;
    padding: 0;
    font-size: 12px;
    color: rgb(var(--color-surface-400));
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 0.18em;
    text-decoration-color: rgb(var(--color-surface-600));
  }

  .map-moment-card-chat:hover {
    color: rgb(var(--color-surface-100));
    text-decoration-color: rgb(var(--color-surface-400));
  }
</style>
