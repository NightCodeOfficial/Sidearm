//! What the mouse does while Sidearm is closed.
//!
//! While Sidearm runs, the onboard memory holds Sidearm's own signals
//! (`F13`..`F24`, `Alt+F23`…) — meaningless without the app. On exit Sidearm
//! writes the active profile into the mouse the way Synapse did, so every
//! binding the firmware can play back on its own keeps working: a single key
//! with modifiers, or a plain mouse button. Text, macros, launches, menus and
//! per-app profiles cannot live in the mouse and stay inert until Sidearm runs
//! again. If the profile uses Hypershift, its trigger becomes the firmware
//! Hypershift key and the layer-1 records get the hypershift bindings.
//!
//! On the next start the marker file says the memory holds the offline
//! profile, and Sidearm writes its own signals back before the guard looks.

use std::path::Path;

use crate::config::{ActionPayload, AppConfig, Layer, MouseActionKind};
use crate::mouse_memory::{
    OnboardAction, VendorDevice, button_id_for_control, hid_usage_for_vk, native_buttons,
    onboard_bindings,
};

/// Present in the config folder while the mouse holds the offline profile.
pub const OFFLINE_MARKER_FILE: &str = "mouse-offline-written";

/// One record to store: button, layer (`0` standard, `1` Hypershift), action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OfflineRecord {
    pub button_id: u8,
    pub layer: u8,
    pub action: OnboardAction,
}

/// The records that make the mouse behave like `profile_id` on its own.
pub fn offline_records(config: &AppConfig, profile_id: &str) -> Vec<OfflineRecord> {
    // `None` = no binding on that layer; `Some(None)` = a binding the firmware
    // cannot play (text, macro, launch…).
    let action_of = |control: &str, layer: Layer| -> Option<Option<OnboardAction>> {
        let binding = config.bindings.iter().find(|b| {
            b.enabled && b.profile_id == profile_id && b.layer == layer && b.control_id.as_str() == control
        })?;
        let action = config.actions.iter().find(|a| a.id == binding.action_id)?;
        Some(onboard_action(&action.payload))
    };
    // Sidearm's own signal of a key control, the fallback for a layer-1 record
    // nothing else fills (so no leftover vendor macro plays on Hypershift).
    let signals: Vec<(String, OnboardAction)> = onboard_bindings(config)
        .into_iter()
        .filter_map(|binding| {
            let spec = crate::hotkeys::parse_hotkey(&binding.encoded_key).ok()?;
            let usage = hid_usage_for_vk(spec.key.code)?;
            let modifier = modifier_mask(spec.modifiers.ctrl, spec.modifiers.shift, spec.modifiers.alt, spec.modifiers.win);
            Some((binding.control_id, OnboardAction::Key { modifier, usage }))
        })
        .collect();

    // (control, button id, what the button does when nothing else applies)
    let mut controls: Vec<(String, u8, Option<OnboardAction>)> = signals
        .iter()
        .filter_map(|(control, signal)| Some((control.clone(), button_id_for_control(control)?, Some(*signal))))
        .collect();
    for (control, button_id, value) in native_buttons() {
        if matches!(*control, "mouse_4" | "mouse_5") {
            controls.push(((*control).to_string(), *button_id, Some(OnboardAction::Mouse(*value))));
        }
    }

    let mut records = Vec::new();
    let mut uses_hypershift = false;
    let mut layer_one = Vec::new();
    for (control, button_id, fallback) in &controls {
        let standard = action_of(control, Layer::Standard).flatten();
        let hypershift = action_of(control, Layer::Hypershift);
        uses_hypershift |= matches!(hypershift, Some(Some(_)));
        if let Some(action) = standard {
            records.push(OfflineRecord { button_id: *button_id, layer: 0, action });
        }
        // Like the resolver: the standard binding fills in only where the
        // hypershift layer has no binding at all. A hypershift binding the
        // firmware cannot play leaves Sidearm's inert signal instead.
        let layer_one_action = match hypershift {
            Some(Some(action)) => Some(action),
            Some(None) => *fallback,
            None => standard.or(*fallback),
        };
        if let Some(action) = layer_one_action {
            layer_one.push(OfflineRecord { button_id: *button_id, layer: 1, action });
        }
    }
    let trigger = native_buttons()
        .iter()
        .find(|(control, _, _)| *control == config.settings.hypershift_trigger.trim());
    if uses_hypershift && let Some((_, trigger_id, _)) = trigger {
        records.extend(layer_one);
        for layer in [0, 1] {
            records.push(OfflineRecord { button_id: *trigger_id, layer, action: OnboardAction::Hypershift });
        }
    }
    records
}

/// The firmware form of an action, when it has one.
fn onboard_action(payload: &ActionPayload) -> Option<OnboardAction> {
    match payload {
        ActionPayload::Shortcut(shortcut) if !shortcut.key.trim().is_empty() => {
            let key = crate::hotkeys::parse_primary_key(&shortcut.key).ok()?;
            Some(OnboardAction::Key {
                modifier: modifier_mask(shortcut.ctrl, shortcut.shift, shortcut.alt, shortcut.win),
                usage: hid_usage_for_vk(key.code)?,
            })
        }
        ActionPayload::MouseAction(mouse) if !(mouse.ctrl || mouse.shift || mouse.alt || mouse.win) => {
            Some(OnboardAction::Mouse(match mouse.action {
                MouseActionKind::LeftClick => 1,
                MouseActionKind::RightClick => 2,
                MouseActionKind::MiddleClick => 3,
                MouseActionKind::MouseBack => 4,
                MouseActionKind::MouseForward => 5,
                _ => return None,
            }))
        }
        _ => None,
    }
}

fn modifier_mask(ctrl: bool, shift: bool, alt: bool, win: bool) -> u8 {
    u8::from(ctrl) | u8::from(shift) << 1 | u8::from(alt) << 2 | u8::from(win) << 3
}

/// Writes the offline profile into the mouse and leaves the marker. Best
/// effort: a record the mouse refuses is logged and skipped.
pub fn write_offline_profile(config: &AppConfig, profile_id: &str, config_dir: &Path) {
    let records = offline_records(config, profile_id);
    let device = match VendorDevice::open(0x1532, 0x00B4) {
        Ok(device) => device,
        Err(message) => {
            log::warn!("[mouse-offline] mouse not reachable, offline profile not written: {message}");
            return;
        }
    };
    let failed = records
        .iter()
        .filter(|r| {
            device
                .write_record(r.button_id, r.layer, r.action)
                .inspect_err(|e| log::warn!("[mouse-offline] {e}"))
                .is_err()
        })
        .count();
    let _ = std::fs::write(config_dir.join(OFFLINE_MARKER_FILE), profile_id);
    log::info!(
        "[mouse-offline] profile `{profile_id}` written for use without Sidearm: {} records, {failed} failed",
        records.len()
    );
}

/// Puts Sidearm's own signals back when the mouse still holds the offline
/// profile. Returns `true` when it restored (the marker was there).
pub fn restore_after_offline(config: &AppConfig, config_dir: &Path) -> bool {
    let marker = config_dir.join(OFFLINE_MARKER_FILE);
    if !marker.exists() {
        return false;
    }
    let result = VendorDevice::open(0x1532, 0x00B4).and_then(|device| {
        device.apply_bindings(&onboard_bindings(config))?;
        let natives: Vec<&str> = native_buttons().iter().map(|(id, _, _)| *id).collect();
        device.restore_native_buttons(&natives)
    });
    match result {
        Ok(()) => {
            let _ = std::fs::remove_file(&marker);
            log::info!("[mouse-offline] Sidearm signals restored after the offline profile");
            true
        }
        Err(message) => {
            log::warn!("[mouse-offline] restore after the offline profile failed: {message}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_mask_matches_the_hid_bits() {
        assert_eq!(modifier_mask(true, false, false, false), 0x01);
        assert_eq!(modifier_mask(true, true, false, false), 0x03);
        assert_eq!(modifier_mask(false, false, true, false), 0x04);
        assert_eq!(modifier_mask(false, true, false, true), 0x0a);
    }

    #[test]
    fn shortcuts_and_mouse_buttons_have_a_firmware_form() {
        use crate::config::{MouseActionPayload, ShortcutActionPayload};
        let shortcut = |key: &str, ctrl: bool, shift: bool| {
            ActionPayload::Shortcut(ShortcutActionPayload {
                key: key.into(),
                ctrl,
                shift,
                alt: false,
                win: false,
                raw: None,
            })
        };
        assert_eq!(
            onboard_action(&shortcut("T", true, true)),
            Some(OnboardAction::Key { modifier: 0x03, usage: 0x17 }),
            "Ctrl+Shift+T as Synapse stored it"
        );
        assert_eq!(
            onboard_action(&shortcut("Delete", false, false)),
            Some(OnboardAction::Key { modifier: 0, usage: 0x4c })
        );
        assert_eq!(onboard_action(&shortcut("", true, false)), None, "modifier-only");
        let back = ActionPayload::MouseAction(MouseActionPayload {
            action: MouseActionKind::MouseBack,
            ctrl: false,
            shift: false,
            alt: false,
            win: false,
        });
        assert_eq!(onboard_action(&back), Some(OnboardAction::Mouse(4)));
    }
}

