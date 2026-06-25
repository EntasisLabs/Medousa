<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";

  type Screen = "welcome" | "workloads" | "custom" | "progress" | "complete";

  interface ProfileSummary {
    id: string;
    displayName: string;
    packages: string[];
    sizeLabel: string;
  }

  interface PackageSummary {
    id: string;
    displayName: string;
    depends: string[];
    sizeLabel: string;
    optional: boolean;
    selected: boolean;
  }

  interface DownloadProgress {
    packageId: string;
    phase: string;
    percent: number;
    message: string;
  }

  let screen = $state<Screen>("welcome");
  let installRoot = $state("");
  let profiles = $state<ProfileSummary[]>([]);
  let packages = $state<PackageSummary[]>([]);
  let selectedProfile = $state("express");
  let customMode = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let progress = $state<DownloadProgress[]>([]);
  let modifyMode = $state(false);

  const diskEstimate = $derived.by(() => {
    const ids = customMode
      ? packages.filter((entry) => entry.selected).map((entry) => entry.id)
      : (profiles.find((entry) => entry.id === selectedProfile)?.packages ?? []);
    return ids.length ? `${ids.length} packages selected` : "Nothing selected";
  });

  async function bootstrap() {
    try {
      const summary = await invoke<{
        installRoot: string;
        profiles: ProfileSummary[];
        packages: PackageSummary[];
        modifyMode: boolean;
      }>("installer_bootstrap");
      installRoot = summary.installRoot;
      profiles = summary.profiles;
      packages = summary.packages;
      modifyMode = summary.modifyMode;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function startInstall() {
    busy = true;
    error = null;
    screen = "progress";
    progress = [];
    const unlisten = await listen<DownloadProgress>("install-progress", (event) => {
      const item = event.payload;
      const existing = progress.findIndex((entry) => entry.packageId === item.packageId);
      if (existing >= 0) {
        progress[existing] = item;
        progress = [...progress];
      } else {
        progress = [...progress, item];
      }
    });
    try {
      const packageIds = customMode
        ? packages.filter((entry) => entry.selected).map((entry) => entry.id)
        : (profiles.find((entry) => entry.id === selectedProfile)?.packages ?? []);
      await invoke("installer_run", {
        request: {
          installRoot,
          packageIds,
          modifyMode,
        },
      });
      screen = "complete";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      screen = customMode ? "custom" : "workloads";
    } finally {
      await unlisten();
      busy = false;
    }
  }

  function togglePackage(id: string) {
    packages = packages.map((entry) =>
      entry.id === id ? { ...entry, selected: !entry.selected } : entry,
    );
  }

  async function launchMedousa() {
    await invoke("installer_launch_medousa");
  }

  $effect(() => {
    void bootstrap();
  });
</script>

<div class="installer">
  {#if screen === "welcome"}
    <h1>Medousa Installer</h1>
    <p class="lead">
      {modifyMode
        ? "Add, remove, or repair Medousa packages."
        : "Install Medousa with the packages you need."}
    </p>
    <div class="card">
      <div class="muted">Install location</div>
      <div>{installRoot || "…"}</div>
    </div>
    <div class="actions">
      <button class="primary" onclick={() => (screen = "workloads")}>Continue</button>
    </div>
  {:else if screen === "workloads"}
    <h1>Choose a workload</h1>
    <p class="lead">Express is recommended for most people. Pick Offline if you want Gemma on-device.</p>
    <div class="profile-grid">
      {#each profiles as profile}
        <button
          class="profile {selectedProfile === profile.id ? 'selected' : ''}"
          onclick={() => {
            selectedProfile = profile.id;
            customMode = false;
          }}
        >
          <div class="profile-title">{profile.displayName}</div>
          <div class="profile-desc">{profile.packages.join(" · ")} · {profile.sizeLabel}</div>
        </button>
      {/each}
      <button
        class="profile {customMode ? 'selected' : ''}"
        onclick={() => {
          customMode = true;
          screen = "custom";
        }}
      >
        <div class="profile-title">Custom</div>
        <div class="profile-desc">Pick individual packages and optional model packs</div>
      </button>
    </div>
    {#if error}<p class="error">{error}</p>{/if}
    <div class="actions">
      <button class="secondary" onclick={() => (screen = "welcome")}>Back</button>
      <button class="primary" disabled={busy} onclick={startInstall}>Install</button>
    </div>
  {:else if screen === "custom"}
    <h1>Custom packages</h1>
    <p class="lead">{diskEstimate}</p>
    <div class="card packages">
      {#each packages as pkg}
        <label class="package-row">
          <input
            type="checkbox"
            checked={pkg.selected}
            onchange={() => togglePackage(pkg.id)}
          />
          <span>{pkg.displayName}</span>
          <span class="muted">{pkg.sizeLabel}</span>
        </label>
      {/each}
    </div>
    {#if error}<p class="error">{error}</p>{/if}
    <div class="actions">
      <button class="secondary" onclick={() => (screen = "workloads")}>Back</button>
      <button class="primary" disabled={busy} onclick={startInstall}>Install selected</button>
    </div>
  {:else if screen === "progress"}
    <h1>Installing…</h1>
    <p class="lead">Downloading and verifying packages. This may take a while for model packs.</p>
    <div class="progress-list">
      {#each progress as item}
        <div class="progress-item">
          <div>{item.packageId} — {item.message}</div>
          <div class="progress-bar">
            <div class="progress-fill" style="width: {item.percent}%"></div>
          </div>
        </div>
      {/each}
      {#if progress.length === 0}
        <p class="status">Preparing…</p>
      {/if}
    </div>
  {:else}
    <h1>Installation complete</h1>
    <p class="lead">Medousa is ready. The first-run wizard will help you configure your brain and pairing.</p>
    <div class="actions">
      <button class="primary" onclick={launchMedousa}>Launch Medousa</button>
      <button class="secondary" onclick={() => openUrl("https://github.com/EntasisLabs/Medousa")}>Release notes</button>
    </div>
  {/if}
</div>
