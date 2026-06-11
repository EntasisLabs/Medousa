<script lang="ts">
  import ContextCrossLinks from "$lib/components/context/ContextCrossLinks.svelte";
  import ContextPlumbingSection from "$lib/components/context/ContextPlumbingSection.svelte";
  import WorkshopLivelinessChip from "$lib/components/ui/WorkshopLivelinessChip.svelte";
  import type { LocusNodeDetailResponse } from "$lib/types/locus";
  import {
    extractThreadMemory,
    formatContextWhen,
    humanMomentTitle,
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

  let technicalOpen = $state(false);

  $effect(() => {
    detail;
    technicalOpen = false;
  });

  const title = $derived(detail ? humanMomentTitle(detail.node) : "");
  const when = $derived(detail ? formatContextWhen(detail.node.timestamp) : "");
  const lead = $derived(
    detail
      ? extractThreadMemory(detail.raw, detail.node.context_summary)
      : null,
  );
  const showLead = $derived(Boolean(lead?.trim()) && lead!.trim() !== title.trim());
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
    <header class="context-witness-hero">
      <WorkshopLivelinessChip variant="live" label={tierHumanLabel(detail.node.tier)} />
      <h2 class="context-witness-title mt-3">{title}</h2>
      <p class="context-witness-meta">{when}</p>
      {#if showLead}
        <p class="context-witness-lead">{lead}</p>
      {/if}
    </header>

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

    <ContextPlumbingSection label="Technical">
      <button
        type="button"
        class="context-layer-toggle"
        aria-expanded={technicalOpen}
        onclick={() => {
          technicalOpen = !technicalOpen;
        }}
      >
        <span>Identifiers & raw</span>
        <span class="workshop-faint text-[11px]">sync_key · JSON</span>
      </button>
      {#if technicalOpen}
        <dl class="context-layer-body text-xs">
          <div>
            <dt class="workshop-label">sync_key</dt>
            <dd class="mt-0.5 break-all font-mono text-surface-300">{detail.node.sync_key}</dd>
          </div>
          <div>
            <dt class="workshop-label">Session id</dt>
            <dd class="mt-0.5 break-all font-mono text-surface-300">{detail.node.session_id}</dd>
          </div>
        </dl>
        <pre class="context-layer-raw max-h-64">{JSON.stringify(detail, null, 2)}</pre>
      {/if}
    </ContextPlumbingSection>
  </article>
{/if}
