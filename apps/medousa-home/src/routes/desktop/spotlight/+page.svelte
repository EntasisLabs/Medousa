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
  import { spotlightDismissal, type SpotlightPointer } from "$lib/desktop/spotlightDismissal";
  import { createDesktopSpotlightHost } from "$lib/desktop/spotlightHost";
  import { openDesktopWindow } from "$lib/desktop/windows";
  import { prepareForWorkshopSwitch } from "$lib/workshopConnection";
  import { showMainWindow } from "$lib/window";
  import { friendlyUserError } from "$lib/utils/normieErrors";
  import type { SpotlightHost } from "$lib/spotlight/host";
  import type { CreativeProjectSummary } from "$lib/types/generated/daemon_api";
  import type { ItemProjection } from "$lib/forge";
  import { terminalSessions, type TerminalSessionSummary } from "$lib/terminal";

  let canvases = $state<CreativeProjectSummary[]>([]), projects = $state<ItemProjection[]>([]);
  let terminals = $state<TerminalSessionSummary[]>([]);
  let host = $state<SpotlightHost>();
  let spotlight = $state<{ dismiss: () => void }>();
  let ready = $state(false), error = $state<string | null>(null);
  let pendingActions = $state(0);
  let generation = 0, scope = "", disposed = false, visibleRequested = true;
  let disconnect: (() => void) | undefined;
  let opening = Promise.resolve();
  const commands = $derived(desktopWindowCommands(workshops.activeWorkshopId, chat.sessions, canvases, projects, openDesktopWindow, newChat, showMainWindow, terminals)
    .map(command => ({ ...command, id: command.id === "desktop-new-chat" ? "do-create-chat" : command.id === "desktop-chat" ? "go-chat" : command.id === "desktop-code" ? "go-code" : command.id === "desktop-create" ? "go-create" : command.id.startsWith("desktop-chat:") ? command.id.replace("desktop-chat:", "open-session:") : command.id })));
  async function newChat() {
    const owner = scope;
    const response = await creativeBridgeApi(owner).newSession("New conversation");
    await openDesktopWindow({ kind: "chat", workshopId: owner, resourceId: response.session_id, title: "New conversation" });
  }
  async function refresh(epoch: number, openPalette = false) {
    if (disposed || epoch !== generation) return;
    ready = true;
    if (visibleRequested && openPalette) commandSpotlight.openSpotlight();
    const owner = scope;
    const results = await Promise.allSettled([chat.refreshSessions({ force: true }), creativeApi(owner).list(), creativeBridgeApi(owner).projects(), terminalSessions(null)]);
    if (disposed || epoch !== generation) return;
    if (results[1].status === "fulfilled") canvases = results[1].value.projects;
    if (results[2].status === "fulfilled") projects = results[2].value;
    if (results[3].status === "fulfilled") terminals = results[3].value;
    // Optional catalogs retain their last results without interrupting the launcher.
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
    scope = owner; canvases = []; projects = []; terminals = []; pendingActions = 0;
    host = createDesktopSpotlightHost(owner, {
      isCurrent: () => !disposed && epoch === generation,
      pending: delta => { if (epoch === generation) pendingActions += delta; },
      close: () => { if (spotlight) spotlight.dismiss(); else commandSpotlight.closeSpotlight(); },
    });
    disconnect = connectDesktopWindow(owner, () => refresh(epoch, true), health => {
      if (disposed || epoch !== generation || !health) return;
      error = health.ok ? null : health.message;
      if (!health.ok) { ready = true; if (visibleRequested) commandSpotlight.openSpotlight(); }
    });
  }
  function requestOpen() {
    dismissal.cancel();
    visibleRequested = true;
    opening = opening.then(open).catch(cause => { if (!disposed) error = cause instanceof Error ? cause.message : String(cause); });
  }
  function dismiss() { dismissal.cancel(); visibleRequested = false; commandSpotlight.closeSpotlight(); void getCurrentWindow().hide(); }
  const dismissal = spotlightDismissal({
    pointer: () => invoke<SpotlightPointer | null>("desktop_spotlight_pointer"),
    releaseInside: () => { void getCurrentWindow().setFocus().catch(() => {}); },
    canDismiss: () => !disposed && visibleRequested && (!ready || commandSpotlight.open) && !pendingActions,
    dismiss: () => { visibleRequested = false; if (spotlight) spotlight.dismiss(); else dismiss(); },
  });
  onMount(() => {
    const stops: (() => void)[] = [];
    const keep = (pending: Promise<() => void>) => { void pending.then(stop => disposed ? stop() : stops.push(stop)); };
    keep(listen("desktop-spotlight-open", requestOpen));
    keep(listen("desktop-spotlight-dismiss", () => { visibleRequested = false; if (spotlight) spotlight.dismiss(); else commandSpotlight.closeSpotlight(); }));
    keep(getCurrentWindow().onFocusChanged(event => {
      // A shell handoff hides the overlay but keeps its state for an actionable failure.
      if (!event.payload && pendingActions) { dismissal.cancel(); void getCurrentWindow().hide(); return; }
      dismissal.focusChanged(event.payload);
    }));
    keep(getCurrentWindow().onDragDropEvent(event => dismissal.dragChanged(event.payload.type)));
    requestOpen();
    return () => { dismissal.cancel(); disposed = true; generation++; stops.forEach(stop => stop()); disconnect?.(); };
  });
</script>

<main class="desktop-launcher">
  <header data-tauri-drag-region><span>Medousa <span class="text-content-quiet">/ {workshops.activeLabel}</span></span></header>
  {#if ready}<CommandSpotlight bind:this={spotlight} {host} additionalCommands={commands} onDismiss={dismiss} />{:else}<p class="p-6 text-content-quiet" role="status">Connecting to your workshop…</p>{/if}
  {#if error}<details class="desktop-launcher-notice"><summary>Couldn’t connect to this workshop</summary><p>{friendlyUserError(error)}</p></details>{/if}
</main>

<style>
  :global(html:has(.desktop-launcher)),
  :global(body:has(.desktop-launcher)),
  :global(#medousa-app-root:has(.desktop-launcher)) { background: transparent !important; }
  .desktop-launcher {
    --spotlight-window-radius: 20px;
    position: relative;
    isolation: isolate;
    height: 100dvh;
    border-radius: var(--spotlight-window-radius);
    overflow: hidden;
    clip-path: inset(0 round var(--spotlight-window-radius));
    color: rgb(var(--theme-text));
    background: rgb(var(--theme-pane));
    border: 1px solid rgb(var(--theme-border) / .45);
  }
  header { height: 38px; display: flex; justify-content: space-between; align-items: center; padding: 0 20px; font-size: 11px; user-select: none; }
  :global(.command-spotlight-backdrop.command-spotlight-desktop) { position: absolute; inset: 38px 0 0; padding: 0; background: transparent; backdrop-filter: none; }
  :global(.command-spotlight-desktop .command-spotlight-panel) { width: 100%; height: 100%; max-height: none; border: none; border-radius: 0; box-shadow: none; }
  .desktop-launcher-notice { position: absolute; z-index: 201; bottom: 16px; left: 20px; right: 20px; font-size: 12px; padding: 10px 12px; border: 1px solid rgb(var(--theme-border) / .4); border-radius: 8px; background: rgb(var(--theme-pane)); color: rgb(var(--theme-text-secondary)); }
  summary { cursor: pointer; }
  .desktop-launcher-notice p { margin: 8px 0 0; }
</style>
