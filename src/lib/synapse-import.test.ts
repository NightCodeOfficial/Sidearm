import { describe, it, expect } from "vitest";
import { describeParsedAction, type ParsedMacro } from "./synapse-import";

const macros: ParsedMacro[] = [
  { synapseGuid: "m1", name: "Copy", steps: [{ kind: "send", value: "Ctrl+C" }, { kind: "sleep", delayMs: 50 }] },
];

describe("describeParsedAction", () => {
  it("summarises each imported action kind", () => {
    expect(describeParsedAction({ kind: "shortcut", key: "C", ctrl: true, shift: true }, macros)).toBe(
      "Шорткат: Ctrl + Shift + C",
    );
    expect(describeParsedAction({ kind: "sequence", macroGuid: "m1" }, macros)).toBe(
      "Последовательность из 2 шагов",
    );
    expect(describeParsedAction({ kind: "sequence", macroGuid: "missing" }, macros)).toBe(
      "Последовательность из 0 шагов",
    );
    expect(describeParsedAction({ kind: "unmappable", reason: "x" }, macros)).not.toContain("unmappable");
  });
});
