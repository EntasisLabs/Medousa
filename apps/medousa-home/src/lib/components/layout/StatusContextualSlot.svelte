<script lang="ts">
  import { untrack } from "svelte";
  import { CircleAlert, CircleCheck } from "@lucide/svelte";
  import { graphemeScriptEditor } from "$lib/stores/graphemeScriptEditor.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { codeEditorStatus } from "$lib/stores/codeEditorStatus.svelte";
  import { undertakings } from "$lib/stores/undertakings.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultVersions } from "$lib/stores/vaultVersions.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import { formatVaultNoteStats, vaultNoteStats } from "$lib/utils/vaultNoteStats";
  import { dispatchScriptWorkbenchOpenConsole } from "$lib/utils/scriptWorkbenchChromeEvents";

  function reviewStatusLabel(issue: string): string {
    if (/no project check|project checks haven'?t run/i.test(issue)) {
      return "Project checks haven't run";
    }
    return issue;
  }

  const activeLme = $derived(lmeWorkspace.activeTab);
  const onLibrary = $derived(layout.desktopSurface === "library");

  const showVault = $derived(
    onLibrary &&
      activeLme?.kind === "note" &&
      Boolean(vault.selectedPath) &&
      !vault.noteLoading,
  );

  const showScript = $derived(
    onLibrary &&
      Boolean(graphemeScriptEditor.activeTab) &&
      (activeLme?.kind === "script" ||
        (lmeWorkspace.explorerMode === "scripts" && activeLme?.kind !== "note")),
  );

  const codeStatus = $derived(codeEditorStatus.snapshot);
  const showCode = $derived(
    activeLme?.kind === "code" &&
      activeLme.resource.kind === "file" &&
      codeStatus?.workId === activeLme.workId &&
      codeStatus.path === activeLme.resource.path,
  );
  const codeReview = $derived(
    activeLme?.kind === "code" &&
      activeLme.resource.kind === "review" &&
      undertakings.review?.work_id === activeLme.workId
      ? undertakings.review
      : null,
  );

  function showCodeProblems() {
    window.dispatchEvent(new CustomEvent("medousa-code-show-problems"));
  }

  const noteSummary = $derived(formatVaultNoteStats(vaultNoteStats(vault.content)));
  const saveWhisper = $derived(vault.saveWhisper());
  const versionsDirtyLabel = $derived.by(() => {
    if (!vaultVersions.enabled || !vaultVersions.status?.isRepo) return "";
    const dirty = vaultVersions.status.dirtyCount;
    if (dirty <= 0) return "";
    const branch = vaultVersions.status.branch ?? "main";
    return `${branch} · ${dirty} changed`;
  });

  /** Dirty / unsaved only — saved scripts stay silent. */
  const scriptDirty = $derived.by(() => {
    const tab = graphemeScriptEditor.activeTab;
    if (!tab) return "No script";
    if (tab.dirty) return "Unsaved";
    if (!tab.scriptId) return "Unsaved";
    return null;
  });

  /** In-flight or failed only — Ready / success stay off the bar. */
  const scriptAction = $derived.by(() => {
    if (graphemeScriptEditor.compileBusy) {
      return { label: "Compiling…", tone: "busy" as const };
    }
    if (graphemeScriptEditor.compileError) {
      return { label: "Compile failed", tone: "error" as const };
    }
    if (
      graphemeScriptEditor.compileResult &&
      !graphemeScriptEditor.compileResult.validated
    ) {
      return { label: "Compile invalid", tone: "warn" as const };
    }
    if (workshop.runBusy) {
      return { label: "Running…", tone: "busy" as const };
    }
    if (workshop.runError || workshop.runResult?.result?.succeeded === false) {
      return { label: "Run failed", tone: "error" as const };
    }
    return null;
  });

  const scriptError = $derived(
    graphemeScriptEditor.saveError?.trim() || null,
  );

  const scriptToneClass = $derived.by(() => {
    if (!scriptAction) return "";
    if (scriptAction.tone === "error") return "text-content-error";
    if (scriptAction.tone === "warn") return "text-content-warning";
    return "text-content-secondary";
  });

  const hasScriptChrome = $derived(
    Boolean(scriptDirty || scriptAction || scriptError),
  );

  $effect(() => {
    if (!showVault || !vaultVersions.enabled) return;
    untrack(() => {
      void vaultVersions.refresh();
    });
  });
</script>

{#if codeReview}
  <div class="status-contextual status-contextual--code" aria-label="Code review status">
    {#if codeReview.synthesis.verification}
      <span
        class="status-contextual-item status-contextual-item--icon"
        class:text-content-success={codeReview.synthesis.verification.success}
        class:text-content-error={!codeReview.synthesis.verification.success}
      >
        {#if codeReview.synthesis.verification.success}
          <CircleCheck size={11} strokeWidth={2} aria-hidden="true" />
        {:else}
          <CircleAlert size={11} strokeWidth={2} aria-hidden="true" />
        {/if}
        <span class="truncate">
          {codeReview.synthesis.verification.success ? "Checks passed" : "Checks failed"}
        </span>
      </span>
    {:else if codeReview.synthesis.unresolved_issues.length > 0}
      <span
        class="status-contextual-item status-contextual-item--icon text-content-warning"
        title={codeReview.synthesis.unresolved_issues.join(" · ")}
      >
        <CircleAlert size={11} strokeWidth={2} aria-hidden="true" />
        <span class="truncate">{reviewStatusLabel(codeReview.synthesis.unresolved_issues[0]!)}</span>
      </span>
    {:else}
      <span class="status-contextual-whisper">Review open</span>
    {/if}
  </div>
{:else if showCode && codeStatus}
  <div class="status-contextual status-contextual--code" aria-label="Code editor status">
    <span class="status-contextual-item font-mono tabular-nums">
      Ln {codeStatus.line}/{codeStatus.totalLines}, Col {codeStatus.column}
    </span>
    <span class="status-contextual-sep" aria-hidden="true">·</span>
    <span class="status-contextual-item">{codeStatus.indentation}</span>
    <span class="status-contextual-sep" aria-hidden="true">·</span>
    <span class="status-contextual-item font-mono">{codeStatus.language}</span>
    <span class="status-contextual-sep" aria-hidden="true">·</span>
    <button
      type="button"
      class="status-contextual-action"
      class:text-content-warning={codeStatus.issueCount > 0}
      title="Show issues"
      onclick={showCodeProblems}
    >
      {codeStatus.issueCount} {codeStatus.issueCount === 1 ? "issue" : "issues"}
    </button>
    <span class="status-contextual-sep" aria-hidden="true">·</span>
    <span class="status-contextual-item">{codeStatus.control}</span>
    {#if codeStatus.saving || codeStatus.saveWhisper || codeStatus.dirty}
      <span class="status-contextual-sep" aria-hidden="true">·</span>
      <span class="status-contextual-whisper">
        {codeStatus.saving
          ? "Saving…"
          : codeStatus.saveWhisper || "Unsaved"}
      </span>
    {/if}
    {#if codeStatus.languageDetail || codeStatus.languageState !== "ready"}
      <span class="status-contextual-sep" aria-hidden="true">·</span>
      <span
        class="status-contextual-item status-contextual-item--detail"
        class:text-content-warning={codeStatus.languageState === "editing-only" ||
          codeStatus.languageState === "reconnecting"}
        class:text-content-error={codeStatus.languageState === "failed"}
        title={codeStatus.languageDetail ??
          (codeStatus.languageState === "connecting"
            ? "Language starting…"
            : codeStatus.languageState === "reconnecting"
              ? "Language reconnecting…"
              : codeStatus.languageState === "failed"
                ? "Language failed"
                : "Editing only")}
      >
        {#if codeStatus.languageDetail}
          {codeStatus.languageDetail}
        {:else if codeStatus.languageState === "connecting"}
          Language starting…
        {:else if codeStatus.languageState === "reconnecting"}
          Language reconnecting…
        {:else if codeStatus.languageState === "failed"}
          Language failed
        {:else}
          Editing only
        {/if}
      </span>
    {/if}
  </div>
{:else if showVault}
  <div class="status-contextual status-contextual--vault" aria-label="Note status">
    <span class="status-contextual-item truncate">{noteSummary}</span>
    {#if versionsDirtyLabel}
      <span class="status-contextual-sep" aria-hidden="true">·</span>
      <button
        type="button"
        class="status-contextual-action truncate text-content-warning/85"
        title="Open Versions"
        onclick={() => vaultVersions.openPanel()}
      >
        {versionsDirtyLabel}
      </button>
    {/if}
    {#if saveWhisper}
      <span class="status-contextual-sep" aria-hidden="true">·</span>
      <span class="status-contextual-whisper">{saveWhisper}</span>
    {/if}
  </div>
{:else if showScript && hasScriptChrome}
  <div class="status-contextual status-contextual--script" aria-label="Script status">
    {#if scriptDirty}
      <span class="status-contextual-item truncate">{scriptDirty}</span>
    {/if}
    {#if scriptAction}
      {#if scriptDirty}
        <span class="status-contextual-sep" aria-hidden="true">·</span>
      {/if}
      <button
        type="button"
        class="status-contextual-action truncate {scriptToneClass}"
        title="Show output"
        onclick={() => dispatchScriptWorkbenchOpenConsole()}
      >
        {scriptAction.label}
      </button>
    {/if}
    {#if scriptError}
      {#if scriptDirty || scriptAction}
        <span class="status-contextual-sep" aria-hidden="true">·</span>
      {/if}
      <span class="status-contextual-item truncate text-content-error">{scriptError}</span>
    {/if}
  </div>
{/if}

<style>
  .status-contextual {
    display: inline-flex;
    min-width: 0;
    max-width: 100%;
    flex: 0 1 auto;
    flex-wrap: nowrap;
    align-items: center;
    justify-content: flex-end;
    gap: 0.35rem;
    color: rgb(var(--theme-text-quiet));
    overflow: hidden;
    text-align: right;
    white-space: nowrap;
  }

  .status-contextual--code {
    /* Prefer yielding to trailing chrome over wrapping into the fixed-height bar. */
    max-width: min(38rem, 100%);
  }

  .status-contextual-item {
    flex-shrink: 0;
    white-space: nowrap;
  }

  /* Long LSP / language notices absorb squeeze; full text stays on title. */
  .status-contextual-item--detail {
    min-width: 0;
    flex: 1 1 auto;
    max-width: 16rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status-contextual-item--icon {
    display: inline-flex;
    min-width: 0;
    max-width: 100%;
    flex-shrink: 1;
    align-items: center;
    gap: 0.3rem;
    overflow: hidden;
  }

  :global(html.dark) .status-contextual-item.text-content-warning,
  :global(html.dark) .status-contextual-item--icon.text-content-warning {
    color: color-mix(
      in srgb,
      rgb(var(--theme-warning)) 72%,
      rgb(var(--theme-text-secondary))
    );
  }

  .status-contextual-sep {
    flex-shrink: 0;
    opacity: 0.45;
  }

  .status-contextual-action {
    flex-shrink: 0;
    border: 0;
    background: transparent;
    padding: 0;
    color: inherit;
    font: inherit;
    text-align: right;
    white-space: nowrap;
    transition: color 140ms ease;
  }

  .status-contextual-action:hover {
    color: rgb(var(--color-surface-200));
  }

  .status-contextual-whisper {
    flex-shrink: 0;
    color: rgb(var(--theme-text-tertiary));
    white-space: nowrap;
  }
</style>
