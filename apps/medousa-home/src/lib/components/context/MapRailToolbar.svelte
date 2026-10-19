<script lang="ts">
  import { contextThreads } from "$lib/context/contextThreads.svelte";
  import { contextNotes } from "$lib/context/contextNotes.svelte";
  import { RefreshCw } from "@lucide/svelte";
  interface Props { variant?: "popover" | "rail-row"; onPick?: () => void; }
  let { variant: _variant = "popover", onPick: _onPick }: Props = $props();
</script>

<span class="min-w-0 flex-1 text-xs text-content-secondary">Context</span>
<button type="button" class="vault-dock-icon-btn" title="Refresh Context" aria-label="Refresh Context" disabled={contextThreads.loading || contextNotes.loading} onclick={() => { void contextThreads.refresh({ limit: 200 }); void contextNotes.refresh(); }}>
  <RefreshCw size={14} strokeWidth={1.75} />
</button>
