<script lang="ts">
  import type { DownloadProgress } from "../types";

  interface Props {
    progress: DownloadProgress[];
  }

  let { progress }: Props = $props();
</script>

<section class="screen">
  <header class="screen-header">
    <h1>Installing…</h1>
    <p class="lead">Downloading and verifying packages. Model packs may take a while.</p>
  </header>

  <div class="progress-list">
    {#each progress as item (item.packageId)}
      <div class="progress-item">
        <div class="progress-label">{item.packageId} — {item.message}</div>
        <div class="progress-bar">
          <div class="progress-fill" style="width: {item.percent}%"></div>
        </div>
      </div>
    {/each}
    {#if progress.length === 0}
      <p class="status">Preparing…</p>
    {/if}
  </div>
</section>
