<script lang="ts">
  /** `button` atom — emits a `run` scene event carrying its action. */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const label = $derived(typeof node.props.label === "string" ? node.props.label : "");
  const action = $derived(typeof node.props.action === "string" ? node.props.action : "");
  const tone = $derived(node.props.tone === "primary" ? "primary" : "default");

  function activate() {
    const payload =
      node.props.payload && typeof node.props.payload === "object"
        ? { action, ...(node.props.payload as Record<string, unknown>) }
        : { action };
    ctx.sink?.emit(createSceneEvent(node.id, "run", payload));
  }
</script>

<button type="button" class="liquid-button" data-tone={tone} onclick={activate}>
  {label}
</button>

<style>
  .liquid-button {
    align-self: flex-start;
    padding: 0.25rem 0.6rem;
    border-radius: 0.375rem;
    font-size: 0.6875rem;
    font-weight: 500;
    cursor: pointer;
    border: 1px solid color-mix(in srgb, var(--color-primary-400) 35%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 12%, transparent);
    color: rgb(var(--color-primary-200));
  }

  .liquid-button:hover {
    background: color-mix(in srgb, var(--color-primary-500) 20%, transparent);
  }

  .liquid-button[data-tone="primary"] {
    color: rgb(var(--color-surface-50));
    background: rgb(var(--color-primary-600));
    border-color: transparent;
  }
</style>
