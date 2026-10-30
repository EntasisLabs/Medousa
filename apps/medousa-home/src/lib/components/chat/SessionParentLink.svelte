<script lang="ts">
  import { chat } from '$lib/stores/chat.svelte';
  import { connection } from '$lib/stores/connection.svelte';
  import { sidebarKey } from '$lib/utils/sidebarState';
  import { sessionFamilies } from '$lib/utils/sessionRelations';
  import { formatSessionLabel } from '$lib/utils/formatSession';
  import { sidebarRelations } from './sidebarRelations.svelte';
  let { sessionId, onOpen }: { sessionId: string; onOpen: (id: string) => void | Promise<void> } = $props();
  const scope = $derived(sidebarKey(chat.workshopScopeId, connection.health?.runtime?.authority_id ?? '', connection.health?.active_profile_id ?? '', 'families'));
  $effect(() => { sidebarRelations.load(scope); });
  const families = $derived(sessionFamilies(chat.sessions, sidebarRelations.scope === scope ? sidebarRelations.links : [], new Set()));
  const parent = $derived(chat.sessions.find(session => session.session_id === families.parents.get(sessionId)));
</script>
{#if parent}
  <button type="button" class="parent-link" onclick={() => void onOpen(parent.session_id)} title={'Return to ' + formatSessionLabel(parent)}>
    From: {formatSessionLabel(parent)}
  </button>
{/if}
<style>
  .parent-link { flex: 0 0 auto; align-self: flex-start; max-width: calc(100% - 32px); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; margin: 4px 16px; font-size: 11px; color: rgb(var(--theme-text-secondary)); }
  .parent-link:hover { text-decoration: underline; }
  .parent-link:focus-visible { outline: 1px solid currentColor; outline-offset: 2px; }
</style>
