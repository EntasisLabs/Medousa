<script lang="ts">
  import { onMount } from "svelte";
  import { LoaderCircle, UserRound } from "@lucide/svelte";
  import { openConfigPath } from "$lib/config";
  import {
    exportIdentityMarkdown,
    getIdentityDigestPreview,
    rememberIdentityFact,
  } from "$lib/daemon";
  import { identity } from "$lib/stores/identity.svelte";
  import { userProfiles } from "$lib/stores/userProfiles.svelte";
  import type { IdentityDigestPreviewResponse } from "$lib/types/identity";
  import {
    parseIdentityTeachInput,
    preferenceDisplayValue,
  } from "$lib/utils/identityTeach";
  import { isTauriMobilePlatform } from "$lib/platform";

  interface Props {
    mobile?: boolean;
  }

  let { mobile = false }: Props = $props();

  let teachText = $state("");
  let teachBusy = $state(false);
  let teachMessage = $state<string | null>(null);
  let teachOk = $state(false);

  let digest = $state<IdentityDigestPreviewResponse | null>(null);
  let digestLoading = $state(false);
  let digestError = $state<string | null>(null);

  let personName = $state("");
  let personNote = $state("");
  let personBusy = $state(false);

  let timezoneDraft = $state("");
  let timezoneBusy = $state(false);

  let exportBusy = $state(false);
  let exportMessage = $state<string | null>(null);
  let lastExportDir = $state<string | null>(null);

  const readOnly = $derived(mobile && isTauriMobilePlatform());

  const preferenceEntries = $derived.by(() => {
    const prefs = identity.context?.user?.preferences;
    if (!prefs || typeof prefs !== "object") return [] as [string, string][];
    return Object.entries(prefs)
      .slice(0, 12)
      .map(([key, value]) => [key, preferenceDisplayValue(value)] as [string, string]);
  });

  onMount(() => {
    void refreshAll();
  });

  async function refreshAll() {
    await Promise.all([identity.refresh({ relationshipLimit: 24 }), loadDigest()]);
    timezoneDraft =
      preferenceDisplayValue(identity.context?.user?.preferences?.timezone) ||
      identity.context?.user?.timezone ||
      "";
  }

  async function loadDigest() {
    digestLoading = true;
    digestError = null;
    try {
      digest = await getIdentityDigestPreview({ mode: "cognitive", relationship_limit: 32 });
    } catch (err) {
      digest = null;
      digestError = err instanceof Error ? err.message : String(err);
    } finally {
      digestLoading = false;
    }
  }

  async function submitTeach() {
    const parsed = parseIdentityTeachInput(teachText);
    if (!parsed.statement.trim()) return;
    teachBusy = true;
    teachMessage = null;
    teachOk = false;
    try {
      const result = await rememberIdentityFact(parsed);
      teachOk = result.committed || !result.requires_confirmation;
      teachMessage = result.message;
      if (result.committed) {
        teachText = "";
      }
      await refreshAll();
    } catch (err) {
      teachMessage = err instanceof Error ? err.message : String(err);
    } finally {
      teachBusy = false;
    }
  }

  async function saveTimezone() {
    const value = timezoneDraft.trim();
    if (!value) return;
    timezoneBusy = true;
    teachMessage = null;
    try {
      const result = await rememberIdentityFact({
        fact_kind: "preference",
        subject: "timezone",
        statement: value,
        source: "user_direct",
      });
      teachOk = result.committed;
      teachMessage = result.message;
      await refreshAll();
    } catch (err) {
      teachMessage = err instanceof Error ? err.message : String(err);
    } finally {
      timezoneBusy = false;
    }
  }

  async function addPerson(event: SubmitEvent) {
    event.preventDefault();
    const name = personName.trim();
    const note = personNote.trim();
    if (!name || !note) return;
    personBusy = true;
    teachMessage = null;
    try {
      const result = await rememberIdentityFact({
        fact_kind: "person",
        subject: name,
        statement: note,
        source: "user_direct",
      });
      teachOk = result.committed;
      teachMessage = result.message;
      if (result.committed) {
        personName = "";
        personNote = "";
      }
      await refreshAll();
    } catch (err) {
      teachMessage = err instanceof Error ? err.message : String(err);
    } finally {
      personBusy = false;
    }
  }

  async function exportMarkdown() {
    exportBusy = true;
    exportMessage = null;
    try {
      const result = await exportIdentityMarkdown();
      lastExportDir = result.export_dir;
      exportMessage = `Exported ${result.files.join(", ")} to your identity folder.`;
    } catch (err) {
      exportMessage = err instanceof Error ? err.message : String(err);
    } finally {
      exportBusy = false;
    }
  }

  async function openExportFolder() {
    if (!lastExportDir) {
      try {
        const result = await exportIdentityMarkdown();
        lastExportDir = result.export_dir;
      } catch {
        return;
      }
    }
    if (lastExportDir) {
      await openConfigPath(lastExportDir);
    }
  }

  function relationshipKind(value: unknown): string {
    if (typeof value === "string") return value;
    if (value && typeof value === "object" && "type" in value) {
      return String((value as { type?: string }).type ?? "relationship");
    }
    return "relationship";
  }
</script>

<article class="settings-connection-card mt-8">
  <header>
    <h3 class="text-sm font-semibold text-surface-50">Memory &amp; You</h3>
    <p class="workshop-faint mt-1 text-xs leading-relaxed">
      What Medousa knows about {userProfiles.activeDisplayName} — teach her in plain language,
      no terminal.
    </p>
  </header>

  <div class="mt-5">
    <span class="block text-sm font-medium text-surface-100">Ranked digest preview</span>
    <span class="workshop-faint mt-0.5 block text-xs">
      The slice she uses for continuity — preferences, people, and notes ranked for recall.
    </span>
    {#if digestLoading}
      <p class="workshop-faint mt-3 flex items-center gap-2 text-xs">
        <LoaderCircle class="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
        Loading digest…
      </p>
    {:else if digestError}
      <p class="mt-3 text-xs text-warning-400">{digestError}</p>
    {:else if digest?.digest_text}
      <pre
        class="mt-3 max-h-48 overflow-y-auto whitespace-pre-wrap rounded-container-token border border-surface-500/35 bg-surface-950/50 p-3 font-sans text-xs leading-relaxed text-surface-300"
      >{digest.digest_text.trim()}</pre>
      <p class="workshop-faint mt-2 text-[11px]">
        {digest.preference_count} preferences · {digest.contact_count} people ·
        {digest.relationship_count} relationships · {digest.claim_count} recall claims
      </p>
    {:else}
      <p class="workshop-faint mt-3 text-xs">
        Nothing ranked yet — teach her something below and it will appear here.
      </p>
    {/if}
  </div>

  <div class="mt-6 border-t border-surface-500/35 pt-5">
    <label class="block" for="teach-medousa">
      <span class="block text-sm font-medium text-surface-100">Teach Medousa</span>
      <span class="workshop-faint mt-0.5 block text-xs leading-relaxed">
        e.g. “Mario is my partner”, “My timezone is America/New_York”, “I prefer matcha over
        coffee”
      </span>
      <textarea
        id="teach-medousa"
        class="textarea mt-2 w-full text-sm"
        rows="3"
        placeholder="Tell her something she should remember…"
        bind:value={teachText}
        readonly={readOnly}
        disabled={readOnly || teachBusy}
      ></textarea>
    </label>
    <div class="mt-3 flex flex-wrap items-center gap-2">
      <button
        type="button"
        class="btn btn-sm variant-filled-primary"
        disabled={readOnly || teachBusy || !teachText.trim()}
        onclick={() => void submitTeach()}
      >
        {teachBusy ? "Saving…" : "Remember this"}
      </button>
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface"
        disabled={readOnly || digestLoading}
        onclick={() => void loadDigest()}
      >
        Refresh preview
      </button>
    </div>
    {#if teachMessage}
      <p
        class="mt-2 text-xs {teachOk ? 'text-success-400' : 'text-warning-400'}"
        role="status"
      >
        {teachMessage}
      </p>
    {/if}
  </div>

  <div class="mt-6 border-t border-surface-500/35 pt-5">
    <span class="block text-sm font-medium text-surface-100">Key preferences</span>
    <div class="mt-3 grid gap-3 sm:grid-cols-2">
      <label class="block">
        <span class="workshop-label">Timezone</span>
        <input
          class="input mt-1 w-full"
          bind:value={timezoneDraft}
          placeholder="America/New_York"
          readonly={readOnly}
          disabled={readOnly || timezoneBusy}
        />
      </label>
      {#if preferenceEntries.length > 0}
        <div class="sm:col-span-2">
          <span class="workshop-label">Other preferences</span>
          <ul class="mt-2 divide-y divide-surface-500/25 rounded-container-token border border-surface-500/35 text-xs">
            {#each preferenceEntries as [key, value] (key)}
              {#if key !== "timezone"}
                <li class="flex justify-between gap-3 px-3 py-2">
                  <span class="text-surface-400">{key}</span>
                  <span class="text-right text-surface-200">{value}</span>
                </li>
              {/if}
            {/each}
          </ul>
        </div>
      {/if}
    </div>
    <button
      type="button"
      class="btn btn-sm variant-soft-surface mt-3"
      disabled={readOnly || timezoneBusy || !timezoneDraft.trim()}
      onclick={() => void saveTimezone()}
    >
      {timezoneBusy ? "Saving…" : "Save timezone"}
    </button>
  </div>

  <div class="mt-6 border-t border-surface-500/35 pt-5">
    <span class="block text-sm font-medium text-surface-100">People &amp; relationships</span>
    <span class="workshop-faint mt-0.5 block text-xs">
      Names and roles she should recognize across threads.
    </span>

    {#if identity.context?.contacts && identity.context.contacts.length > 0}
      <ul class="mt-3 space-y-2">
        {#each identity.context.contacts.slice(0, 8) as contact (contact.contact_id)}
          <li
            class="flex items-start gap-2 rounded-container-token border border-surface-500/35 bg-surface-950/40 px-3 py-2 text-sm"
          >
            <UserRound class="mt-0.5 h-4 w-4 shrink-0 text-surface-400" aria-hidden="true" />
            <span class="text-surface-100">{contact.display_name}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="workshop-faint mt-3 text-xs">No people saved yet.</p>
    {/if}

    {#if identity.context?.relationships && identity.context.relationships.length > 0}
      <ul class="mt-3 space-y-1 text-xs text-surface-300">
        {#each identity.context.relationships.slice(0, 6) as rel (rel.relationship_id)}
          <li>
            {relationshipKind(rel.relationship_kind)} · trust
            {(rel.trust_level * 100).toFixed(0)}%
          </li>
        {/each}
      </ul>
    {/if}

    <form class="mt-4 grid gap-3 sm:grid-cols-2" onsubmit={(event) => void addPerson(event)}>
      <label class="block">
        <span class="workshop-label">Name</span>
        <input
          class="input mt-1 w-full"
          bind:value={personName}
          placeholder="Mario"
          readonly={readOnly}
          disabled={readOnly || personBusy}
        />
      </label>
      <label class="block">
        <span class="workshop-label">Relationship or note</span>
        <input
          class="input mt-1 w-full"
          bind:value={personNote}
          placeholder="partner · handles finances with me"
          readonly={readOnly}
          disabled={readOnly || personBusy}
        />
      </label>
      <div class="sm:col-span-2">
        <button
          type="submit"
          class="btn btn-sm variant-soft-surface"
          disabled={readOnly || personBusy || !personName.trim() || !personNote.trim()}
        >
          {personBusy ? "Adding…" : "Add person"}
        </button>
      </div>
    </form>
  </div>

  <div class="mt-6 border-t border-surface-500/35 pt-5">
    <span class="block text-sm font-medium text-surface-100">Export &amp; hand-edit</span>
    <p class="workshop-faint mt-1 text-xs leading-relaxed">
      Writes SOUL.md, USER.md, PEOPLE.md, and IDENTITY.md — same as
      <span class="font-mono text-surface-400">medousa identity-export</span>. Edit the files,
      then teach corrections here or refresh the digest.
    </p>
    <div class="mt-3 flex flex-wrap gap-2">
      <button
        type="button"
        class="btn btn-sm variant-soft-surface"
        disabled={readOnly || exportBusy}
        onclick={() => void exportMarkdown()}
      >
        {exportBusy ? "Exporting…" : "Export identity markdown"}
      </button>
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface"
        disabled={readOnly}
        onclick={() => void openExportFolder()}
      >
        Open export folder
      </button>
    </div>
    {#if exportMessage}
      <p class="mt-2 text-xs text-surface-300">{exportMessage}</p>
    {/if}
  </div>
</article>
