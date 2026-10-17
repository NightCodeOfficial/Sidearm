# src-tauri — Backend (Rust + Tauri v2)

## Purpose

The Rust backend: low-level keyboard capture, Razer-encoding detection, action execution (keystrokes, mouse, text/snippets, sequences, launches), config persistence with backups/recovery, Synapse import, the mouse's own onboard button memory, window/foreground resolution, and the Tauri IPC surface.

## Ownership

- `src/lib.rs` — Tauri app setup + the `#[tauri::command]` handlers (the IPC surface called from `src/lib/backend.ts`).
- `src/config.rs` — `AppConfig`, load/save, schema + semantic validation, atomic writes, rolling/daily backups, corrupt-recovery + newer-schema guard.
- `src/executor.rs` — runs actions; `src/input_synthesis.rs` — `SendInput` keystroke/mouse/text, clipboard paste, snippet-token expansion.
- `src/backup.rs`, `src/resolver.rs`, `src/window_capture.rs`, `src/hotkeys.rs`, `src/vk.rs`, `src/chord.rs`, `src/recorder.rs`.
- `src/mouse_memory.rs` — the mouse's vendor HID collection: opens it, writes a button record into its onboard slot and verifies the mouse's echo (`apply_onboard_mapping`, `verify_onboard_mapping`), reads the device mode and the stored per-button records, and puts the mode back to `Normal` (`restore_onboard_control`). Windows-only (`#[cfg(target_os = "windows")]` on the module).
- `src/onboard_guard.rs` — the takeover watchdog: while the runtime is active it polls the device mode and the stored thumb records, puts them back when Razer's engine changed them, and reports it as `onboard_takeover` (throttled per kind, `NOTIFY_COOLDOWN`). Windows-only.
- Manifests: `Cargo.toml`, `tauri.conf.json`, `capabilities/` (under src-tauri). The config JSON schema is `schemas/config.v3.schema.json` at the repo ROOT, bundled into `config.rs` via `include_str!("../schemas/...")`.
- Delegated to children: `src/capture_backend/`, `src/synapse_import/`, `src/platform/`.

## Local Contracts

- Internal fns return `Result<T, String>`; convert to `CommandError` at the IPC boundary.
- Config is schema-first: the bundled `config.v3.schema.json` validates raw JSON before deserialize. A persisted field rename needs `anyOf`(both names) in the schema + `#[serde(alias)]`. Keep the Rust structs, `src/lib/config.ts`, and the schema in sync.
- Config writes are atomic (temp + rename); backups (rolling rotation, daily snapshot, last-known-good) are best-effort and never fail the save. A corrupt `config.json` recovers from last-known-good/rolling backups; a config declaring a newer schema version is never overwritten.
- `src/admin_autostart.rs`: the elevated logon task never starts Sidearm.exe directly — it runs System32 PowerShell with a verifier script (in the task XML) that checks SHA-256 of the exe + every sibling `*.dll` pinned at enable time and holds them open until Sidearm is up. Re-running `enable()` re-pins (the "confirm after update" path, `needsReconfirm`). Keep `%` and `"` out of the script.
- Self-injected input carries `INTERNAL_SENDINPUT_EXTRA_INFO`; VK 0xE8 is the mask/probe key (`vk.rs` `VK_MASK_KEY`), deliberately not a modifier. **Every `dwExtraInfo` tag must fit in 32 bits**: a `WH_MOUSE_LL` hook receives it truncated to its low dword (measured: `0x1122334455667788` arrives as `0x55667788`, while a `WH_KEYBOARD_LL` hook sees the full value). A wider tag stops matching in the mouse hook, which then treats our own injected clicks as physical presses and swallows them.
- Takeover guard (`onboard_guard.rs`): a takeover is only ever reported for a mode the guard **read** as non-`Normal` and for stored records it **read** as different from the config. Two rules come from hardware measurement: a mode read-back is retried (`MODE_CONFIRM_ATTEMPTS`) because a `SET Normal` whose read-back still said `Driver` was `Normal` one second later, so a transition must never be reported as a failed takeover; and a drifted record is read twice before it counts, because a read that catches the device mid-transition answers with plausible but wrong bytes. A refused command (an idle/sleeping wireless mouse answers status `0x04` to every vendor query) is not evidence of a takeover — log at debug and wait for the next tick. The guard is silent while `RuntimeStore::is_running()` is false: a stopped Sidearm does not fight Razer for the mouse. It guards every addressable button: the key records (thumbs + top buttons, `onboard_bindings`) and the plain mouse buttons (`NATIVE_BUTTONS`: left/right/middle, wheel up/down, `0x34`/`0x35` = mouse 4/5). Razer stores its profile with a mouse button as a firmware Hypershift key (category `0x0c`); the firmware then swallows that button (the software Hypershift trigger dies) and emits Synapse's layer-1 shortcuts itself — so any native button that is not its plain mouse button is drift. Timing: a check at once on start, `POLL_INTERVAL` normally, `FAST_POLL` while the mouse refuses queries, the runtime is not running yet, or Razer's engine (`RazerAppEngine.exe`/`RzEngineMon.exe`) runs, and immediately on `request_check`. A Razer engine start is reported as `razerStarted`. A clean verdict goes into the runtime log once per change; a throttled notification is still logged (at `info`).
- Software Hypershift: `Settings.hypershiftTrigger` names the native mouse button held as the modifier (default the right button). The device never emits the layer-1 record, so `resolver::resolve_input_preview_with_overrides` imposes the layer from the trigger state and falls back to the control's standard binding when the hypershift layer has no binding for it. The wheel needs the same treatment from the capture side: the firmware never encodes a scroll tick, so an encoder mapping tagged as a wheel direction is emitted by the mouse hook itself while the trigger is held. Mechanism lives in `src/capture_backend/`.
- Vendor HID (`mouse_memory.rs`): the collection is opened with **no** requested access rights and a sharing mode (Razer's driver holds it — GENERIC rights fail `ERROR_ACCESS_DENIED`), the feature buffer is 91 bytes for report id `0x03`, and a write is only trusted when the mouse echoes it (`SET` → two `GET` → retry). The **slot** query cannot cold-read: a `GET_REPORT` answers only with what the previous `SET_REPORT` stored, so `verify_onboard_mapping` confirms an assignment by writing it. The **command** class does answer honestly and is what the guard reads: `class 0x00 / cmd 0x84` returns the device mode (`0x00` Normal = the firmware emits the onboard records; `0x03` Driver = Razer's driver maps the buttons instead) and `class 0x02 / cmd 0x8c` with `[profile=1, button, layer=0, …]` returns the stored record at reply bytes 11..=14 (`category, modifier, usage`; a mouse-category `0x01` record carries the button number where a key record has its modifier). Its twin `class 0x02 / cmd 0x0c` with `[profile=1, button, layer=0, category, len, value…]` writes a record (`restore_native_button`, and key records for the slot-less top buttons `0x0b`/`0x0c`) — the slot path's "marker" `0a 02 0c 01` is this same header. Key records carry a HID modifier mask (Ctrl `0x01`, Shift `0x02`, Alt `0x04`, Win `0x08`): thumbs are bare `F13`..`F24`, the top buttons `Alt+F23`/`Alt+F24`; only `F13`..`F24` targets are ever written. Proven on hardware (`cargo test -- --ignored`). The record payload bytes 9..14 are `button_id, layer, 0x02, len, modifier, hid_usage`; `layer` must be `0x00` (the firmware emits that record — `0x01` stores a copy the mouse never sends) and `modifier` is the HID mask above (`0x00` for a plain `F13`..`F24`).

## Work Guidance

- `unwrap`/`expect` only where a preceding branch guarantees safety; never on user/hardware-controlled input.

## Verification

- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` (zero warnings) and `cargo test --manifest-path src-tauri/Cargo.toml`.
- `cargo` is at `C:\Users\User\.cargo\bin\cargo.exe` (not on the bash PATH).

## Child DOX Index

- `src-tauri/src/capture_backend/AGENTS.md` — LL keyboard hook + Razer encoding detection
- `src-tauri/src/synapse_import/AGENTS.md` — Razer Synapse profile import
- `src-tauri/src/platform/AGENTS.md` — OS-specific display/input/window/shell
