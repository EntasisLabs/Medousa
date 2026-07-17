<script lang="ts">
  /**
   * Quiet label+body list for liquid builders (accordion / tabs / steps).
   * Compact rows until pencil opens the nested item editor.
   */
  import { Pencil, Plus, Trash2 } from "@lucide/svelte";
  import type { Snippet } from "svelte";

  export type QuietListRow = {
    title: string;
    body: string;
  };

  interface Props {
    rows: QuietListRow[];
    addLabel?: string;
    minRows?: number;
    /** Optional leading control per row (Open toggle, status, …). */
    leading?: Snippet<[number]>;
    onEdit: (index: number) => void;
    onRemove: (index: number) => void;
    onAdd: () => void;
  }

  let {
    rows,
    addLabel = "Add item",
    minRows = 1,
    leading,
    onEdit,
    onRemove,
    onAdd,
  }: Props = $props();

  function truncate(text: string, max = 72): string {
    const one = text.replace(/\s+/g, " ").trim();
    if (!one) return "";
    if (one.length <= max) return one;
    return `${one.slice(0, max - 1)}…`;
  }

  function preview(row: QuietListRow): string {
    const title = truncate(row.title, 40) || "Untitled";
    const body = truncate(row.body, 48);
    return body ? `${title} — ${body}` : title;
  }
</script>

<div class="vault-liquid-quiet-list">
  {#each rows as row, index (index)}
    <div class="vault-liquid-quiet-list__row">
      {#if leading}
        <div class="vault-liquid-quiet-list__leading">
          {@render leading(index)}
        </div>
      {/if}
      <button
        type="button"
        class="vault-liquid-quiet-list__preview"
        onclick={() => onEdit(index)}
        title="Edit"
      >
        <span class="vault-liquid-quiet-list__preview-text">{preview(row)}</span>
      </button>
      <div class="vault-liquid-quiet-list__actions">
        <button
          type="button"
          class="vault-liquid-quiet-list__icon"
          aria-label="Edit"
          title="Edit"
          onclick={() => onEdit(index)}
        >
          <Pencil size={13} strokeWidth={2} />
        </button>
        <button
          type="button"
          class="vault-liquid-quiet-list__icon vault-liquid-quiet-list__icon--danger"
          aria-label="Remove"
          title="Remove"
          disabled={rows.length <= minRows}
          onclick={() => onRemove(index)}
        >
          <Trash2 size={13} strokeWidth={2} />
        </button>
      </div>
    </div>
  {/each}
  <button type="button" class="vault-liquid-quiet-list__add" onclick={onAdd}>
    <Plus size={14} strokeWidth={2} />
    {addLabel}
  </button>
</div>
