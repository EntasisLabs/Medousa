<script lang="ts">
  /** `chip` atom — a selectable token (filter / tag). Emits `select`. */
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  type Tone = "default" | "accent" | "success" | "warn";
  const TONES: Tone[] = ["default", "accent", "success", "warn"];

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const label = $derived(typeof node.props.label === "string" ? node.props.label : "");
  const tone = $derived<Tone>(TONES.includes(node.props.tone as Tone) ? (node.props.tone as Tone) : "default");

  function select() {
    const value = node.props.value ?? label;
    ctx.sink?.emit(createSceneEvent(node.id, "select", { value }));
  }
</script>

<button type="button" class="liquid-chip" data-tone={tone} onclick={select}>
  {label}
</button>

<style>
  .liquid-chip {
    display: inline-flex;
    align-items: center;
    padding: 0.2rem 0.6rem;
    border-radius: 999px;
    font-size: 0.7rem;
    font-weight: 500;
    cursor: pointer;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 40%, transparent);
    background: color-mix(in srgb, var(--color-surface-700) 55%, transparent);
    color: rgb(var(--color-surface-200));
    transition: background 0.15s ease;
  }

  .liquid-chip:hover {
    background: color-mix(in srgb, var(--color-surface-600) 60%, transparent);
  }

  .liquid-chip[data-tone="accent"] {
    color: rgb(var(--color-primary-200));
    border-color: color-mix(in srgb, var(--color-primary-500) 40%, transparent);
    background: color-mix(in srgb, var(--color-primary-500) 14%, transparent);
  }

  .liquid-chip[data-tone="success"] {
    color: rgb(var(--color-success-200));
    border-color: color-mix(in srgb, var(--color-success-500) 40%, transparent);
    background: color-mix(in srgb, var(--color-success-500) 14%, transparent);
  }

  .liquid-chip[data-tone="warn"] {
    color: rgb(var(--color-warning-200));
    border-color: color-mix(in srgb, var(--color-warning-500) 40%, transparent);
    background: color-mix(in srgb, var(--color-warning-500) 14%, transparent);
  }
</style>
