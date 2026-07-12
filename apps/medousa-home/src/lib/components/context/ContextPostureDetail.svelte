<script lang="ts">
  import ContextCrossLinks from "$lib/components/context/ContextCrossLinks.svelte";
  import ContextPlumbingSection from "$lib/components/context/ContextPlumbingSection.svelte";
  import ContextPostureFingerprint from "$lib/components/context/ContextPostureFingerprint.svelte";
  import ContextWitnessHero from "$lib/components/context/ContextWitnessHero.svelte";
  import type { ContextPostureEntry } from "$lib/utils/contextPosture";
  import {
    formatContextWhen,
    postureHumanFeel,
  } from "$lib/utils/contextHuman";

  interface Props {
    entry: ContextPostureEntry | null;
    chatSessionAvailable?: boolean;
    onOpenChat?: () => void;
    onOpenThreads?: () => void;
    onOpenLatestThread?: () => void;
  }

  let {
    entry,
    chatSessionAvailable = false,
    onOpenChat,
    onOpenThreads,
    onOpenLatestThread,
  }: Props = $props();

  let rawOpen = $state(false);

  $effect(() => {
    entry;
    rawOpen = false;
  });

  const herFeel = $derived(
    entry?.modelAvec ? postureHumanFeel(entry.modelAvec) : null,
  );
</script>

{#if !entry}
  <div class="flex h-full min-h-[12rem] items-center justify-center px-4">
    <p class="workshop-muted max-w-sm text-center text-sm leading-relaxed">
      Pick a session — how you showed up when she last captured your thread.
    </p>
  </div>
{:else}
  <article>
    <ContextWitnessHero
      title={entry.title}
      meta={`${entry.threadCount} moment${entry.threadCount === 1 ? "" : "s"} · ${formatContextWhen(entry.latestTimestamp)}`}
      lead={postureHumanFeel(entry.userAvec)}
      kicker="Session mood"
    />

    <ContextCrossLinks
      links={[
        ...(onOpenThreads ? [{ label: "Moments", onClick: onOpenThreads }] : []),
        ...(onOpenLatestThread
          ? [{ label: "Latest", onClick: onOpenLatestThread }]
          : []),
        ...(chatSessionAvailable && onOpenChat
          ? [{ label: "Chat", onClick: onOpenChat }]
          : []),
      ]}
    />

    {#if entry.latestSummary.trim()}
      <section class="context-story-chapter">
        <p class="context-story-label">Latest capture</p>
        <p class="context-story-copy">{entry.latestSummary}</p>
      </section>
    {/if}

    <section class="mt-6 max-w-xl">
      <ContextPostureFingerprint avec={entry.userAvec} label="Your footing" />
      {#if herFeel}
        <p class="workshop-faint mt-3 text-xs leading-relaxed">
          Her side — {herFeel.replace(/\.$/, "").toLowerCase()}.
        </p>
      {/if}
    </section>

    <ContextPlumbingSection resetKey={entry.id}>
      <dl class="context-plumbing-meta">
        <div class="context-plumbing-meta-row">
          <dt>Latest sync</dt>
          <dd class="font-mono text-[11px]">{entry.latestSyncKey}</dd>
        </div>
        <div class="context-plumbing-meta-row">
          <dt>Captured</dt>
          <dd class="font-mono text-[11px]">{entry.latestTimestamp}</dd>
        </div>
        {#if entry.modelAvec}
          <div class="context-plumbing-meta-row">
            <dt>Her ψ</dt>
            <dd class="font-mono text-[11px]">{entry.modelAvec.psi.toFixed(2)}</dd>
          </div>
        {/if}
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
        <span class="workshop-faint text-[11px]">session entry</span>
      </button>
      {#if rawOpen}
        <pre class="context-layer-raw">{JSON.stringify(entry, null, 2)}</pre>
      {/if}
    </ContextPlumbingSection>
  </article>
{/if}
