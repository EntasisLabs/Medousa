<script lang="ts">
  import { tick, untrack } from "svelte";
  import { Plus, Search, X, Ellipsis, ChevronDown, Check, RefreshCw, Scan } from "@lucide/svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import CreateProjectRow from "./CreateProjectRow.svelte";
  import type { CreateBrowser } from "$lib/create/createBrowser.svelte";
  import { creativeStarters } from "$lib/create/creativeStarters";
  import type { CreativeStarter } from "$lib/create/creativeScene";
  import { saveCreativeRailPreferences } from "$lib/create/creativeRailPreferences";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { portLmeDock } from "$lib/utils/lmeDockHost";
  import { ensureRailPopoverOpen } from "$lib/utils/railPopoverChrome";
  import "$lib/styles/lme.postcss";
  import "$lib/styles/vault-workshop.postcss";
  import "$lib/styles/create-rail.postcss";
  let { browser, scope, onopen, oncreate, onimport = () => {} }: {
    browser: CreateBrowser; scope: string; onopen: (id: string, title: string) => void;
    oncreate: (starter: CreativeStarter) => void; onimport?: () => void;
  } = $props();
  let searching = $state(false), query = $state(""), newMenu = $state(false), options = $state(false);
  let input = $state<HTMLInputElement | null>(null), list = $state<HTMLDivElement | null>(null);
  let searchButton = $state<HTMLButtonElement | null>(null);
  const projects = $derived(browser.projects.filter(p => p.title.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()))
    .toSorted((a, b) => browser.rail.sort === "name"
      ? a.title.localeCompare(b.title, undefined, { numeric: true, sensitivity: "base" }) || a.id.localeCompare(b.id)
      : b.updated_at.localeCompare(a.updated_at) || a.id.localeCompare(b.id)));
  const groups = $derived([
    { id: "pinnedOpen" as const, label: "Pinned", items: projects.filter(p => p.pinned) },
    { id: "recentOpen" as const, label: "Recent", items: projects.filter(p => !p.pinned) },
  ]);
  $effect(() => { saveCreativeRailPreferences(scope, browser.rail); });
  $effect(() => {
    const node = list, owner = browser;
    void owner.projects;
    if (!node || owner.loading) return;
    const top = untrack(() => owner.rail.scrollTop);
    void tick().then(() => { if (list === node) node.scrollTop = top; });
  });
  async function openSearch() {
    newMenu = false; options = false;
    await ensureRailPopoverOpen();
    searching = true; await tick(); input?.focus();
  }
  function closeSearch() { query = ""; searching = false; void tick().then(() => searchButton?.focus()); }
  async function focusProject(id: string) {
    await tick();
    const rows = [...(list?.querySelectorAll<HTMLElement>("[data-project-id]") ?? [])];
    const row = rows.find(node => node.dataset.projectId === id);
    (row?.querySelector<HTMLButtonElement>("[data-project-open]") ?? list?.querySelector<HTMLButtonElement>("[data-rail-nav]"))?.focus();
  }
  function navigate(event: KeyboardEvent) {
    if (!(event.target instanceof HTMLElement) || !event.target.matches("[data-rail-nav]")) return;
    const keys = ["ArrowDown", "ArrowUp", "Home", "End"];
    if (!keys.includes(event.key) || event.metaKey || event.ctrlKey || event.altKey) return;
    const items = [...(list?.querySelectorAll<HTMLButtonElement>("[data-rail-nav]") ?? [])];
    const index = items.indexOf(event.target as HTMLButtonElement);
    const next = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1 : Math.max(0, Math.min(items.length - 1, index + (event.key === "ArrowDown" ? 1 : -1)));
    event.preventDefault(); event.stopPropagation(); items[next]?.focus();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions (Arrow keys move focus between the rail's native buttons.) -->
<aside class="create-project-rail" aria-label="Creative projects" onkeydown={navigate}>
  <header class="lme-side-rail-dock create-rail-dock" use:portLmeDock>
    {#if searching}
      <div class="lme-dock-search-expand create-rail-search">
        <Search size={14} strokeWidth={1.75}/>
        <input bind:this={input} bind:value={query} type="search" placeholder="Search projects…" aria-label="Search creative projects"
          onkeydown={(event) => { event.stopPropagation(); if (event.key === "Escape") { event.preventDefault(); closeSearch(); } }}/>
        <button type="button" class="vault-dock-icon-btn" aria-label="Close project search" onclick={closeSearch}><X size={14}/></button>
      </div>
    {:else}
      <span class="create-rail-title">Projects</span>
      <button bind:this={searchButton} type="button" class="vault-dock-icon-btn" aria-label="Search projects" title="Search projects" onclick={() => void openSearch()}><Search size={15} strokeWidth={1.75}/></button>
      <OverflowMenu bind:open={newMenu} label="New project" panelClass="vault-dock-popover create-rail-menu" panelWidth={224}>
        {#snippet trigger({ open, toggle })}
          <button type="button" class="vault-dock-icon-btn" aria-label="New project" title="New project" aria-haspopup="menu" aria-expanded={open}
            disabled={browser.creating || Boolean(browser.pending)} onclick={toggle}><Plus size={16} strokeWidth={1.75}/></button>
        {/snippet}
        <button type="button" role="menuitem" onclick={() => { newMenu = false; onimport(); }}><Scan size={14}/>Import page</button>
        {#each creativeStarters as starter}
          <button type="button" role="menuitem" onclick={() => { newMenu = false; oncreate(starter.id); }}><starter.icon size={14}/>{starter.label}</button>
        {/each}
      </OverflowMenu>
      <OverflowMenu bind:open={options} label="Project list options" panelClass="vault-dock-popover create-rail-menu" panelWidth={210}>
        {#snippet trigger({ open, toggle })}
          <button type="button" class="vault-dock-icon-btn" aria-label="Project list options" title="Project list options" aria-haspopup="menu" aria-expanded={open} onclick={toggle}><Ellipsis size={15}/></button>
        {/snippet}
        <div class="create-rail-menu-label">Sort projects</div>
        {#each [{ id: "recent", label: "Recently edited" }, { id: "name", label: "Name" }] as sort}
          <button type="button" role="menuitemcheckbox" aria-checked={browser.rail.sort === sort.id} onclick={() => { browser.rail.sort = sort.id as "recent" | "name"; options = false; }}>
            <Check size={14} style={`opacity:${browser.rail.sort === sort.id ? 1 : 0}`}/>{sort.label}</button>
        {/each}
        <div class="vault-dock-popover__sep" role="separator"></div>
        <button type="button" role="menuitem" disabled={browser.loading} onclick={() => { options = false; void browser.load(); }}><RefreshCw size={14}/>Refresh projects</button>
      </OverflowMenu>
    {/if}
  </header>
  {#if browser.error}<div class="create-rail-error" role="alert">{browser.error}<button type="button" onclick={() => browser.pending ? oncreate("blank") : void browser.load()}>Retry</button></div>{/if}
  <div class="create-rail-list" bind:this={list} onscroll={() => { if (list) browser.rail.scrollTop = list.scrollTop; }} aria-busy={browser.loading}>
    {#each groups as group}
      {#if group.items.length}
        <button type="button" class="create-rail-section" data-rail-nav aria-expanded={Boolean(query.trim()) || browser.rail[group.id]}
          onclick={() => { browser.rail[group.id] = !browser.rail[group.id]; }}
          onkeydown={(event) => { if (event.key === "ArrowLeft" || event.key === "ArrowRight") { event.preventDefault(); event.stopPropagation(); browser.rail[group.id] = event.key === "ArrowRight"; } }}>
          <ChevronDown size={12} class={query.trim() || browser.rail[group.id] ? "" : "create-rail-collapsed"}/>{group.label}
        </button>
        {#if query.trim() || browser.rail[group.id]}
          {#each group.items as project (project.id)}
            <CreateProjectRow {project} {scope} current={shellTabs.activeTab?.kind === "create" && shellTabs.activeTab.documentId === project.id && shellTabs.activeTab.workshopId === scope}
              onfocus={() => void focusProject(project.id)} onopen={() => onopen(project.id, project.title)}/>
          {/each}
        {/if}
      {/if}
    {/each}
    {#if !projects.length}<p class="create-rail-empty" role="status">{browser.loading ? "Loading projects…" : query.trim() ? "No matching projects" : "Start with a blank canvas."}</p>{/if}
    {#if browser.cursor}<button type="button" class="create-rail-load" disabled={browser.loading} onclick={() => void browser.load(true)}>Load more projects</button>{/if}
  </div>
</aside>
