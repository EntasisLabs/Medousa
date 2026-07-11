<script lang="ts">
  import { serializeCalloutBlock } from "$lib/utils/vaultMarkdownEdit";
  import { tick } from "svelte";
  import { X } from "@lucide/svelte";

  interface Props {
    open: boolean;
    onInsert: (markdown: string) => void;
    onClose: () => void;
  }

  let { open, onInsert, onClose }: Props = $props();

  const KINDS = [
    { id: "note", label: "Note" },
    { id: "tip", label: "Tip" },
    { id: "warning", label: "Warning" },
    { id: "important", label: "Important" },
    { id: "caution", label: "Caution" },
  ] as const;

  let kind = $state<(typeof KINDS)[number]["id"]>("note");
  let title = $state("");
  let body = $state("");

  $effect(() => {
    if (!open) return;
    kind = "note";
    title = "";
    body = "";
    void tick().then(() => {
      const el = document.querySelector(
        "[data-callout-body]",
      ) as HTMLTextAreaElement | null;
      el?.focus();
    });
  });

  function commit() {
    onInsert(serializeCalloutBlock(kind, title, body));
    onClose();
  }

  function onSheetKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="vault-interact-backdrop"
    role="dialog"
    aria-modal="true"
    aria-labelledby="callout-builder-title"
    onkeydown={onSheetKeydown}
    onclick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <form
      class="vault-interact-sheet vault-compose-sheet vault-bridge-sheet"
      onsubmit={(event) => {
        event.preventDefault();
        commit();
      }}
    >
      <header class="vault-interact-header vault-compose-header">
        <h3 id="callout-builder-title" class="sr-only">Callout</h3>
        <button
          type="button"
          class="vault-interact-dismiss ml-auto"
          aria-label="Close"
          onclick={onClose}
        >
          <X size={14} strokeWidth={2} />
        </button>
      </header>

      <p class="vault-compose-sentence">
        Add a
        <span class="vault-compose-em">{KINDS.find((row) => row.id === kind)?.label ?? "Note"}</span>
        callout
      </p>

      <div class="vault-chip-row" role="listbox" aria-label="Callout type">
        {#each KINDS as option (option.id)}
          <button
            type="button"
            class="vault-chip"
            class:vault-chip--active={kind === option.id}
            role="option"
            aria-selected={kind === option.id}
            onclick={() => (kind = option.id)}
          >
            {option.label}
          </button>
        {/each}
      </div>

      <input
        class="vault-interact-field"
        type="text"
        placeholder="Title (optional)"
        bind:value={title}
      />
      <textarea
        class="vault-interact-field vault-bridge-body"
        rows="3"
        placeholder="What should it say?"
        data-callout-body
        bind:value={body}
      ></textarea>

      <div class="vault-compose-footer">
        <button type="submit" class="vault-interact-commit">Insert callout</button>
      </div>
    </form>
  </div>
{/if}
