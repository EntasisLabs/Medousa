<script lang="ts">
  import { ChevronDown, ChevronRight } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import { connection } from "$lib/stores/connection.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { readSidebarValue, writeSidebarValue, sidebarKey } from "$lib/utils/sidebarState";
  let { id, label, count, searching = false, initiallyCollapsed = false, children, content }: {
    id: string; label: string; count?: number; searching?: boolean; initiallyCollapsed?: boolean;
    children?: Snippet; content: Snippet;
  } = $props();
  const key = $derived(sidebarKey(chat.workshopScopeId, connection.health?.runtime?.authority_id ?? '', connection.health?.active_profile_id ?? '', id));
  let collapsed = $state(false);
  let loaded = $state('');
  $effect(() => { collapsed = readSidebarValue(key, initiallyCollapsed); loaded = key; });
  const hidden = $derived(!searching && (loaded === key ? collapsed : initiallyCollapsed));
  function toggle() { collapsed = !hidden; writeSidebarValue(key, collapsed); }
</script>
<div class="session-sidebar-section-heading">
  <button type="button" class="section-disclosure" aria-label={label} aria-expanded={!hidden} disabled={searching} title={searching ? 'Search results stay expanded' : undefined} onclick={toggle}>
    {#if hidden}<ChevronRight size={11} aria-hidden="true" />{:else}<ChevronDown size={11} aria-hidden="true" />{/if}
    <span class="session-sidebar-section-title">{label}</span>
    {#if count !== undefined}<span class="session-sidebar-section-count">{count}</span>{/if}
  </button>
  {#if children}<span class="session-sidebar-section-heading__trailing">{@render children()}</span>{/if}
</div>
{#if !hidden}{@render content()}{/if}
<style>
  .section-disclosure { display: flex; align-items: center; gap: 5px; flex: 1; min-width: 0; height: 100%; text-align: left; color: rgb(var(--theme-text-secondary)); border-radius: 4px; }
  .section-disclosure .session-sidebar-section-count { margin-left: auto; }
  .section-disclosure:hover { color: rgb(var(--theme-text)); }
  .section-disclosure:disabled { cursor: default; }
  .section-disclosure:focus-visible { outline: 1px solid currentColor; outline-offset: 2px; }
</style>
