<script lang="ts">
  /**
   * Quiet row/col controls when the Live caret is inside a table.
   */
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import type { Editor } from "@tiptap/core";
  import {
    Columns3,
    Rows3,
    Trash2,
  } from "@lucide/svelte";

  interface Props {
    open: boolean;
    anchor: { left: number; top: number; width: number; height: number } | null;
    editor: Editor | null;
    disabled?: boolean;
  }

  let { open, anchor, editor, disabled = false }: Props = $props();

  const style = $derived.by(() => {
    if (!anchor) return "";
    const pad = 8;
    const w = 220;
    const h = 36;
    let left = anchor.left + anchor.width / 2 - w / 2;
    left = Math.max(pad, Math.min(left, window.innerWidth - w - pad));
    let top = anchor.top - h - 6;
    if (top < pad) top = anchor.top + anchor.height + 6;
    return `left:${Math.round(left)}px;top:${Math.round(top)}px;`;
  });

  function run(cmd: () => boolean) {
    if (!editor || disabled) return;
    cmd();
  }

  const WIDTHS = ["narrow", "medium", "wide", "full"] as const;

  function cycleTableWidth() {
    if (!editor || disabled) return;
    const { $from } = editor.state.selection;
    for (let d = $from.depth; d > 0; d -= 1) {
      if ($from.node(d).type.name === "table") {
        const dom = editor.view.nodeDOM($from.before(d));
        const table =
          dom instanceof HTMLTableElement
            ? dom
            : dom instanceof HTMLElement
              ? dom.querySelector("table")
              : null;
        if (!table) return;
        const cur =
          WIDTHS.find((w) =>
            table.classList.contains(`vault-live-embed-width--${w}`),
          ) ?? "wide";
        const next = WIDTHS[(WIDTHS.indexOf(cur) + 1) % WIDTHS.length] ?? "wide";
        for (const w of WIDTHS) {
          table.classList.remove(`vault-live-embed-width--${w}`);
        }
        table.classList.add("vault-live-embed-width", `vault-live-embed-width--${next}`);
        return;
      }
    }
  }
</script>

{#if open && anchor && editor}
  <BodyPortal>
    <div
      class="vault-live-table-chrome"
      style={style}
      role="toolbar"
      tabindex="-1"
      aria-label="Table"
      onmousedown={(event) => event.preventDefault()}
    >
      <button
        type="button"
        class="vault-live-table-chrome__btn"
        title="Add column"
        aria-label="Add column"
        {disabled}
        onclick={() => run(() => editor.chain().focus().addColumnAfter().run())}
      >
        <Columns3 size={14} strokeWidth={2} />
      </button>
      <button
        type="button"
        class="vault-live-table-chrome__btn"
        title="Add row"
        aria-label="Add row"
        {disabled}
        onclick={() => run(() => editor.chain().focus().addRowAfter().run())}
      >
        <Rows3 size={14} strokeWidth={2} />
      </button>
      <button
        type="button"
        class="vault-live-table-chrome__btn"
        title="Cycle table width"
        aria-label="Cycle table width"
        {disabled}
        onclick={() => cycleTableWidth()}
      >
        Width
      </button>
      <span class="vault-live-table-chrome__sep" aria-hidden="true"></span>
      <button
        type="button"
        class="vault-live-table-chrome__btn"
        title="Delete column"
        aria-label="Delete column"
        {disabled}
        onclick={() => run(() => editor.chain().focus().deleteColumn().run())}
      >
        <Columns3 size={14} strokeWidth={2} class="opacity-60" />
        <Trash2 size={11} strokeWidth={2} class="vault-live-table-chrome__badge" />
      </button>
      <button
        type="button"
        class="vault-live-table-chrome__btn"
        title="Delete row"
        aria-label="Delete row"
        {disabled}
        onclick={() => run(() => editor.chain().focus().deleteRow().run())}
      >
        <Rows3 size={14} strokeWidth={2} class="opacity-60" />
        <Trash2 size={11} strokeWidth={2} class="vault-live-table-chrome__badge" />
      </button>
      <button
        type="button"
        class="vault-live-table-chrome__btn vault-live-table-chrome__btn--danger"
        title="Delete table"
        aria-label="Delete table"
        {disabled}
        onclick={() => run(() => editor.chain().focus().deleteTable().run())}
      >
        <Trash2 size={14} strokeWidth={2} />
      </button>
    </div>
  </BodyPortal>
{/if}
