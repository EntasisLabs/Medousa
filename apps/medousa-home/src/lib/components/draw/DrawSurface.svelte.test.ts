/** @vitest-environment happy-dom */
import { afterEach,beforeEach,expect,it,vi } from "vitest";
import { mount,unmount,flushSync } from "svelte";
import DrawSurface from "./DrawSurface.svelte";
import { createEmptyDrawDocument } from "$lib/draw/drawDocument";
let instance:ReturnType<typeof mount>|null=null;
const change=vi.fn();
beforeEach(()=>{
  vi.useFakeTimers();change.mockReset();
  vi.stubGlobal("DOMPoint",class {constructor(public x:number,public y:number){}matrixTransform(){return this;}});
  Object.defineProperty(SVGElement.prototype,"getScreenCTM",{configurable:true,value:()=>({inverse:()=>({})})});
  HTMLElement.prototype.setPointerCapture=vi.fn();HTMLElement.prototype.hasPointerCapture=()=>false;
});
afterEach(async()=>{if(instance)await unmount(instance);instance=null;document.body.replaceChildren();vi.unstubAllGlobals();vi.useRealTimers();});
function render(){const target=document.createElement("div");document.body.append(target);instance=mount(DrawSurface,{target,props:{document:createEmptyDrawDocument(),editable:true,variant:"full",onchange:change}});flushSync();return target;}
function pointer(target:Element,type:string,x:number,y:number){target.dispatchEvent(new PointerEvent(type,{bubbles:true,clientX:x,clientY:y,button:0,pointerId:1,pointerType:"pen",pressure:.7,tiltX:25}));flushSync();}
it("preserves Notes' immediate ink, settling persistence, and brush metadata",()=>{
  const root=render(),stage=root.querySelector('[aria-label="Editable drawing canvas"]')!;
  pointer(stage,"pointerdown",10,10);pointer(stage,"pointermove",60,30);expect(root.querySelector('.medousa-draw-stroke--active')).toBeTruthy();
  pointer(stage,"pointerup",60,30);expect(change).not.toHaveBeenCalled();
  vi.advanceTimersByTime(650);expect(change).toHaveBeenCalledTimes(1);
  expect(change.mock.calls[0][0].strokes[0]).toMatchObject({input:"pen",brush:{kind:"pen",size:6},points:expect.arrayContaining([expect.objectContaining({pressure:expect.any(Number),tiltX:25})])});
});
it("retains partial ink erasing and undo through Notes' existing options UI",()=>{
  const root=render(),stage=root.querySelector('[aria-label="Editable drawing canvas"]')!;
  pointer(stage,"pointerdown",0,50);pointer(stage,"pointermove",100,50);pointer(stage,"pointerup",100,50);vi.advanceTimersByTime(650);
  const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>("button")].find(b=>b.getAttribute("aria-label")===text||b.textContent?.trim()===text)!;
  button("Erase").click();flushSync();pointer(stage,"pointerdown",50,50);pointer(stage,"pointerup",50,50);vi.advanceTimersByTime(650);
  expect(change.mock.calls.at(-1)![0].strokes).toHaveLength(2);
  button("Drawing options").click();flushSync();button("Undo").click();flushSync();vi.advanceTimersByTime(650);
  expect(change.mock.calls.at(-1)![0].strokes).toHaveLength(1);
});
