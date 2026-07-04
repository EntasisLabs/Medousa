/** Marker id for the injected artifact navigation bridge script. */
export const MEDOUSA_NAVIGATE_CLIENT_SCRIPT_ID = "medousa-navigate-client-script";

/** postMessage: iframe → parent */
export type MedousaNavigateRequest = {
  type: "medousa:navigate";
  target: "vault";
  path: string;
};

export function isMedousaNavigateRequest(data: unknown): data is MedousaNavigateRequest {
  if (!data || typeof data !== "object") return false;
  const msg = data as Record<string, unknown>;
  return msg.type === "medousa:navigate" && msg.target === "vault" && typeof msg.path === "string";
}

export function isSafeVaultNavigatePath(path: string): boolean {
  const trimmed = path.trim();
  if (!trimmed) return false;
  if (trimmed.includes("..")) return false;
  if (trimmed.startsWith("/") || trimmed.startsWith("\\")) return false;
  return true;
}

export function parseMedousaVaultHref(href: string): string | null {
  const trimmed = href.trim();
  const prefix = "medousa://vault/";
  if (!trimmed.toLowerCase().startsWith(prefix)) return null;
  const raw = trimmed.slice(prefix.length);
  if (raw.includes("..")) return null;
  try {
    const path = decodeURIComponent(raw);
    return isSafeVaultNavigatePath(path) ? path : null;
  } catch {
    return isSafeVaultNavigatePath(raw) ? raw : null;
  }
}

/**
 * Injects Medousa.navigate + medousa://vault/ link interception for artifact iframes.
 */
export function buildMedousaNavigateClientScript(): string {
  const source = `(function(){
if(window.__MEDOUSA_NAVIGATE__)return;
window.__MEDOUSA_NAVIGATE__=true;
function safePath(path){
  if(typeof path!=="string")return null;
  var trimmed=path.trim();
  if(!trimmed||trimmed.indexOf("..")!==-1)return null;
  if(trimmed.charAt(0)==="/"||trimmed.charAt(0)==="\\\\")return null;
  return trimmed;
}
function navigateVault(path){
  var safe=safePath(path);
  if(!safe)return false;
  try{
    parent.postMessage({type:"medousa:navigate",target:"vault",path:safe},"*");
    return true;
  }catch(e){
    return false;
  }
}
window.Medousa=window.Medousa||{};
window.Medousa.navigate=function(input){
  if(!input||typeof input!=="object")return false;
  if(input.type==="vault"&&input.path)return navigateVault(input.path);
  return false;
};
window.Medousa.openVaultNote=function(path){
  return navigateVault(path);
};
document.addEventListener("click",function(ev){
  var anchor=ev.target&&ev.target.closest?ev.target.closest("a[href]"):null;
  if(!anchor)return;
  var href=anchor.getAttribute("href")||"";
  if(href.toLowerCase().indexOf("medousa://vault/")!==0)return;
  var raw=href.slice("medousa://vault/".length);
  var path;
  try{path=decodeURIComponent(raw);}catch(e){path=raw;}
  if(navigateVault(path)){
    ev.preventDefault();
    ev.stopPropagation();
  }
},true);
})();`;

  return `<script id="${MEDOUSA_NAVIGATE_CLIENT_SCRIPT_ID}">${source}</script>`;
}
