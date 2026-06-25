<script lang="ts">
  import InstallationSidebar from "../components/InstallationSidebar.svelte";
  import type {
    BootstrapResponse,
    HubTab,
    PackageSummary,
    ProfileSummary,
    ResolveSelectionResponse,
  } from "../types";

  interface Props {
    bootstrap: BootstrapResponse;
    tab: HubTab;
    profiles: ProfileSummary[];
    packages: PackageSummary[];
    selectedProfileId: string;
    selection: ResolveSelectionResponse;
    busy: boolean;
    error: string | null;
    onTabChange: (tab: HubTab) => void;
    onSelectProfile: (id: string) => void;
    onTogglePackage: (id: string) => void;
    onBack: () => void;
    onInstall: () => void;
    onPickLocation: () => void;
  }

  let {
    bootstrap,
    tab,
    profiles,
    packages,
    selectedProfileId,
    selection,
    busy,
    error,
    onTabChange,
    onSelectProfile,
    onTogglePackage,
    onBack,
    onInstall,
    onPickLocation,
  }: Props = $props();

  let search = $state("");

  const filteredPackages = $derived(
    packages.filter((pkg) => {
      const q = search.trim().toLowerCase();
      if (!q) return true;
      return (
        pkg.displayName.toLowerCase().includes(q) ||
        pkg.id.toLowerCase().includes(q) ||
        pkg.category.toLowerCase().includes(q)
      );
    }),
  );

  const groupedPackages = $derived.by(() => {
    const groups = new Map<string, PackageSummary[]>();
    for (const pkg of filteredPackages) {
      const key = pkg.category;
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(pkg);
    }
    return [...groups.entries()];
  });

  function statusBadge(pkg: PackageSummary): string | null {
    if (pkg.updateAvailable) return "update available";
    if (pkg.installed) return "installed";
    return null;
  }
</script>

<section class="hub">
  <header class="screen-header">
    <h1>Install Medousa</h1>
    <p class="lead">Choose workloads and components for your installation.</p>
  </header>

  <div class="hub-body">
    <div class="hub-main">
      <nav class="tabs">
        <button class="tab {tab === 'workloads' ? 'active' : ''}" type="button" onclick={() => onTabChange("workloads")}>
          Workloads
        </button>
        <button class="tab {tab === 'components' ? 'active' : ''}" type="button" onclick={() => onTabChange("components")}>
          Individual components
        </button>
        <button class="tab {tab === 'locations' ? 'active' : ''}" type="button" onclick={() => onTabChange("locations")}>
          Installation locations
        </button>
      </nav>

      {#if tab === "workloads"}
        <div class="section-label">Desktop &amp; Core</div>
        <div class="profile-grid">
          {#each profiles as profile (profile.id)}
            <button
              class="profile {selectedProfileId === profile.id ? 'selected' : ''}"
              type="button"
              onclick={() => onSelectProfile(profile.id)}
            >
              <div class="profile-title">{profile.displayName}</div>
              <div class="profile-desc">{profile.description}</div>
              <div class="profile-meta">{profile.sizeLabel}</div>
            </button>
          {/each}
        </div>
      {:else if tab === "components"}
        <div class="search-row">
          <input
            class="search-input"
            type="search"
            placeholder="Search components"
            bind:value={search}
          />
        </div>
        {#each groupedPackages as [category, items] (category)}
          <div class="section-label">{category}</div>
          <div class="card packages">
            {#each items as pkg (pkg.id)}
              <label class="package-row">
                <input
                  type="checkbox"
                  checked={pkg.selected}
                  disabled={!pkg.optional}
                  onchange={() => onTogglePackage(pkg.id)}
                />
                <span class="package-name">{pkg.displayName}</span>
                {#if statusBadge(pkg)}
                  <span class="badge">{statusBadge(pkg)}</span>
                {/if}
                <span class="muted package-size">{pkg.sizeLabel}</span>
              </label>
            {/each}
          </div>
        {/each}
      {:else}
        <div class="card locations">
          <div class="location-item">
            <div class="muted">Application</div>
            <div>{bootstrap.installRoot}</div>
            <button class="link-btn" type="button" onclick={onPickLocation}>Change…</button>
          </div>
          <div class="location-item">
            <div class="muted">Data &amp; packages</div>
            <div>{bootstrap.dataDir}</div>
          </div>
          <div class="location-item">
            <div class="muted">Model cache</div>
            <div>{bootstrap.modelCacheDir}</div>
          </div>
          <div class="location-item">
            <div class="muted">Release endpoint</div>
            <div class="muted small">{bootstrap.releaseManifestUrl}</div>
          </div>
        </div>
      {/if}

      {#if error}<p class="error">{error}</p>{/if}

      <footer class="footer-actions hub-footer">
        <button class="secondary" type="button" onclick={onBack}>Back</button>
        <button class="primary" type="button" disabled={busy} onclick={onInstall}>Install</button>
      </footer>
    </div>

    <InstallationSidebar
      tree={selection.tree}
      sizeLabel={selection.sizeLabel}
      warnings={selection.warnings}
    />
  </div>
</section>
