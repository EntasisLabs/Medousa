<script lang="ts">
  interface CrossLink {
    label: string;
    onClick: () => void;
  }

  interface Props {
    links: CrossLink[];
  }

  let { links }: Props = $props();
</script>

{#if links.length > 0}
  <div class="context-cross-links">
    {#each links as link (link.label)}
      <button type="button" class="context-nav-link" onclick={link.onClick}>
        {link.label}
      </button>
    {/each}
  </div>
{/if}
