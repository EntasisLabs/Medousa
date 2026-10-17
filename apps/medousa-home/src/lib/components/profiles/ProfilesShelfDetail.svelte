<script lang="ts">
  import ContextPlumbingSection from "$lib/components/context/ContextPlumbingSection.svelte";
  import ContextWitnessHero from "$lib/components/context/ContextWitnessHero.svelte";
  import ProfilesQuickActions from "$lib/components/profiles/ProfilesQuickActions.svelte";
  import { rememberIdentityFact } from "$lib/daemon";
  import type { IdentityRememberRequest } from "$lib/types/identity";
  import type { ProfileShelfEntry } from "$lib/types/profileShelf";
  import { withIdentityUserId } from "$lib/utils/identityTeach";
  import { profileKindLabel } from "$lib/utils/profileShelf";
  import { MessageCircle } from "@lucide/svelte";

  interface Props {
    entry: ProfileShelfEntry | null;
    digestLines: string[];
    onOpenChat?: () => void;
    onUpdated?: (parsed?: IdentityRememberRequest) => void | Promise<void>;
    onAddPerson?: () => void;
    readOnly?: boolean;
  }

  let {
    entry,
    digestLines,
    onOpenChat,
    onUpdated,
    onAddPerson,
    readOnly = false,
  }: Props = $props();

  let correction = $state("");
  let correcting = $state(false);
  let correctionMessage = $state<string | null>(null);
  let idsOpen = $state(false);

  $effect(() => {
    entry;
    correction = entry?.subtitle ?? "";
    correctionMessage = null;
    idsOpen = false;
  });

  async function submitCorrection() {
    if (!entry?.rememberKind || !entry.rememberSubject) return;
    const statement = correction.trim();
    if (!statement) return;
    correcting = true;
    correctionMessage = null;
    try {
      const parsed = withIdentityUserId({
        fact_kind: entry.rememberKind,
        subject: entry.rememberSubject,
        statement,
        source: "user_direct",
      });
      const result = await rememberIdentityFact(parsed);
      correctionMessage = result.message;
      await onUpdated?.(parsed);
    } catch (err) {
      correctionMessage = err instanceof Error ? err.message : String(err);
    } finally {
      correcting = false;
    }
  }

  const correctionLabel = $derived(
    entry?.kind === "contact" || entry?.kind === "relationship"
      ? "Relationship"
      : entry?.kind === "preference"
        ? "Value"
        : "Correct or refine",
  );
</script>

{#if !entry}
  <div class="flex h-full min-h-[12rem] flex-col justify-center px-2">
    {#if digestLines.length > 0}
      <p class="workshop-label">Saved context</p>
      <ul class="mt-3 space-y-2 text-sm leading-relaxed text-content-secondary">
        {#each digestLines.slice(0, 6) as line, index (index)}
          <li class="workshop-inset px-3 py-2">{line}</li>
        {/each}
      </ul>
      <p class="workshop-faint mt-4 text-xs leading-relaxed">
        Select an item on the left to review your saved context.
      </p>
    {:else}
      <p class="workshop-muted max-w-sm text-sm leading-relaxed">
        Select a person, preference, or fact to view its details, or add information to your profile.
      </p>
    {/if}
    <ProfilesQuickActions
      {readOnly}
      onAddPerson={onAddPerson}
      onSaved={onUpdated}
    />
  </div>
{:else}
  <article>
    <ContextWitnessHero
      title={entry.title}
      meta={null}
      lead={entry.subtitle !== entry.title ? entry.subtitle : null}
      chipLabel={profileKindLabel(entry.kind)}
      chipVariant="ready"
    />

    {#if onOpenChat}
      <button
        type="button"
        class="context-related-memory mt-6 flex w-full items-center gap-2 text-left"
        onclick={() => onOpenChat()}
      >
        <MessageCircle size={16} class="shrink-0 text-content-link" aria-hidden="true" />
        <span>
          <span class="context-related-memory-title block">Talk about this in chat</span>
          <span class="context-related-memory-meta">Use this saved context in a conversation</span>
        </span>
      </button>
    {/if}

    {#if entry.rememberKind && !readOnly}
      <section class="mt-6">
        <p class="workshop-label">{correctionLabel}</p>
        <p class="workshop-faint mt-1 text-xs leading-relaxed">
          {#if entry.kind === "contact" || entry.kind === "relationship"}
            Update this person’s relationship to you — partner, colleague, family.
          {:else}
            Update the information saved in your profile.
          {/if}
        </p>
        <div class="composer-bar mt-3 max-w-xl">
          <textarea
            class="composer-bar-input min-h-[2.75rem] w-full resize-none bg-transparent px-1 py-1 text-sm leading-relaxed text-surface-100 focus:outline-none"
            rows="2"
            bind:value={correction}
            disabled={correcting}
            placeholder={entry.kind === "contact" || entry.kind === "relationship"
              ? "Role — colleague, collaborator, family…"
              : "Enter the updated information"}
          ></textarea>
          <div class="flex justify-end border-t border-surface-500/25 pt-2">
            <button
              type="button"
              class="composer-bar-send"
              disabled={correcting || !correction.trim()}
              onclick={() => void submitCorrection()}
            >
              {correcting ? "Saving…" : "Update"}
            </button>
          </div>
        </div>
        {#if correctionMessage}
          <p class="mt-2 text-xs text-content-success">{correctionMessage}</p>
        {/if}
      </section>
    {/if}

    <ContextPlumbingSection>
      <button
        type="button"
        class="context-layer-toggle"
        aria-expanded={idsOpen}
        onclick={() => {
          idsOpen = !idsOpen;
        }}
      >
        <span>Details</span>
        <span class="workshop-faint text-[11px]">advanced</span>
      </button>
      {#if idsOpen}
        <dl class="workshop-inset context-layer-body text-xs">
          <div>
            <dt class="workshop-label">kind</dt>
            <dd class="mt-0.5 text-surface-200">{entry.kind}</dd>
          </div>
          {#if entry.confidence !== undefined}
            <div><dt class="workshop-label">Confidence</dt><dd>{(entry.confidence * 100).toFixed(0)}%</dd></div>
          {/if}
          {#if entry.trustLevel !== undefined}
            <div><dt class="workshop-label">Trust</dt><dd>{(entry.trustLevel * 100).toFixed(0)}%</dd></div>
          {/if}
          {#each Object.entries(entry.meta ?? {}) as [key, value] (key)}
            <div>
              <dt class="workshop-label">{key}</dt>
              <dd class="mt-0.5 break-all font-mono text-content-secondary">{value}</dd>
            </div>
          {/each}
        </dl>
      {/if}
    </ContextPlumbingSection>
  </article>
{/if}
