<script lang="ts">
  import { onMount, untrack } from "svelte";
  import {
    Search,
    PenTool,
    ArrowUpRight,
    Pin,
    RefreshCw,
    Scan,
  } from "@lucide/svelte";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  import { createBrowser } from "$lib/create/createBrowser.svelte";
  import {
    acquireCreativeProject,
    type CreativeProjectLease,
  } from "$lib/create/creativeWorkspace.svelte";
  import type { CreativeStarter } from "$lib/create/creativeScene";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import CreatePageImport from "./CreatePageImport.svelte";
  import CreateEditor from "./CreateEditor.svelte";
  import CreateRail from "./CreateRail.svelte";
  import { creativeStarters as starters } from "$lib/create/creativeStarters";
  import "$lib/styles/create-workspace.postcss";
  let {
    rail = false,
    documentId = null,
    workshopId = null,
    onPick = () => {},
    onOpenProject = null,
    onBrowse = () => shellTabs.openSurface("create"),
  }: {
    rail?: boolean;
    documentId?: string | null;
    workshopId?: string | null;
    onPick?: () => void;
    onOpenProject?: ((id: string, scope: string, title: string) => void) | null;
    onBrowse?: () => void;
  } = $props();
  const scope = $derived(workshopId ?? activeWorkshopId());
  const browser = $derived(createBrowser(scope));
  let importOpen = $state(false);
  let query = $state(""),
    lease = $state.raw<CreativeProjectLease | null>(null),
    openError = $state<string | null>(null);
  const projects = $derived(
    browser.projects
      .filter((p) =>
        p.title.toLocaleLowerCase().includes(query.toLocaleLowerCase()),
      )
      .sort(
        (a, b) =>
          Number(b.pinned) - Number(a.pinned) ||
          b.updated_at.localeCompare(a.updated_at),
      ),
  );
  function open(id: string, title: string) {
    if (onOpenProject) onOpenProject(id, scope, title);
    else shellTabs.openCreative(id, scope, { title });
    onPick();
  }
  async function create(starter: CreativeStarter) {
    try {
      const doc = await browser.create(starter);
      if (activeWorkshopId() === scope) open(doc.id, doc.title);
    } catch {
      /* Browser retains the exact create request for retry. */
    }
  }
  $effect(() => {
    const current = browser;
    if (!documentId) untrack(() => void current.load());
  });
  onMount(() => {
    if (documentId) {
      try {
        lease = acquireCreativeProject(scope, documentId);
      } catch (e) {
        openError = e instanceof Error ? e.message : String(e);
      }
    }

    return () => {
      lease?.release();
    };
  });
</script>

<div class="create-root" class:create-rail={rail}>
  {#if documentId}
    {#if lease}<CreateEditor {lease} {onBrowse} />{:else}<div
        class="create-panel-loading"
        role="status"
      >
        {openError ?? "Opening canvas…"}
      </div>{/if}
  {:else if rail}
    {#key scope}<CreateRail {browser} {scope} onopen={open} onimport={() => importOpen = true} oncreate={(starter) => void create(starter)}/>{/key}
  {:else}
    <main class="create-home">
      <div class="create-home-kicker"><PenTool size={14} />Create</div>
      <h1>What are we making?</h1>
      <p class="create-home-intro">
        A place for the idea, the rough edges, and the finished thing.
      </p>
      <div class="create-starters">
        <button type="button" onclick={() => importOpen = true}><Scan size={15}/>Import page</button>
        {#each starters as starter}<button
            type="button"
            disabled={browser.creating || Boolean(browser.pending)}
            onclick={() => void create(starter.id)}
            ><starter.icon size={15} />{starter.label}</button
          >{/each}
      </div>
      {#if browser.error}<div class="create-browser-error" role="alert">
          <span>{browser.error}</span><button
            type="button"
            onclick={() =>
              browser.pending ? void create("blank") : void browser.load()}
            >Retry</button
          >
        </div>{/if}
      <div class="create-home-projects-heading">
        <h2>Your projects</h2>
        <div>
          <label class="create-search"
            ><Search size={13} /><input
              aria-label="Search creative projects"
              placeholder="Search projects…"
              bind:value={query}
            /></label
          ><button
            type="button"
            aria-label="Refresh creative projects"
            disabled={browser.loading}
            onclick={() => void browser.load()}><RefreshCw size={13} /></button
          >
        </div>
      </div>
      <div class="create-project-grid">
        {#each projects as project (project.id)}<button
            type="button"
            class="create-project-card"
            onclick={() => open(project.id, project.title)}
            ><div class="create-project-cover">
              {@render FrameIcon()}<span>{project.title.slice(0, 1)}</span>
            </div>
            <div>
              <strong>{project.title}</strong>{#if project.pinned}<Pin
                  size={11}
                />{/if}<ArrowUpRight size={13} />
            </div>
            <small
              >{new Date(project.updated_at).toLocaleDateString(undefined, {
                month: "short",
                day: "numeric",
              })} · Editable canvas</small
            ></button
          >
        {:else}<div class="create-home-empty">
            <PenTool size={28} strokeWidth={1.2} /><span
              >{browser.loading
                ? "Loading projects…"
                : query
                  ? "No matching projects"
                  : "Your next idea starts here."}</span
            >
          </div>{/each}
      </div>
      {#if browser.cursor}<button
          type="button"
          class="create-more"
          disabled={browser.loading}
          onclick={() => void browser.load(true)}>Load more projects</button
        >{/if}
    </main>
  {/if}
</div>
<CreatePageImport {scope} bind:open={importOpen} onimport={doc => open(doc.id, doc.title)}/>
{#snippet FrameIcon()}<svg
    width="48"
    height="40"
    viewBox="0 0 48 40"
    fill="none"
    aria-hidden="true"
    ><rect
      x="1"
      y="1"
      width="46"
      height="38"
      rx="2"
      stroke="currentColor"
      opacity=".2"
    /><path
      d="M10 13h21M10 19h14M10 27h28"
      stroke="currentColor"
      stroke-width="2"
      opacity=".2"
    /></svg
  >{/snippet}
