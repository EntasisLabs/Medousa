/** @vitest-environment happy-dom */
import { expect,it } from "vitest";
import { formatTextRange,remapTextSpans,textSegments,editableText,editableRange,selectEditableText } from "./creativeRichText";
import { createNode } from "./creativeScene";
import { styleEditingText } from "./creativeTextCommands";
import { styleNode,updateTextNode } from "./creativeInteraction";
import { layoutText } from "./creativeText";
const node=()=>createNode({kind:"text",text:"Keep your promises.",font_size:30,font_family:"Georgia",color:"#123456"},{x:0,y:0,width:300,height:40});
it("formats a word without changing neighboring text and survives unrelated geometry styling",()=>{
  const original=node(), rich=formatTextRange(original,10,18,{color:"#a7442d",font_style:"italic"});
  expect(original.text_layout).toBeUndefined();
  const resized=styleNode(rich,{font_size:60,letter_spacing:-2});
  const segments=textSegments(resized.content.kind==="text"?resized.content.text:"",resized.text_layout);
  expect(segments.map(s=>s.text).join("")).toBe("Keep your promises.");
  expect(segments.find(s=>s.text==="promises")?.style).toMatchObject({color:"#a7442d",font_style:"italic"});
  expect(resized.text_layout?.letter_spacing).toBe(-2);
});
it("remaps inline formatting when text is inserted, replaced or deleted",()=>{
  const rich=formatTextRange(node(),10,18,{font_weight:700});
  expect(remapTextSpans("Keep your promises.","Keep all your promises.",rich.text_layout!.spans??[]).find(s=>s.font_weight===700)).toMatchObject({start:14,end:22});
  const edited=updateTextNode(rich,{text:"Keep your promise."});
  expect(edited.text_layout?.spans?.find(s=>s.font_weight===700)).toMatchObject({start:10,end:17});
});
it("applies whole-box metrics and range emphasis within one live text edit",()=>{
  const rich=styleEditingText(node(),"Keep your promises.",{font_weight:700,font_size:40},[10,18]);
  expect(rich.content).toMatchObject({font_size:40});expect(rich.text_layout?.spans?.some(s=>s.start===10&&s.end===18&&s.font_weight===700)).toBe(true);
  const all=styleEditingText(rich,"Keep your promises.",{font_weight:500},[0,19]);
  expect(all.content).toMatchObject({font_weight:500});expect(all.text_layout?.spans?.some(s=>s.font_weight===700)).toBe(false);
});
it("uses tracking and paragraph spacing in shared bounds, wrapping and line positions",()=>{
  const content={kind:"text" as const,text:"AB\nCD",font_family:"sans-serif",font_size:20,color:"#000"};
  const layout=layoutText(content,t=>({width:t.length*10}),{width:null,align:"left",line_height:1,letter_spacing:3,paragraph_spacing:12});
  expect(layout.width).toBeCloseTo(28.4);expect(layout.height).toBeCloseTo(54.4);expect(layout.ys).toEqual([0,32]);expect(layout.starts).toEqual([0,3]);
});
it("roundtrips browser selection offsets across paragraphs and empty lines",()=>{
  const root=document.createElement("div");root.contentEditable="true";document.body.append(root);
  for(const value of ["First","","Last"]){const p=document.createElement("div");p.dataset.paragraph="";if(value)p.textContent=value;else{const br=document.createElement("br");br.dataset.placeholder="";p.append(br);}root.append(p);}
  expect(editableText(root)).toBe("First\n\nLast");selectEditableText(root,7,11);expect(editableRange(root)).toEqual([7,11]);root.remove();
});
