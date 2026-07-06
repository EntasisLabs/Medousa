<script lang="ts">
  import {
    isAllowedMediaEmbedUrl,
    resolveMediaEmbedConfig,
    type MediaEmbedProvider,
  } from "$lib/utils/mediaEmbed";

  interface Props {
    config: Record<string, unknown>;
    label?: string | null;
    fill?: boolean;
  }

  let { config, label = "Media", fill = false }: Props = $props();

  const resolved = $derived(resolveMediaEmbedConfig(config));
  const safeUrl = $derived(
    resolved && isAllowedMediaEmbedUrl(resolved.provider, resolved.embedUrl)
      ? resolved.embedUrl
      : null,
  );

  const iframeHeight = $derived(fill ? "100%" : `${resolved?.height ?? 352}px`);
  const frameClass = $derived(
    fill ? "media-embed-frame media-embed-frame-fill" : "media-embed-frame",
  );

  function spotifyAllow(): string {
    return "autoplay; clipboard-write; encrypted-media; fullscreen; picture-in-picture";
  }

  function appleSandbox(): string {
    return "allow-forms allow-popups allow-same-origin allow-scripts allow-storage-access-by-user-activation allow-top-navigation-by-user-activation";
  }
</script>

{#if resolved?.hidden}
  <div class="media-embed-hidden" aria-hidden="true">
    {#if safeUrl}
      <iframe
        class="media-embed-hidden-iframe"
        title={label ?? "Media embed"}
        src={safeUrl}
        allow={resolved.provider === "spotify" ? spotifyAllow() : "autoplay *; encrypted-media *;"}
        sandbox={resolved.provider === "apple_music" ? appleSandbox() : undefined}
        loading="lazy"
      ></iframe>
    {/if}
  </div>
{:else if !resolved}
  <p class="media-embed-error">Invalid media embed configuration.</p>
{:else if !safeUrl}
  <p class="media-embed-error">Media embed URL is not allowed.</p>
{:else}
  <div class={frameClass} style:min-height={fill ? "0" : iframeHeight}>
    <iframe
      class="media-embed-iframe"
      title={label ?? "Media embed"}
      src={safeUrl}
      width="100%"
      height={fill ? "100%" : iframeHeight}
      frameborder="0"
      allow={resolved.provider === "spotify" ? spotifyAllow() : "autoplay *; encrypted-media *;"}
      sandbox={resolved.provider === "apple_music" ? appleSandbox() : undefined}
      loading="lazy"
    ></iframe>
  </div>
{/if}

<style>
  .media-embed-frame {
    display: flex;
    flex-direction: column;
    width: 100%;
    min-width: 0;
    border-radius: 0.875rem;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--color-surface-600) 40%, transparent);
    background: rgb(var(--color-surface-900));
  }

  .media-embed-frame-fill {
    flex: 1 1 auto;
    min-height: 0;
    height: 100%;
    border-radius: 0;
    border-left: 0;
    border-right: 0;
  }

  .media-embed-iframe {
    display: block;
    border: 0;
    width: 100%;
    min-height: 0;
  }

  .media-embed-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
  }

  .media-embed-hidden-iframe {
    width: 1px;
    height: 1px;
    border: 0;
  }

  .media-embed-error {
    margin: 0;
    padding: 0.75rem 1rem;
    font-size: 0.75rem;
    color: rgb(var(--color-error-300));
  }
</style>
