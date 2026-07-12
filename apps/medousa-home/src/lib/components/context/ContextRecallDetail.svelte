<script lang="ts">
  import ContextCrossLinks from "$lib/components/context/ContextCrossLinks.svelte";
  import ContextPlumbingSection from "$lib/components/context/ContextPlumbingSection.svelte";
  import ContextWitnessHero from "$lib/components/context/ContextWitnessHero.svelte";
  import type { ContextRecallEntry } from "$lib/types/context";
  import type { IdentityContextResponse } from "$lib/types/identity";
  import type { LocusNodeSummary } from "$lib/types/locus";
  import { threadSearchQueryForClaim } from "$lib/utils/contextCrossLinks";
  import {
    formatContextWhen,
    recallKindHumanLabel,
    sessionDisplayName,
    tierHumanLabel,
  } from "$lib/utils/contextHuman";
  import { threadTitle } from "$lib/utils/contextThreads";
  import { preferenceDisplayValue } from "$lib/utils/identityTeach";

  interface Props {
    entry: ContextRecallEntry | null;
    context: IdentityContextResponse | null;
    sessionLabels?: Record<string, string>;
    relatedThreads?: LocusNodeSummary[];
    onOpenThread?: (syncKey: string, sessionId: string) => void;
    onSearchThreads?: (query: string) => void;
  }

  let {
    entry,
    context,
    sessionLabels = {},
    relatedThreads = [],
    onOpenThread,
    onSearchThreads,
  }: Props = $props();

  let rawOpen = $state(false);

  $effect(() => {
    entry;
    rawOpen = false;
  });

  const preferences = $derived.by(() => {
    if (!entry || entry.kind !== "user" || !context?.user?.preferences) {
      return [];
    }
    return Object.entries(context.user.preferences).slice(0, 12);
  });

  const heroMeta = $derived.by(() => {
    if (!entry) return null;
    if (entry.kind === "claim" && entry.confidence !== undefined) {
      return `${(entry.confidence * 100).toFixed(0)}% sure`;
    }
    if (entry.kind === "relationship") {
      const parts: string[] = [];
      if (entry.trustLevel !== undefined) {
        parts.push(`${(entry.trustLevel * 100).toFixed(0)}% trust`);
      }
      if (entry.meta?.policy_tags) parts.push(entry.meta.policy_tags);
      return parts.join(" · ") || entry.subtitle;
    }
    if (entry.kind === "persona") {
      return entry.meta?.status ? `${entry.meta.status} · workshop persona` : entry.subtitle;
    }
    if (entry.kind === "user") {
      return entry.meta?.timezone ?? entry.subtitle;
    }
    return entry.subtitle !== entry.title ? entry.subtitle : null;
  });

  function humanizePrefKey(key: string): string {
    if (key.startsWith("note_")) return "Remembers";
    return key
      .split(/[_-]+/)
      .filter(Boolean)
      .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
      .join(" ");
  }
</script>

{#if !entry}
  <div class="flex h-full min-h-[12rem] items-center justify-center px-4">
    <p class="workshop-muted max-w-sm text-center text-sm leading-relaxed">
      Pick something from the list — a fact, a person, a thread of who you are that she carries
      forward.
    </p>
  </div>
{:else}
  <article>
    <ContextWitnessHero
      title={entry.title}
      meta={heroMeta}
      lead={null}
      kicker={recallKindHumanLabel(entry.kind)}
    />

    {#if entry.kind === "relationship" && entry.meta?.transition_reason}
      <section class="context-story-chapter">
        <p class="context-story-label">How this shifted</p>
        <p class="context-story-copy">{entry.meta.transition_reason}</p>
      </section>
    {/if}

    {#if entry.kind === "claim" && relatedThreads.length > 0 && onOpenThread}
      <section class="context-story-chapter">
        <p class="context-story-label">Echoes in your sessions</p>
        <ul class="mt-3 space-y-2">
          {#each relatedThreads as thread (thread.sync_key)}
            <li>
              <button
                type="button"
                class="context-related-memory"
                onclick={() => onOpenThread(thread.sync_key, thread.session_id)}
              >
                <p class="context-related-memory-title">
                  {threadTitle(thread)}
                </p>
                <p class="context-related-memory-meta">
                  {sessionDisplayName(thread.session_id, sessionLabels)} · {formatContextWhen(
                    thread.timestamp,
                  )} · {tierHumanLabel(thread.tier)}
                </p>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <ContextCrossLinks
      links={[
        ...(relatedThreads.length > 0 && onOpenThread
          ? [
              {
                label: "Open best match",
                onClick: () =>
                  onOpenThread(relatedThreads[0].sync_key, relatedThreads[0].session_id),
              },
            ]
          : []),
        ...(onSearchThreads && entry.kind === "claim"
          ? [
              {
                label: "Find in sessions",
                onClick: () => onSearchThreads(threadSearchQueryForClaim(entry)),
              },
            ]
          : []),
      ]}
    />

    {#if entry.kind === "user" && preferences.length > 0}
      <section class="context-story-chapter">
        <p class="context-story-label">What she knows about you</p>
        <div class="mt-3">
          {#each preferences as [key, value] (key)}
            <p class="context-pref-line">
              <span class="text-surface-400">{humanizePrefKey(key)}</span>
              — {preferenceDisplayValue(value)}
            </p>
          {/each}
        </div>
      </section>
    {/if}

    {#if context && context.policy_profiles && context.policy_profiles.length > 0 && entry.kind === "persona"}
      <section class="context-story-chapter">
        <p class="context-story-label">Policy profiles</p>
        <div class="mt-3">
          {#each context.policy_profiles as profile (profile.policy_profile_id)}
            <p class="context-pref-line">
              {profile.policy_profile_id}
              <span class="text-surface-500"> · depth {profile.graph_max_depth}</span>
            </p>
          {/each}
        </div>
      </section>
    {/if}

    <ContextPlumbingSection resetKey={entry.id}>
      <dl class="context-plumbing-meta">
        {#each Object.entries(entry.meta ?? {}) as [key, value] (key)}
          {#if key !== "transition_reason" && key !== "policy_tags"}
            <div class="context-plumbing-meta-row">
              <dt>{key}</dt>
              <dd class="font-mono text-[11px]">{value}</dd>
            </div>
          {/if}
        {/each}
        {#if context?.channel}
          <div class="context-plumbing-meta-row">
            <dt>channel</dt>
            <dd class="font-mono text-[11px]">
              {context.channel.channel_id} · {context.channel.channel_type}
            </dd>
          </div>
        {/if}
        {#if context}
          <div class="context-plumbing-meta-row">
            <dt>graph depth</dt>
            <dd>{context.graph_depth_used}</dd>
          </div>
        {/if}
        <div class="context-plumbing-meta-row">
          <dt>entry</dt>
          <dd class="font-mono text-[11px]">{entry.id}</dd>
        </div>
      </dl>
      <button
        type="button"
        class="context-layer-toggle"
        aria-expanded={rawOpen}
        onclick={() => {
          rawOpen = !rawOpen;
        }}
      >
        <span>Raw JSON</span>
        <span class="workshop-faint text-[11px]">entry</span>
      </button>
      {#if rawOpen}
        <pre class="context-layer-raw">{JSON.stringify(entry, null, 2)}</pre>
      {/if}
    </ContextPlumbingSection>
  </article>
{/if}
