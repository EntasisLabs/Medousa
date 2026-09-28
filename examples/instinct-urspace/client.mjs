import { readFile, writeFile, mkdir, rename, lstat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { homedir } from "node:os";
import { fileURLToPath, pathToFileURL } from "node:url";
import { randomUUID } from "node:crypto";

let loaded;
export function loadClient() {
  // @urspace/client 0.5.0 ships bundler bindings, but its entrypoint imports
  // a web-target default initializer. Instantiate the pinned SDK's own WASM
  // and glue directly; no protocol or cryptography is reimplemented here.
  return loaded ??= (async () => {
    const root = dirname(dirname(fileURLToPath(import.meta.resolve("@urspace/client"))));
    const glue = await import(pathToFileURL(join(root, "wasm/urspace_browser_bg.js")));
    const bytes = await readFile(join(root, "wasm/urspace_browser_bg.wasm"));
    const { instance } = await WebAssembly.instantiate(bytes, { "./urspace_browser_bg.js": glue });
    glue.__wbg_set_wasm(instance.exports);
    instance.exports.__wbindgen_start();
    return glue.SiteClient;
  })();
}

export async function requestWorkshop(session, token, method, path, body = new Uint8Array()) {
  if (!token || /\s/.test(token)) throw new Error("A private API token is required");
  if (!path.startsWith("/v1/") || /[\r\n#]/.test(path)) throw new Error("Use a workshop /v1/ path");
  if (!["GET", "POST"].includes(method)) throw new Error("Supported methods are GET and POST");
  return session.fetch(method, path, [
    { name: "authorization", value: `Bearer ${token}` },
    { name: "content-type", value: "application/json" },
  ], body);
}

export async function readRequestBody(stream) {
  const chunks = [];
  let size = 0;
  for await (const chunk of stream) {
    const bytes = Buffer.from(chunk);
    size += bytes.length;
    if (size > 1024 * 1024) throw new Error("Request body exceeds 1 MiB");
    chunks.push(bytes);
  }
  return Buffer.concat(chunks);
}

async function readSession(path) {
  try {
    const info = await lstat(path);
    if (!info.isFile() || (info.mode & 0o077) !== 0) throw new Error("Session file must be private");
    return JSON.parse(await readFile(path, "utf8"));
  } catch (error) {
    if (error.code === "ENOENT") return null;
    throw error;
  }
}

async function saveSession(path, session, origin) {
  const secret = session.exportResumeKey();
  try {
    const state = JSON.stringify({ origin, grant: session.resumeCredential, key: Buffer.from(secret).toString("base64") });
    await mkdir(dirname(path), { recursive: true, mode: 0o700 });
    const temp = `${path}.${randomUUID()}.tmp`;
    await writeFile(temp, state, { flag: "wx", mode: 0o600 });
    await rename(temp, path);
  } finally {
    secret.fill(0);
  }
}

async function main() {
  const [method = "GET", path = "/v1/capabilities"] = process.argv.slice(2);
  const token = process.env.MEDOUSA_API_TOKEN;
  if (!token) throw new Error("Missing credential");
  const sessionPath = process.env.MEDOUSA_URSPACE_SESSION_FILE ?? join(homedir(), ".config/medousa-instinct/session.json");
  const invite = process.env.URSPACE_INVITE_URL;
  let session;
  const deadline = setTimeout(() => {
    session?.close();
    process.stderr.write("Urspace request timed out. Check job state before retrying work.\n", () => process.exit(1));
  }, 45_000);
  try {
    const SiteClient = await loadClient();
    const saved = await readSession(sessionPath);
    const now = Math.floor(Date.now() / 1000);
    const origin = invite ? new URL(invite).origin : saved?.origin;
    if (saved) {
      if (saved.origin !== origin) throw new Error("Session belongs to another host");
      const key = Buffer.from(saved.key, "base64");
      try { session = await SiteClient.resume(saved.grant, key, now, origin); }
      finally { key.fill(0); }
    } else {
      if (!invite) throw new Error("An invitation is required for the first request");
      session = await SiteClient.connect(invite, now);
    }
    await saveSession(sessionPath, session, origin);
    const body = method === "POST" ? await readRequestBody(process.stdin) : new Uint8Array();
    const response = await requestWorkshop(session, token, method, path, body);
    await saveSession(sessionPath, session, origin);
    await new Promise((resolve) => process.stdout.write(Buffer.from(response.body), resolve));
    return response.status >= 200 && response.status < 300 ? 0 : 1;
  } finally {
    clearTimeout(deadline);
    session?.close();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().then((code) => process.exit(code), () => {
    // SDK errors can contain invitation data. Never print raw exceptions.
    process.stderr.write("Request failed. Check private credentials, saved session, and connectivity. No retry was attempted.\n", () => process.exit(1));
  });
}
