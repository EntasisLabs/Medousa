<script lang="ts">
  import ContextCrossLinks from "$lib/components/context/ContextCrossLinks.svelte";
  import ContextPlumbingSection from "$lib/components/context/ContextPlumbingSection.svelte";
  import ContextWitnessHero from "$lib/components/context/ContextWitnessHero.svelte";
  import type { LocusNodeDetailResponse } from "$lib/types/locus";
  import {
    extractThreadMemory,
    formatContextWhen,
    humanMomentTitle,
    postureHumanFeel,
    sessionDisplayName,
    tierHumanLabel,
  } from "$lib/utils/contextHuman";

  interface Props {
    detail: LocusNodeDetailResponse | null;
    loading: boolean;
    error: string | null;
    sessionLabels?: Record<string, string>;
    chatSessionAvailable?: boolean;
    postureAvailable?: boolean;
    onOpenChat?: () => void;
    onOpenPosture?: () => void;
  }

  let {
    detail,
    loading,
    error,
    sessionLabels = {},
    chatSessionAvailable = false,
    postureAvailable = false,
    onOpenChat,
    onOpenPosture,
  }: Props = $props();

  let rawOpen = $state(false);

  $effect(() => {
    detail;
    rawOpen = false;
  });

  const title = $derived(detail ? humanMomentTitle(detail.node) : "");
  const memory = $derived(
    detail ? extractThreadMemory(detail.raw, detail.node.context_summary) : null,
  );
  const showMemoryBody = $derived(
    Boolean(memory?.trim()) && memory!.trim() !== title.trim(),
  );
  const atmosphere = $derived(
    detail?.node.user_avec ? postureHumanFeel(detail.node.user_avec) : null,
  );
  const tags = $derived(
    (detail?.node.semantic_tags ?? []).map((tag) => tag.trim()).filter(Boolean),
  );
</script>

{#if loading && !detail}
  <p class="workshop-muted text-sm">Loading this moment…</p>
{:else if error}
  <p class="text-sm text-warning-400">{error}</p>
{:else if !detail}
  <div class="flex h-full min-h-[12rem] items-center justify-center px-4">
    <p class="workshop-muted max-w-sm text-center text-sm leading-relaxed">
      Pick a moment from the list — a slice of life she kept from one of your sessions.
    </p>
  </div>
{:else}
  <article>
    <ContextWitnessHero
      title={title}
      meta={formatContextWhen(detail.node.timestamp)}
      lead={atmosphere}
      kicker={tierHumanLabel(detail.node.tier)}
    />

    <ContextCrossLinks
      links={[
        ...(chatSessionAvailable && onOpenChat
          ? [{ label: "Open in Chat", onClick: onOpenChat }]
          : []),
        ...(postureAvailable && onOpenPosture
          ? [{ label: "How you showed up", onClick: onOpenPosture }]
          : []),
      ]}
    />

    {#if showMemoryBody}
      <section class="context-story-chapter">
        <p class="context-story-label">What she kept</p>
        <p class="context-story-copy">{memory}</p>
      </section>
    {/if}

    {#if tags.length > 0}
      <div class="context-story-tags max-w-xl">
        {#each tags as tag (tag)}
          <span class="context-story-tag">{tag}</span>
        {/each}
      </div>
    {/if}

    <ContextPlumbingSection resetKey={detail.node.sync_key}>
      <dl class="context-plumbing-meta">
        <div class="context-plumbing-meta-row">
          <dt>Session</dt>
          <dd>{sessionDisplayName(detail.node.session_id, sessionLabels)}</dd>
        </div>
        <div class="context-plumbing-meta-row">
          <dt>Tier</dt>
          <dd>{tierHumanLabel(detail.node.tier)}</dd>
        </div>
        <div class="context-plumbing-meta-row">
          <dt>sync_key</dt>
          <dd class="font-mono text-[11px]">{detail.node.sync_key}</dd>
        </div>
        <div class="context-plumbing-meta-row">
          <dt>Signal</dt>
          <dd class="font-mono text-[11px]">
            ρ {detail.node.rho.toFixed(2)} · κ {detail.node.kappa.toFixed(2)} · ψ
            {detail.node.psi.toFixed(2)}
          </dd>
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
        <span>Raw capture</span>
        <span class="workshop-faint text-[11px]">STTP · JSON</span>
      </button>
      {#if rawOpen}
        <pre class="context-layer-raw max-h-80 whitespace-pre-wrap">{detail.raw}</pre>
        <pre class="context-layer-raw">{JSON.stringify(detail.node, null, 2)}</pre>
      {/if}
    </ContextPlumbingSection>
  </article>
{/if}
