import i18n from "../i18n";

/**
 * User-facing text from the backend (Rust `ui_text::UiText`): the English
 * `text`, and — when the backend gave one — an i18n `key` under `backend.` with
 * its `{{name}}` params. A plain string is accepted for older payloads.
 */
export type UiText = string | { text: string; key?: string; params?: Record<string, string> };

/** The text in the current language, falling back to the backend's English. */
export function uiText(value: UiText | undefined | null): string {
  if (value === undefined || value === null) return "";
  if (typeof value === "string") return value;
  if (!value.key) return value.text;
  return i18n.t(`backend.${value.key}`, { ...value.params, defaultValue: value.text });
}

/** The current language's `backend.*` translations, flat (`area.name` → text),
 *  for the surfaces Rust draws itself (tray, OSD). */
export function nativeTexts(): Record<string, string> {
  const bundle = i18n.getResourceBundle(i18n.resolvedLanguage ?? i18n.language, "translation") as { backend?: Record<string, Record<string, string>> } | undefined;
  const texts: Record<string, string> = {};
  for (const [area, entries] of Object.entries(bundle?.backend ?? {})) {
    for (const [name, text] of Object.entries(entries)) texts[`${area}.${name}`] = text;
  }
  return texts;
}

/** A runtime log category (Rust sends stable ids such as `capture`) in the current language. */
export function logCategory(category: string): string {
  return i18n.t(`log.category.${category}`, { defaultValue: category });
}
