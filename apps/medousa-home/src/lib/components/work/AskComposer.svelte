<script lang="ts">
  import { tick } from "svelte";
  import GrowingTextarea from "$lib/components/ui/GrowingTextarea.svelte";
  import ComposerSkillPills from "$lib/components/chat/ComposerSkillPills.svelte";
  import ComposerSkillSlashMenu from "$lib/components/chat/ComposerSkillSlashMenu.svelte";
  import { catalog } from "$lib/stores/catalog.svelte";
  import { composerAttachments } from "$lib/stores/composerAttachments.svelte";
  import { runtime } from "$lib/stores/runtime.svelte";
  import { workspace } from "$lib/stores/workspace.svelte";
  import {
    buildAskJobRequest,
    canSubmitAskJob,
  } from "$lib/utils/askPrompt";
  import {
    buildComposerSlashItems,
    composerSlashToken,
    stripComposerSlashToken,
    type ComposerSlashItem,
  } from "$lib/utils/composerSkillSlash";
  import {
    placeComposerSlashMenuAnchor,
    type SlashMenuAnchor,
  } from "$lib/utils/slashMenuPlacement";

  interface Props {
    /** Sheet mode: taller textarea, no outer border. */
    sheet?: boolean;
    placeholder?: string;
    onQueued?: () => void;
    /** Called when host should focus the textarea (dock open). */
    autofocus?: boolean;
  }

  let {
    sheet = false,
    placeholder = "Ask Medousa to work on something… Type / for skills",
    onQueued,
    autofocus = false,
  }: Props = $props();

  const attachments = composerAttachments.ask;

  let prompt = $state("");
  let rootEl = $state<HTMLElement | null>(null);
  let textareaEl = $state<HTMLTextAreaElement | null>(null);
  let cursor = $state(0);
  let highlightIndex = $state(0);
  let slashAnchor = $state<SlashMenuAnchor | null>(null);

  const slashToken = $derived(composerSlashToken(prompt, cursor));
  const slashItems = $derived(
    slashToken
      ? buildComposerSlashItems({
          filter: slashToken.filter,
          manuscripts: catalog.manuscripts,
          capabilities: catalog.capabilities,
          attachedSkillIds: attachments.skillIds,
          attachedToolIds: attachments.toolIds,
          includeCommands: false,
        })
      : [],
  );
  const slashOpen = $derived(Boolean(slashToken && slashItems.length > 0));

  const canSubmit = $derived(
    !workspace.askSubmitting &&
      canSubmitAskJob(prompt, attachments.skillIds),
  );

  $effect(() => {
    if (catalog.manuscripts.length === 0 && !catalog.loading) {
      void catalog.refresh();
    }
  });

  $effect(() => {
    if (!autofocus) return;
    void tick().then(() => textareaEl?.focus());
  });

  $effect(() => {
    void slashToken;
    void slashItems.length;
    highlightIndex = 0;
    if (!slashOpen || !textareaEl) {
      slashAnchor = null;
      return;
    }
    const rect = textareaEl.getBoundingClientRect();
    // Viewport-aware: dock sits low — prefer flip above the input.
    slashAnchor = placeComposerSlashMenuAnchor({
      top: rect.top,
      bottom: rect.bottom,
      left: rect.left,
    });
  });

  function syncCursorFromDom() {
    if (!textareaEl) return;
    cursor = textareaEl.selectionStart ?? prompt.length;
  }

  function applySlashItem(item: ComposerSlashItem) {
    const token = slashToken;
    if (!token) return;
    if (item.kind === "skill") {
      attachments.attachSkill(item.id);
      const next = stripComposerSlashToken(prompt, token, "");
      prompt = next.value;
      cursor = next.cursor;
    } else if (item.kind === "tool") {
      attachments.attachTool(item.id);
      const next = stripComposerSlashToken(prompt, token, "");
      prompt = next.value;
      cursor = next.cursor;
    } else {
      const next = stripComposerSlashToken(prompt, token, item.insert);
      prompt = next.value;
      cursor = next.cursor;
    }
    void tick().then(() => {
      if (!textareaEl) return;
      textareaEl.focus();
      textareaEl.setSelectionRange(cursor, cursor);
    });
  }

  function resetComposer() {
    prompt = "";
    cursor = 0;
    attachments.clear();
  }

  async function submit(event?: Event) {
    event?.preventDefault();
    if (!canSubmit) return;
    const request = buildAskJobRequest(
      prompt,
      attachments.skillIds,
      attachments.toolIds,
    );
    try {
      await workspace.submitAsk({ ...request, modelHint: runtime.model });
      resetComposer();
      onQueued?.();
    } catch {
      // workspace.askError
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (slashOpen && slashItems.length > 0) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        highlightIndex = (highlightIndex + 1) % slashItems.length;
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        highlightIndex =
          (highlightIndex - 1 + slashItems.length) % slashItems.length;
        return;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        event.preventDefault();
        const item = slashItems[highlightIndex];
        if (item) applySlashItem(item);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        const token = slashToken;
        if (token) {
          const next = stripComposerSlashToken(prompt, token, "");
          prompt = next.value;
          cursor = next.cursor;
        }
        return;
      }
    }

    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void submit();
    }
  }

</script>

<form
  bind:this={rootEl}
  class={sheet ? "flex flex-col gap-2" : "ask-composer flex flex-col gap-2"}
  onsubmit={submit}
>
  <ComposerSkillPills host="ask" disabled={workspace.askSubmitting} />

  <div class="composer-bar chat-composer-bar">
    <GrowingTextarea
      bind:value={prompt}
      bind:element={textareaEl}
      {placeholder}
      disabled={workspace.askSubmitting}
      maxHeight={sheet ? 160 : 128}
      onkeydown={handleKeydown}
      oninput={syncCursorFromDom}
      onclick={syncCursorFromDom}
      onkeyup={syncCursorFromDom}
      onselect={syncCursorFromDom}
      aria-label="Ask prompt"
    />
    <button
      type="submit"
      class="composer-bar-send"
      disabled={!canSubmit}
      aria-label="Queue ask job"
    >
      {workspace.askSubmitting ? "…" : "↑"}
    </button>
  </div>

  {#if workspace.askError}
    <p class="text-xs text-error-400">{workspace.askError}</p>
  {:else if workspace.askMessage}
    <p class="text-xs text-surface-400">{workspace.askMessage}</p>
  {/if}

  <ComposerSkillSlashMenu
    open={slashOpen}
    items={slashItems}
    anchor={slashAnchor}
    {highlightIndex}
    onSelect={applySlashItem}
    onClose={() => {
      const token = slashToken;
      if (!token) return;
      const next = stripComposerSlashToken(prompt, token, "");
      prompt = next.value;
      cursor = next.cursor;
    }}
    onHighlight={(index) => (highlightIndex = index)}
  />
</form>
