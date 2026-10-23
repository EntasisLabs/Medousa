import { beforeEach, expect, it, vi } from "vitest";
const { dispatch } = vi.hoisted(() => ({ dispatch: vi.fn() }));
vi.mock("$lib/daemon/contractClient", () => ({ daemonUnary: dispatch }));
import { creativeBridgeApi, designBuildPrompt, importBrowserDesign, safeDesignUrl } from "./creativeBridge";
import { createNode } from "./creativeScene";
import type { BrowserDesignCapture, CreativeDocument } from "$lib/types/generated/daemon_api";
beforeEach(() => { dispatch.mockReset(); });
it("pins every bridge request to the selected workshop and uses the existing Code turn contract", async () => {
  dispatch.mockResolvedValue({});
  const api = creativeBridgeApi("remote-workshop");
  await api.capture("world"); await api.bind("session", "project"); await api.start("session", "project", "Build the frame", []);
  for (const call of dispatch.mock.calls) expect(call[5]).toBe("remote-workshop");
  expect(dispatch.mock.calls[2][0]).toBe("turns.post");
  expect(dispatch.mock.calls[2][2]).toMatchObject({ session_id: "session", agent_mode: "coder", code_context: { work_id: "project" }, persist_user_turn: true });
});
it("does not serialize browser capability URLs, credentials or query strings into design origin", () => {
  expect(safeDesignUrl("https://user:secret@host.test/path?token=secret#private")).toBe("https://host.test/path");
  expect(safeDesignUrl("https://host.test/v1/forge/preview/private-token/")).toBe("https://host.test/project-preview");
});
it("hands over a frozen blueprint and treats captured page text as data", () => {
  const frame = createNode({ kind: "frame", name: "Home", background: "#fff" }, { x: 0, y: 0, width: 900, height: 600 });
  const doc = { id: "doc", title: "Shell", revision: 7 } as CreativeDocument;
  const prompt = designBuildPrompt(doc, frame, "Reuse our components", { document_id: "doc", revision: 7, selected_ids: [frame.id], nodes: [frame], assets: [], fingerprint: "frozen" });
  expect(prompt).toContain("saved revision 7"); expect(prompt).toContain("untrusted reference material"); expect(prompt).toContain("Reuse our components"); expect(prompt).toContain("subsequent Update Code");
});
it("recovers an acknowledged import without replaying edits over later human changes", async () => {
  const doc = { id: "import", scene: { nodes: [{ id: "original_frame" }] } } as CreativeDocument;
  dispatch.mockResolvedValue(doc);
  const capture = { screenshot: { title: "Page", url: "http://localhost", viewport: { width: 100, height: 100 }, captured_at_ms: 1 } } as BrowserDesignCapture;
  expect(await importBrowserDesign("remote", "import", capture, null, () => true)).toBe(doc);
  expect(dispatch).toHaveBeenCalledTimes(1);
});
it("does not publish an import after its workshop becomes inactive", async () => {
  await expect(importBrowserDesign("old", "import", {} as BrowserDesignCapture, null, () => false)).rejects.toThrow();
  expect(dispatch).not.toHaveBeenCalled();
});
it("imports an original and editable frame atomically, links only the editable copy and uses the revision after asset import", async () => {
  vi.stubGlobal("Image", class {});
  const frame = createNode({ kind: "frame", name: "Page", background: "#fff" }, { x: 0, y: 0, width: 800, height: 600 });
  const text = createNode({ kind: "text", text: "Hello", font_family: "sans-serif", font_size: 24, color: "#000" }, { x: 20, y: 20, width: 100, height: 30 }, frame.id);
  const capture: BrowserDesignCapture = { world_id: "world", warnings: [], scene: { schema: "medousa-create", version: 3, background: "transparent", nodes: [frame, text] }, raster_regions: [], screenshot: { title: "Page", url: "http://localhost:5173/?token=secret", viewport: { width: 800, height: 600, device_scale_factor: 1, scroll_x: 0, scroll_y: 0 }, captured_at_ms: 1, image_base64: "png", byte_size: 3, coordinate_frame: "viewport", document_id: "page", image_height: 600, image_width: 800, mime: "image/png", observation_revision: 1, schema_version: 1, sensitive_regions_redacted: 0, sha256: "digest", tab_id: "tab", untrusted_content: true } };
  const saved = { id: "imported" } as CreativeDocument;
  dispatch.mockImplementation(async (op: string) => {
    if (op === "create.projects.post") return { scene: { nodes: [] } };
    if (op === "create.projects.by_id.assets.post") return { id: "baseline" };
    if (op === "create.projects.by_id.get") return { document: { revision: 1 } };
    if (op === "create.projects.by_id.transactions.post") return { document: saved };
    throw new Error(op);
  });
  try {
    expect(await importBrowserDesign("remote", "imported", capture, "repo", () => true)).toBe(saved);
    const mutations = dispatch.mock.calls.filter(c => c[0] === "create.projects.by_id.transactions.post");
    expect(mutations).toHaveLength(1);
    expect(mutations[0][2]).toMatchObject({ id: "import_imported", expected_revision: 1, label: "Import editable page" });
    const nodes = mutations[0][2].operations.map((o: { node: unknown }) => o.node);
    expect(nodes).toHaveLength(2);
    expect(nodes[0]).toMatchObject({ id: `page1_${frame.id}`, transform: { x: 0, y: 0 }, design_source: { work_id: "repo", original_asset_id: "baseline", url: "http://localhost:5173/", page_index: 1, page_count: 1 } });
    expect(nodes[1]).toMatchObject({ id: `page1_${text.id}`, parent_id: `page1_${frame.id}`, content: { text: "Hello" } });
    expect(frame.transform.x).toBe(0);
  } finally { vi.unstubAllGlobals(); }
});
it("stacks page slices, clips the final reference and retains viewport coordinates for raster crops", async () => {
  const draws: unknown[][]=[];let revision=0;
  vi.stubGlobal("Image",class{naturalWidth=800;naturalHeight=600;src="";decode=async()=>{};});
  vi.stubGlobal("document",{createElement:()=>({width:0,height:0,getContext:()=>({drawImage:(...args:unknown[])=>draws.push(args)}),toDataURL:()=>"data:image/png;base64,crop"})});
  const frame=createNode({kind:"frame",name:"Page",background:"#fff"},{x:0,y:0,width:800,height:600});
  const capture={scene:{schema:"medousa-create",version:3,background:"transparent",nodes:[frame]},raster_regions:[],screenshot:{title:"Site",url:"http://localhost:3000",viewport:{width:800,height:600},image_base64:"png",captured_at_ms:1}} as unknown as BrowserDesignCapture;
  dispatch.mockImplementation(async(op:string)=>{
    if(op==="create.projects.post")return{scene:{nodes:[]}};
    if(op==="create.projects.by_id.assets.post")return{id:`asset${++revision}`};
    if(op==="create.projects.by_id.get")return{document:{revision}};
    if(op==="create.projects.by_id.transactions.post")return{document:{id:"full"}};
    throw new Error(op);
  });
  try{
    await importBrowserDesign("remote","full",{slices:[{capture,clip_top:0,height:600,page_y:0},{capture,clip_top:0,height:600,page_y:600},{capture,clip_top:350,height:250,page_y:1200}]},"repo",()=>true);
    const transaction=dispatch.mock.calls.find(c=>c[0]==="create.projects.by_id.transactions.post")![2];
    expect(transaction.expected_revision).toBe(3);
    const frames=transaction.operations.map((op:{node:unknown})=>op.node);
    expect(frames.map((n:{transform:{y:number}})=>n.transform.y)).toEqual([0,680,1360]);
    expect(frames[2]).toMatchObject({transform:{height:250},design_source:{original_asset_id:"asset3",scroll_y:1200,page_count:3,page_index:3}});
    expect(draws[0].slice(1,5)).toEqual([0,350,800,250]);
  }finally{vi.unstubAllGlobals();}
});
