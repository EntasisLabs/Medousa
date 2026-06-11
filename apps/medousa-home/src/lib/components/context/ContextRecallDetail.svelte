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

  let idsOpen = $state(false);
  let sourceOpen = $state(false);
  let rawOpen = $state(false);

  $effect(() => {
    entry;
    idsOpen = false;
    sourceOpen = false;
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
    const parts = [entry.subtitle];
    if (entry.confidence !== undefined) {
      parts.push(`${(entry.confidence * 100).toFixed(0)}% sure`);
    }
    if (entry.trustLevel !== undefined) {
      parts.push(`${(entry.trustLevel * 100).toFixed(0)}% trust`);
    }
    return parts.join(" · ");
  });
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
      lead={entry.kind === "claim"
        ? null
        : entry.subtitle !== entry.title
          ? entry.subtitle
          : null}
      chipLabel={recallKindHumanLabel(entry.kind)}
      chipVariant="ready"
    />

    {#if entry.kind === "claim" && relatedThreads.length > 0 && onOpenThread}
      <section class="mt-6">
        <p class="workshop-label">Echoes in your sessions</p>
        <ul class="mt-2 space-y-2">
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
                  {sessionDisplayName(thread.session_id, sessionLabels)} · {formatContextWhen(thread.timestamp)} · {tierHumanLabel(thread.tier)}
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
      <section class="mt-6">
        <p class="workshop-label">Preferences she knows</p>
        <dl class="mt-2 space-y-2 text-sm">
          {#each preferences as [key, value] (key)}
            <div class="workshop-inset px-3 py-2">
              <dt class="workshop-faint text-[11px] uppercase tracking-wide">{key}</dt>
              <dd class="mt-1 text-sm text-surface-200">
                {typeof value === "string" ? value : JSON.stringify(value)}
              </dd>
            </div>
          {/each}
        </dl>
      </section>
    {/if}

    {#if context && context.policy_profiles && context.policy_profiles.length > 0 && entry.kind === "persona"}
      <section class="mt-6">
        <p class="workshop-label">Policy profiles</p>
        <ul class="mt-2 space-y-1 text-sm text-surface-300">
          {#each context.policy_profiles as profile (profile.policy_profile_id)}
            <li class="workshop-faint text-[11px]">
              {profile.policy_profile_id} · depth {profile.graph_max_depth}
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <ContextPlumbingSection>
      <button
        type="button"
        class="context-layer-toggle"
        aria-expanded={idsOpen}
        onclick={() => {
          idsOpen = !idsOpen;
        }}
      >
        <span>Identifiers</span>
        <span class="workshop-faint text-[11px]">ids · channel</span>
      </button>
      {#if idsOpen}
        <dl class="context-layer-body text-xs">
          {#each Object.entries(entry.meta ?? {}) as [key, value] (key)}
            <div>
              <dt class="workshop-label">{key}</dt>
              <dd class="mt-0.5 font-mono text-surface-300">{value}</dd>
            </div>
          {/each}
          {#if context?.channel}
            <div>
              <dt class="workshop-label">channel</dt>
              <dd class="mt-0.5 font-mono text-surface-300">
                {context.channel.channel_id} · {context.channel.channel_type}
              </dd>
            </div>
          {/if}
        </dl>
      {/if}

      <button
        type="button"
        class="context-layer-toggle"
        aria-expanded={sourceOpen}
        onclick={() => {
          sourceOpen = !sourceOpen;
        }}
      >
        <span>Source</span>
        <span class="workshop-faint text-[11px]">graph · kind</span>
      </button>
      {#if sourceOpen}
        <dl class="context-layer-body text-xs">
          <div>
            <dt class="workshop-label">kind</dt>
            <dd class="mt-0.5 text-surface-200">{entry.kind}</dd>
          </div>
          {#if context}
            <div>
              <dt class="workshop-label">graph_depth_used</dt>
              <dd class="mt-0.5 text-surface-200">{context.graph_depth_used}</dd>
            </div>
          {/if}
          <div>
            <dt class="workshop-label">entry_id</dt>
            <dd class="mt-0.5 break-all font-mono text-surface-300">{entry.id}</dd>
          </div>
        </dl>
      {/if}

      <button
        type="button"
        class="context-layer-toggle"
        aria-expanded={rawOpen}
        onclick={() => {
          rawOpen = !rawOpen;
        }}
      >
        <span>Raw JSON</span>
        <span class="workshop-faint text-[11px]">advanced</span>
      </button>
      {#if rawOpen}
        <pre class="context-layer-raw">{JSON.stringify(entry, null, 2)}</pre>
      {/if}
    </ContextPlumbingSection>
  </article>
{/if}
