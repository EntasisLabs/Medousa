<script lang="ts">
  /**
   * @deprecated Use NavShell — kept as a thin alias for any stale imports.
   */
  import NavShell from "$lib/components/layout/NavShell.svelte";
  import type { DaemonHealth } from "$lib/daemon";

  interface Props {
    active: string;
    onSelect: (surface: string) => void;
    chatActivity?: number;
    workActivity?: number;
    peersActivity?: number;
    activeProfileLabel?: string;
    health?: DaemonHealth | null;
  }

  let {
    active,
    onSelect,
    chatActivity = 0,
    workActivity = 0,
    peersActivity = 0,
    activeProfileLabel = "Personal",
    health = null,
  }: Props = $props();
</script>

<NavShell
  {active}
  {onSelect}
  onOpenChat={() => onSelect("chat")}
  {health}
  {chatActivity}
  {workActivity}
  {peersActivity}
  {activeProfileLabel}
/>
