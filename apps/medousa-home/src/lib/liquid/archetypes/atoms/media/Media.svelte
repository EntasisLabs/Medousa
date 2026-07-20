<script lang="ts">
  /** `media` atom — inline image with optional caption + aspect ratio. */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import {
    isLocalImageHref,
    isRemoteImageHref,
    resolveLocalImagePreviewUrl,
  } from "$lib/utils/vaultLocalImages";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const rawSrc = $derived(typeof node.props.src === "string" ? node.props.src.trim() : "");
  const alt = $derived(typeof node.props.alt === "string" ? node.props.alt : "");
  const caption = $derived(typeof node.props.caption === "string" ? node.props.caption : "");
  const ratio = $derived(typeof node.props.ratio === "string" ? node.props.ratio : "");

  let resolvedSrc = $state<string | null>(null);
  let missing = $state(false);

  $effect(() => {
    const src = rawSrc;
    const notePath = ctx.localImagePath ?? null;
    if (!src) {
      resolvedSrc = null;
      missing = false;
      return;
    }
    if (isRemoteImageHref(src) || !isLocalImageHref(src)) {
      resolvedSrc = src;
      missing = false;
      return;
    }
    let cancelled = false;
    resolvedSrc = null;
    missing = false;
    void resolveLocalImagePreviewUrl(src, notePath).then((url) => {
      if (cancelled) return;
      if (url) {
        resolvedSrc = url;
        missing = false;
      } else {
        resolvedSrc = null;
        missing = true;
      }
    });
    return () => {
      cancelled = true;
    };
  });
</script>

{#if rawSrc}
  <figure class="liquid-media" class:liquid-media--missing={missing}>
    {#if resolvedSrc}
      <img
        src={resolvedSrc}
        {alt}
        loading="lazy"
        decoding="async"
        style={ratio ? `aspect-ratio:${ratio}` : undefined}
        onerror={() => (missing = true)}
      />
    {:else if missing}
      <div class="liquid-media-fallback" role="img" aria-label={alt || "Image unavailable"}>
        Image unavailable
      </div>
    {:else}
      <div class="liquid-media-fallback liquid-media-fallback--loading" aria-hidden="true"></div>
    {/if}
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

  .liquid-media-fallback {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 6rem;
    padding: 1rem;
    font-size: 0.75rem;
    color: rgb(var(--color-surface-400));
  }

  .liquid-media-fallback--loading {
    min-height: 4rem;
  }

  .liquid-media figcaption {
    padding: 0.4rem 0.65rem;
    font-size: 0.7rem;
    color: rgb(var(--color-surface-300));
  }
</style>
