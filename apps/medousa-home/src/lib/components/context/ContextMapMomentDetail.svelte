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
    tierHumanLabel,
  } from "$lib/utils/contextHuman";

  interface Props {
    detail: LocusNodeDetailResponse | null;
    loading: boolean;
    error: string | null;
    chatSessionAvailable?: boolean;
    postureAvailable?: boolean;
    onOpenChat?: () => void;
    onOpenPosture?: () => void;
  }

  let {
    detail,
    loading,
    error,
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
</script>

{#if loading && !detail}
  <p class="workshop-muted text-sm">Loading this moment…</p>
{:else if error}
  <p class="text-sm text-warning-400">{error}</p>
{:else if !detail}
  <div class="flex h-full min-h-[12rem] items-center justify-center px-4">
    <p class="workshop-muted max-w-sm text-center text-sm leading-relaxed">
      Tap a moment on the map — one beat from a session she kept.
    </p>
  </div>
{:else}
  <article class="context-map-detail">
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

    <ContextPlumbingSection resetKey={detail.node.sync_key}>
      <dl class="context-plumbing-meta">
        <div class="context-plumbing-meta-row">
          <dt>sync_key</dt>
          <dd class="font-mono text-[11px]">{detail.node.sync_key}</dd>
        </div>
        <div class="context-plumbing-meta-row">
          <dt>Session id</dt>
          <dd class="font-mono text-[11px]">{detail.node.session_id}</dd>
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
        <span class="workshop-faint text-[11px]">full node</span>
      </button>
      {#if rawOpen}
        <pre class="context-layer-raw max-h-64">{JSON.stringify(detail, null, 2)}</pre>
      {/if}
    </ContextPlumbingSection>
  </article>
{/if}
