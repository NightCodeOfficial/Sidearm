import { describe, expect, it } from "vitest";

import type { EncoderMapping } from "./config";
import { onboardBindings } from "./onboard-mapping";
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
