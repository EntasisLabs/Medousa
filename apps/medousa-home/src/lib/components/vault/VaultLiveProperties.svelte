<script lang="ts">
  /**
   * Live properties — title + human tags as a living instrument.
   * Kind lives on the chrome pill; workshop tags stay in Build/YAML.
   */
  import { tick } from "svelte";
  import { AlignLeft, ChevronRight, Plus, Tag, X } from "@lucide/svelte";
  import {
    isWorkshopVaultTag,
    parseFrontmatterTitle,
    setFrontmatterTagsYaml,
    setFrontmatterTitleYaml,
    sortVaultTagsForDisplay,
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

  let open = $state(true);
  let titleDraft = $state("");
  let addingTag = $state(false);
  let newTag = $state("");
  let titleFocused = $state(false);
  let titleInputEl = $state<HTMLInputElement | null>(null);
  let tagInputEl = $state<HTMLInputElement | null>(null);

  const humanTags = $derived(
    sortVaultTagsForDisplay(tags).filter((t) => !isWorkshopVaultTag(t)),
  );
  const tagsEmpty = $derived(humanTags.length === 0 && !addingTag);

  $effect(() => {
    const fromFm = parseFrontmatterTitle(frontmatter);
    titleDraft = fromFm || fallbackTitle;
  });

  function commitTitle() {
    titleFocused = false;
    const next = setFrontmatterTitleYaml(frontmatter, titleDraft);
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

  async function focusTitle() {
    if (disabled) return;
    titleInputEl?.focus();
    // Caret at end — select-all on every key click is hostile.
    const len = titleInputEl?.value.length ?? 0;
    titleInputEl?.setSelectionRange(len, len);
  }

  async function beginAddTag() {
    if (disabled) return;
    addingTag = true;
    newTag = "";
    await tick();
    tagInputEl?.focus();
  }

  function onTagsRowClick(event: MouseEvent) {
    if (disabled || addingTag) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest("button, input, a, .vault-live-properties__tag")) return;
    void beginAddTag();
  }
</script>

<div
  class="vault-live-properties"
  class:vault-live-properties--open={open}
  class:vault-live-properties--disabled={disabled}
>
  <button
    type="button"
    class="vault-live-properties__disclosure"
    aria-expanded={open}
    aria-controls="vault-live-properties-body"
    onclick={() => {
      open = !open;
    }}
  >
    <span class="vault-live-properties__disclosure-label">Properties</span>
    <ChevronRight
      size={12}
      strokeWidth={2.25}
      class="vault-live-properties__chevron"
      aria-hidden="true"
    />
  </button>

  {#if open}
    <div
      id="vault-live-properties-body"
      class="vault-live-properties__body"
      role="group"
      aria-label="Note properties"
    >
      <div
        class="vault-live-properties__row"
        class:vault-live-properties__row--active={titleFocused}
        class:vault-live-properties__row--disabled={disabled}
      >
        <button
          type="button"
          class="vault-live-properties__key"
          {disabled}
          tabindex="-1"
          onclick={() => void focusTitle()}
        >
          <AlignLeft
            size={14}
            strokeWidth={1.75}
            class="vault-live-properties__icon"
            aria-hidden="true"
          />
          <span class="vault-live-properties__label">title</span>
        </button>
        <input
          bind:this={titleInputEl}
          class="vault-live-properties__value"
          type="text"
          spellcheck="true"
          placeholder="Untitled"
          aria-label="Note title"
          bind:value={titleDraft}
          {disabled}
          onfocus={() => {
            titleFocused = true;
          }}
          onblur={commitTitle}
          onkeydown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              (event.currentTarget as HTMLInputElement).blur();
            }
            if (event.key === "Escape") {
              event.preventDefault();
              const fromFm = parseFrontmatterTitle(frontmatter);
              titleDraft = fromFm || fallbackTitle;
              (event.currentTarget as HTMLInputElement).blur();
            }
          }}
        />
      </div>

      <div
        class="vault-live-properties__row vault-live-properties__row--tags"
        class:vault-live-properties__row--active={addingTag}
        class:vault-live-properties__row--empty={tagsEmpty}
        class:vault-live-properties__row--disabled={disabled}
        onclick={onTagsRowClick}
        role="group"
      >
        <button
          type="button"
          class="vault-live-properties__key"
          {disabled}
          tabindex="-1"
          onclick={() => void beginAddTag()}
        >
          <Tag
            size={14}
            strokeWidth={1.75}
            class="vault-live-properties__icon"
            aria-hidden="true"
          />
          <span class="vault-live-properties__label">tags</span>
        </button>
        <div class="vault-live-properties__tags">
          {#each humanTags as tag (tag)}
            <span class="vault-live-properties__tag">
              <span class="vault-live-properties__tag-text">{tag}</span>
              <button
                type="button"
                class="vault-live-properties__tag-remove"
                title="Remove {tag}"
                aria-label="Remove tag {tag}"
                {disabled}
                onclick={() => removeTag(tag)}
              >
                <X size={10} strokeWidth={2.25} aria-hidden="true" />
              </button>
            </span>
          {/each}
          {#if addingTag}
            <input
              bind:this={tagInputEl}
              class="vault-live-properties__tag-input"
              type="text"
              spellcheck="false"
              placeholder="tag"
              aria-label="New tag"
              style:width="{Math.max(3.5, (newTag || 'tag').length + 1.25)}ch"
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
          {:else if tagsEmpty}
            <button
              type="button"
              class="vault-live-properties__add vault-live-properties__add--empty"
              {disabled}
              onclick={() => void beginAddTag()}
            >
              Add tags…
            </button>
          {:else}
            <button
              type="button"
              class="vault-live-properties__add"
              {disabled}
              onclick={() => void beginAddTag()}
              title="Add tag"
              aria-label="Add tag"
            >
              <Plus size={12} strokeWidth={2} aria-hidden="true" />
            </button>
          {/if}
        </div>
      </div>
    </div>
  {/if}
</div>
