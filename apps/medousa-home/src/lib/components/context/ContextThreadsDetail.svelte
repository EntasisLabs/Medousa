<script lang="ts">
  import ContextCrossLinks from "$lib/components/context/ContextCrossLinks.svelte";
  import ContextPlumbingSection from "$lib/components/context/ContextPlumbingSection.svelte";
  import ContextWitnessHero from "$lib/components/context/ContextWitnessHero.svelte";
  import type { LocusNodeDetailResponse } from "$lib/types/locus";
  import {
    extractThreadMemory,
    humanMomentTitle,
    sessionDisplayName,
    threadMetaLine,
    tierHumanLabel,
  } from "$lib/utils/contextHuman";
  import { avecWhisper } from "$lib/utils/contextThreads";

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

  let whenWhereOpen = $state(false);
  let originOpen = $state(false);
  let storedOpen = $state(false);
  let signalOpen = $state(false);
  let rawOpen = $state(false);

  $effect(() => {
    detail;
    whenWhereOpen = false;
    originOpen = false;
    storedOpen = false;
    signalOpen = false;
    rawOpen = false;
  });

  const memoryLead = $derived(
    detail
      ? extractThreadMemory(detail.raw, detail.node.context_summary)
      : null,
  );
  const heroMeta = $derived(
    detail
      ? threadMetaLine(
          detail.node.session_id,
          detail.node.timestamp,
          detail.node.tier,
          sessionLabels,
        )
      : null,
  );
  const postureWhisper = $derived(
    detail ? avecWhisper(detail.node.user_avec) : null,
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
      title={humanMomentTitle(detail.node)}
      meta={heroMeta}
      lead={memoryLead}
      chipLabel={tierHumanLabel(detail.node.tier)}
      chipVariant="live"
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

    <ContextPlumbingSection label="If you need the machinery">
      <button
        type="button"
        class="context-layer-toggle"
        aria-expanded={whenWhereOpen}
        onclick={() => {
          whenWhereOpen = !whenWhereOpen;
        }}
      >
        <span>When & where</span>
        <span class="workshop-faint text-[11px]">session · time</span>
      </button>
      {#if whenWhereOpen}
        <dl class="context-layer-body text-xs">
          <div>
            <dt class="workshop-label">Session</dt>
            <dd class="mt-0.5 text-surface-200">
              {sessionDisplayName(detail.node.session_id, sessionLabels)}
            </dd>
          </div>
          {#if sessionLabels[detail.node.session_id]?.trim() && sessionLabels[detail.node.session_id] !== detail.node.session_id}
            <div>
              <dt class="workshop-label">Session id</dt>
              <dd class="mt-0.5 font-mono text-surface-300">{detail.node.session_id}</dd>
            </div>
          {/if}
          <div>
            <dt class="workshop-label">Captured</dt>
            <dd class="mt-0.5 text-surface-200">{heroMeta}</dd>
          </div>
          <div>
            <dt class="workshop-label">sync_key</dt>
            <dd class="mt-0.5 break-all font-mono text-surface-300">{detail.node.sync_key}</dd>
          </div>
        </dl>
      {/if}

      <button
        type="button"
        class="context-layer-toggle"
        aria-expanded={originOpen}
        onclick={() => {
          originOpen = !originOpen;
        }}
      >
        <span>Origin</span>
        <span class="workshop-faint text-[11px]">summary · model signal</span>
      </button>
      {#if originOpen}
        <dl class="context-layer-body text-xs">
          <div>
            <dt class="workshop-label">Context summary</dt>
            <dd class="mt-0.5 leading-relaxed text-surface-200">
              {detail.node.context_summary || "—"}
            </dd>
          </div>
          {#if detail.node.model_avec}
            <div>
              <dt class="workshop-label">Model posture</dt>
              <dd class="mt-0.5 font-mono text-surface-300">
                {avecWhisper(detail.node.model_avec)}
              </dd>
            </div>
          {/if}
          {#if postureWhisper}
            <div>
              <dt class="workshop-label">Your posture at capture</dt>
              <dd class="mt-0.5 font-mono text-surface-300">{postureWhisper}</dd>
            </div>
          {/if}
        </dl>
      {/if}

      <button
        type="button"
        class="context-layer-toggle"
        aria-expanded={storedOpen}
        onclick={() => {
          storedOpen = !storedOpen;
        }}
      >
        <span>What she stored</span>
        <span class="workshop-faint text-[11px]">STTP body</span>
      </button>
      {#if storedOpen}
        <pre class="context-layer-raw max-h-80 whitespace-pre-wrap">{detail.raw}</pre>
      {/if}

      <button
        type="button"
        class="context-layer-toggle"
        aria-expanded={signalOpen}
        onclick={() => {
          signalOpen = !signalOpen;
        }}
      >
        <span>Signal</span>
        <span class="workshop-faint text-[11px]">ρ · κ · ψ</span>
      </button>
      {#if signalOpen}
        <dl class="context-layer-body text-xs">
          <div>
            <dt class="workshop-label">ρ signal</dt>
            <dd class="mt-0.5 text-surface-200">{detail.node.rho.toFixed(3)}</dd>
          </div>
          <div>
            <dt class="workshop-label">κ coherence</dt>
            <dd class="mt-0.5 text-surface-200">{detail.node.kappa.toFixed(3)}</dd>
          </div>
          <div>
            <dt class="workshop-label">ψ</dt>
            <dd class="mt-0.5 text-surface-200">{detail.node.psi.toFixed(3)}</dd>
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
        <pre class="context-layer-raw">{JSON.stringify(detail, null, 2)}</pre>
      {/if}
    </ContextPlumbingSection>
  </article>
{/if}
