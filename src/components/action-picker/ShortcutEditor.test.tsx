import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import i18n from "../../i18n";
import { ShortcutEditor } from "./ShortcutEditor";

afterEach(cleanup);
const draft = { key: "1", ctrl: true, alt: false, shift: false, win: false };
function setup(props: Partial<Parameters<typeof ShortcutEditor>[0]> = {}) {
  const onChange = vi.fn();
  const setIsCapturing = vi.fn();
  render(<ShortcutEditor draft={draft} onChange={onChange} isCapturing={false}
    setIsCapturing={setIsCapturing} {...props} />);
  return { onChange, setIsCapturing };
}

describe("key assignment browser", () => {
  it("searches and picks a numpad key without recording, keeping modifiers", () => {
    const state = setup();
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "num1" } });
    const option = screen.getByRole("option", { name: /Num 1/ });
    expect(screen.getAllByRole("option")).toHaveLength(1);
    fireEvent.click(option);
    expect(state.onChange).toHaveBeenCalledWith({ ...draft, key: "Numpad1" });
    expect(state.setIsCapturing).toHaveBeenCalledWith(false);
  });
  it("supports keyboard selection and empty results", () => {
    const state = setup();
    const search = screen.getByRole("combobox");
    fireEvent.change(search, { target: { value: "num" } });
    fireEvent.keyDown(search, { key: "ArrowDown" });
    fireEvent.keyDown(search, { key: "Enter" });
    expect(state.onChange).toHaveBeenCalledWith({ ...draft, key: "Numpad1" });
    fireEvent.change(search, { target: { value: "nonexistent key" } });
    expect(screen.queryAllByRole("option")).toHaveLength(0);
    expect(screen.getByText(i18n.t("picker.noResults"))).toBeTruthy();
  });
  it("accepts the modal's key search and clears a key for modifier-only bindings", () => {
    const state = setup({ searchQuery: "num1" });
    expect(screen.getAllByRole("option")).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: i18n.t("picker.clearKey") }));
    expect(state.onChange).toHaveBeenCalledWith({ ...draft, key: "" });
  });
  it("records numpad identity while continuing to ignore the injected probe", () => {
    const state = setup({ isCapturing: true });
    const input = screen.getByDisplayValue("1");
    fireEvent.keyDown(input, { key: "Unidentified", keyCode: 232 });
    expect(state.onChange).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: "1", code: "Numpad1", ctrlKey: false });
    expect(state.onChange).toHaveBeenCalledWith({ ...draft, ctrl: false, key: "Numpad1" });
  });
});
