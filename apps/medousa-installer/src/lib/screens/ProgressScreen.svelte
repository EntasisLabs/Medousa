<script lang="ts">
  import { Download, HardDriveDownload, ShieldCheck } from "@lucide/svelte";
  import type { DownloadProgress } from "../types";

  interface Props {
    progress: DownloadProgress[];
    version?: string;
  }

  let { progress, version = "" }: Props = $props();

  const masterPercent = $derived.by(() => {
    if (progress.length === 0) return 0;
    const total = progress.reduce((sum, item) => sum + item.percent, 0);
    return Math.round(total / progress.length);
  });

  function phaseIcon(phase: string) {
    if (phase === "verify" || phase === "verifying") return ShieldCheck;
    if (phase === "install" || phase === "installing") return HardDriveDownload;
    return Download;
  }
</script>

<section class="progress-screen screen-fill">
  <header class="screen-header">
    <h1>Installing Medousa{version ? ` ${version}` : ""}</h1>
    <p class="lead">Downloading and verifying packages. Model packs may take a while.</p>
  </header>

  <div class="master-bar" role="progressbar" aria-valuenow={masterPercent} aria-valuemin={0} aria-valuemax={100}>
    <div class="master-fill" style="width: {masterPercent}%"></div>
  </div>

  <div class="progress-list scroll-pane">
    {#each progress as item (item.packageId)}
      {@const PhaseIcon = phaseIcon(item.phase)}
      <div class="progress-row">
        <div class="progress-header">
          <span class="progress-name">{item.displayName}</span>
          <span class="progress-phase">
            <span class="phase-icon" aria-hidden="true">
              <PhaseIcon size={12} strokeWidth={2} />
            </span>
            {item.phaseLabel}
          </span>
        </div>
        <div class="progress-bar" role="progressbar" aria-valuenow={item.percent} aria-valuemin={0} aria-valuemax={100}>
          <div class="progress-fill" style="width: {item.percent}%"></div>
        </div>
      </div>
    {/each}
    {#if progress.length === 0}
      <div class="progress-skeleton" aria-hidden="true">
        <div class="skeleton-row"></div>
        <div class="skeleton-row"></div>
        <div class="skeleton-row"></div>
      </div>
    {/if}
  </div>
</section>
