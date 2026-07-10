<script lang="ts">
  /** `media` atom — inline image with optional caption + aspect ratio. */
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();

  const src = $derived(typeof node.props.src === "string" ? node.props.src : "");
  const alt = $derived(typeof node.props.alt === "string" ? node.props.alt : "");
  const caption = $derived(typeof node.props.caption === "string" ? node.props.caption : "");
  const ratio = $derived(typeof node.props.ratio === "string" ? node.props.ratio : "");
</script>

{#if src}
  <figure class="liquid-media">
    <img {src} {alt} loading="lazy" style={ratio ? `aspect-ratio:${ratio}` : undefined} />
    {#if caption}
      <figcaption>{caption}</figcaption>
    {/if}
  </figure>
{/if}

<style>
  .liquid-media {
    margin: 0;
    overflow: hidden;
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 35%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 55%, transparent);
  }

  .liquid-media img {
    display: block;
    width: 100%;
    max-width: 100%;
    object-fit: cover;
  }

  .liquid-media figcaption {
    padding: 0.4rem 0.65rem;
    font-size: 0.7rem;
    color: rgb(var(--color-surface-300));
  }
</style>
