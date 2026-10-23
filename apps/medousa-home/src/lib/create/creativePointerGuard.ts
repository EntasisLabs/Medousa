/** Extend Notes' palm protection to Create's separate shape/transform gestures. */
export function creativePointerGuard() {
  const pens = new Set<number>(); let lastPen = -Infinity;
  return {
    begin(event: Pick<PointerEvent,"pointerType"|"pointerId"|"timeStamp">) {
      if (event.pointerType === "pen") { pens.add(event.pointerId); lastPen=event.timeStamp; }
      return event.pointerType !== "touch" || !pens.size && event.timeStamp-lastPen>=650;
    },
    finish(event: Pick<PointerEvent,"pointerType"|"pointerId"|"timeStamp">) { if(event.pointerType==="pen"){pens.delete(event.pointerId);lastPen=event.timeStamp;} },
    clear(){pens.clear();lastPen=-Infinity;},
  };
}
/** A toolbar toggle supplies Shift semantics without changing Notes' interaction core. */
export function selectionPointerEvent(event: PointerEvent, additive: boolean): PointerEvent {
  return additive ? new Proxy(event,{get(target,key){if(key==="shiftKey")return true;const value=Reflect.get(target,key,target);return typeof value==="function"?value.bind(target):value;}}) : event;
}
/** Overlapping touch targets on small objects resolve to the nearest visible control. */
export function selectionControl(event: PointerEvent, svg: SVGSVGElement): SVGElement | null {
  const selector="[data-resize], [data-endpoint], [data-rotate], [data-radius]";
  const target=(event.target as Element).closest<SVGElement>(selector); if(!target)return null;
  let nearest=target,distance=Infinity;
  for(const control of svg.querySelectorAll<SVGElement>(selector)){
    const b=control.getBoundingClientRect();
    if(!b.width||!b.height||event.clientX<b.left||event.clientX>b.right||event.clientY<b.top||event.clientY>b.bottom)continue;
    const d=Math.hypot(event.clientX-(b.left+b.right)/2,event.clientY-(b.top+b.bottom)/2);
    if(d<distance){nearest=control;distance=d;}
  }
  return nearest;
}
