import { expect,it } from "vitest";
import {creativePointerGuard,selectionPointerEvent,selectionControl} from "./creativePointerGuard";
it("rejects resting touches throughout a Create transform and after the pen is lifted",()=>{
  const guard=creativePointerGuard(),event=(pointerType:string,pointerId:number,timeStamp:number)=>({pointerType,pointerId,timeStamp});
  expect(guard.begin(event("pen",1,100))).toBe(true);expect(guard.begin(event("touch",2,1000))).toBe(false);
  guard.finish(event("pen",1,1200));expect(guard.begin(event("touch",2,1500))).toBe(false);expect(guard.begin(event("touch",2,1900))).toBe(true);
  guard.clear();expect(guard.begin(event("touch",2,0))).toBe(true);
});
it("adds selection semantics without mutating the native event or losing its methods",()=>{
  let prevented=false;const event={shiftKey:false,pointerId:4,preventDefault(){prevented=true;}} as unknown as PointerEvent;
  const additive=selectionPointerEvent(event,true);expect(additive.shiftKey).toBe(true);expect(event.shiftKey).toBe(false);additive.preventDefault();expect(prevented).toBe(true);expect(selectionPointerEvent(event,false)).toBe(event);
});

it("chooses the closest endpoint when invisible touch targets overlap",()=>{
  const control=(x:number)=>({getBoundingClientRect:()=>({left:x-22,right:x+22,top:-22,bottom:22,width:44,height:44})});
  const start=control(0),end=control(20);
  const event={target:{closest:()=>end},clientX:1,clientY:0} as unknown as PointerEvent;
  const svg={querySelectorAll:()=>[start,end]} as unknown as SVGSVGElement;
  expect(selectionControl(event,svg)).toBe(start);
});
