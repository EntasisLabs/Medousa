<script lang="ts">
  import { onMount, tick, setContext } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { Search, PanelLeft, ArrowUpLeft } from "@lucide/svelte";
  import LazyFeatureView from "$lib/components/layout/LazyFeatureView.svelte";
  const loadChat = () => import("$lib/components/chat/ChatPanel.svelte");
  const loadCreate = () => import("$lib/components/create/CreatePanel.svelte");
  const loadCode = () => import("$lib/components/work/UndertakingsPanel.svelte");
  const loadTerminal = () => import("$lib/components/terminal/DesktopTerminal.svelte");
  import SessionSidebar from "$lib/components/chat/SessionSidebar.svelte";
  import { WINDOW_SAVE, type WindowSaveRegistration } from "$lib/desktop/windowSave";
  import LmeCodeExplorer from "$lib/components/lme/explorers/LmeCodeExplorer.svelte";
  import CodeRepositoryTree from "$lib/components/lme/explorers/CodeRepositoryTree.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { codeWorkspace } from "$lib/stores/codeWorkspace.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { workshops } from "$lib/stores/workshops.svelte";
  import { creativeBridgeApi } from "$lib/create/creativeBridge";
  import { flushCreativeProject } from "$lib/create/creativeWorkspace.svelte";
  import { connectDesktopWindow } from "$lib/desktop/connect";
  import { openDesktopWindow, toggleDesktopSpotlight, type DesktopTarget } from "$lib/desktop/windows";
  import { showMainWindow } from "$lib/window";
  import ToastHost from "$lib/components/layout/ToastHost.svelte";
  import "$lib/styles/lme.postcss";

  let target = $state<DesktopTarget | null>(null), ready = $state(false), error = $state<string | null>(null);
  let closing = $state(false), closeFailed = $state(false), rail = $state(true);
  let saveCode: (() => Promise<boolean>) | null = null;
  setContext<WindowSaveRegistration>(WINDOW_SAVE, save => { saveCode = save; return () => { if (saveCode === save) saveCode = null; }; });
  let navigateTail = Promise.resolve();
  let pendingNavigation: DesktopTarget | null = null;
  function navigate(next: DesktopTarget) {
    if (!ready) { pendingNavigation = next; return; }
    navigateTail = navigateTail.then(async () => {
      if (!target || next.kind !== target.kind || next.workshopId !== target.workshopId || next.resourceId !== target.resourceId) return;
      if (next.kind === "code" && next.resourceId && next.path) await codeWorkspace.open(next.resourceId, next.path);
    }).catch(cause => { error = cause instanceof Error ? cause.message : String(cause); });
  }
  async function initialize() {
    if (ready || !target) return;
    try {
      if (target.kind === "chat" && target.resourceId) { await chat.switchSession(target.resourceId); void chat.tryReattachActiveTurn(); }
      if (target.kind === "code" && target.resourceId) {
        await undertakings.select(target.resourceId);
        if (undertakings.detail?.id !== target.resourceId) throw new Error(undertakings.error ?? "Couldn’t open this project");
        await codeWorkspace.hydrate(target.resourceId);
        if (target.path) await codeWorkspace.open(target.resourceId, target.path);
      }
      ready = true; error = null;
      if (pendingNavigation) { const next = pendingNavigation; pendingNavigation = null; navigate(next); }
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause); }
  }
  function openChat(id: string, title?: string) { return openDesktopWindow({ kind: "chat", workshopId: target!.workshopId, resourceId: id, title }); }
  async function newChat() {
    try {
      const response = await creativeBridgeApi(target!.workshopId).newSession("New conversation");
      await openChat(response.session_id, "New conversation");
      await chat.refreshSessions({ force: true });
    } catch (cause) { report(cause); }
  }
  function openCanvas(id: string, scope: string, title: string) { void openDesktopWindow({ kind: "create", workshopId: scope, resourceId: id, title }).catch(report); }
  async function openCode(id: string, title: string) { await openDesktopWindow({ kind: "code", workshopId: target!.workshopId, resourceId: id, title }); }
  function report(cause: unknown) { error = cause instanceof Error ? cause.message : String(cause); }
  async function close(discard = false) {
    if (closing) return;
    closing = true; closeFailed = false;
    try {
      chat.flushDraftPersist();
      // Commit pending text DOM changes before checking editor dirty state.
      await tick();
      if (!discard && target?.resourceId) {
        if (target.kind === "create") await flushCreativeProject(target.workshopId, target.resourceId);
        if (target.kind === "code" && saveCode && !(await saveCode())) throw new Error("Some files couldn’t save. Keep editing to resolve them, or close without saving this window’s changes.");
      }
      await getCurrentWindow().destroy();
    } catch (cause) { report(cause); closeFailed = true; }
    finally { closing = false; }
  }
  $effect(() => { if (ready && target?.kind === "chat" && target.resourceId) { void chat.draft; chat.scheduleDraftPersist(); } });
  onMount(() => {
    let disposed = false, disconnect: (() => void) | undefined;
    const stops: (() => void)[] = [];
    void (async () => {
      try {
        const context = await invoke<DesktopTarget>("desktop_window_context");
        if (disposed) return;
        target = context;
        for (const pending of [listen<DesktopTarget>("desktop-resource-navigate", e => navigate(e.payload)), listen("popout-close-requested", () => void close())]) {
          const stop = await pending; if (disposed) stop(); else stops.push(stop);
        }
        if (disposed) return;
        target = await invoke<DesktopTarget>("desktop_window_context");
        if (disposed) return;
        disconnect = connectDesktopWindow(context.workshopId, initialize, health => { if (health && !health.ok) error = health.message; });
      } catch (cause) { report(cause); }
    })();
    return () => { disposed = true; stops.forEach(stop => stop()); disconnect?.(); };
  });
</script>

<div class="desktop-work" class:desktop-work-closing={closing}>
  <header>
    <button type="button" title="Open Medousa" onclick={() => void showMainWindow()}><ArrowUpLeft size={14} />Medousa</button>
    <span class="desktop-work-location">{workshops.activeLabel} <span>/</span> {target?.title || (target?.kind === "chat" ? "Chat" : target?.kind === "create" ? "Create" : target?.kind === "terminal" ? "Terminal" : "Code")}</span>
    <div class="desktop-work-actions">
      {#if target?.kind === "code" && target.resourceId}<button type="button" title="Toggle files" aria-pressed={rail} onclick={() => rail = !rail}><PanelLeft size={15}/></button>{/if}
      <button type="button" title="Desktop Spotlight" onclick={() => void toggleDesktopSpotlight()}><Search size={15} />Spotlight</button>
    </div>
  </header>
  {#if error}<div class="desktop-work-error" role="alert">{error}<button type="button" onclick={() => { error = null; if (!ready) void initialize(); }}> {ready ? "Dismiss" : "Try again"}</button></div>{/if}
  {#if closeFailed}<div class="desktop-work-error"><button type="button" onclick={() => { closeFailed = false; error = null; }}>Keep editing</button><button type="button" onclick={() => void close(true)}>Close without saving</button></div>{/if}
  <div class="desktop-work-content">
    {#if !ready}<p class="m-auto p-8 text-content-quiet" role="status">{error ? "This window couldn’t connect to its workshop." : "Opening your workspace…"}</p>
    {:else if target?.kind === "create"}<LazyFeatureView loader={loadCreate} documentId={target.resourceId ?? null} workshopId={target.workshopId} onOpenProject={openCanvas} onBrowse={() => void openDesktopWindow({ kind: "create", workshopId: target!.workshopId }).catch(report)} />
    {:else if target?.kind === "chat"}
      {#if target.resourceId}<LazyFeatureView loader={loadChat} visible popout onOpenSession={async (id: string) => { await openChat(id); }} onNewSession={newChat} />{:else}<SessionSidebar open variant="inline" onOpenSession={(id, title) => { void openChat(id, title).catch(report); }} onNewSession={newChat}/><div class="desktop-work-empty"><h1>Chat</h1><p>Pick a conversation, or start something new.</p><button type="button" onclick={() => void newChat().catch(report)}>New chat</button></div>{/if}
    {:else if target?.kind === "code"}
      {#if target.resourceId}
        {#if rail}<aside class="desktop-work-files"><CodeRepositoryTree workId={target.resourceId} prepared={Boolean(undertakings.active?.worktree)} fill /></aside>{/if}
        <LazyFeatureView loader={loadCode} workId={target.resourceId} showBrowser={false} />
      {:else}<aside class="desktop-work-files"><LmeCodeExplorer onOpenProject={openCode}/></aside><div class="desktop-work-empty"><h1>Code</h1><p>Open a project to edit files and run your app.</p></div>{/if}
    {:else if target?.kind === "terminal" && target.resourceId}
      <LazyFeatureView loader={loadTerminal} sessionId={target.resourceId} workshopId={target.workshopId} title={target.title ?? "Terminal"} onClose={() => void close()} />
    {/if}
  </div>
  <ToastHost />
</div>

<style>
  .desktop-work { height: 100dvh; display: flex; flex-direction: column; background: rgb(var(--theme-canvas)); color: rgb(var(--theme-text)); }
  header { height: 44px; flex-shrink: 0; display: flex; align-items: center; gap: 18px; padding: 0 14px; border-bottom: 1px solid rgb(var(--theme-border) / .3); font-size: 12px; }
  button { display: inline-flex; align-items: center; gap: 6px; padding: 5px 8px; border-radius: 5px; }
  button:hover { background: rgb(var(--theme-border) / .2); }
  .desktop-work-location { min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .desktop-work-location span { opacity: .4; padding: 0 8px; }
  .desktop-work-actions { margin-left: auto; display: flex; gap: 4px; flex-shrink: 0; }
  .desktop-work-content { display: flex; flex: 1; min-height: 0; min-width: 0; overflow: hidden; }
  .desktop-work-files { width: 260px; flex-shrink: 0; overflow: auto; border-right: 1px solid rgb(var(--theme-border) / .3); }
  .desktop-work-empty { margin: auto; color: rgb(var(--theme-text-muted)); font-size: 13px; }
  h1 { font-size: 22px; color: rgb(var(--theme-text)); margin-bottom: 8px; }
  .desktop-work-error { padding: 8px 16px; font-size: 12px; border-bottom: 1px solid rgb(var(--theme-border) / .3); }
  .desktop-work-closing .desktop-work-content { pointer-events: none; opacity: .7; }
</style>
