<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import InstallerChrome from "./lib/components/InstallerChrome.svelte";
  import InstallerFooter from "./lib/components/InstallerFooter.svelte";
  import CompleteScreen from "./lib/screens/CompleteScreen.svelte";
  import HubScreen from "./lib/screens/HubScreen.svelte";
  import ProgressScreen from "./lib/screens/ProgressScreen.svelte";
  import WelcomeScreen from "./lib/screens/WelcomeScreen.svelte";
  import type {
    BootstrapResponse,
    DownloadProgress,
    HubTab,
    InstallerStep,
    PackageSummary,
    ResolveSelectionResponse,
  } from "./lib/types";
  import markUrl from "./assets/medousa-mark.png";

  let screen = $state<InstallerStep>("welcome");
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

  const expressSizeLabel = $derived.by(() => {
    const express = bootstrap?.profiles.find((p) => p.id === "express");
    return express?.sizeLabel ?? selection.sizeLabel;
  });

  const hasSelection = $derived(selectedPackageIds().length > 0);

  const hasChanges = $derived.by(() => {
    if (!modifyMode) return hasSelection;
    const current = new Set(selectedPackageIds());
    const initial = new Set(initialPackageIds);
    if (current.size !== initial.size) return true;
    for (const id of current) {
      if (!initial.has(id)) return true;
    }
    return false;
  });

  const footerPrimaryLabel = $derived(
    modifyMode && hasChanges ? "Apply changes" : "Install",
  );

  const showFooter = $derived(screen === "welcome" || screen === "hub");

  function selectedPackageIds(): string[] {
    return packages.filter((entry) => entry.selected).map((entry) => entry.id);
  }

  async function refreshCatalog(selectedIds: string[]) {
    const catalog = await invoke<{ packages: PackageSummary[] }>("installer_catalog", {
      selectedIds,
    });
    packages = catalog.packages;
    selection = await invoke<ResolveSelectionResponse>("installer_resolve_selection", {
      packageIds: selectedIds,
    });
  }

  async function applyExpressSelection() {
    if (!bootstrap) return;
    const express = bootstrap.profiles.find((p) => p.id === "express");
    const ids = express?.packages ?? ["desktop", "engine"];
    packages = packages.map((pkg) => ({ ...pkg, selected: ids.includes(pkg.id) }));
    selectedProfileId = "express";
    await refreshCatalog(ids);
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
      screen = summary.modifyMode ? "hub" : "welcome";
      hubTab = summary.modifyMode ? "components" : "workloads";
      if (!summary.modifyMode) {
        await applyExpressSelection();
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleExpressInstall() {
    if (!bootstrap) return;
    await applyExpressSelection();
    await startInstall();
  }

  function goToCustomize() {
    screen = "hub";
    hubTab = "components";
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
    if (!bootstrap || !hasSelection) return;
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

  function goBackFromHub() {
    if (modifyMode) return;
    screen = "welcome";
  }

  async function openLicense() {
    const url = bootstrap?.releaseBaseUrl ?? "https://github.com/EntasisLabs/Medousa";
    try {
      await openUrl(url);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  onMount(() => {
    void loadBootstrap();
  });
</script>

<div class="installer-shell">
  {#if !bootstrap}
    <div class="boot-splash" role="status" aria-live="polite">
      <img class="boot-mark" src={markUrl} alt="" width="56" height="56" />
      <div class="boot-bar" aria-hidden="true"></div>
    </div>
  {:else}
    <InstallerChrome
      step={screen}
      version={bootstrap.installerVersion}
    >
        {#if screen === "welcome"}
          <WelcomeScreen
            {bootstrap}
            onExpressInstall={handleExpressInstall}
            onCustomize={goToCustomize}
            onPickLocation={pickInstallRoot}
            {busy}
          />
        {:else if screen === "hub"}
          <HubScreen
            {bootstrap}
            tab={hubTab}
            profiles={bootstrap.profiles}
            {packages}
            {selectedProfileId}
            {selection}
            {modifyMode}
            {error}
            onTabChange={(tab) => (hubTab = tab)}
            onSelectProfile={handleSelectProfile}
            onTogglePackage={handleTogglePackage}
            onPickLocation={pickInstallRoot}
          />
        {:else if screen === "progress"}
          <ProgressScreen
            {progress}
            version={bootstrap.remoteVersion ?? bootstrap.installedVersion ?? ""}
          />
        {:else}
          <CompleteScreen
            onLaunch={launchMedousa}
            onModify={goToModify}
            onReleaseNotes={openReleaseNotes}
          />
        {/if}

      {#if showFooter}
        {#snippet footer()}
          <InstallerFooter
            {installRoot}
            sizeLabel={screen === "welcome" ? expressSizeLabel : selection.sizeLabel}
            primaryLabel={footerPrimaryLabel}
            busy={busy}
            disabled={!hasSelection || (modifyMode && !hasChanges)}
            showBack={screen === "hub" && !modifyMode}
            onPrimary={screen === "welcome" ? handleExpressInstall : startInstall}
            onPickLocation={pickInstallRoot}
            onBack={goBackFromHub}
            onLicense={openLicense}
          />
        {/snippet}
      {/if}
    </InstallerChrome>
  {/if}
</div>
