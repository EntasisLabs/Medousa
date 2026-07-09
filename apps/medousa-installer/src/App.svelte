<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import InstallerChrome from "./lib/components/InstallerChrome.svelte";
  import ErrorBanner from "./lib/components/ErrorBanner.svelte";
  import { humanizeError } from "./lib/copy";
  import CompleteScreen from "./lib/screens/CompleteScreen.svelte";
  import ManageScreen from "./lib/screens/ManageScreen.svelte";
  import ProgressScreen from "./lib/screens/ProgressScreen.svelte";
  import WelcomeScreen from "./lib/screens/WelcomeScreen.svelte";
  import type {
    BootstrapResponse,
    DownloadProgress,
    InstallerStep,
    PackageSummary,
    ResolveSelectionResponse,
  } from "./lib/types";
  import markUrl from "./assets/medousa-mark.png";

  let screen = $state<InstallerStep>("welcome");
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
  let bootstrapError = $state<string | null>(null);
  let progress = $state<DownloadProgress[]>([]);
  let modifyMode = $state(false);
  let initialPackageIds = $state<string[]>([]);
  let showOptions = $state(false);
  let bootstrapping = $state(true);

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

  const humanizedWarnings = $derived(selection.warnings.map((w) => w));

  function selectedPackageIds(): string[] {
    return packages.filter((entry) => entry.selected).map((entry) => entry.id);
  }

  function setError(err: unknown) {
    const raw = err instanceof Error ? err.message : String(err);
    error = humanizeError(raw);
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
    bootstrapping = true;
    bootstrapError = null;
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
      screen = summary.modifyMode ? "manage" : "welcome";
      if (!summary.modifyMode) {
        await applyExpressSelection();
      }
      error = null;
    } catch (err) {
      bootstrapError = humanizeError(err instanceof Error ? err.message : String(err));
      bootstrap = null;
    } finally {
      bootstrapping = false;
    }
  }

  async function handleInstall() {
    if (!bootstrap) return;
    if (!modifyMode) {
      await applyExpressSelection();
    }
    await startInstall();
  }

  async function handleSelectProfile(profileId: string) {
    selectedProfileId = profileId;
    const profile = bootstrap?.profiles.find((p) => p.id === profileId);
    if (!profile) return;
    packages = packages.map((pkg) => ({
      ...pkg,
      selected: profile.packages.includes(pkg.id),
    }));
    showOptions = true;
    await refreshCatalog(selectedPackageIds());
  }

  async function handleTogglePackage(id: string) {
    packages = packages.map((entry) =>
      entry.id === id ? { ...entry, selected: !entry.selected } : entry,
    );
    selectedProfileId = "";
    error = null;
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
      setError(err);
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
      setError(err);
      screen = modifyMode ? "manage" : "welcome";
    } finally {
      await unlisten();
      busy = false;
    }
  }

  async function launchMedousa() {
    try {
      await invoke("installer_launch_medousa");
    } catch (err) {
      setError(err);
    }
  }

  async function openReleaseNotes() {
    const url = bootstrap?.releaseBaseUrl
      ? `${bootstrap.releaseBaseUrl}/${bootstrap.releaseChannel}`
      : "https://github.com/EntasisLabs/Medousa";
    try {
      await openUrl(url);
    } catch (err) {
      setError(err);
    }
  }

  function goToManage() {
    screen = "manage";
    modifyMode = true;
  }

  onMount(() => {
    void loadBootstrap();
  });
</script>

<div class="installer-shell">
  {#if bootstrapping}
    <div class="boot-splash" role="status" aria-live="polite">
      <img class="boot-mark" src={markUrl} alt="" width="56" height="56" />
      <div class="boot-bar" aria-hidden="true"></div>
    </div>
  {:else if bootstrapError && !bootstrap}
    <div class="boot-splash boot-error">
      <img class="boot-mark" src={markUrl} alt="" width="56" height="56" />
      <h1 class="boot-title">Medousa</h1>
      <ErrorBanner message={bootstrapError} onRetry={loadBootstrap} />
    </div>
  {:else if bootstrap}
    <InstallerChrome
      step={screen}
      showProgress={screen === "progress" || screen === "complete"}
    >
      {#if screen === "welcome"}
        <WelcomeScreen
          {bootstrap}
          {packages}
          warnings={humanizedWarnings}
          sizeLabel={selection.sizeLabel || expressSizeLabel}
          {error}
          {busy}
          {showOptions}
          onInstall={handleInstall}
          onToggleOptions={() => (showOptions = !showOptions)}
          onTogglePackage={handleTogglePackage}
          onApplyPreset={handleSelectProfile}
          onPickLocation={pickInstallRoot}
          onRetry={loadBootstrap}
        />
      {:else if screen === "manage"}
        <ManageScreen
          {bootstrap}
          {packages}
          warnings={humanizedWarnings}
          {error}
          sizeLabel={selection.sizeLabel}
          {hasChanges}
          onTogglePackage={handleTogglePackage}
          onPickLocation={pickInstallRoot}
          onApply={startInstall}
          onRetry={loadBootstrap}
        />
      {:else if screen === "progress"}
        <ProgressScreen
          {progress}
          version={bootstrap.remoteVersion ?? bootstrap.installedVersion ?? ""}
        />
      {:else}
        <CompleteScreen
          onLaunch={launchMedousa}
          onManage={goToManage}
          onReleaseNotes={openReleaseNotes}
        />
      {/if}
    </InstallerChrome>
  {/if}
</div>

<style>
  .boot-error {
    padding: 1.5rem;
    max-width: 420px;
    margin: 0 auto;
    text-align: center;
  }

  .boot-title {
    font-size: 1.5rem;
    font-weight: 700;
    margin: 0 0 1rem;
  }

  .boot-error :global(.error-banner) {
    text-align: left;
  }
</style>
