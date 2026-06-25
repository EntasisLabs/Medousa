<script lang="ts">
  import { onMount } from "svelte";
  import { Search } from "@lucide/svelte";
  import ComponentRow from "../components/ComponentRow.svelte";
  import InstallationSidebar from "../components/InstallationSidebar.svelte";
  import WorkloadCard from "../components/WorkloadCard.svelte";
  import { categoryLabel, truncatePath } from "../copy";
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
    modifyMode: boolean;
    error: string | null;
    onTabChange: (tab: HubTab) => void;
    onSelectProfile: (id: string) => void;
    onTogglePackage: (id: string) => void;
    onPickLocation: () => void;
  }

  let {
    bootstrap,
    tab,
    profiles,
    packages,
    selectedProfileId,
    selection,
    modifyMode,
    error,
    onTabChange,
    onSelectProfile,
    onTogglePackage,
    onPickLocation,
  }: Props = $props();

  let search = $state("");
  let searchInput: HTMLInputElement | undefined = $state();

  const filteredPackages = $derived(
    packages.filter((pkg) => {
      const q = search.trim().toLowerCase();
      if (!q) return true;
      return (
        pkg.displayName.toLowerCase().includes(q) ||
        pkg.categoryLabel.toLowerCase().includes(q)
      );
    }),
  );

  const groupedPackages = $derived.by(() => {
    const groups = new Map<string, PackageSummary[]>();
    for (const pkg of filteredPackages) {
      const key = pkg.categoryLabel || categoryLabel(pkg.category);
      if (!groups.has(key)) groups.set(key, []);
      groups.get(key)!.push(pkg);
    }
    return [...groups.entries()];
  });

  const profileSections = $derived.by(() => {
    const sections = new Map<string, ProfileSummary[]>();
    for (const profile of profiles) {
      const key = profile.section || "Desktop & Core";
      if (!sections.has(key)) sections.set(key, []);
      sections.get(key)!.push(profile);
    }
    return [...sections.entries()];
  });

  onMount(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
        if (tab === "components") {
          event.preventDefault();
          searchInput?.focus();
        }
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });
</script>

<section class="hub">
  {#if modifyMode}
    <div class="modify-banner" role="status">Modify your Medousa installation</div>
  {/if}

  <header class="screen-header">
    <h1>{modifyMode ? "Modify installation" : "Customize installation"}</h1>
    <p class="lead">Choose workloads and components for your installation.</p>
  </header>

  <div class="hub-body">
    <div class="hub-main">
      <div class="tabs" role="tablist" aria-label="Installation sections">
        <button
          class="tab"
          class:active={tab === "workloads"}
          type="button"
          role="tab"
          aria-selected={tab === "workloads"}
          onclick={() => onTabChange("workloads")}
        >
          Workloads
        </button>
        <button
          class="tab"
          class:active={tab === "components"}
          type="button"
          role="tab"
          aria-selected={tab === "components"}
          onclick={() => onTabChange("components")}
        >
          Individual components
        </button>
        <button
          class="tab"
          class:active={tab === "locations"}
          type="button"
          role="tab"
          aria-selected={tab === "locations"}
          onclick={() => onTabChange("locations")}
        >
          Installation locations
        </button>
      </div>

      {#if tab === "workloads"}
        {#each profileSections as [section, items] (section)}
          <div class="section-label">{section}</div>
          <div class="profile-grid" role="group" aria-label={section}>
            {#each items as profile (profile.id)}
              <WorkloadCard
                title={profile.displayName}
                description={profile.description}
                sizeLabel={profile.sizeLabel}
                icon={profile.icon}
                selected={selectedProfileId === profile.id}
                onclick={() => onSelectProfile(profile.id)}
              />
            {/each}
          </div>
        {/each}
      {:else if tab === "components"}
        <div class="search-row">
          <span class="search-icon" aria-hidden="true">
            <Search size={16} strokeWidth={1.75} />
          </span>
          <input
            bind:this={searchInput}
            class="search-input"
            type="search"
            placeholder="Search components"
            bind:value={search}
            aria-label="Search components"
          />
          <span class="search-hint">Ctrl+F</span>
        </div>
        {#each groupedPackages as [label, items] (label)}
          <div class="section-label">{label}</div>
          <div class="card packages">
            {#each items as pkg (pkg.id)}
              <ComponentRow
                name={pkg.displayName}
                sizeLabel={pkg.sizeLabel}
                selected={pkg.selected}
                optional={pkg.optional}
                installed={pkg.installed}
                updateAvailable={pkg.updateAvailable}
                ontoggle={() => onTogglePackage(pkg.id)}
              />
            {/each}
          </div>
        {/each}
      {:else}
        <div class="card locations">
          <div class="location-item">
            <div class="location-label">Application</div>
            <div class="location-value" title={bootstrap.installRoot}>
              {truncatePath(bootstrap.installRoot, 48)}
            </div>
            <button class="link-btn" type="button" onclick={onPickLocation}>Change…</button>
          </div>
          <div class="location-item">
            <div class="location-label">Data</div>
            <div class="location-value">{bootstrap.dataDir}</div>
          </div>
          <div class="location-item">
            <div class="location-label">Models</div>
            <div class="location-value">{bootstrap.modelCacheDir}</div>
          </div>
        </div>
      {/if}

      {#if error}<p class="error">{error}</p>{/if}
    </div>

    <InstallationSidebar tree={selection.tree} warnings={selection.warnings} />
  </div>
</section>
