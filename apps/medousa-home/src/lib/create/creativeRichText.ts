import type { CreativeNode, CreativeTextLayout, CreativeTextSpan } from "$lib/types/generated/daemon_api";
export type InlineStyle = Pick<CreativeTextSpan, "font_weight" | "font_style" | "color" | "underline">;
export function textSegments(text: string, layout?: CreativeTextLayout | null, start = 0, end = text.length) {
  const spans = layout?.spans ?? [], breaks = new Set([start, end]);
  for (const span of spans) { if (span.start > start && span.start < end) breaks.add(span.start); if (span.end > start && span.end < end) breaks.add(span.end); }
  const points = [...breaks].sort((a,b) => a-b);
  return points.slice(0,-1).map((from,i) => ({ text: text.slice(from,points[i+1]), start: from, end: points[i+1], style: spans.find(s=>s.start<=from&&s.end>=points[i+1]) ?? {} as InlineStyle }));
}
function compact(spans: CreativeTextSpan[]) {
  const result: CreativeTextSpan[] = [];
  for (const span of spans) {
    if (span.end <= span.start) continue;
    const previous = result.at(-1);
    if (previous?.end === span.start && JSON.stringify({ ...previous, start:0, end:0 }) === JSON.stringify({ ...span, start:0, end:0 })) previous.end = span.end;
    else result.push({ ...span });
  }
  return result;
}
export function formatTextRange(node: CreativeNode, start: number, end: number, style: InlineStyle): CreativeNode {
  if (node.content.kind !== "text" || end <= start) return node;
  const text = node.content.text;
  const segments = textSegments(text, node.text_layout, 0, text.length);
  const spans: CreativeTextSpan[] = [];
  for (const s of segments) {
    const points = [...new Set([s.start, Math.max(s.start, Math.min(s.end,start)), Math.max(s.start, Math.min(s.end,end)), s.end])].sort((a,b)=>a-b);
    for (let i=0;i<points.length-1;i++) {
      const from=points[i], to=points[i+1];
      spans.push({ font_weight:null,font_style:null,color:null,underline:null,...s.style,...(from>=start&&to<=end?style:{}),start:from,end:to });
    }
  }
  return { ...node, text_layout: { width:null,align:"left",line_height:1.3,...node.text_layout,spans:compact(spans) } };
}
/** A single input edit shifts surviving spans and gives inserted text the caret's style. */
export function remapTextSpans(before: string, after: string, spans: CreativeTextSpan[] = []) {
  let start=0, suffix=0;
  while(start<before.length&&start<after.length&&before[start]===after[start])start++;
  while(suffix<before.length-start&&suffix<after.length-start&&before[before.length-1-suffix]===after[after.length-1-suffix])suffix++;
  const oldEnd=before.length-suffix,newEnd=after.length-suffix,delta=after.length-before.length;
  const result: CreativeTextSpan[]=[];
  for(const span of spans){
    if(span.start<start)result.push({...span,end:Math.min(span.end,start)});
    if(span.end>oldEnd)result.push({...span,start:Math.max(span.start,oldEnd)+delta,end:span.end+delta});
  }
  const inherited=spans.find(s=>s.start<=start&&s.end>start)??spans.find(s=>s.end===start);
  if(inherited&&newEnd>start)result.push({...inherited,start,end:newEnd});
  return compact(result.sort((a,b)=>a.start-b.start));
}
/** Only application-created text and breaks enter the rich editor; pasted HTML is never interpreted. */
export function editableText(root: Node): string {
  return [...root.childNodes].map((node,i)=> {
    const block = node instanceof Element && node.hasAttribute("data-paragraph");
    const text = node.nodeType===Node.TEXT_NODE ? node.textContent??"" : node instanceof HTMLBRElement ? node.hasAttribute("data-placeholder")?"":"\n" : editableText(node);
    return (block && i>0 ? "\n" : "") + text;
  }).join("");
}
export function editableRange(root: HTMLElement): [number,number] | null {
  const selection=root.ownerDocument.getSelection(); if(!selection?.rangeCount)return null;
  const range=selection.getRangeAt(0); if(!root.contains(range.startContainer)||!root.contains(range.endContainer))return null;
  const offset=(node:Node,n:number)=>{const r=root.ownerDocument.createRange();r.selectNodeContents(root);r.setEnd(node,n);return editableText(r.cloneContents()).length;};
  return [offset(range.startContainer,range.startOffset),offset(range.endContainer,range.endOffset)];
}
export function selectEditableText(root: HTMLElement, start=0, end=editableText(root).length) {
  const positions: {node:Node;offset:number;at:number}[]=[];let at=0;
  function walk(node:Node){
    [...node.childNodes].forEach((child,i)=>{
      if(child instanceof Element&&child.hasAttribute("data-paragraph")&&i>0)at++;
      if(child.nodeType===Node.TEXT_NODE){positions.push({node:child,offset:0,at});at+=(child.textContent??"").length;positions.push({node:child,offset:(child.textContent??"").length,at});}
      else if(child instanceof HTMLBRElement){positions.push({node:child.parentNode!,offset:[...child.parentNode!.childNodes].indexOf(child),at});if(!child.hasAttribute("data-placeholder"))at++;}
      else walk(child);
    });
  }walk(root);
  function point(n:number){for(let i=0;i<positions.length;i++){const p=positions[i];if(p.node.nodeType===Node.TEXT_NODE&&n>=p.at&&n<=p.at+(p.node.textContent??"").length-p.offset)return {node:p.node,offset:p.offset+n-p.at};}return positions.find(p=>p.at>=n)??positions.at(-1)??{node:root,offset:0};}
  const a=point(start),b=point(end),r=root.ownerDocument.createRange();r.setStart(a.node,a.offset);r.setEnd(b.node,b.offset);const s=root.ownerDocument.getSelection();s?.removeAllRanges();s?.addRange(r);
}
