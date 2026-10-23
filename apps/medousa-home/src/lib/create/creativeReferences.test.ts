import {expect,it} from "vitest";
import {editableImportScene,originalReferences} from "./creativeReferences";
import type {CreativeDocument} from "$lib/types/generated/daemon_api";
it("presents legacy originals as references without rewriting their saved scene",()=>{
  const document={scene:{nodes:[{id:"original_frame",content:{kind:"frame"}},{id:"original_image",parent_id:"original_frame",content:{kind:"image",asset_id:"old"}},{id:"editable",content:{kind:"frame"},design_source:{url:"http://localhost"}}]}} as CreativeDocument;
  expect(editableImportScene(document.scene).nodes.map(n=>n.id)).toEqual(["editable"]);expect(document.scene.nodes).toHaveLength(3);
  expect(originalReferences(document)[0].asset).toBe("old");
});
