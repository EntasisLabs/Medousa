<script lang="ts">
  import { onMount } from "svelte";
  import { layoutText, textMetricsVersion, type CreativeText, type TextStylePatch } from "$lib/create/creativeText";
  import { editableText, editableRange, selectEditableText, textSegments } from "$lib/create/creativeRichText";
  import type { CreativeNode } from "$lib/types/generated/daemon_api";
  let { node, matrix, value = $bindable(""), input = $bindable<HTMLElement>(), disabled, oninput, onfinish, onselection = () => {}, onformat = () => {} }: {
    node: CreativeNode & { content: CreativeText }; matrix: number[]; value?: string; input?: HTMLElement; disabled: boolean;
    oninput: (value: string) => void; onfinish: (focus: boolean) => void; onselection?: (range: [number,number]) => void; onformat?: (patch: TextStylePatch) => void;
  } = $props();
  let composing = $state(false), rendered = "";
  const layout = $derived.by(() => { void $textMetricsVersion; return layoutText({ ...node.content, text: value }, undefined, node.text_layout); });
  function selection() { if (input) { const range = editableRange(input); if (range) onselection(range); } }
  function read() {
    if (!input || composing) return;
    const text = editableText(input).replace(/\r/g, "").slice(0,16000);
    selection(); value = text; oninput(text);
  }
  function insert(text: string) {
    if (!input || disabled) return;
    const selection = input.ownerDocument.getSelection(); if (!selection?.rangeCount || !input.contains(selection.anchorNode)) return;
    const range = selection.getRangeAt(0); range.deleteContents(); const content = document.createTextNode(text); range.insertNode(content); range.setStartAfter(content); range.collapse(true); selection.removeAllRanges(); selection.addRange(range); read();
  }
  $effect(() => {
    if (!input || composing) return;
    const next = JSON.stringify([value,node.text_layout,node.content]); if (rendered === next) return;
    const range = editableRange(input); rendered = next;
    const fragment = document.createDocumentFragment(); let offset = 0;
    for (const paragraph of value.split("\n")) {
      const block = document.createElement("div"); block.dataset.paragraph = ""; block.style.minHeight = `${layout.lineHeight}px`;
      block.style.marginBottom = `${node.text_layout?.paragraph_spacing ?? 0}px`;
      for (const segment of textSegments(value,node.text_layout,offset,offset+paragraph.length)) {
        const span = document.createElement("span"); span.textContent = segment.text;
        span.style.fontWeight = String(segment.style.font_weight ?? node.content.font_weight ?? 400);
        span.style.fontStyle = segment.style.font_style ?? node.content.font_style ?? "normal";
        span.style.color = segment.style.color ?? node.content.color;
        span.style.textDecoration = (segment.style.underline ?? node.text_layout?.underline) ? "underline" : "none";
        block.append(span);
      }
      if (!paragraph) { const br=document.createElement("br");br.dataset.placeholder="";block.append(br); }
      fragment.append(block); offset += paragraph.length + 1;
    }
    if (fragment.lastChild instanceof HTMLElement) fragment.lastChild.style.marginBottom = "0";
    input.replaceChildren(fragment);
    if (range) selectEditableText(input,Math.min(value.length,range[0]),Math.min(value.length,range[1]));
  });
  onMount(() => { document.addEventListener("selectionchange",selection); return () => document.removeEventListener("selectionchange",selection); });
</script>
<div bind:this={input} class="create-inline-text" role="textbox" aria-label="Canvas text" aria-multiline="true" contenteditable={!disabled} spellcheck="false" tabindex="0"
  style:white-space={node.text_layout?.width ? "pre-wrap" : "pre"} style:overflow-wrap="anywhere" style:text-align={layout.align}
  style:transform={`matrix(${matrix.join(",")})`} style:width={`${layout.width}px`} style:height={`${layout.height}px`}
  style:padding={`${layout.padding}px ${layout.padding}px ${layout.padding}px ${layout.insetX}px`} style:line-height={`${layout.lineHeight}px`}
  style:font-weight={node.content.font_weight ?? 400} style:font-style={node.content.font_style ?? "normal"} style:font-size={`${node.content.font_size}px`}
  style:font-family={node.content.font_family} style:color={node.content.color} style:letter-spacing={`${node.text_layout?.letter_spacing ?? 0}px`}
  oninput={read} onpointerup={selection} onkeyup={selection}
  oncompositionstart={() => composing = true} oncompositionend={() => { composing=false;read(); }}
  ondrop={e => { e.preventDefault(); insert(e.dataTransfer?.getData("text/plain") ?? ""); }}
  onpaste={e => { e.preventDefault(); insert(e.clipboardData?.getData("text/plain") ?? ""); }}
  onbeforeinput={e => {
    if (e.inputType === "insertParagraph" || e.inputType === "insertLineBreak") { e.preventDefault(); insert("\n"); }
    if (e.inputType.startsWith("format")) e.preventDefault();
  }}
  onblur={e => { if (!(e.relatedTarget instanceof Element && e.relatedTarget.closest("[data-create-text-controls]"))) onfinish(false); }}
  onkeydown={e => {
    e.stopPropagation();
    if ((e.metaKey || e.ctrlKey) && ["b","i","u"].includes(e.key.toLowerCase())) {
      e.preventDefault(); selection(); const key=e.key.toLowerCase();
      const range=input?editableRange(input):null, style=range?textSegments(value,node.text_layout,range[0],Math.min(value.length,range[0]+1))[0]?.style:undefined;
      onformat(key==="b"?{font_weight:(style?.font_weight??node.content.font_weight??400)>=600?400:700}:key==="i"?{font_style:(style?.font_style??node.content.font_style)==="italic"?"normal":"italic"}:{underline:!(style?.underline??node.text_layout?.underline)});
    } else if (e.key === "Escape" || (e.metaKey || e.ctrlKey) && e.key === "Enter") { e.preventDefault();onfinish(true); }
  }}></div>
<style>
  .create-inline-text { position:absolute;left:0;top:0;transform-origin:0 0;z-index:1;margin:0;border:0;border-radius:0;overflow:visible;background:transparent;box-shadow:none;appearance:none;box-sizing:border-box;min-width:0;word-spacing:normal;text-transform:none;text-indent:0;tab-size:8;outline:1px solid var(--create-selection);cursor:text;user-select:text;-webkit-user-select:text;touch-action:auto; }
</style>
