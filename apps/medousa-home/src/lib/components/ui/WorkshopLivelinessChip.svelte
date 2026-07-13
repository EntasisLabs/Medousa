<script lang="ts">
  import type { LivelinessVariant } from "$lib/types/liveliness";

  interface Props {
    variant: LivelinessVariant;
    label?: string;
  }

  let { variant, label }: Props = $props();

  const defaultLabels: Record<LivelinessVariant, string> = {
    live: "Live",
    ready: "Configured",
    setup: "Needs setup",
    muted: "Offline",
    scheduled: "Scheduled",
    paused: "Paused",
    running: "Running",
    warning: "Attention",
  };


  const text = $derived(label ?? defaultLabels[variant]);
</script>

<span class="workshop-liveliness-chip workshop-liveliness-chip-{variant}">{text}</span>
