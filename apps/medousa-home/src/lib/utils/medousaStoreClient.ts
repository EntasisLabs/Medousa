/** Marker id for host-injected component scope bootstrap. */
export const MEDOUSA_STORE_BOOTSTRAP_SCRIPT_ID = "medousa-store-bootstrap-script";

/** Marker id for the injected MedousaStore client script. */
export const MEDOUSA_STORE_CLIENT_SCRIPT_ID = "medousa-store-client-script";

/** postMessage: iframe → parent */
export type MedousaStoreRequest =
  | MedousaStoreGetRequest
  | MedousaStoreSetRequest
  | MedousaStoreDeleteRequest
  | MedousaStoreListRequest;

export type MedousaStoreGetRequest = {
  type: "medousa:store:get";
  requestId: string;
  key?: string;
};

export type MedousaStoreSetRequest = {
  type: "medousa:store:set";
  requestId: string;
  key: string;
  value: unknown;
};

export type MedousaStoreDeleteRequest = {
  type: "medousa:store:delete";
  requestId: string;
  key: string;
};

export type MedousaStoreListRequest = {
  type: "medousa:store:list";
  requestId: string;
};

/** postMessage: parent → iframe */
export type MedousaStoreResponse = {
  type: "medousa:store:response";
  requestId: string;
  ok: boolean;
  value?: unknown;
  entries?: Record<string, unknown>;
  keys?: string[];
  error?: string;
};

export function isMedousaStoreRequest(data: unknown): data is MedousaStoreRequest {
  if (!data || typeof data !== "object") return false;
  const msg = data as Record<string, unknown>;
  if (typeof msg.requestId !== "string") return false;
  return (
    msg.type === "medousa:store:get" ||
    msg.type === "medousa:store:set" ||
    msg.type === "medousa:store:delete" ||
    msg.type === "medousa:store:list"
  );
}

const STORE_KEY_PATTERN = /^[A-Za-z0-9._-]{1,128}$/;

export function isValidStoreKey(key: string): boolean {
  return STORE_KEY_PATTERN.test(key.trim());
}

/**
 * Host-backed key/value store for sandboxed presentation artifacts.
 * Requires `window.__MEDOUSA_STORE__.componentId` injected by the host.
 */
export function buildMedousaStoreClientScript(): string {
  const source = `(function(){
if(window.__MEDOUSA_STORE_CLIENT__)return;
window.__MEDOUSA_STORE_CLIENT__=true;
var pending=Object.create(null);
var reqSeq=0;
function componentId(){
  var root=window.__MEDOUSA_STORE__;
  return root&&typeof root.componentId==="string"?root.componentId.trim():"";
}
function post(type,payload){
  return new Promise(function(resolve,reject){
    var id=componentId();
    if(!id){
      if(type==="medousa:store:get"){
        resolve(keyInPayload(payload)?{value:null}:{entries:{}});
        return;
      }
      if(type==="medousa:store:list"){
        resolve({keys:[]});
        return;
      }
      reject(new Error("MedousaStore unavailable (no componentId)"));
      return;
    }
    var requestId="ms-"+(++reqSeq);
    pending[requestId]={resolve:resolve,reject:reject};
    window.parent.postMessage(Object.assign({type:type,requestId:requestId},payload||{}),"*");
    setTimeout(function(){
      if(!pending[requestId])return;
      delete pending[requestId];
      reject(new Error("MedousaStore request timeout"));
    },15000);
  });
  function keyInPayload(payload){
    return payload&&typeof payload.key==="string"&&payload.key;
  }
}
window.addEventListener("message",function(event){
  var data=event.data;
  if(!data||typeof data!=="object"||data.type!=="medousa:store:response")return;
  var p=pending[data.requestId];
  if(!p)return;
  delete pending[data.requestId];
  if(data.ok)p.resolve(data);
  else p.reject(new Error(data.error||"MedousaStore request failed"));
});
window.MedousaStore={
  ready:function(){return !!componentId();},
  get:function(key){
    return post("medousa:store:get",key?{key:key}:{}).then(function(res){
      if(key)return res.value===undefined?null:res.value;
      return res.entries||{};
    });
  },
  set:function(key,value){
    return post("medousa:store:set",{key:key,value:value}).then(function(){return true;});
  },
  delete:function(key){
    return post("medousa:store:delete",{key:key}).then(function(){return true;});
  },
  list:function(){
    return post("medousa:store:list",{}).then(function(res){return res.keys||[];});
  }
};
})();`;

  return `<script id="${MEDOUSA_STORE_CLIENT_SCRIPT_ID}">${source}</script>`;
}

export function buildMedousaStoreBootstrapScript(componentId: string): string {
  const payload = JSON.stringify({ componentId });
  return `<script id="${MEDOUSA_STORE_BOOTSTRAP_SCRIPT_ID}">window.__MEDOUSA_STORE__=${payload};</script>`;
}
