import { useEffect, useId, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import type { ShortcutActionPayload } from "../../lib/config";
import { normalizeKeyName, resolveKeyName } from "../../lib/action-picker-helpers";
import { HelpTip } from "../shared";
import { CaptureRow } from "./shared/CaptureRow";
import { ModifierRow } from "./shared/ModifierRow";
import { keyChoiceLabel, searchKeys } from "../../lib/key-catalog";
import { useListKeyboard } from "../../hooks/useListKeyboard";

export function ShortcutEditor({
  draft,
  onChange,
  isCapturing,
  setIsCapturing,
  searchQuery = "",
}: {
  draft: ShortcutActionPayload;
  onChange: (draft: ShortcutActionPayload) => void;
  isCapturing: boolean;
  setIsCapturing: (capturing: boolean) => void;
  searchQuery?: string;
}) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const listId = useId();
  const listRef = useRef<HTMLDivElement>(null);
  const externalMatches = searchQuery.trim() ? searchKeys(searchQuery, t) : [];
  const choices = searchKeys(query || (externalMatches.length ? searchQuery : ""), t);
  const index = Math.min(activeIndex, choices.length - 1);

  function selectKey(key: string) {
    onChange({ key, ctrl: draft.ctrl, shift: draft.shift, alt: draft.alt, win: draft.win });
    setIsCapturing(false);
  }

  const handleListKey = useListKeyboard({
    itemCount: choices.length, activeIndex: index, setActiveIndex,
    onSelect: (i) => selectKey(choices[i].value),
  });
  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>("[data-active='true']")?.scrollIntoView?.({ block: "nearest" });
  }, [index, query, searchQuery]);

  function handleKeyCapture(event: ReactKeyboardEvent) {
    if (!isCapturing) return;
    event.preventDefault();
    event.stopPropagation();

    const key = resolveKeyName(event);
    if (["Control", "Shift", "Alt", "Meta"].includes(key)) return;
    // Ignore Sidearm's own injected "menu mask" / hook-probe key (VK 0xE8 = 232).
    // While our window is focused, SendInput delivers it to the WebView as a real
    // keydown (the hook-health probe fires ~every 5s), so without this guard it
    // gets captured as "VK_232" even though the user pressed nothing.
    if (key === "VK_232") return;

    onChange({
      key: normalizeKeyName(key),
      ctrl: event.ctrlKey,
      shift: event.shiftKey,
      alt: event.altKey,
      win: event.metaKey,
    });
    setIsCapturing(false);
  }

  return (
    <div className="editor-grid" onKeyDown={handleKeyCapture}>
      <div className="key-browser">
        <input className="input" type="search" role="combobox" aria-label={t("picker.searchKeys")}
          placeholder={t("picker.searchKeys")} value={query} disabled={isCapturing}
          aria-autocomplete="list" aria-expanded="true" aria-controls={listId}
          aria-activedescendant={index >= 0 ? `${listId}-${index}` : undefined}
          onChange={(event) => { setQuery(event.target.value); setActiveIndex(0); }}
          onKeyDown={(event) => {
            if (["ArrowDown", "ArrowUp", "Home", "End", "Enter"].includes(event.key)) {
              event.stopPropagation();
              handleListKey(event);
            }
          }} />
        <div className="key-browser__list" id={listId} ref={listRef} role="listbox" aria-label={t("picker.availableKeys")}>
          {choices.map((choice, i) => (
            <button type="button" role="option" key={choice.value} id={`${listId}-${i}`} tabIndex={-1}
              aria-selected={draft.key === choice.value} data-active={i === index}
              className={`key-browser__key${draft.key === choice.value ? " key-browser__key--selected" : ""}`}
              onClick={() => selectKey(choice.value)}>
              <span>{keyChoiceLabel(choice, t)}</span>
              <span className="key-browser__group">{t(`picker.keyGroups.${choice.group}`)}</span>
            </button>
          ))}
          {choices.length === 0 ? <p className="panel__muted" role="status">{t("picker.noResults")}</p> : null}
        </div>
      </div>
      <label className="field">
        <span className="field__label">
          {t("picker.keyLabel")}
          <HelpTip text={t("picker.keyHelp")} />
        </span>
        <CaptureRow
          value={draft.key}
          placeholder={isCapturing ? t("picker.keyCapturing") : t("picker.keyEmpty")}
          capturing={isCapturing}
          onToggle={() => setIsCapturing(!isCapturing)}
          recordLabel={t("picker.recordKey")}
        />
      </label>
      {draft.key ? <button type="button" className="action-button action-button--small"
        onClick={() => selectKey("")}>{t("picker.clearKey")}</button> : null}
      <ModifierRow
        value={draft}
        onChange={(mods) => onChange({ ...draft, ...mods })}
      />
      <p className="field__description">
        {t("picker.modifiersHint")}
      </p>
    </div>
  );
}
