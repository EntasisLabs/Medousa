<script lang="ts">
  /**
   * Renders a Lucide icon (from `icon:` / allowlisted name) or falls back to emoji/text.
   */
  import { resolveLiquidGlyph } from "$lib/liquid/icons/liquidIcons";

  interface Props {
    icon?: string | null;
    emoji?: string | null;
    /** Shown when neither icon nor emoji resolves. */
    fallback?: string;
    size?: number;
    strokeWidth?: number;
    class?: string;
  }

  let {
    icon = null,
    emoji = null,
    fallback = "",
    size = 14,
    strokeWidth = 2,
    class: className = "",
  }: Props = $props();

  const resolved = $derived(resolveLiquidGlyph({ icon, emoji }));
</script>

{#if resolved?.kind === "icon"}
  {@const Icon = resolved.component}
  <span class="liquid-glyph liquid-glyph-icon {className}" aria-hidden="true">
    <Icon {size} {strokeWidth} aria-hidden="true" />
  </span>
{:else if resolved?.kind === "text"}
  <span class="liquid-glyph liquid-glyph-text {className}" aria-hidden="true">{resolved.text}</span>
{:else if fallback}
  <span class="liquid-glyph liquid-glyph-text {className}" aria-hidden="true">{fallback}</span>
{/if}

<style>
  .liquid-glyph {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    line-height: 1;
  }

  .liquid-glyph-text {
    font-size: 1em;
  }

  .liquid-glyph-icon :global(svg) {
    display: block;
  }
</style>
