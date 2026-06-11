<script lang="ts">
  import type { LocusAvecSnapshot } from "$lib/types/locus";
  import { AVEC_DIMENSIONS } from "$lib/utils/contextPosture";

  interface Props {
    avec: LocusAvecSnapshot;
    compact?: boolean;
    label?: string;
  }

  let { avec, compact = false, label = "Your posture" }: Props = $props();
</script>

<div class="context-posture-fingerprint {compact ? 'context-posture-fingerprint-compact' : ''}">
  <div class="flex flex-wrap items-center justify-between gap-2">
    <p class="workshop-label">{label}</p>
    <span class="context-posture-psi-chip">ψ {avec.psi.toFixed(2)}</span>
  </div>

  <div class="mt-3 space-y-2">
    {#each AVEC_DIMENSIONS as dim (dim.key)}
      {@const value = avec[dim.key]}
      <div class="context-posture-dim">
        <span class="context-posture-dim-label">{dim.label}</span>
        <span class="context-posture-dim-track" aria-hidden="true">
          <span class="context-posture-dim-fill" style="width: {Math.min(100, Math.max(0, value * 100))}%"></span>
        </span>
        <span class="context-posture-dim-value">{value.toFixed(2)}</span>
      </div>
    {/each}
  </div>
</div>
