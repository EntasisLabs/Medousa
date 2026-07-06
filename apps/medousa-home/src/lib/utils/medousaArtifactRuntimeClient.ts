/** Marker id for the injected artifact runtime bridge script. */
export const MEDOUSA_ARTIFACT_RUNTIME_SCRIPT_ID = "medousa-artifact-runtime-script";

/** postMessage: iframe → parent */
export type MedousaArtifactRuntimeEvent = {
  type: "medousa:artifact:runtime";
  level: string;
  message: string;
  stack?: string;
  source?: string;
  ts: number;
};

/** postMessage: parent → iframe */
export type MedousaArtifactProbeRequest = {
  type: "medousa:artifact:probe";
  probeId: string;
};

/** postMessage: iframe → parent */
export type MedousaArtifactProbeResult = {
  type: "medousa:artifact:probe:result";
  probeId: string;
  storeReady: boolean;
  storeRoundTripOk: boolean;
  errors: string[];
};

export function isMedousaArtifactRuntimeEvent(
  data: unknown,
): data is MedousaArtifactRuntimeEvent {
  if (!data || typeof data !== "object") return false;
  const msg = data as Record<string, unknown>;
  return (
    msg.type === "medousa:artifact:runtime" &&
    typeof msg.level === "string" &&
    typeof msg.message === "string" &&
    typeof msg.ts === "number"
  );
}

export function isMedousaArtifactProbeResult(
  data: unknown,
): data is MedousaArtifactProbeResult {
  if (!data || typeof data !== "object") return false;
  const msg = data as Record<string, unknown>;
  return (
    msg.type === "medousa:artifact:probe:result" &&
    typeof msg.probeId === "string" &&
    typeof msg.storeReady === "boolean" &&
    typeof msg.storeRoundTripOk === "boolean" &&
    Array.isArray(msg.errors)
  );
}

/**
 * Forwards console.error/warn, window.onerror, and unhandledrejection to the host.
 * Handles medousa:artifact:probe self-tests for the runtime doctor.
 */
export function buildMedousaArtifactRuntimeClientScript(): string {
  const source = `(function(){
if(window.__MEDOUSA_ARTIFACT_RUNTIME__)return;
window.__MEDOUSA_ARTIFACT_RUNTIME__=true;
function emit(level,message,stack,source){
  try{
    parent.postMessage({type:"medousa:artifact:runtime",level:level,message:String(message),stack:stack||undefined,source:source||undefined,ts:Date.now()},"*");
  }catch(e){}
}
var origError=console.error;
console.error=function(){
  var args=Array.prototype.slice.call(arguments);
  emit("error",args.map(function(a){try{return typeof a==="string"?a:JSON.stringify(a);}catch(e){return String(a);}}).join(" "),undefined,"console.error");
  return origError.apply(console,arguments);
};
var origWarn=console.warn;
console.warn=function(){
  var args=Array.prototype.slice.call(arguments);
  emit("warn",args.map(function(a){try{return typeof a==="string"?a:JSON.stringify(a);}catch(e){return String(a);}}).join(" "),undefined,"console.warn");
  return origWarn.apply(console,arguments);
};
window.addEventListener("error",function(ev){
  emit("error",ev.message||"script error",ev.error&&ev.error.stack?String(ev.error.stack):undefined,ev.filename?ev.filename+":"+ev.lineno:"window.onerror");
});
window.addEventListener("unhandledrejection",function(ev){
  var reason=ev.reason;
  var msg=reason&&reason.message?reason.message:String(reason);
  var stack=reason&&reason.stack?String(reason.stack):undefined;
  emit("error",msg,stack,"unhandledrejection");
});
window.addEventListener("message",function(ev){
  if(!ev.data||ev.data.type!=="medousa:artifact:probe")return;
  var probeId=ev.data.probeId;
  var errors=[];
  var storeReady=false;
  function finish(roundTripOk){
    parent.postMessage({type:"medousa:artifact:probe:result",probeId:probeId,storeReady:storeReady,storeRoundTripOk:!!roundTripOk,errors:errors},"*");
  }
  try{
    storeReady=!!(window.MedousaStore&&window.MedousaStore.ready&&window.MedousaStore.ready());
    if(!storeReady){
      errors.push("MedousaStore not ready");
      finish(false);
      return;
    }
    var key="__medousa_probe__";
    window.MedousaStore.set(key,"ok").then(function(){
      return window.MedousaStore.get(key);
    }).then(function(v){
      var ok=v==="ok";
      if(!ok)errors.push("round-trip value mismatch");
      return window.MedousaStore.delete(key).then(function(){return ok;});
    }).then(finish).catch(function(e){
      errors.push(String(e));
      finish(false);
    });
  }catch(e){
    errors.push(String(e));
    finish(false);
  }
});
})();`;

  return `<script id="${MEDOUSA_ARTIFACT_RUNTIME_SCRIPT_ID}">${source}</script>`;
}
