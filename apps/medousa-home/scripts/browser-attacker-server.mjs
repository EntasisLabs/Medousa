import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const tauriRoot = join(root, "src-tauri");
const inventory = JSON.parse(
  await readFile(join(tauriRoot, "security", "browser-authority-inventory.json"), "utf8"),
);
let acl;
try {
  acl = JSON.parse(await readFile(join(tauriRoot, "gen", "schemas", "acl-manifests.json"), "utf8"));
} catch (error) {
  if (error?.code === "ENOENT") {
    throw new Error("generated Tauri ACL is missing; run cargo check in src-tauri first");
  }
  throw error;
}

const attempts = inventory.applicationCommands.map((qualified) => ({
  class: "application",
  command: qualified.split("::").at(-1),
}));
for (const [plugin, manifest] of Object.entries(acl)) {
  if (plugin === "browser-bridge" || plugin === "core") continue;
  const invokePlugin = plugin.startsWith("core:") ? plugin.slice("core:".length) : plugin;
  const commands = new Set();
  for (const permission of Object.values(manifest.permissions ?? {})) {
    for (const command of permission.commands?.allow ?? []) commands.add(command);
  }
  for (const command of commands) {
    attempts.push({ class: plugin, command: `plugin:${invokePlugin}|${command}` });
  }
}

const html = `<!doctype html>
<meta charset="utf-8">
<title>Medousa browser authority attacker</title>
<style>body{font:14px system-ui;margin:2rem;max-width:72rem}pre{white-space:pre-wrap} .ok{color:#087830}.bad{color:#b42318}</style>
<h1>Medousa browser authority attacker</h1>
<p id="status">Running ${attempts.length} forbidden command attempts…</p>
<pre id="results"></pre>
<script>
const attempts=${JSON.stringify(attempts)};
const denied=(error)=>/not allowed|denied|acl/i.test(String(error));
const invoke=(command,args)=>{const api=window.__TAURI_INTERNALS__||window.__TAURI__;if(!api?.invoke)throw new Error("Tauri invoke unavailable");return api.invoke(command,args)};
async function probe(entry){try{await invoke(entry.command,{});return {...entry,ok:false,result:"resolved"}}catch(error){return {...entry,ok:denied(error),result:denied(error)?"denied":"reached-handler"}}}
async function run(){
  const results=[];
  for(let offset=0;offset<attempts.length;offset+=16)results.push(...await Promise.all(attempts.slice(offset,offset+16).map(probe)));
  let bridge;
  try{await invoke("plugin:browser-bridge|report",{report:{version:1,kind:"attacker"}});bridge={ok:false,result:"invalid report resolved"}}
  catch(error){bridge={ok:!/not allowed|denied|acl/i.test(String(error)),result:"report bridge admitted, closed schema rejected"}}
  const failures=results.filter((result)=>!result.ok);
  const summary={revision:${JSON.stringify(inventory.lockedRuntime)},attempted:results.length,denied:results.length-failures.length,failures,bridge};
  document.getElementById("status").className=failures.length||!bridge.ok?"bad":"ok";
  document.getElementById("status").textContent=failures.length||!bridge.ok?"FAIL: authority escaped":"PASS: forbidden authority denied; report bridge remained schema-closed";
  document.getElementById("results").textContent=JSON.stringify(summary,null,2);
}
run().catch((error)=>{document.getElementById("status").className="bad";document.getElementById("status").textContent="Harness error";document.getElementById("results").textContent=String(error)});
</script>`;

const server = createServer((request, response) => {
  response.setHeader("Cache-Control", "no-store");
  response.setHeader("Content-Security-Policy", "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'");
  response.setHeader("Content-Type", "text/html; charset=utf-8");
  response.end(html);
});
server.listen(0, "127.0.0.1", () => {
  const address = server.address();
  console.log(`Open http://127.0.0.1:${address.port}/ in the packaged Medousa Web surface.`);
  console.log("Keep this process running until the page reports PASS or FAIL; press Ctrl+C to stop.");
});
