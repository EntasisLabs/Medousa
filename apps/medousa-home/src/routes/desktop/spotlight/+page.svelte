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
  import { createDesktopSpotlightHost } from "$lib/desktop/spotlightHost";
  import { openDesktopWindow } from "$lib/desktop/windows";
  import { prepareForWorkshopSwitch } from "$lib/workshopConnection";
  import { showMainWindow } from "$lib/window";
  import type { SpotlightHost } from "$lib/spotlight/host";
  import type { CreativeProjectSummary } from "$lib/types/generated/daemon_api";
  import type { ItemProjection } from "$lib/forge";

  let canvases = $state<CreativeProjectSummary[]>([]), projects = $state<ItemProjection[]>([]);
  let host = $state<SpotlightHost>();
  let spotlight = $state<{ dismiss: () => void }>();
  let ready = $state(false), error = $state<string | null>(null), shortcutError = $state<string | null>(null);
  let pendingActions = $state(0);
  let generation = 0, scope = "", disposed = false, visibleRequested = true;
  let disconnect: (() => void) | undefined;
  let opening = Promise.resolve();
  const commands = $derived(desktopWindowCommands(workshops.activeWorkshopId, chat.sessions, canvases, projects, openDesktopWindow, newChat, showMainWindow)
    .map(command => ({ ...command, id: command.id === "desktop-new-chat" ? "do-create-chat" : command.id === "desktop-chat" ? "go-chat" : command.id === "desktop-code" ? "go-code" : command.id === "desktop-create" ? "go-create" : command.id.startsWith("desktop-chat:") ? command.id.replace("desktop-chat:", "open-session:") : command.id })));
  async function newChat() {
    const owner = scope;
    const response = await creativeBridgeApi(owner).newSession("New conversation");
    await openDesktopWindow({ kind: "chat", workshopId: owner, resourceId: response.session_id, title: "New conversation" });
  }
  async function refresh(epoch: number, openPalette = false) {
    if (disposed || epoch !== generation) return;
    ready = true; error = null;
    if (visibleRequested && openPalette) commandSpotlight.openSpotlight();
    const owner = scope;
    const results = await Promise.allSettled([chat.refreshSessions({ force: true }), creativeApi(owner).list(), creativeBridgeApi(owner).projects()]);
    if (disposed || epoch !== generation) return;
    if (results[1].status === "fulfilled") canvases = results[1].value.projects;
    if (results[2].status === "fulfilled") projects = results[2].value;
    if (results.some(result => result.status === "rejected")) error = "Some recent work couldn’t load. Search and available destinations remain usable.";
  }
  async function open() {
    const owner = await invoke<string>("desktop_spotlight_context");
    if (disposed) return;
    if (owner === scope && ready) { commandSpotlight.openSpotlight(); void refresh(generation); return; }
    const epoch = ++generation;
    ready = false; commandSpotlight.closeSpotlight();
    disconnect?.(); disconnect = undefined;
    if (scope) await prepareForWorkshopSwitch();
    if (disposed) return;
    scope = owner; canvases = []; projects = []; pendingActions = 0;
    host = createDesktopSpotlightHost(owner, {
      isCurrent: () => !disposed && epoch === generation,
      pending: delta => { if (epoch === generation) pendingActions += delta; },
      close: () => { if (spotlight) spotlight.dismiss(); else commandSpotlight.closeSpotlight(); },
    });
    disconnect = connectDesktopWindow(owner, () => refresh(epoch, true), health => {
      if (!disposed && epoch === generation && health && !health.ok) { error = health.message; ready = true; if (visibleRequested) commandSpotlight.openSpotlight(); }
    });
  }
  function requestOpen() {
    visibleRequested = true;
    opening = opening.then(open).catch(cause => { if (!disposed) error = cause instanceof Error ? cause.message : String(cause); });
  }
  function dismiss() { commandSpotlight.closeSpotlight(); void getCurrentWindow().hide(); }
  onMount(() => {
    const stops: (() => void)[] = [];
    const keep = (pending: Promise<() => void>) => { void pending.then(stop => disposed ? stop() : stops.push(stop)); };
    keep(listen("desktop-spotlight-open", requestOpen));
    keep(listen("desktop-spotlight-dismiss", () => { visibleRequested = false; if (spotlight) spotlight.dismiss(); else commandSpotlight.closeSpotlight(); }));
    keep(getCurrentWindow().onFocusChanged(event => {
      visibleRequested = event.payload;
      if (!event.payload && !pendingActions) { if (spotlight) spotlight.dismiss(); else commandSpotlight.closeSpotlight(); }
    }));
    void invoke<string | null>("desktop_shortcut_error").then(value => { shortcutError = value; });
    requestOpen();
    return () => { disposed = true; generation++; stops.forEach(stop => stop()); disconnect?.(); };
  });
</script>

<main class="desktop-launcher">
  <header data-tauri-drag-region><span>Medousa <span class="text-content-quiet">/ {workshops.activeLabel}</span></span><span class="text-content-quiet">⌘ / Ctrl ⇧ Space</span></header>
  {#if ready}<CommandSpotlight bind:this={spotlight} {host} additionalCommands={commands} onDismiss={dismiss} />{:else}<p class="p-6 text-content-quiet" role="status">Connecting to your workshop…</p>{/if}
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
