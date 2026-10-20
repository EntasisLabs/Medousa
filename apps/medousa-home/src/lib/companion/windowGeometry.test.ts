import { describe, expect, it } from "vitest";
import {
  anchoredCompanionPosition,
  companionWorkAreaForWindow,
} from "./windowGeometry";

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

  it("round-trips to the original pet position after toolbelt expansion", () => {
    const petPosition = { x: 1440, y: 720 };
    const petSize = { width: 112, height: 170 };
    const toolbeltPosition = anchoredCompanionPosition({
      position: petPosition,
      previousSize: petSize,
      targetSize,
    });
    expect(
      anchoredCompanionPosition({
        position: toolbeltPosition,
        previousSize: targetSize,
        targetSize: petSize,
      }),
    ).toEqual(petPosition);
  });
});

describe("companionWorkAreaForWindow", () => {
  const left = {
    position: { x: -1920, y: 0 },
    size: { width: 1920, height: 1080 },
  };
  const right = {
    position: { x: 0, y: 24 },
    size: { width: 1920, height: 1056 },
  };

  it("uses the display containing most of a pet that straddles an edge", () => {
    expect(
      companionWorkAreaForWindow({
        position: { x: -40, y: 700 },
        size: { width: 112, height: 170 },
        workAreas: [left, right],
      }),
    ).toEqual(right);
  });

  it("falls back to the nearest display for a fully offscreen pet", () => {
    expect(
      companionWorkAreaForWindow({
        position: { x: 1980, y: 700 },
        size: { width: 112, height: 170 },
        workAreas: [left, right],
      }),
    ).toEqual(right);
  });
});

describe("floating bar input anchor", () => {
  const full = { width: 620, height: 422 };
  const slim = { width: 620, height: 82 };
  const workArea = { position: { x: 0, y: 24 }, size: { width: 1920, height: 1056 } };
  it("keeps the input at the same position when results and transcripts expand and contract", () => {
    const position = { x: 800, y: 250 };
    const expanded = anchoredCompanionPosition({ position, previousSize: slim, targetSize: full, workArea, anchor: "top-left", reserveSize: full });
    expect(expanded).toEqual(position);
    expect(anchoredCompanionPosition({ position: expanded, previousSize: full, targetSize: slim, workArea, anchor: "top-left", reserveSize: full })).toEqual(position);
  });
  it("reserves expansion room when the slim bar first opens near the screen edge", () => {
    const position = anchoredCompanionPosition({ position: { x: 1850, y: 950 }, previousSize: slim, targetSize: slim, workArea, anchor: "top-left", reserveSize: full });
    expect(position).toEqual({ x: 1300, y: 658 });
    expect(anchoredCompanionPosition({ position, previousSize: slim, targetSize: full, workArea, anchor: "top-left", reserveSize: full })).toEqual(position);
  });
  it("keeps the same anchor on a Retina display with negative desktop coordinates", () => {
    const position = { x: -2200, y: 300 };
    expect(anchoredCompanionPosition({ position, previousSize: { width: 1240, height: 164 }, targetSize: { width: 1240, height: 844 }, anchor: "top-left", workArea: { position: { x: -3840, y: 0 }, size: { width: 3840, height: 2160 } } })).toEqual(position);
  });
});
