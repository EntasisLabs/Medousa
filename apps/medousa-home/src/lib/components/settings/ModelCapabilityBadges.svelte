<script lang="ts">
  interface Props {
    badges: string[];
    compact?: boolean;
  }

  let { badges, compact = false }: Props = $props();
</script>

{#if badges.length > 0}
  <span class="model-cap-badges {compact ? 'model-cap-badges-compact' : ''}">
    {#each badges as badge (badge)}
      <span class="model-cap-badge">{badge}</span>
    {/each}
  </span>
{/if}
