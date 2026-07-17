<script lang="ts">
  /**
   * Cursor-like quiet properties for Live — title, kind, tags.
   */
  import {
    isWorkshopVaultTag,
    kindLabel,
    normalizeKind,
    parseFrontmatterKindValue,
    parseFrontmatterTitle,
    setFrontmatterKindYaml,
    setFrontmatterTagsYaml,
    setFrontmatterTitleYaml,
    sortVaultTagsForDisplay,
    VAULT_KIND_OPTIONS,
    type VaultNoteKind,
  } from "$lib/utils/vaultFrontmatter";

  interface Props {
    frontmatter: string | null;
    tags: string[];
    /** Fallback when frontmatter has no title (usually note display title). */
    fallbackTitle?: string;
    disabled?: boolean;
    onFrontmatterChange: (next: string | null) => void;
  }

  let {
    frontmatter,
    tags,
    fallbackTitle = "",
    disabled = false,
    onFrontmatterChange,
  }: Props = $props();

  let titleDraft = $state("");
  let kindDraft = $state<VaultNoteKind>("note");
  let addingTag = $state(false);
  let newTag = $state("");
  let showWorkshop = $state(false);

  const orderedTags = $derived(sortVaultTagsForDisplay(tags));
  const humanTags = $derived(orderedTags.filter((t) => !isWorkshopVaultTag(t)));
  const workshopTags = $derived(orderedTags.filter((t) => isWorkshopVaultTag(t)));
  const visibleTags = $derived(
    showWorkshop ? orderedTags : humanTags,
  );

  $effect(() => {
    const fromFm = parseFrontmatterTitle(frontmatter);
    titleDraft = fromFm || fallbackTitle;
    const rawKind = parseFrontmatterKindValue(frontmatter);
    kindDraft = normalizeKind(rawKind || "note");
  });

  function commitTitle() {
    const next = setFrontmatterTitleYaml(frontmatter, titleDraft);
    onFrontmatterChange(next || null);
  }

  function commitKind(kind: VaultNoteKind) {
    kindDraft = kind;
    const next = setFrontmatterKindYaml(frontmatter, kind);
    onFrontmatterChange(next || null);
  }

  function removeTag(tag: string) {
    const nextTags = tags.filter((t) => t !== tag);
    const next = setFrontmatterTagsYaml(frontmatter, nextTags);
    onFrontmatterChange(next || null);
  }

  function addTag() {
    const value = newTag.trim();
    if (!value) {
      addingTag = false;
      return;
    }
    if (tags.some((t) => t.toLowerCase() === value.toLowerCase())) {
      newTag = "";
      addingTag = false;
      return;
    }
    const next = setFrontmatterTagsYaml(frontmatter, [...tags, value]);
    onFrontmatterChange(next || null);
    newTag = "";
    addingTag = false;
  }
</script>

<div class="vault-live-properties" aria-label="Properties">
  <p class="vault-live-properties__eyebrow">Properties</p>

  <div class="vault-live-properties__row">
    <span class="vault-live-properties__label">title</span>
    <input
      class="vault-live-properties__value"
      type="text"
      spellcheck="true"
      placeholder="Untitled"
      bind:value={titleDraft}
      {disabled}
      onblur={commitTitle}
      onkeydown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          (event.currentTarget as HTMLInputElement).blur();
        }
      }}
    />
  </div>

  <div class="vault-live-properties__row">
    <span class="vault-live-properties__label">kind</span>
    <select
      class="vault-live-properties__value vault-live-properties__select"
      value={kindDraft}
      {disabled}
      onchange={(event) => {
        const value = (event.currentTarget as HTMLSelectElement).value;
        commitKind(normalizeKind(value));
      }}
    >
      {#each VAULT_KIND_OPTIONS as kind (kind)}
        <option value={kind}>{kindLabel(kind)}</option>
      {/each}
    </select>
  </div>

  <div class="vault-live-properties__row vault-live-properties__row--tags">
    <span class="vault-live-properties__label">tags</span>
    <div class="vault-live-properties__tags">
      {#each visibleTags as tag (tag)}
        <span class="vault-live-properties__tag">
          {tag}
          <button
            type="button"
            class="vault-live-properties__tag-remove"
            title="Remove {tag}"
            aria-label="Remove tag {tag}"
            {disabled}
            onclick={() => removeTag(tag)}
          >
            ×
          </button>
        </span>
      {/each}
      {#if !showWorkshop && workshopTags.length > 0}
        <button
          type="button"
          class="vault-live-properties__tag vault-live-properties__tag--more"
          {disabled}
          onclick={() => (showWorkshop = true)}
        >
          +{workshopTags.length}
        </button>
      {/if}
      {#if addingTag}
        <input
          class="vault-live-properties__tag-input"
          type="text"
          spellcheck="false"
          placeholder="tag"
          bind:value={newTag}
          {disabled}
          onblur={addTag}
          onkeydown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              addTag();
            }
            if (event.key === "Escape") {
              event.preventDefault();
              addingTag = false;
              newTag = "";
            }
          }}
        />
      {:else}
        <button
          type="button"
          class="vault-live-properties__add"
          {disabled}
          onclick={() => {
            addingTag = true;
            newTag = "";
          }}
        >
          + Add
        </button>
      {/if}
    </div>
  </div>
</div>
