/** @vitest-environment happy-dom */
import { expect, it } from "vitest";
import { mount, unmount } from "svelte";
import MedousaCompanion from "./MedousaCompanion.svelte";

it("does not let the mascot start a browser image/file drag", async () => {
  const component = mount(MedousaCompanion, { target: document.body, props: { markId: "mascot-starfish" } });
  try {
    const image = document.querySelector("img");
    expect(image).not.toBeNull();
    expect(image!.getAttribute("draggable")).toBe("false");
  } finally {
    await unmount(component);
  }
});
