import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { searchKeys } from "./key-catalog";
import { filterActionCategories } from "./action-picker-helpers";

describe("searchable key catalog", () => {
  it("finds numpad aliases without conflating them with the number row", () => {
    for (const query of ["num1", "Num 1", "numpad1", " NUMPAD 1 "]) {
      expect(searchKeys(query, i18n.t).map((key) => key.value)).toEqual(["Numpad1"]);
    }
    expect(searchKeys("num", i18n.t)).toHaveLength(17);
    expect(filterActionCategories("num1", i18n.t).map((category) => category.id)).toEqual(["shortcut"]);
  });
  it("covers named keys, F1–F24, punctuation, and localized search", () => {
    const keys = searchKeys("", i18n.t).map((key) => key.value);
    expect(new Set(keys).size).toBe(keys.length);
    expect(keys).toEqual(expect.arrayContaining(["1", "Numpad1", "NumpadEnter", "NumpadDivide", "F24", "Enter", ";", "\\"]));
    expect(searchKeys(i18n.t("picker.keys.Space"), i18n.t).map((key) => key.value)).toContain("Space");
    expect(searchKeys("nonexistent key", i18n.t)).toEqual([]);
  });
});
