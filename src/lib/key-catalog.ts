import type { TFunction } from "i18next";

type KeyGroup = "letters" | "numbers" | "function" | "navigation" | "symbols" | "numpad";
interface KeyChoice {
  value: string;
  group: KeyGroup;
  label?: string;
  aliases?: string;
}

/** Canonical values are understood by hotkeys::parse_primary_key in Rust. */
const KEYS: readonly KeyChoice[] = [
  ...Array.from("ABCDEFGHIJKLMNOPQRSTUVWXYZ", (value): KeyChoice => ({ value, group: "letters" })),
  ...Array.from("0123456789", (value): KeyChoice => ({ value, group: "numbers" })),
  ...Array.from({ length: 24 }, (_, i): KeyChoice => ({ value: `F${i + 1}`, group: "function" })),
  ...["Enter", "Tab", "Space", "Backspace", "Delete", "Insert", "Esc", "Home", "End", "PageUp", "PageDown", "Left", "Right", "Up", "Down", "CapsLock", "PrintScreen", "ScrollLock", "Pause", "Apps"].map((value): KeyChoice => ({ value, group: "navigation", label: `picker.keys.${value}` })),
  ...[
    ["-", "minus hyphen"], ["=", "equals plus"], ["[", "left bracket"], ["]", "right bracket"],
    ["\\", "backslash"], [";", "semicolon"], ["'", "apostrophe quote"], [",", "comma"],
    [".", "period dot"], ["/", "slash"], ["`", "grave backtick"],
  ].map(([value, aliases]): KeyChoice => ({ value, aliases, group: "symbols" })),
  ...Array.from({ length: 10 }, (_, i): KeyChoice => ({ value: `Numpad${i}`, group: "numpad", label: "picker.numKey", aliases: `num${i} numpad${i}` })),
  ...["Multiply", "Add", "Subtract", "Decimal", "Divide", "Enter"].map((name): KeyChoice => ({ value: `Numpad${name}`, group: "numpad", label: `picker.keys.Numpad${name}` })),
  { value: "NumLock", group: "numpad", label: "picker.keys.NumLock" },
];

export function keyChoiceLabel(choice: KeyChoice, t: TFunction): string {
  return t(choice.label ?? "picker.keyName", { key: choice.value.startsWith("Numpad") ? choice.value.slice(6) : choice.value });
}

export function searchKeys(query: string, t: TFunction): KeyChoice[] {
  const normalize = (text: string) => text.toLocaleLowerCase().replace(/\s+/g, "");
  const needle = normalize(query.trim());
  return KEYS.filter((choice) => !needle || [choice.value, choice.aliases ?? "", keyChoiceLabel(choice, t)]
    .some((text) => normalize(text).includes(needle))
    || normalize(t(`picker.keyGroups.${choice.group}`)) === needle);
}
