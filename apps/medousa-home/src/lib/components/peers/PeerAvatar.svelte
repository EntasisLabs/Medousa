<script lang="ts">
  import { peerMonogram } from "$lib/utils/peerHomePreview";

  interface Props {
    label: string;
    size?: "md" | "lg";
    /** ready | nearby | reconnect | none */
    status?: "ready" | "nearby" | "reconnect" | "none";
  }

  let { label, size = "md", status = "none" }: Props = $props();
</script>

<span class="peers-avatar-wrap">
  <span
    class="peers-avatar {size === 'lg' ? 'peers-avatar-lg' : ''}"
    aria-hidden="true"
  >
    {peerMonogram(label)}
  </span>
  {#if status !== "none"}
    <span
      class="peers-status-dot peers-status-dot--{status}"
      aria-hidden="true"
    ></span>
  {/if}
</span>
