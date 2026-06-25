<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import CompleteScreen from "./lib/screens/CompleteScreen.svelte";
  import ConfigureScreen from "./lib/screens/ConfigureScreen.svelte";
  import HubScreen from "./lib/screens/HubScreen.svelte";
  import ProgressScreen from "./lib/screens/ProgressScreen.svelte";
  import type {
    BootstrapResponse,
    ConfigureMode,
    DownloadProgress,
    HubTab,
    PackageSummary,
    ResolveSelectionResponse,
    Screen,
  } from "./lib/types";

  let screen = $state<Screen>("configure");
  let hubTab = $state<HubTab>("workloads");
  let bootstrap = $state<BootstrapResponse | null>(null);
  let packages = $state<PackageSummary[]>([]);
  let selectedProfileId = $state("express");
  let installRoot = $state("");
  let selection = $state<ResolveSelectionResponse>({
    expandedPackageIds: [],
    totalBytes: 0,
    sizeLabel: "Nothing selected",
    tree: [],
    warnings: [],
  });
  let busy = $state(false);
  let error = $state<string | null>(null);
  let progress = $state<DownloadProgress[]>([]);
  let modifyMode = $state(false);
  let initialPackageIds = $state<string[]>([]);

  async function refreshCatalog(selectedIds: string[]) {
    const catalog = await invoke<{ packages: PackageSummary[] }>("installer_catalog", {
      selectedIds,
    });
    packages = catalog.packages;
    selection = await invoke<ResolveSelectionResponse>("installer_resolve_selection", {
      packageIds: selectedIds,
    });
  }

  async function loadBootstrap() {
    try {
      const summary = await invoke<BootstrapResponse>("installer_bootstrap");
      bootstrap = summary;
      installRoot = summary.installRoot;
      packages = summary.packages;
      modifyMode = summary.modifyMode;
      selectedProfileId = "express";
      const selectedIds = summary.packages.filter((p) => p.selected).map((p) => p.id);
      initialPackageIds = summary.modifyMode
        ? summary.packages.filter((p) => p.installed).map((p) => p.id)
        : selectedIds;
      await refreshCatalog(selectedIds);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function selectedPackageIds(): string[] {
    return packages.filter((entry) => entry.selected).map((entry) => entry.id);
  }

  async function handleConfigureNext(mode: ConfigureMode) {
    if (!bootstrap) return;
    if (mode === "express") {
      const express = bootstrap.profiles.find((p) => p.id === "express");
      const ids = express?.packages ?? ["desktop", "engine"];
      packages = packages.map((pkg) => ({ ...pkg, selected: ids.includes(pkg.id) }));
      selectedProfileId = "express";
    } else if (mode === "existing") {
      packages = packages.map((pkg) => ({
        ...pkg,
        selected: pkg.installed || initialPackageIds.includes(pkg.id),
      }));
    }
    await refreshCatalog(selectedPackageIds());
    screen = "hub";
    hubTab = mode === "manual" || mode === "existing" ? "components" : "workloads";
  }

  async function handleSelectProfile(profileId: string) {
    selectedProfileId = profileId;
    const profile = bootstrap?.profiles.find((p) => p.id === profileId);
    if (!profile) return;
    packages = packages.map((pkg) => ({
      ...pkg,
      selected: profile.packages.includes(pkg.id),
    }));
    await refreshCatalog(selectedPackageIds());
  }

  async function handleTogglePackage(id: string) {
    packages = packages.map((entry) =>
      entry.id === id ? { ...entry, selected: !entry.selected } : entry,
    );
    selectedProfileId = "";
    await refreshCatalog(selectedPackageIds());
  }

  async function pickInstallRoot() {
    try {
      const picked = await invoke<string | null>("installer_pick_install_root");
      if (picked) {
        installRoot = picked;
        if (bootstrap) bootstrap = { ...bootstrap, installRoot: picked };
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function startInstall() {
    if (!bootstrap) return;
    busy = true;
    error = null;
    screen = "progress";
    progress = [];

    const selected = new Set(selectedPackageIds());
    const removePackageIds = modifyMode
      ? initialPackageIds.filter((id) => !selected.has(id))
      : [];

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
      await invoke("installer_run", {
        request: {
          installRoot,
          packageIds: selectedPackageIds(),
          modifyMode,
          removePackageIds,
        },
      });
      screen = "complete";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
      screen = "hub";
    } finally {
      await unlisten();
      busy = false;
    }
  }

  async function launchMedousa() {
    try {
      await invoke("installer_launch_medousa");
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function openReleaseNotes() {
    const url = bootstrap?.releaseBaseUrl
      ? `${bootstrap.releaseBaseUrl}/${bootstrap.releaseChannel}`
      : "https://github.com/EntasisLabs/Medousa";
    try {
      await openUrl(url);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function goToModify() {
    screen = "hub";
    hubTab = "components";
    modifyMode = true;
  }

  onMount(() => {
    void loadBootstrap();
  });
</script>

<div class="installer-shell">
  {#if !bootstrap}
    <section class="screen">
      <p class="status">Loading installer…</p>
      {#if error}<p class="error">{error}</p>{/if}
    </section>
  {:else if screen === "configure"}
    <ConfigureScreen
      {bootstrap}
      onNext={handleConfigureNext}
      onPickLocation={pickInstallRoot}
    />
  {:else if screen === "hub"}
    <HubScreen
      {bootstrap}
      tab={hubTab}
      profiles={bootstrap.profiles}
      {packages}
      {selectedProfileId}
      {selection}
      {busy}
      {error}
      onTabChange={(tab) => (hubTab = tab)}
      onSelectProfile={handleSelectProfile}
      onTogglePackage={handleTogglePackage}
      onBack={() => (screen = "configure")}
      onInstall={startInstall}
      onPickLocation={pickInstallRoot}
    />
  {:else if screen === "progress"}
    <ProgressScreen {progress} />
  {:else}
    <CompleteScreen onLaunch={launchMedousa} onModify={goToModify} onReleaseNotes={openReleaseNotes} />
  {/if}
</div>
