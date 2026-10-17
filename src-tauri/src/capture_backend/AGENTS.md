# src-tauri/src/capture_backend — Keyboard + mouse capture

## Purpose

The low-level input hooks and the Razer-encoding detector: turns raw key events into encoded button signals (F13–F24), imposes the software Hypershift layer, suppresses/replays modifiers, dispatches actions, and keeps holds alive.

## Ownership

- `windows.rs` — `WH_KEYBOARD_LL` hook, `WH_MOUSE_LL` hook (software Hypershift) plus its `hyper-inject` right-click thread and liveness check, encoding-chord detection, modifier buffering, hook-health probe, the capture worker + bounded event channel, foreground watcher.
- `mod.rs` — `process_encoded_key_event`: profile resolution, action lookup, dispatch, hold/release handling.
- `linux.rs` — Linux capture (not the primary target).

## Local Contracts

- This is a hard-latency path (the LL hook callback has a ~200–300ms system budget): no heavy work, allocations, or blocking in the hook callback. Per-keystroke work already got audited/optimized — do not regress it. The mouse hook is worse: it sees every mouse move, so non-button messages must bail on a single compare.
- **Never call `SendInput` from inside the `WH_MOUSE_LL` callback** — measured, not theoretical: an injection issued there blocks for the full `LowLevelHooksTimeout` (~1000 ms), because the injected event has to travel the very hook chain whose thread is still inside the callback. Every other input event queues behind it, so the pointer freezes in ~1 s batches (two per translated click: down + up) until Windows silently drops the hook. The same call from the hook thread *outside* a callback, or from another thread while no callback injection is pending, costs 0.4–4 ms. The callback therefore only enqueues; the `hyper-inject` thread injects. (The keyboard hook is unaffected: its in-callback injections measure ~1 ms.)
- **Software Hypershift**: this device's firmware only ever emits the standard-layer record as long as no button holds a firmware Hypershift record (measured — with the right button stored as Hypershift `0x0c` the firmware swallows it and emits the layer-1 records itself; `onboard_guard.rs` puts the trigger back), so the layer is *imposed* by the helper, not read. The mouse hook holds `HELPER_HYPER_ACTIVE` while the trigger button is held, swallows that button (it is a pure modifier), and turns the left button into a right click. A scroll tick carries no encoded key to stamp — the firmware turns buttons, never the wheel, into `F13`..`F24` — so the hook emits the encoder mapping of a wheel direction the user bound on the hypershift layer and swallows the tick (`wheel_capture_action`; an unbound direction keeps scrolling natively, which is why the parent tags a wheel direction as such only when a hypershift binding for it exists). The keyboard hook stamps every match it emits with `HS:` while the state is held, and the parent turns that into `EncodedKeyEvent.is_hypershift` at parse time. Stamping must stay at capture time — reading the state at dispatch time races a fast trigger release. The translation decisions live in the pure `hyper_mouse_action` and `wheel_capture_action` (unit-tested); the hook only applies their verdict.
- **The mouse hook needs its own liveness check**: Windows drops a `WH_MOUSE_LL` hook silently after a low-level-hook timeout, and the keyboard probe can never see that (it only reports a dead *keyboard* hook) — the trigger would stay dead for the rest of the session. Any mouse callback proves life; pointer movement (≥ `MOUSE_HEALTH_MOVE_PX`) with no callback means the hook is gone; a tagged 1-px probe settles the parked-pointer case. A dead verdict reinstalls the hook (which also clears a stuck held state) and a failed install keeps retrying.
- Helper diagnostics: the helper's stderr is piped into the parent log as `[helper] …` (it used to go nowhere, so a dropped hook or a panic left no trace), and helper death (stdout EOF) is logged and surfaced as a runtime warning instead of failing silently. Hook-path diagnostics ride the match stream instead — `DEBUG:…` lines the parent logs at `debug`, and `NOTE:…` lines it logs at `info` for hook events with no action dispatch to be inferred from (the translated Hypershift click) — so the hook itself never formats into a log sink.
- The trigger comes from `Settings.hypershift_trigger` (`HYPER_TRIGGER_CONTROLS` in `config.rs`) and must be a button the mouse still reports as a native mouse button: a button whose onboard record was rewritten into a keystroke never reaches the mouse hook. Absent/`none` = no mouse hook is installed at all.
- The parent's helper-line parser feeds every unstamped key-down to `onboard_guard::note_signal`: an `F13`..`F24` with Ctrl/Shift/Win is an encoding Sidearm never writes, i.e. the firmware's own Hypershift fired, and it triggers an immediate guard check. Our own wheel ticks carry `HS:` and are excluded.
- The mouse hook only swallows the trigger button, LMB-while-held, and a bound hypershift wheel tick; every other message — including our own `INTERNAL_SENDINPUT_EXTRA_INFO` injections — returns `CallNextHookEx` untouched.
- The mask/probe key is VK 0xE8 (`VK_PROBE_KEY` / `VK_MASK_KEY`); the hook passes it through and it is not a modifier.
- The event channel is bounded and drops on full by design (a stalled consumer must not block the producer). Prefer draining releases promptly so holds don't stick.
- Razer encoding window (~10ms) vs modifier replay buffer (~20ms) are tuned constants — change only with real hardware measurements.
- An elevated foreground window (UIPI blocks our input) emits `elevated_foreground_blocked` `{exe}` once per app (`should_notify_elevated`), and only while Sidearm itself is not elevated.

## Work Guidance

- Prefer linear scans over hash maps for the small hotkey/action sets (cache locality beats hashing at these sizes on this budget).

## Verification

- `cargo test` + `cargo clippy ... -D warnings`. Behaviour that depends on real hardware/UAC needs a manual check on the device — including the trigger button and the LMB→RMB translation, and that holding the trigger keeps the pointer smooth (no ~1 s stutter) so the mouse hook is never injected from inside its own callback.

## Child DOX Index

- None. Leaf directory.
