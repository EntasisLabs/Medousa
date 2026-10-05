<script lang="ts">
  import "$lib/styles/editor-find.postcss";
  import { CaseSensitive, ChevronDown, ChevronUp, Regex, Replace, TextSelect, WholeWord, X } from "@lucide/svelte";
  import type { CodeFindController } from "$lib/code/codeFindController.svelte";
  import { titleWithKeys } from "$lib/utils/keyboardShortcutsCatalog";
  import { usesMetaModKey } from "$lib/platform";

  let { find }: { find: CodeFindController } = $props();
  const ids = $props.id();
  let input = $state<HTMLInputElement>();
  let replaceInput = $state<HTMLInputElement>();
  const disabled = $derived(!find.matchCount || !!find.error);
  const replaceVisible = $derived(find.state.replaceMode && !find.readOnly);

  $effect(() => {
    void find.focusEpoch;
    input?.focus();
    input?.select();
  });

  function keepFocus(event: MouseEvent) { event.preventDefault(); }

  function avoidMatch(node: HTMLDivElement, controller: CodeFindController) { return controller.attachBar(node); }

  function escapeFromControls(node: HTMLDivElement) {
    const handle = (event: KeyboardEvent) => { if (event.key === "Escape") keydown(event); };
    node.addEventListener("keydown", handle);
    return { destroy: () => node.removeEventListener("keydown", handle) };
  }

  function toggleReplace() {
    find.state.replaceMode = !find.state.replaceMode;
    queueMicrotask(() => {
      const field = find.state.replaceMode ? replaceInput : input;
      field?.focus();
      field?.select();
    });
  }

  function keydown(event: KeyboardEvent, replacing = false) {
    const mod = event.metaKey || event.ctrlKey;
    const key = event.key.toLowerCase();
    if (event.key === "Escape") find.close();
    else if ((mod && key === "f" && !event.shiftKey) || (!usesMetaModKey() && event.ctrlKey && key === "h")) {
      if (event.altKey || key === "h") {
        if (!find.readOnly) {
          find.state.replaceMode = true;
          queueMicrotask(() => replaceInput?.focus());
        }
      } else { input?.focus(); input?.select(); }
    } else if (event.key === "F3" || (mod && key === "g")) {
      if (event.shiftKey) find.previous(); else find.next();
    } else if (event.key === "Enter") {
      if (replacing) find.replace(mod);
      else if (event.shiftKey) find.previous();
      else find.next();
    } else return;
    event.preventDefault();
    event.stopPropagation();
  }
</script>

<div use:escapeFromControls use:avoidMatch={find} class="editor-find-bar editor-find-bar--code"
  class:editor-find-bar--below={find.floatBelow} role="search" aria-label="Find in file">
  <div class="editor-find-bar-row">
    <input bind:this={input} class="editor-find-input" type="text" placeholder="Find"
      aria-label="Find in file" aria-invalid={!!find.error} aria-describedby={find.error ? `${ids}-error` : undefined}
      value={find.state.query} oninput={(e) => find.setQuery(e.currentTarget.value)}
      onkeydown={(e) => keydown(e)} autocomplete="off" spellcheck="false" />
    {#if find.state.query}
      <span class="editor-find-divider" aria-hidden="true"></span>
      <span class="editor-find-status" aria-live="polite">{find.status}</span>
    {/if}
    <span class="editor-find-divider" aria-hidden="true"></span>
    <div class="editor-find-nav">
      <button type="button" class="editor-find-btn" class:editor-find-btn--active={find.state.matchCase}
        aria-label="Match case" aria-pressed={find.state.matchCase} title="Match case"
        onmousedown={keepFocus} onclick={() => find.toggle("matchCase")}><CaseSensitive size={13} strokeWidth={2.25} /></button>
      <button type="button" class="editor-find-btn" class:editor-find-btn--active={find.state.wholeWord}
        aria-label="Whole word" aria-pressed={find.state.wholeWord} title="Whole word"
        onmousedown={keepFocus} onclick={() => find.toggle("wholeWord")}><WholeWord size={13} strokeWidth={2.25} /></button>
      <button type="button" class="editor-find-btn" class:editor-find-btn--active={find.state.regexp}
        aria-label="Regular expression" aria-pressed={find.state.regexp} title="Regular expression"
        onmousedown={keepFocus} onclick={() => find.toggle("regexp")}><Regex size={13} strokeWidth={2.25} /></button>
      <button type="button" class="editor-find-btn" class:editor-find-btn--active={!!find.state.selection}
        aria-label="Find in selection" aria-pressed={!!find.state.selection}
        title={find.state.selection ? "Search the whole file" : "Find in selection"}
        disabled={!find.canSelect && !find.state.selection}
        onmousedown={keepFocus} onclick={() => find.toggleSelection()}><TextSelect size={13} strokeWidth={2.25} /></button>
      {#if !find.readOnly}
        <button type="button" class="editor-find-btn" class:editor-find-btn--active={replaceVisible}
          aria-label="Toggle replace" aria-expanded={replaceVisible} aria-controls={`${ids}-replace`}
          title={titleWithKeys("Replace", usesMetaModKey() ? "mod:⌥F" : "mod:H")} onmousedown={keepFocus} onclick={toggleReplace}><Replace size={13} strokeWidth={2.25} /></button>
      {/if}
      <button type="button" class="editor-find-btn" aria-label="Previous match" title="Previous match (Shift+Enter)"
        disabled={disabled} onmousedown={keepFocus} onclick={() => find.previous()}><ChevronUp size={13} strokeWidth={2.25} /></button>
      <button type="button" class="editor-find-btn" aria-label="Next match" title="Next match (Enter)"
        disabled={disabled} onmousedown={keepFocus} onclick={() => find.next()}><ChevronDown size={13} strokeWidth={2.25} /></button>
      <span class="editor-find-divider" aria-hidden="true"></span>
      <button type="button" class="editor-find-btn" aria-label="Close find" title="Close (Esc)"
        onmousedown={keepFocus} onclick={() => find.close()}><X size={13} strokeWidth={2.25} /></button>
    </div>
  </div>
  {#if replaceVisible}
    <div id={`${ids}-replace`} class="editor-find-bar-row editor-find-bar-row--replace">
      <input bind:this={replaceInput} class="editor-find-input" type="text" placeholder="Replace"
        aria-label="Replace with" value={find.state.replacement}
        oninput={(e) => find.setReplacement(e.currentTarget.value)} onkeydown={(e) => keydown(e, true)}
        autocomplete="off" spellcheck="false" />
      <button type="button" class="editor-find-text-btn" title="Replace match (Enter)" disabled={disabled}
        onmousedown={keepFocus} onclick={() => find.replace()}>Replace</button>
      <button type="button" class="editor-find-text-btn" title={titleWithKeys("Replace all", "mod:Enter")} disabled={disabled}
        onmousedown={keepFocus} onclick={() => find.replace(true)}>All</button>
    </div>
  {/if}
  {#if find.error}
    <div id={`${ids}-error`} class="editor-find-message editor-find-message--error" role="status">{find.error}</div>
  {:else if find.feedback}
    <div class="editor-find-message editor-find-bar-row" role="status">
      <span>{find.feedback}</span>
      {#if find.replacementUndoAvailable}
        <button type="button" class="editor-find-text-btn" onmousedown={keepFocus} onclick={() => find.undoReplacement()}>Undo</button>
      {/if}
    </div>
  {:else if find.state.selection || find.readOnly}
    <div class="editor-find-message">{find.state.selection ? "In selection" : "Read-only file"}</div>
  {/if}
</div>
