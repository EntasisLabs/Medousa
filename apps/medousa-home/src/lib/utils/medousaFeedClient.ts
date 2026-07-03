/** Marker id for the injected script tag — used to avoid duplicate injection. */
export const MEDOUSA_FEED_CLIENT_SCRIPT_ID = "medousa-feed-client-script";

/** postMessage protocol: iframe → parent (tail pull on reconnect). */
export type MedousaFeedTailRequest = {
  type: "medousa:feed:tail";
  requestId: string;
  feedId: string;
  limit?: number;
};

/** postMessage protocol: parent → iframe (tail response). */
export type MedousaFeedTailResponse = {
  type: "medousa:feed:tail:response";
  requestId: string;
  feedId: string;
  ok: boolean;
  events?: unknown[];
  error?: string;
};

/** postMessage protocol: parent → iframe (live patch without srcdoc reload). */
export type MedousaFeedPatchMessage = {
  type: "medousa:feed:patch";
  feedState?: Record<string, unknown>;
  feedId?: string;
  patch?: unknown;
};

export function isMedousaFeedTailRequest(data: unknown): data is MedousaFeedTailRequest {
  if (!data || typeof data !== "object") return false;
  const msg = data as Record<string, unknown>;
  return (
    msg.type === "medousa:feed:tail" &&
    typeof msg.requestId === "string" &&
    typeof msg.feedId === "string"
  );
}

const FEED_ID_PATTERN = /^[a-z][a-z0-9._-]*$/;

export function isValidFeedId(feedId: string): boolean {
  const trimmed = feedId.trim();
  return trimmed.length > 0 && FEED_ID_PATTERN.test(trimmed);
}

/**
 * Lightweight read-only feed client for artifact iframes.
 * Registers `<medousa-feed feed="trip.london.trains">` and `window.MedousaFeed`.
 */
export function buildMedousaFeedClientScript(): string {
  const source = `(function(){
if(window.__MEDOUSA_FEED_CLIENT__)return;
window.__MEDOUSA_FEED_CLIENT__=true;
var handlers=Object.create(null);
var pending=Object.create(null);
var reqSeq=0;
function feedsRoot(){
  var root=window.__MEDOUSA_FEED__;
  if(!root)return Object.create(null);
  if(root.feeds&&typeof root.feeds==="object")return root.feeds;
  if(root.feedId&&root.lastPatch){
    var m=Object.create(null);
    m[root.feedId]=root.lastPatch;
    return m;
  }
  return Object.create(null);
}
function getPatch(feedId){
  var feeds=feedsRoot();
  return feeds[feedId]||null;
}
function emit(feedId,patch){
  var list=handlers[feedId];
  if(!list)return;
  for(var i=0;i<list.length;i++)list[i](patch,feedId);
}
function escapeHtml(value){
  return String(value)
    .replace(/&/g,"&amp;")
    .replace(/</g,"&lt;")
    .replace(/>/g,"&gt;")
    .replace(/"/g,"&quot;");
}
window.MedousaFeed={
  on:function(feedId,handler){
    if(!feedId||typeof handler!=="function")return function(){};
    (handlers[feedId]=handlers[feedId]||[]).push(handler);
    var existing=getPatch(feedId);
    if(existing)handler(existing,feedId);
    return function(){
      var list=handlers[feedId];
      if(!list)return;
      handlers[feedId]=list.filter(function(h){return h!==handler;});
    };
  },
  get:function(feedId){return getPatch(feedId);},
  fetchTail:function(feedId,limit){
    return new Promise(function(resolve,reject){
      var requestId="mf-"+(++reqSeq);
      pending[requestId]={resolve:resolve,reject:reject};
      window.parent.postMessage({
        type:"medousa:feed:tail",
        requestId:requestId,
        feedId:feedId,
        limit:limit||10
     },"*");
      setTimeout(function(){
        if(!pending[requestId])return;
        delete pending[requestId];
        reject(new Error("feed tail timeout"));
      },15000);
    });
  },
  syncFromHost:function(state){
    if(state)window.__MEDOUSA_FEED__=state;
    var feeds=feedsRoot();
    Object.keys(feeds).forEach(function(id){emit(id,feeds[id]);});
  }
};
window.addEventListener("message",function(event){
  var data=event.data;
  if(!data||typeof data!=="object")return;
  if(data.type==="medousa:feed:patch"){
    if(data.feedState)window.MedousaFeed.syncFromHost(data.feedState);
    else if(data.feedId&&data.patch){
      var root=window.__MEDOUSA_FEED__||(window.__MEDOUSA_FEED__={});
      root.feeds=root.feeds||{};
      root.feeds[data.feedId]=data.patch;
      emit(data.feedId,data.patch);
    }
  }else if(data.type==="medousa:feed:tail:response"){
    var p=pending[data.requestId];
    if(!p)return;
    delete pending[data.requestId];
    if(data.ok)p.resolve(data.events||[]);
    else p.reject(new Error(data.error||"feed tail failed"));
  }
});
class MedousaFeedElement extends HTMLElement{
  connectedCallback(){
    this._feedId=this.getAttribute("feed")||"";
    this._off=window.MedousaFeed.on(this._feedId,this._render.bind(this));
    this._render(window.MedousaFeed.get(this._feedId));
  }
  disconnectedCallback(){
    if(this._off)this._off();
  }
  _render(patch){
    if(!patch){
      this.textContent="Waiting for "+(this._feedId||"feed")+"…";
      return;
    }
    var phase=patch.phase||patch.status||"update";
    var excerpt=patch.excerpt||patch.summary||"";
    var checked=patch.checkedAt||patch.emittedAt||"";
    var statusCode=patch.statusCode!=null?String(patch.statusCode):"";
    this.innerHTML=
      '<div class="medousa-feed-card" part="card">'+
      '<div class="medousa-feed-phase" part="phase">'+escapeHtml(phase)+'</div>'+
      (statusCode?'<div class="medousa-feed-status" part="status">HTTP '+escapeHtml(statusCode)+'</div>':"")+
      (checked?'<div class="medousa-feed-time" part="time">'+escapeHtml(checked)+'</div>':"")+
      (excerpt?'<div class="medousa-feed-excerpt" part="excerpt">'+escapeHtml(excerpt)+'</div>':"")+
      '</div>';
  }
}
if(!customElements.get("medousa-feed")){
  customElements.define("medousa-feed",MedousaFeedElement);
}
window.MedousaFeed.syncFromHost(window.__MEDOUSA_FEED__);
})();`;

  return `<script id="${MEDOUSA_FEED_CLIENT_SCRIPT_ID}">${source}</script>`;
}
