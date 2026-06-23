<script lang="ts">
  import type { RoleStatusChip } from "$lib/utils/modelsWorkshopStatus";

  interface Props {
    chips: RoleStatusChip[];
  }

  let { chips }: Props = $props();
</script>

<div class="models-workshop-status" aria-label="Model roles status">
  {#each chips as chip (chip.id)}
    <span class="models-workshop-status-chip models-workshop-status-{chip.level}">
      <span class="models-workshop-status-dot" aria-hidden="true"></span>
      <span class="models-workshop-status-label">{chip.label}</span>
      <span class="models-workshop-status-detail">{chip.detail}</span>
    </span>
  {/each}
</div>
