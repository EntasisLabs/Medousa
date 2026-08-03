import { describe, expect, it } from "vitest";
import { anchoredCompanionPosition } from "./windowGeometry";

const targetSize = { width: 390, height: 580 };

describe("anchoredCompanionPosition", () => {
  it("expands up and left around the pet's bottom-right corner", () => {
    expect(
      anchoredCompanionPosition({
        position: { x: 1200, y: 600 },
        previousSize: { width: 112, height: 170 },
        targetSize,
      }),
    ).toEqual({ x: 922, y: 190 });
  });

  it("keeps expansion inside the usable screen near every edge", () => {
    const workArea = {
      position: { x: 0, y: 24 },
      size: { width: 1920, height: 1016 },
    };
    expect(
      anchoredCompanionPosition({
        position: { x: 8, y: 28 },
        previousSize: { width: 112, height: 170 },
        targetSize,
        workArea,
      }),
    ).toEqual({ x: 0, y: 24 });
    expect(
      anchoredCompanionPosition({
        position: { x: 1808, y: 870 },
        previousSize: { width: 112, height: 170 },
        targetSize,
        workArea,
      }),
    ).toEqual({ x: 1530, y: 460 });
  });

  it("supports monitors with negative desktop coordinates", () => {
    expect(
      anchoredCompanionPosition({
        position: { x: -1912, y: 10 },
        previousSize: { width: 112, height: 170 },
        targetSize,
        workArea: {
          position: { x: -1920, y: 0 },
          size: { width: 1920, height: 1080 },
        },
      }),
    ).toEqual({ x: -1920, y: 0 });
  });
});
