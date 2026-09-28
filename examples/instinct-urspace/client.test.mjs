import test from "node:test";
import assert from "node:assert/strict";
import { Readable } from "node:stream";
import { loadClient, requestWorkshop, readRequestBody } from "./client.mjs";

test("POST input preserves streamed UTF-8 JSON and rejects oversized input", async () => {
  const json = Buffer.from('{"prompt":"hello 🌱"}');
  const body = await readRequestBody(Readable.from([json.subarray(0, 20), json.subarray(20)]));
  assert.deepEqual(body, json);
  await assert.rejects(readRequestBody(Readable.from([Buffer.alloc(1024 * 1024 + 1)])));
});

test("published SDK WASM initializes in Node and provides identity primitives", async () => {
  const Client = await loadClient();
  const key = Client.generateSessionKey();
  try {
    assert.equal(key.length, 32);
    assert.equal(Client.sessionPublicKey(key).length, 32);
    await assert.rejects(Client.connect("https://example.invalid/invalid", Math.floor(Date.now() / 1000)));
  } finally { key.fill(0); }
});

test("API calls preserve bearer and JSON body through the SDK, without HTTP fallback or retry", async () => {
  const body = new TextEncoder().encode('{"prompt":"test"}');
  let calls = 0;
  const session = { fetch: async (method, path, headers, sent) => {
    calls++;
    assert.equal(method, "POST");
    assert.equal(path, "/v1/jobs/ask");
    assert.deepEqual(sent, body);
    assert.deepEqual(headers[0], { name: "authorization", value: "Bearer synthetic-token" });
    throw new Error("uncertain transport");
  } };
  await assert.rejects(requestWorkshop(session, "synthetic-token", "POST", "/v1/jobs/ask", body));
  assert.equal(calls, 1);
  await assert.rejects(requestWorkshop(session, "synthetic-token", "GET", "https://example.invalid/"));
  assert.equal(calls, 1);
});
