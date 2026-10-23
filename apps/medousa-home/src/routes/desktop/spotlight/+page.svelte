<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import CommandSpotlight from "$lib/components/layout/CommandSpotlight.svelte";
  import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { creativeApi } from "$lib/create/creativeApi";
  import { creativeBridgeApi } from "$lib/create/creativeBridge";
  import { desktopWindowCommands } from "$lib/desktop/commands";
  import { connectDesktopWindow } from "$lib/desktop/connect";
  import { openDesktopWindow } from "$lib/desktop/windows";
  import { showMainWindow } from "$lib/window";
  import type { CreativeProjectSummary } from "$lib/types/generated/daemon_api";
  import type { ItemProjection } from "$lib/forge";

  let canvases = $state<CreativeProjectSummary[]>([]), projects = $state<ItemProjection[]>([]);
  let ready = $state(false), error = $state<string | null>(null), shortcutError = $state<string | null>(null);
  let generation = 0;
  const commands = $derived(desktopWindowCommands(workshops.activeWorkshopId, chat.sessions, canvases, projects, openDesktopWindow, newChat, showMainWindow));
  async function newChat() {
    const scope = workshops.activeWorkshopId;
    const response = await creativeBridgeApi(scope).newSession("New conversation");
    await openDesktopWindow({ kind: "chat", workshopId: scope, resourceId: response.session_id, title: "New conversation" });
  }
  async function refresh() {
    const epoch = ++generation;
    try {
      await workshops.load({ restoreShell: false });
      if (epoch !== generation) return;
      const scope = workshops.activeWorkshopId;
      canvases = []; projects = [];
      commandSpotlight.openSpotlight(); ready = true; error = null;
      const results = await Promise.allSettled([chat.refreshSessions({ force: true }), creativeApi(scope).list(), creativeBridgeApi(scope).projects()]);
      if (epoch !== generation || scope !== workshops.activeWorkshopId) return;
      if (results[1].status === "fulfilled") canvases = results[1].value.projects;
      if (results[2].status === "fulfilled") projects = results[2].value;
      const failures = results.filter(r => r.status === "rejected");
      if (failures.length) error = "Some recent work couldn’t load. You can still open Chat, Create, or Code.";
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause); }
  }
  function dismiss() { void getCurrentWindow().hide(); }
  onMount(() => {
    let disposed = false;
    const stops: (() => void)[] = [];
    void listen("desktop-spotlight-open", () => { commandSpotlight.openSpotlight(); requestAnimationFrame(() => document.querySelector<HTMLInputElement>(".command-spotlight-input")?.focus()); void refresh(); }).then(stop => disposed ? stop() : stops.push(stop));
    void invoke<string | null>("desktop_shortcut_error").then(value => { shortcutError = value; });
    const stop = connectDesktopWindow(undefined, refresh, health => { if (health && !health.ok) { error = health.message; ready = true; commandSpotlight.openSpotlight(); } }, "observer");
    return () => { disposed = true; generation++; stops.forEach(stop => stop()); stop(); };
  });
</script>

<main class="desktop-launcher">
  <header data-tauri-drag-region><span>Medousa <span class="text-content-quiet">/ {workshops.activeLabel}</span></span><span class="text-content-quiet">⌘ / Ctrl ⇧ Space</span></header>
  {#if ready}<CommandSpotlight desktopCommands={commands} onDismiss={dismiss} />{:else}<p class="p-6 text-content-quiet" role="status">Connecting to your workshop…</p>{/if}
  {#if error || shortcutError}<p class="desktop-launcher-notice" role="status">{error ?? shortcutError}</p>{/if}
</main>

<style>
  :global(body:has(.desktop-launcher)) { background: transparent; }
  .desktop-launcher { height: 100dvh; border-radius: 14px; overflow: hidden; color: rgb(var(--theme-text)); background: rgb(var(--theme-pane)); border: 1px solid rgb(var(--theme-border) / .45); }
  header { height: 38px; display: flex; justify-content: space-between; align-items: center; padding: 0 20px; font-size: 11px; user-select: none; }
  :global(.command-spotlight-backdrop.command-spotlight-desktop) { position: absolute; top: 38px; padding: 0; background: transparent; backdrop-filter: none; }
  :global(.command-spotlight-desktop .command-spotlight-panel) { width: 100%; height: 100%; max-height: none; border: none; border-radius: 0; box-shadow: none; }
  .desktop-launcher-notice { position: fixed; z-index: 201; bottom: 34px; left: 16px; right: 16px; font-size: 11px; padding: 8px 12px; border-radius: 6px; background: rgb(var(--theme-pane)); color: rgb(var(--theme-text-muted)); }
</style>
