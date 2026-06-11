<script lang="ts">
  import WorkshopLivelinessChip from "$lib/components/ui/WorkshopLivelinessChip.svelte";

  interface Props {
    title: string;
    meta?: string | null;
    lead?: string | null;
    chipLabel?: string | null;
    chipVariant?: "live" | "ready" | "setup" | "muted" | "scheduled" | "paused" | "warning";
  }

  let {
    title,
    meta = null,
    lead = null,
    chipLabel = null,
    chipVariant = "ready",
  }: Props = $props();

  const showLead = $derived(Boolean(lead?.trim()) && lead!.trim() !== title.trim());
</script>

<header class="context-witness-hero">
  {#if chipLabel}
    <div class="flex flex-wrap items-center gap-2">
      <WorkshopLivelinessChip variant={chipVariant} label={chipLabel} />
    </div>
  {/if}
  <h2 class="context-witness-title {chipLabel ? 'mt-3' : ''}">{title}</h2>
  {#if meta}
    <p class="context-witness-meta">{meta}</p>
  {/if}
  {#if showLead}
    <p class="context-witness-lead">{lead}</p>
  {/if}
</header>
