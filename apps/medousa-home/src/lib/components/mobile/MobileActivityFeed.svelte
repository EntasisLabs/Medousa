<script lang="ts">
  import {
    AlertCircle,
    ArrowRightLeft,
    CheckCircle2,
    Circle,
    FileText,
    Play,
  } from "@lucide/svelte";
  import { settings } from "$lib/stores/settings.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import type { WorkspaceEvent } from "$lib/types/workspace";
  import { filterOperatorActivity } from "$lib/utils/activityFilter";
  import { resolveActivityEnrichment } from "$lib/utils/activityEnrichment";
  import {
    presentActivityEvent,
    type ActivityTone,
  } from "$lib/utils/activityPresentation";
  import type { Component } from "svelte";

  interface Props {
    events: WorkspaceEvent[];
    error: string | null;
  }

  let { events, error }: Props = $props();

  const visibleEvents = $derived(
    filterOperatorActivity(events, {
      showTechnical: settings.showTechnicalActivity,
    }),
  );

  const feed = $derived([...visibleEvents].reverse());

  const cardsById = $derived(
    new Map(workspace.cards.map((card) => [card.id, card])),
  );

  const toneIcons: Record<ActivityTone, Component> = {
    success: CheckCircle2,
    motion: ArrowRightLeft,
    attention: AlertCircle,
    neutral: Play,
    vault: FileText,
  };
</script>

<div class="mobile-activity-feed flex flex-col" aria-label="Recent activity">
  {#if error}
    <p class="mobile-activity-banner text-error-300">{error}</p>
  {/if}

  {#if feed.length > 0}
    <p class="mobile-activity-intro">
      {feed.length === 1 ? "1 update" : `${feed.length} updates`} · newest first
    </p>
  {/if}

  <ol class="mobile-activity-list">
    {#each feed as event, index (event.id)}
      {@const enrichment = resolveActivityEnrichment(
        event,
        cardsById,
        workspace.cardDetailsCache,
      )}
      {@const item = presentActivityEvent(event, enrichment)}
      {@const Icon = toneIcons[item.tone]}
      <li class="mobile-activity-item mobile-activity-{item.tone}">
        <div class="mobile-activity-rail" aria-hidden="true">
          <span class="mobile-activity-dot">
            <Icon size={14} strokeWidth={1.75} />
          </span>
          {#if index < feed.length - 1}
            <span class="mobile-activity-line"></span>
          {/if}
        </div>
        <article class="mobile-activity-card">
          <div class="mobile-activity-card-top">
            <span class="mobile-activity-label">{item.label}</span>
            <time class="mobile-activity-time" datetime={event.timestamp_utc}>
              {item.time}
            </time>
          </div>
          <p class="mobile-activity-summary">{item.summary}</p>
          {#if item.context}
            <p class="mobile-activity-context">{item.context}</p>
          {/if}
        </article>
      </li>
    {:else}
      <li class="mobile-activity-empty">
        <span class="mobile-activity-empty-icon" aria-hidden="true">
          <Circle size={28} strokeWidth={1.25} />
        </span>
        <p class="text-sm font-medium text-surface-200">All quiet</p>
        <p class="workshop-faint mt-1 max-w-xs text-center text-xs leading-relaxed">
          When Medousa finishes work or needs you, it shows up here.
        </p>
      </li>
    {/each}
  </ol>
</div>
