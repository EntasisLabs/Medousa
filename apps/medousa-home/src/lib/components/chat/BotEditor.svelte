<script lang="ts">
  import { ArrowLeft, Check, ChevronRight, Plus, X } from "@lucide/svelte";
  import { tick } from "svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import BotAvatar from "./BotAvatar.svelte";
  import BotBrowserContinuity from "./BotBrowserContinuity.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import { BOT_AVATARS } from "$lib/utils/botAvatar";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import type { BotWorldBinding } from "$lib/types/generated/daemon_api";
  import type { ManuscriptCatalogEntry } from "$lib/types/catalog";

  let { name = $bindable(), purpose = $bindable(), avatar = $bindable(), archetypeId = $bindable(),
    worldBinding = $bindable(), editing = false, saving = false, error = null, onclose, onsubmit }: {
    name: string; purpose: string; avatar: string; archetypeId: string; worldBinding: BotWorldBinding | null;
    editing?: boolean; saving?: boolean; error?: string | null; onclose: () => void; onsubmit: (event: SubmitEvent) => void;
  } = $props();
  let view = $state<"bot" | "archetypes" | "create">("bot");
  let avatarsOpen = $state(false);
  let search = $state("");
  let archetypeName = $state("");
  let expertise = $state("");
  let approach = $state("");
  let creating = $state(false);
  let createError = $state<string | null>(null);
  let created = $state<ManuscriptCatalogEntry[]>([]);
  let dialogEl: HTMLDialogElement;
  const busy = $derived(saving || creating);
  const archetypes = $derived([...catalog.manuscripts, ...created.filter((entry) => !catalog.manuscripts.some((item) => item.id === entry.id))]);
  const selected = $derived(archetypes.find((entry) => entry.id === archetypeId));
  const filtered = $derived(archetypes.filter((entry) => `${entry.name} ${entry.description ?? ""}`.toLowerCase().includes(search.trim().toLowerCase())));
  const title = $derived(view === "create" ? "Create archetype" : view === "archetypes" ? "Choose archetype" : editing ? "Edit Bot" : "New Bot");
  const originalWorkshop = chat.workshopScopeId;

  function dismiss() {
    if (busy) return;
    if (view === "create") view = "archetypes";
    else if (view === "archetypes") view = "bot";
    else onclose();
  }
  async function navigate(next: typeof view) {
    view = next;
    await tick();
    dialogEl.querySelector<HTMLButtonElement>("header button")?.focus();
  }
  function mountDialog(node: HTMLDialogElement) {
    dialogEl = node;
    node.showModal();
    const unregister = registerMobileBackHandler(() => { dismiss(); return true; }, "modal");
    return { destroy() { unregister(); node.close(); } };
  }
  $effect(() => {
    if (chat.workshopScopeId !== originalWorkshop) onclose();
  });
  async function createArchetype() {
    if (busy || !archetypeName.trim() || !expertise.trim()) return;
    creating = true;
    createError = null;
    try {
      const detail = await catalog.createManuscript({ name: archetypeName.trim(), description: expertise.trim(), template: approach.trim() || undefined });
      if (chat.workshopScopeId !== originalWorkshop) return;
      created = [...created, { id: detail.id, name: detail.name, description: detail.description,
        scope: detail.scope, path: detail.path, has_scripts: detail.has_scripts, scripts: detail.scripts, openshell_enabled: detail.openshell.enabled }];
      archetypeId = detail.id;
      archetypeName = ""; expertise = ""; approach = "";
      await navigate("bot");
    } catch (err) {
      createError = err instanceof Error ? err.message : String(err);
    } finally { creating = false; }
  }
</script>

<BodyPortal>
  <dialog use:mountDialog aria-labelledby="bot-editor-title" onclick={(event) => event.stopPropagation()} onkeydown={(event) => { if (event.key === "Escape") event.stopPropagation(); }} onpointerdown={(event) => { const rect = dialogEl.getBoundingClientRect(); if (event.target === dialogEl && !busy && (event.clientX < rect.left || event.clientX > rect.right || event.clientY < rect.top || event.clientY > rect.bottom)) onclose(); }} oncancel={(event) => { event.preventDefault(); event.stopPropagation(); dismiss(); }}>
    <form onsubmit={(event) => { if (view === "bot") onsubmit(event); else { event.preventDefault(); if (view === "create") void createArchetype(); } }}>
      <header>
        {#if view !== "bot"}<button type="button" class="icon-button" aria-label="Back" disabled={busy} onclick={dismiss}><ArrowLeft size={18} /></button>{/if}
        <h2 id="bot-editor-title">{title}</h2>
        <button type="button" class="icon-button" aria-label="Close Bot editor" disabled={busy} onclick={onclose}><X size={18} /></button>
      </header>
      <div class="body">
        {#if view === "bot"}
          <p class="intro">A teammate with its own conversation and memory.</p>
          <div class="identity">
            <button type="button" class="avatar-button" aria-label="Choose Bot avatar" aria-expanded={avatarsOpen} disabled={busy} onclick={() => avatarsOpen = !avatarsOpen}><BotAvatar reference={avatar} size={54} /></button>
            <label class="name-field">Name<input class="input" bind:value={name} maxlength="80" placeholder="Ada" required disabled={busy} /></label>
          </div>
          {#if avatarsOpen}
            <div class="avatars" role="group" aria-label="Bot avatar">
              {#each BOT_AVATARS as option}
                <button type="button" aria-label={`${option.label} Medousa avatar`} aria-pressed={avatar === option.id} disabled={busy} onclick={() => { avatar = option.id; avatarsOpen = false; }}><BotAvatar reference={option.id} size={38} /></button>
              {/each}
            </div>
          {/if}
          <label>Purpose<textarea class="textarea" bind:value={purpose} maxlength="500" rows="3" placeholder="Look after Medousa’s mobile experience." required disabled={busy}></textarea></label>
          <div>
            <span class="field-label">Archetype</span>
            <button type="button" class="archetype-trigger" disabled={busy} onclick={() => void navigate("archetypes")}>
              <span><strong>{selected?.name ?? (archetypeId ? "Unavailable archetype" : "Choose an archetype")}</strong>{#if selected?.description}<small>{selected.description}</small>{/if}</span><ChevronRight size={16} />
            </button>
            <p class="hint">Your Bot’s reusable expertise and approach.</p>
          </div>
          <details><summary>More options</summary><div class="more"><BotBrowserContinuity bind:binding={worldBinding} disabled={busy} /><p class="hint">Conversation and memory stay with this Bot when you change its archetype.</p></div></details>
          {#if error}<p class="error" role="alert">{error}</p>{/if}
        {:else if view === "archetypes"}
          <p class="intro">Choose reusable expertise, or define your own.</p>
          <input class="input" type="search" aria-label="Search archetypes" bind:value={search} placeholder="Search archetypes…" />
          <button type="button" class="create-choice" onclick={() => void navigate("create")}><Plus size={17} />Create archetype…</button>
          <div class="choices" role="group" aria-label="Archetypes">
            {#each filtered as entry (entry.id)}
              <button type="button" class="choice" aria-pressed={entry.id === archetypeId} onclick={() => { archetypeId = entry.id; void navigate("bot"); }}><span><strong>{entry.name}</strong>{#if entry.description}<small>{entry.description}</small>{/if}</span>{#if entry.id === archetypeId}<Check size={16} />{/if}</button>
            {:else}<p class="hint">{catalog.loading ? "Loading archetypes…" : search ? "No matching archetypes." : "Create your first archetype to get started."}</p>{/each}
          </div>
          {#if catalog.error}<p class="error" role="alert">{catalog.error}</p><button type="button" onclick={() => void catalog.refresh()}>Try again</button>{/if}
        {:else}
          <p class="intro">This archetype will be available to other Bots too.</p>
          <label>Name<input class="input" bind:value={archetypeName} maxlength="80" placeholder="Frontend engineer" required disabled={busy} /></label>
          <label>Expertise<textarea class="textarea" bind:value={expertise} maxlength="500" rows="2" placeholder="Accessible interfaces, interaction design, and frontend development." required disabled={busy}></textarea></label>
          <label>Approach <span class="optional">Optional</span><textarea class="textarea" bind:value={approach} maxlength="8000" rows="4" placeholder="How should Bots with this archetype approach their work?" disabled={busy}></textarea></label>
          {#if createError}<p class="error" role="alert">{createError}</p>{/if}
        {/if}
      </div>
      {#if view !== "archetypes"}
        <footer><button type="button" class="secondary" disabled={busy} onclick={dismiss}>{view === "bot" ? "Cancel" : "Back"}</button><button type="submit" class="primary" disabled={busy || (view === "bot" ? !name.trim() || !purpose.trim() || !selected : !archetypeName.trim() || !expertise.trim())}>{busy ? "Saving…" : view === "create" ? "Create archetype" : editing ? "Save changes" : "Create Bot"}</button></footer>
      {/if}
    </form>
  </dialog>
</BodyPortal>

<style>
  dialog { max-width: none; box-sizing: border-box; position: fixed; inset: 0; margin: auto; width: min(460px, calc(100vw - 40px)); max-height: min(760px, calc(100dvh - 64px)); padding: 0; border: 1px solid rgb(var(--theme-border) / .65); border-radius: 22px; background: rgb(var(--theme-card, 25 23 34)); color: rgb(var(--theme-text-primary, 237 235 242)); box-shadow: 0 24px 80px #0007; overflow: hidden; }
  dialog::backdrop { background: #09081099; backdrop-filter: blur(4px); }
  form { display: flex; flex-direction: column; max-height: inherit; }
  header { display: flex; align-items: center; gap: 10px; padding: 20px 22px 8px; flex-shrink: 0; }
  h2 { margin: 0; flex: 1; font-size: 17px; font-weight: 600; }
  .icon-button { display: grid; place-items: center; width: 32px; height: 32px; border-radius: 50%; color: inherit; opacity: .7; }
  .icon-button:hover { background: #ffffff0a; opacity: 1; }
  .body { display: flex; flex-direction: column; gap: 22px; padding: 0 24px 24px; overflow-y: auto; overscroll-behavior: contain; min-height: 0; }
  .intro, .hint { margin: 0; color: rgb(var(--theme-text-secondary, 155 151 169)); font-size: 12px; line-height: 1.5; }
  .identity { display: flex; align-items: end; gap: 14px; }
  .name-field { flex: 1; min-width: 0; }
  label, .field-label { display: block; font-size: 12px; font-weight: 500; }
  input, textarea { display: block; width: 100%; min-width: 0; margin-top: 8px; padding: 11px 12px; border: 1px solid #ffffff14; border-radius: 11px; color: inherit; background: #ffffff05; font: inherit; font-size: 14px; font-weight: 400; }
  textarea { resize: vertical; line-height: 1.5; }
  input:focus, textarea:focus { outline: 2px solid #b394f680; outline-offset: 1px; }
  .avatar-button { border-radius: 16px; margin-bottom: 0; padding: 2px; }
  .avatars { display: flex; flex-wrap: wrap; gap: 10px; }
  .avatars button { padding: 3px; border: 1px solid transparent; border-radius: 13px; }
  .avatars button[aria-pressed="true"] { border-color: #b394f6; }
  .archetype-trigger, .choice { display: flex; align-items: center; justify-content: space-between; gap: 12px; width: 100%; text-align: left; padding: 12px; border-radius: 11px; }
  .archetype-trigger { margin: 8px 0; border: 1px solid #ffffff14; }
  .archetype-trigger > span, .choice > span { min-width: 0; }
  strong { display: block; font-weight: 500; font-size: 14px; overflow-wrap: anywhere; }
  small { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; margin-top: 4px; font-size: 12px; line-height: 1.45; opacity: .6; }
  summary { font-size: 12px; opacity: .65; cursor: pointer; }
  .more { margin-top: 12px; }
  .more .hint { margin-top: 10px; }
  footer { display: flex; justify-content: flex-end; gap: 10px; padding: 16px 24px 22px; flex-shrink: 0; border-top: 1px solid #ffffff08; }
  footer button { padding: 10px 16px; border-radius: 11px; font-size: 13px; }
  .secondary { background: #ffffff06; }
  .primary { background: #b394f6; color: #191321; font-weight: 500; }
  button:disabled { opacity: .4; cursor: default; }
  .create-choice { display: flex; align-items: center; gap: 10px; color: #b394f6; font-size: 13px; padding: 6px 0; }
  .choice:hover { background: #ffffff06; }
  .choice[aria-pressed="true"] { background: #b394f60c; }
  .optional { font-weight: 400; opacity: .5; margin-left: 5px; }
  .error { color: #f3a1ad; font-size: 12px; line-height: 1.5; }
  @media (max-width: 640px) {
    dialog { inset: auto 0 0; width: 100%; max-height: calc(100dvh - 28px); margin: 0; border-radius: 24px 24px 0 0; padding-bottom: env(safe-area-inset-bottom); }
    header { padding-top: 16px; }
    input, textarea { font-size: 16px; }
    .body { gap: 20px; }
  }
</style>
