<script lang="ts">
  import ContextSavedMoment from "$lib/components/context/ContextSavedMoment.svelte";
  import type { LocusNodeDetailResponse } from "$lib/types/locus";
  let { detail, loading, error }: {
    detail: LocusNodeDetailResponse | null;
    loading: boolean;
    error: string | null;
  } = $props();
</script>

{#if error}<p class="text-sm text-content-warning">{error}</p>
{:else if loading && !detail}<p class="workshop-muted text-sm">Loading saved moment…</p>
{:else if detail}<ContextSavedMoment {detail} />
{:else}<p class="workshop-muted text-sm">Select a saved moment to read its content and source.</p>{/if}
