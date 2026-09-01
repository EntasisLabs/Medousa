<script lang="ts">
  /**
   * Vendor marks used only to identify their matching external runtime.
   * Codex: OpenAI 2025 Blossom kit; Cursor: official 2D Cube kit;
   * Hermes: caduceus used by the official Hermes Agent project.
   */
  interface Props {
    agent: "codex" | "cursor" | "hermes";
    size?: number;
  }

  let { agent, size = 16 }: Props = $props();
</script>

<span
  class="external-agent-logo"
  style:width={`${size}px`}
  style:height={`${size}px`}
  style:font-size={`${size}px`}
  aria-hidden="true"
>
  {#if agent === "codex"}
    <img
      class="external-agent-logo__image external-agent-logo__image--light"
      src="/brand/external-agents/openai-blossom-black.svg"
      alt=""
    />
    <img
      class="external-agent-logo__image external-agent-logo__image--dark"
      src="/brand/external-agents/openai-blossom-white.svg"
      alt=""
    />
  {:else if agent === "cursor"}
    <img
      class="external-agent-logo__image external-agent-logo__image--light"
      src="/brand/external-agents/cursor-cube-light.svg"
      alt=""
    />
    <img
      class="external-agent-logo__image external-agent-logo__image--dark"
      src="/brand/external-agents/cursor-cube-dark.svg"
      alt=""
    />
  {:else}
    <span class="external-agent-logo__hermes">☤</span>
  {/if}
</span>

<style>
  .external-agent-logo {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    line-height: 1;
  }

  .external-agent-logo__image {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .external-agent-logo__image--dark {
    display: none;
  }

  :global(html.dark) .external-agent-logo__image--light {
    display: none;
  }

  :global(html.dark) .external-agent-logo__image--dark {
    display: block;
  }

  .external-agent-logo__hermes {
    display: block;
    color: currentColor;
    font-family: Georgia, "Times New Roman", serif;
    font-size: 1.25em;
    line-height: 0.75;
    transform: translateY(-0.02em);
  }
</style>
