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
  import { postureWhisper } from "$lib/utils/contextPosture";
  import { avecWhisper } from "$lib/utils/contextThreads";

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

  let captureOpen = $state(false);
  let rawOpen = $state(false);

  $effect(() => {
    entry;
    captureOpen = false;
    rawOpen = false;
  });
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
      chipLabel="Session mood"
      chipVariant="live"
    />

    <ContextCrossLinks
      links={[
        ...(onOpenThreads ? [{ label: "Session moments", onClick: onOpenThreads }] : []),
        ...(onOpenLatestThread
          ? [{ label: "Latest moment", onClick: onOpenLatestThread }]
          : []),
        ...(chatSessionAvailable && onOpenChat
          ? [{ label: "Open in Chat", onClick: onOpenChat }]
          : []),
      ]}
    />

    <section class="mt-6">
      <ContextPostureFingerprint avec={entry.userAvec} label="At a glance" />
      <p class="workshop-faint mt-3 text-[11px]">{postureWhisper(entry.userAvec)}</p>
    </section>

    {#if entry.modelAvec}
      <section class="mt-5">
        <ContextPostureFingerprint
          avec={entry.modelAvec}
          label="Her side of the room"
          compact={true}
        />
      </section>
    {/if}

    {#if entry.latestSummary.trim()}
      <section class="mt-6">
        <p class="workshop-label">Latest capture</p>
        <p class="mt-2 text-sm leading-relaxed text-surface-200">{entry.latestSummary}</p>
      </section>
    {/if}

    <ContextPlumbingSection>
      <button
        type="button"
        class="context-layer-toggle"
        aria-expanded={captureOpen}
        onclick={() => {
          captureOpen = !captureOpen;
        }}
      >
        <span>Capture details</span>
        <span class="workshop-faint text-[11px]">sync · metrics</span>
      </button>
      {#if captureOpen}
        <dl class="context-layer-body text-xs">
          <div>
            <dt class="workshop-label">Latest sync_key</dt>
            <dd class="mt-0.5 break-all font-mono text-surface-300">{entry.latestSyncKey}</dd>
          </div>
          <div>
            <dt class="workshop-label">Captured</dt>
            <dd class="mt-0.5 font-mono text-surface-300">{entry.latestTimestamp}</dd>
          </div>
          <div>
            <dt class="workshop-label">Metrics</dt>
            <dd class="mt-0.5 font-mono text-surface-300">
              {avecWhisper(entry.userAvec)}
            </dd>
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
