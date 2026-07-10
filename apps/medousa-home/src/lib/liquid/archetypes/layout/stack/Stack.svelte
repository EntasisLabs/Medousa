<script lang="ts">
  /** `stack` layout primitive — vertical/horizontal flow over the `children` slot. */
  import Slot from "$lib/liquid/render/Slot.svelte";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();

  const direction = $derived(node.props.direction === "h" ? "h" : "v");
  const gap = $derived(
    ["none", "sm", "md", "lg"].includes(node.props.gap as string) ? (node.props.gap as string) : "md",
  );
  const align = $derived(
    ["start", "center", "end", "stretch"].includes(node.props.align as string)
      ? (node.props.align as string)
      : "stretch",
  );
  const children = $derived(node.slots?.children ?? []);
</script>

<div class="liquid-stack" data-dir={direction} data-gap={gap} data-align={align}>
  <Slot nodes={children} />
</div>

<style>
  .liquid-stack {
    display: flex;
    min-width: 0;
  }

  .liquid-stack[data-dir="v"] {
    flex-direction: column;
  }

  .liquid-stack[data-dir="h"] {
    flex-direction: row;
    flex-wrap: wrap;
  }

  .liquid-stack[data-align="start"] {
    align-items: flex-start;
  }
  .liquid-stack[data-align="center"] {
    align-items: center;
  }
  .liquid-stack[data-align="end"] {
    align-items: flex-end;
  }
  .liquid-stack[data-align="stretch"] {
    align-items: stretch;
  }

  .liquid-stack[data-gap="none"] {
    gap: 0;
  }
  .liquid-stack[data-gap="sm"] {
    gap: 0.5rem;
  }
  .liquid-stack[data-gap="md"] {
    gap: 0.9rem;
  }
  .liquid-stack[data-gap="lg"] {
    gap: 1.4rem;
  }
</style>
