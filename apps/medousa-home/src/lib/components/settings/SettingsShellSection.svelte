<script lang="ts">
  import SettingsCharterSaveBar from "$lib/components/settings/SettingsCharterSaveBar.svelte";
  import { workshop } from "$lib/stores/workshop.svelte";
  import { workshopDefaults } from "$lib/stores/workshopDefaults.svelte";
  import {
    listToMultilineText,
    parseMultilineList,
  } from "$lib/types/workshopDefaults";
  import { isTauriMobilePlatform } from "$lib/platform";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  let binariesText = $state("");
  let writableRootsText = $state("");
  let syncedFrom = $state<string | null>(null);

  const agentToolsOn = $derived(workshopDefaults.draft.shellAgentToolsEnabled ?? false);
  const networkOn = $derived(workshopDefaults.draft.shellNetworkDefault ?? false);
  const binariesEmpty = $derived(parseMultilineList(binariesText).length === 0);

  $effect(() => {
    if (!workshopDefaults.loaded) return;
    const fingerprint = JSON.stringify([
      workshopDefaults.draft.shellAllowedBinaries ?? [],
      workshopDefaults.draft.shellWritableRoots ?? [],
    ]);
    if (syncedFrom === fingerprint) return;
    binariesText = listToMultilineText(workshopDefaults.draft.shellAllowedBinaries);
    writableRootsText = listToMultilineText(workshopDefaults.draft.shellWritableRoots);
    syncedFrom = fingerprint;
  });

  $effect(() => {
    if (!workshopDefaults.loaded) return;
    void workshop.loadAllowlist();
  });

  function syncListsIntoDraft() {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      shellAllowedBinaries: parseMultilineList(binariesText),
      shellWritableRoots: parseMultilineList(writableRootsText),
    };
  }

  function setAgentTools(enabled: boolean) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      shellAgentToolsEnabled: enabled,
    };
  }

  function setNetwork(enabled: boolean) {
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      shellNetworkDefault: enabled,
    };
  }

  function setTimeoutMs(event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      shellTimeoutMs: Number.isFinite(value) ? Math.max(100, Math.round(value)) : 30_000,
    };
  }

  function setMaxOutput(event: Event) {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    workshopDefaults.draft = {
      ...workshopDefaults.draft,
      shellMaxOutputBytes: Number.isFinite(value)
        ? Math.max(1024, Math.round(value))
        : 262_144,
    };
  }
</script>

<section class="settings-section">
  <header class="settings-section-header">
    <h2 class="text-base font-semibold text-surface-50">Shell</h2>
    <p class="workshop-faint mt-1 text-sm">
      Process sandbox for agents and Grapheme scripts. Locked down until you open it.
    </p>
  </header>

  <div class="mt-5">
    <h3 class="settings-subsection-heading">Agent tools</h3>
    <p class="settings-subsection-lead">
      Interactive specialists only. Scheduled lanes stay blocked either way.
    </p>
    <div class="mt-1 grid gap-2 sm:grid-cols-2">
      <button
        type="button"
        class="settings-depth-card {!agentToolsOn ? 'settings-depth-card-active' : ''}"
        disabled={readOnly}
        aria-pressed={!agentToolsOn}
        onclick={() => setAgentTools(false)}
      >
        <span class="block text-sm font-medium text-surface-100">Off</span>
        <span class="workshop-faint mt-1 block text-xs leading-snug">
          Block cognition_shell_* until you opt in
        </span>
      </button>
      <button
        type="button"
        class="settings-depth-card {agentToolsOn ? 'settings-depth-card-active' : ''}"
        disabled={readOnly}
        aria-pressed={agentToolsOn}
        onclick={() => setAgentTools(true)}
      >
        <span class="block text-sm font-medium text-surface-100">On</span>
        <span class="workshop-faint mt-1 block text-xs leading-snug">
          Unlock shell tools — still need them on each specialist
        </span>
      </button>
    </div>
    {#if agentToolsOn && binariesEmpty}
      <p class="shell-soft-warn mt-3 text-xs leading-relaxed">
        Agents are unlocked with an empty binary allowlist — any basename inside the jail can run.
        Prefer listing tools you trust below.
      </p>
    {/if}
  </div>

  <div class="mt-6">
    <h3 class="settings-subsection-heading">Sandbox ceilings</h3>
    <p class="settings-subsection-lead">
      Applied to every <span class="font-mono text-surface-300">shell.run</span>. Calls may only
      tighten these, never loosen them.
    </p>

    <div class="mt-1 grid gap-2 sm:grid-cols-2">
      <button
        type="button"
        class="settings-depth-card {!networkOn ? 'settings-depth-card-active' : ''}"
        disabled={readOnly}
        aria-pressed={!networkOn}
        onclick={() => setNetwork(false)}
      >
        <span class="block text-sm font-medium text-surface-100">Network off</span>
        <span class="workshop-faint mt-1 block text-xs leading-snug">
          Calls cannot enable network
        </span>
      </button>
      <button
        type="button"
        class="settings-depth-card {networkOn ? 'settings-depth-card-active' : ''}"
        disabled={readOnly}
        aria-pressed={networkOn}
        onclick={() => setNetwork(true)}
      >
        <span class="block text-sm font-medium text-surface-100">Network allowed</span>
        <span class="workshop-faint mt-1 block text-xs leading-snug">
          Ceiling only — each call still opts in
        </span>
      </button>
    </div>

    <div class="settings-toggle-list mt-3">
      <label class="settings-toggle-row settings-metric-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Timeout</span>
          <span class="workshop-faint mt-0.5 block text-xs">Hard stop for a single process</span>
        </span>
        <span class="settings-metric-value">
          <input
            type="number"
            class="settings-metric-input settings-metric-input-wide"
            min="100"
            step="100"
            inputmode="numeric"
            value={workshopDefaults.draft.shellTimeoutMs ?? 30_000}
            readonly={readOnly}
            disabled={readOnly}
            aria-label="Shell timeout in milliseconds"
            oninput={setTimeoutMs}
          />
          <span class="settings-metric-unit" aria-hidden="true">ms</span>
        </span>
      </label>

      <label class="settings-toggle-row settings-metric-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">Max output</span>
          <span class="workshop-faint mt-0.5 block text-xs">Stdout/stderr cap before truncation</span>
        </span>
        <span class="settings-metric-value">
          <input
            type="number"
            class="settings-metric-input settings-metric-input-wide"
            min="1024"
            step="1024"
            inputmode="numeric"
            value={workshopDefaults.draft.shellMaxOutputBytes ?? 262_144}
            readonly={readOnly}
            disabled={readOnly}
            aria-label="Shell max output in bytes"
            oninput={setMaxOutput}
          />
          <span class="settings-metric-unit" aria-hidden="true">bytes</span>
        </span>
      </label>
    </div>
  </div>

  <div class="mt-6">
    <h3 class="settings-subsection-heading">Allowlists</h3>
    <p class="settings-subsection-lead">
      Leave empty for the current defaults — any basename, cwd-scoped writes when a cwd is set.
    </p>

    <label class="mt-2 block">
      <span class="block text-sm font-medium text-surface-100">Allowed binaries</span>
      <span class="workshop-faint mt-0.5 block text-xs">
        One basename per line. Empty means any command inside the jail.
      </span>
      <textarea
        class="shell-list-input mt-2"
        rows="4"
        bind:value={binariesText}
        placeholder={"git\nls\nrg"}
        readonly={readOnly}
        disabled={readOnly}
        spellcheck="false"
      ></textarea>
    </label>

    <label class="mt-4 block">
      <span class="block text-sm font-medium text-surface-100">Writable roots</span>
      <span class="workshop-faint mt-0.5 block text-xs">
        Absolute paths. Empty keeps per-call / cwd defaults.
      </span>
      <textarea
        class="shell-list-input mt-2"
        rows="3"
        bind:value={writableRootsText}
        placeholder="/Users/you/projects"
        readonly={readOnly}
        disabled={readOnly}
        spellcheck="false"
      ></textarea>
    </label>
  </div>

  <div class="mt-6">
    <h3 class="settings-subsection-heading">Grapheme scripts</h3>
    <p class="settings-subsection-lead">
      Same module allowlist as Automations → Scripts. Empty allowlist means every module, including
      shell.
    </p>
    {#if workshop.allowlistError}
      <p class="mt-2 text-xs text-warning-400">{workshop.allowlistError}</p>
    {/if}
    <div class="settings-toggle-list mt-2">
      <label class="settings-toggle-row">
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-surface-100">
            Allow <span class="font-mono text-surface-300">shell</span> module
          </span>
          <span class="workshop-faint mt-0.5 block text-xs">
            {workshop.allowlistEnforce
              ? "Allowlist is enforcing — this toggles shell for workshop scripts"
              : "Allowlist empty (all modules allowed). Checking this starts an allowlist."}
          </span>
        </span>
        <input
          type="checkbox"
          class="checkbox shrink-0"
          checked={workshop.isModuleAllowed("shell")}
          disabled={readOnly || workshop.allowlistBusy}
          onchange={(event) =>
            workshop.toggleAllowlistModule(
              "shell",
              (event.currentTarget as HTMLInputElement).checked,
            )}
        />
      </label>
    </div>
  </div>

  <div class="mt-6">
    <SettingsCharterSaveBar {mobile} beforeSave={syncListsIntoDraft} />
  </div>
</section>

<style>
  .shell-list-input {
    display: block;
    width: 100%;
    resize: vertical;
    min-height: 5rem;
    border-radius: 0.55rem;
    border: 1px solid rgb(var(--shell-border, var(--color-surface-500)) / 0.45);
    background: rgb(var(--shell-pane-bg, var(--color-surface-900)) / 0.45);
    padding: 0.55rem 0.7rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.75rem;
    line-height: 1.45;
    color: rgb(var(--shell-label, var(--color-surface-100)));
  }

  .shell-list-input::placeholder {
    color: rgb(var(--shell-muted, var(--color-surface-500)));
    opacity: 0.7;
  }

  .shell-list-input:focus {
    outline: none;
    border-color: rgb(var(--color-primary-500) / 0.55);
    box-shadow: 0 0 0 2px rgb(var(--color-primary-500) / 0.18);
  }

  .shell-list-input:disabled,
  .shell-list-input:read-only {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .shell-soft-warn {
    border-radius: 0.55rem;
    border: 1px solid rgb(var(--color-warning-500) / 0.35);
    background: rgb(var(--color-warning-500) / 0.08);
    color: rgb(var(--color-warning-300, var(--color-warning-400)));
    padding: 0.65rem 0.75rem;
  }
</style>
