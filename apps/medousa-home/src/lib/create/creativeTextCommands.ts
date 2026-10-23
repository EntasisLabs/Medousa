import type { CreativeNode } from "$lib/types/generated/daemon_api";
import { styleNode, updateTextNode, type CanvasStylePatch } from "./creativeInteraction";
import { formatTextRange, textSegments, type InlineStyle } from "./creativeRichText";
/** Whole-box geometry and selected-range emphasis share a single undoable text edit. */
export function styleEditingText(node: CreativeNode, text: string, patch: CanvasStylePatch, range: [number, number]) {
  const base = updateTextNode(node,{text}), [start,end] = range;
  const {font_weight,font_style,color,underline,...whole}=patch;
  if (end<=start || start===0 && end>=text.length || font_weight===undefined&&font_style===undefined&&color===undefined&&underline===undefined) return styleNode(base,patch);
  return updateTextNode(formatTextRange(styleNode(base,whole),start,end,{
    ...(font_weight!==undefined?{font_weight}:{}), ...(font_style!==undefined?{font_style}:{}),
    ...(color!==undefined?{color}:{}), ...(underline!==undefined?{underline}:{}),
  }),{});
}
/** Controls reflect a selected word's emphasis without changing the stored text base. */
export function textRangePresentation(node: CreativeNode, range: [number,number]): CreativeNode {
  if (node.content.kind !== "text" || range[1] <= range[0]) return node;
  const selected = textSegments(node.content.text,node.text_layout,range[0],Math.min(range[1],node.content.text.length));
  const first = selected[0]?.style; if (!first) return node;
  const same = (key: keyof InlineStyle) => selected.every(s=>s.style[key]===first[key]) ? first[key] : null;
  return { ...node, content: { ...node.content, font_weight:same("font_weight") as number??node.content.font_weight, font_style:same("font_style") as "normal"|"italic"??node.content.font_style, color:same("color") as string??node.content.color }, text_layout: { width:null,align:"left",line_height:1.3,...node.text_layout,underline:same("underline") as boolean??node.text_layout?.underline } };
}
