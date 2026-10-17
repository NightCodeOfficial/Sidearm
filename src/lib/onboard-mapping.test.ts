import { describe, expect, it } from "vitest";

import type { EncoderMapping } from "./config";
import { isBuiltinControl, onboardBindings } from "./onboard-mapping";
import { makeConfig } from "./test-fixtures";

function mapping(
  controlId: string,
  layer: "standard" | "hypershift",
  encodedKey: string,
): EncoderMapping {
  return {
    controlId,
    layer,
    encodedKey,
    source: "detected",
    verified: false,
  };
}

describe("onboardBindings", () => {
  it("keeps only standard-layer thumb and top-button controls", () => {
    const config = makeConfig({
      encoderMappings: [
        mapping("thumb_01", "standard", "F13"),
        mapping("thumb_12", "standard", "F24"),
        mapping("top_aux_01", "standard", "Alt+F23"),
        mapping("top_aux_01", "hypershift", "Ctrl+Alt+F23"),
        mapping("thumb_01", "hypershift", "Ctrl+Alt+Shift+F13"),
        mapping("mouse_4", "standard", "Alt+F13"),
        mapping("wheel_up", "standard", "ScrollUp"),
      ],
    });

    expect(onboardBindings(config)).toEqual([
      { controlId: "thumb_01", encodedKey: "F13" },
      { controlId: "thumb_12", encodedKey: "F24" },
      { controlId: "top_aux_01", encodedKey: "Alt+F23" },
    ]);
  });

  it("ignores a config with no thumb bindings", () => {
    expect(onboardBindings(makeConfig({ encoderMappings: [] }))).toEqual([]);
  });
});

describe("isBuiltinControl", () => {
  it("is true only for controls of the built-in device", () => {
    const config = makeConfig({
      devices: [
        { id: "naga", name: "Naga", builtin: true },
        { id: "pad", name: "Pad" },
      ],
      physicalControls: [
        {
          id: "thumb_01",
          deviceId: "naga",
          family: "thumbGrid",
          defaultName: "Thumb 1",
          remappable: true,
          capabilityStatus: "verified",
        },
        {
          id: "pad-b1",
          deviceId: "pad",
          family: "thumbGrid",
          defaultName: "B1",
          remappable: true,
          capabilityStatus: "verified",
        },
      ],
    });
    expect(isBuiltinControl(config, "thumb_01")).toBe(true);
    expect(isBuiltinControl(config, "pad-b1")).toBe(false);
    expect(isBuiltinControl(config, "missing")).toBe(false);
  });
});
