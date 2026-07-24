<script lang="ts">
  /**
   * Live properties — title, tags, and extra YAML fields as a living instrument.
   * Kind lives on the chrome pill; workshop tags stay in Build/YAML.
   */
  import { tick, untrack } from "svelte";
  import { AlignLeft, ChevronRight, Plus, Tag, Type, X } from "@lucide/svelte";
  import {
    isWorkshopVaultTag,
    listFrontmatterScalarFields,
    parseFrontmatterTitle,
    removeFrontmatterFieldYaml,
    setFrontmatterFieldYaml,
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

  /** Drafts for existing extra fields (keyed by YAML key). */
  let fieldDrafts = $state<Record<string, string>>({});
  let focusedFieldKey = $state<string | null>(null);
  /** Suppress blur→close while Enter chains another tag after a remount. */
  let keepAddingTag = false;

  /** New property row. */
  let addingField = $state(false);
  let newFieldKey = $state("");
  let newFieldValue = $state("");
  let newFieldKeyEl = $state<HTMLInputElement | null>(null);

  const humanTags = $derived(
    sortVaultTagsForDisplay(tags).filter((t) => !isWorkshopVaultTag(t)),
  );
  const tagsEmpty = $derived(humanTags.length === 0 && !addingTag);
  const extraFields = $derived(listFrontmatterScalarFields(frontmatter));

  $effect(() => {
    const fromFm = parseFrontmatterTitle(frontmatter);
    titleDraft = fromFm || fallbackTitle;
  });

  $effect(() => {
    const fields = listFrontmatterScalarFields(frontmatter);
    const focused = focusedFieldKey;
    const prev = untrack(() => fieldDrafts);
    const next: Record<string, string> = {};
    for (const field of fields) {
      next[field.key] =
        focused === field.key && prev[field.key] != null
          ? prev[field.key]!
          : field.value;
    }
    fieldDrafts = next;
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

  /** Commit tag; keep the input open so Enter chains another chip. */
  async function addTag(options?: { continueAdding?: boolean }) {
    const continueAdding = options?.continueAdding ?? false;
    const value = newTag.trim();
    if (!value) {
      if (!continueAdding && !keepAddingTag) addingTag = false;
      return;
    }
    if (tags.some((t) => t.toLowerCase() === value.toLowerCase())) {
      newTag = "";
      if (continueAdding) {
        keepAddingTag = true;
        addingTag = true;
        await tick();
        tagInputEl?.focus();
        keepAddingTag = false;
        return;
      }
      addingTag = false;
      return;
    }
    if (continueAdding) keepAddingTag = true;
    const next = setFrontmatterTagsYaml(frontmatter, [...tags, value]);
    onFrontmatterChange(next || null);
    newTag = "";
    if (continueAdding) {
      addingTag = true;
      await tick();
      tagInputEl?.focus();
      keepAddingTag = false;
      return;
    }
    addingTag = false;
  }

  async function focusTitle() {
    if (disabled) return;
    titleInputEl?.focus();
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

  function commitField(key: string) {
    if (focusedFieldKey === key) focusedFieldKey = null;
    const value = fieldDrafts[key] ?? "";
    const next = setFrontmatterFieldYaml(frontmatter, key, value);
    onFrontmatterChange(next || null);
  }

  function removeField(key: string) {
    const next = removeFrontmatterFieldYaml(frontmatter, key);
    onFrontmatterChange(next || null);
  }

  async function beginAddField() {
    if (disabled) return;
    addingField = true;
    newFieldKey = "";
    newFieldValue = "";
    await tick();
    newFieldKeyEl?.focus();
  }

  function cancelAddField() {
    addingField = false;
    newFieldKey = "";
    newFieldValue = "";
  }

  function commitNewField() {
    const key = newFieldKey.trim();
    if (!key) {
      cancelAddField();
      return;
    }
    if (/^(title|tags|kind)$/i.test(key)) {
      cancelAddField();
      return;
    }
    if (!/^[A-Za-z_][\w.-]*$/.test(key)) return;
    const next = setFrontmatterFieldYaml(frontmatter, key, newFieldValue);
    onFrontmatterChange(next || null);
    cancelAddField();
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
              onblur={() => {
                if (keepAddingTag) return;
                void addTag();
              }}
              onkeydown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  void addTag({ continueAdding: true });
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

      {#each extraFields as field (field.key)}
        <div
          class="vault-live-properties__row"
          class:vault-live-properties__row--active={focusedFieldKey === field.key}
          class:vault-live-properties__row--disabled={disabled}
        >
          <span class="vault-live-properties__key vault-live-properties__key--static">
            <Type
              size={14}
              strokeWidth={1.75}
              class="vault-live-properties__icon"
              aria-hidden="true"
            />
            <span class="vault-live-properties__label" title={field.key}>{field.key}</span>
          </span>
          <div class="vault-live-properties__value-row">
            <input
              class="vault-live-properties__value"
              type="text"
              spellcheck="false"
              aria-label="{field.key} value"
              value={fieldDrafts[field.key] ?? field.value}
              {disabled}
              onfocus={() => {
                focusedFieldKey = field.key;
              }}
              oninput={(event) => {
                fieldDrafts = {
                  ...fieldDrafts,
                  [field.key]: (event.currentTarget as HTMLInputElement).value,
                };
              }}
              onblur={() => commitField(field.key)}
              onkeydown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  (event.currentTarget as HTMLInputElement).blur();
                }
                if (event.key === "Escape") {
                  event.preventDefault();
                  fieldDrafts = { ...fieldDrafts, [field.key]: field.value };
                  focusedFieldKey = null;
                  (event.currentTarget as HTMLInputElement).blur();
                }
              }}
            />
            <button
              type="button"
              class="vault-live-properties__field-remove"
              title="Remove {field.key}"
              aria-label="Remove property {field.key}"
              {disabled}
              onclick={() => removeField(field.key)}
            >
              <X size={11} strokeWidth={2.25} aria-hidden="true" />
            </button>
          </div>
        </div>
      {/each}

      {#if addingField}
        <div
          class="vault-live-properties__row vault-live-properties__row--active"
          class:vault-live-properties__row--disabled={disabled}
        >
          <input
            bind:this={newFieldKeyEl}
            class="vault-live-properties__key-input"
            type="text"
            spellcheck="false"
            placeholder="key"
            aria-label="New property key"
            bind:value={newFieldKey}
            {disabled}
            onkeydown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                const valueEl = (event.currentTarget as HTMLInputElement)
                  .closest(".vault-live-properties__row")
                  ?.querySelector<HTMLInputElement>(".vault-live-properties__value");
                valueEl?.focus();
              }
              if (event.key === "Escape") {
                event.preventDefault();
                cancelAddField();
              }
            }}
          />
          <div class="vault-live-properties__value-row">
            <input
              class="vault-live-properties__value"
              type="text"
              spellcheck="false"
              placeholder="value"
              aria-label="New property value"
              bind:value={newFieldValue}
              {disabled}
              onkeydown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  commitNewField();
                }
                if (event.key === "Escape") {
                  event.preventDefault();
                  cancelAddField();
                }
              }}
              onblur={() => {
                if (newFieldKey.trim()) commitNewField();
                else cancelAddField();
              }}
            />
          </div>
        </div>
      {:else}
        <button
          type="button"
          class="vault-live-properties__add-field"
          {disabled}
          onclick={() => void beginAddField()}
        >
          <Plus size={12} strokeWidth={2} aria-hidden="true" />
          <span>Add property</span>
        </button>
      {/if}
    </div>
  {/if}
</div>
