<script lang="ts">
  import BotAvatar from "$lib/components/chat/BotAvatar.svelte";
  import { bots } from "$lib/stores/bots.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { shellTabs } from "$lib/stores/shellTabs.svelte";
  import { environment } from "$lib/stores/environment.svelte";
  import { activeWorkshopId } from "$lib/utils/workshopLocality";
  import { getEnvironmentSpec, putEnvironmentSpec } from "$lib/daemon/environment";
  import { botNavigationAccent, setBotNavigationPinned } from "$lib/utils/botNavigation";
  import type { BotProfile } from "$lib/types/generated/daemon_api";

  let { editing = false, onSelect }: { editing?: boolean; onSelect: (surface: string) => void } = $props();
  let busy = $state(false);
  let error = $state<string | null>(null);
  const pinIds = $derived(environment.spec?.nav?.botPins ?? []);
  const available = $derived(bots.bots.filter((bot) => !bot.archived));
  const pinned = $derived(pinIds.map((id) => available.find((bot) => bot.bot_id === id)).filter((bot): bot is BotProfile => Boolean(bot)));
  $effect(() => {
    if (pinIds.length || editing) void bots.refresh().catch(() => {});
  });
  async function open(bot: BotProfile) {
    if (busy) return;
    const scope = activeWorkshopId();
    busy = true; error = null;
    try {
      const response = await bots.open(bot);
      if (scope !== activeWorkshopId()) return;
      await chat.refreshSessions({ force: true });
      if (scope !== activeWorkshopId()) return;
      const sessionId = response.binding.session_id;
      const tab = shellTabs.openChat(sessionId, { activate: true, title: response.bot.display_name });
      if (!tab) await chat.switchSession(sessionId);
      if (scope === activeWorkshopId()) onSelect("chat");
    } catch (err) {
      if (scope === activeWorkshopId()) error = String(err);
    } finally { busy = false; }
  }
  async function pin(botId: string, value: boolean) {
    if (busy) return;
    const scope = activeWorkshopId();
    const profile = environment.spec?.profileId;
    busy = true; error = null;
    try {
      // Fresh-read, preserving unrelated configuration. PUT has no server CAS.
      const current = await getEnvironmentSpec(profile);
      if (scope !== activeWorkshopId()) return;
      setBotNavigationPinned(current.spec, botId, value);
      const saved = await putEnvironmentSpec({ spec: current.spec });
      if (scope === activeWorkshopId()) environment.applySpec(saved.spec, saved.revision);
    } catch (err) {
      if (scope === activeWorkshopId()) error = `Navigation save not acknowledged. Refresh before retrying: ${String(err)}`;
    } finally { busy = false; }
  }
</script>

{#if pinned.length || editing}
  <div class="bot-nav" aria-label="Bot shortcuts">
    {#each pinned as bot (bot.bot_id)}
      <div class="bot-nav-row">
        <button type="button" class="workshop-rail-btn bot-nav-open" title={bot.display_name} disabled={busy}
          style:border-color={botNavigationAccent(environment.spec, bot.bot_id)} onclick={() => void open(bot)}>
          <BotAvatar reference={bot.avatar_ref} size={24} />
          <span class="workshop-rail-btn-label">{bot.display_name}</span>
        </button>
        {#if editing}<button type="button" disabled={busy} aria-label={`Unpin ${bot.display_name}`} onclick={() => void pin(bot.bot_id, false)}>−</button>{/if}
      </div>
    {/each}
    {#if editing}
      {#each available.filter((bot) => !pinIds.includes(bot.bot_id)) as bot (bot.bot_id)}
        <button type="button" class="workshop-rail-btn" disabled={busy} onclick={() => void pin(bot.bot_id, true)} aria-label={`Pin ${bot.display_name}`}>
          <BotAvatar reference={bot.avatar_ref} size={24} /><span class="workshop-rail-btn-label">{bot.display_name}</span><span aria-hidden="true">+</span>
        </button>
      {/each}
    {/if}
    {#if error || bots.error}<p role="alert">{error ?? bots.error}</p>{/if}
  </div>
{/if}

<style>
  .bot-nav { display: flex; flex-direction: column; gap: 4px; }
  .bot-nav-row { display: flex; align-items: center; gap: 4px; }
  .bot-nav-open { flex: 1; border: 1px solid transparent; }
  p { font-size: 11px; overflow-wrap: anywhere; }
</style>
