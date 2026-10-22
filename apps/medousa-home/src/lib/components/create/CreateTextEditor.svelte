<script lang="ts">
  import { layoutText, textMetricsVersion, type CreativeText } from "$lib/create/creativeText";
  import type { CreativeNode } from "$lib/types/generated/daemon_api";
  let { node, matrix, value = $bindable(""), input = $bindable<HTMLTextAreaElement>(), disabled, oninput, onfinish }: {
    node: CreativeNode & { content: CreativeText };
    matrix: number[];
    value?: string;
    input?: HTMLTextAreaElement;
    disabled: boolean;
    oninput: (value: string) => void;
    onfinish: (focus: boolean) => void;
  } = $props();
  const layout = $derived.by(() => { void $textMetricsVersion; return layoutText({ ...node.content, text: value }, undefined, node.text_layout); });
</script>

<textarea bind:this={input} bind:value class="create-inline-text" aria-label="Canvas text" maxlength="16000" spellcheck="false" wrap={node.text_layout?.width ? "soft" : "off"}
  style:white-space={node.text_layout?.width ? "pre-wrap" : "pre"} style:overflow-wrap="anywhere" style:text-align={layout.align}
  style:transform={`matrix(${matrix.join(",")})`} style:width={`${layout.width}px`} style:height={`${layout.height}px`}
  style:padding={`${layout.padding}px ${layout.padding}px ${layout.padding}px ${layout.insetX}px`} style:line-height={`${layout.lineHeight}px`}
  style:font-weight={node.content.font_weight ?? 400} style:font-style={node.content.font_style ?? "normal"} style:font-size={`${node.content.font_size}px`}
  style:font-family={node.content.font_family} style:color={node.content.color} readonly={disabled}
  oninput={(e) => oninput(e.currentTarget.value)}
  onblur={(e) => { if (!(e.relatedTarget instanceof Element && e.relatedTarget.closest("[data-create-text-controls]"))) onfinish(false); }}
  onkeydown={(e) => { e.stopPropagation(); if (e.key === "Escape" || (e.metaKey || e.ctrlKey) && e.key === "Enter") { e.preventDefault(); onfinish(true); } }}></textarea>

<style>
  textarea.create-inline-text {
    position: absolute;
    left: 0;
    top: 0;
    transform-origin: 0 0;
    z-index: 1;
    padding: 0;
    margin: 0;
    border: 0;
    border-radius: 0;
    resize: none;
    overflow: hidden;
    background: transparent;
    box-shadow: none;
    appearance: none;
    box-sizing: border-box;
    min-width: 0;
    min-height: 0;
    letter-spacing: normal;
    word-spacing: normal;
    text-transform: none;
    text-indent: 0;
    tab-size: 8;
    line-height: 1.2;
    white-space: pre;
    outline: 1px solid var(--create-selection);
  }
</style>
