//! Onboard button-mapping memory of the mouse's vendor HID interface.
//!
//! Razer Synapse does not remap buttons at runtime: it writes a 90-byte record
//! into one of 31 onboard slots with a HID `SET_REPORT` control transfer and
//! reads it back with `GET_REPORT`. A USB capture of Synapse proves the mouse
//! then emits the mapped key itself, with Synapse closed, so this module can
//! own the mapping instead of the vendor app.
//!
//! Wire facts (reverse-engineered from `sidearm_map2.pcap`, see the slot
//! fixtures in the tests): the report id is `0x03`, so a Windows feature buffer
//! is 91 bytes with the id first. Inside the 90-byte payload, byte 0 is a
//! command/status byte (`0x00` on a write, non-zero in a reply), byte 1 is the
//! slot, bytes 5..8 are the record marker, the record itself is bytes 9..14
//! (`button_id, layer, 0x02, 0x02, modifier, hid_usage`) and byte 88 is a
//! checksum over bytes 2..87 — the status and slot bytes are excluded. `layer`
//! selects which of the button's two records is meant; the mouse emits the
//! `0x00` one, so that is the only one written here.
//!
//! The slot query cannot cold-read: a `GET_REPORT` answers only with what the
//! previous `SET_REPORT` stored — selecting a slot with the empty marker
//! therefore returns the marker, not the slot's earlier record (proven on
//! hardware: a slot holding `F14` answered `02 07 80 00 00`). The command class
//! does answer honestly, and that is what the takeover guard watches:
//! `class 0x02 / command 0x8c` returns the record the device really holds (all
//! twelve thumbs read back as `F13`..`F24` after a write), and
//! `class 0x00 / command 0x84` returns the real device mode — it reported
//! `Driver` while Razer's engine was running, and `Driver` is never written from
//! here. Writes are still confirmed by their echo.
//!
//! Two hardware quirks are handled here rather than by callers. The vendor
//! collection is opened with no requested access rights: Razer's own driver
//! holds it, and asking for GENERIC rights fails with `ERROR_ACCESS_DENIED`.
//! And the reply to a write lags one transaction on some slots, so a write is
//! verified with two reads and one retry.

use std::io::Error;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Storage::FileSystem::CreateFileW;
use windows_sys::Win32::System::Registry::HKEY;
use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows_sys::Win32::System::Registry::KEY_READ;
use windows_sys::Win32::System::Registry::RegCloseKey;
use windows_sys::Win32::System::Registry::RegEnumKeyExW;
use windows_sys::Win32::System::Registry::RegOpenKeyExW;
use windows_sys::Win32::Devices::HumanInterfaceDevice::HidD_GetFeature;
use windows_sys::Win32::Devices::HumanInterfaceDevice::HidD_SetFeature;

/// Report id of the vendor feature report, as sent in `wValue` by Synapse.
pub const REPORT_ID: u8 = 0x03;
/// Payload length on the wire (the Windows feature buffer adds the id byte).
pub const PAYLOAD_LEN: usize = 90;
/// First button id of the thumb column; ids run `0x40..=0x4b` for 12 buttons.
pub const BUTTON_ID_BASE: u8 = 0x40;

/// Onboard slot holding the record of each thumb button, in button-id order.
/// Not arithmetic: this is the allocation the captured vendor traffic shows,
/// and slot 0x1b is a spare duplicate of button 0x4b that we never write.
const SLOT_FOR_BUTTON: [u8; 12] = [0x07, 0x1c, 0x1d, 0x1e, 0x00, 0x01,
                                   0x02, 0x03, 0x08, 0x04, 0x05, 0x06];
const RECORD_MARKER: [u8; 4] = [0x0a, 0x02, 0x0c, 0x01];
/// First HID usage of the function keys the thumb column can be mapped to.
const USAGE_F13: u8 = 0x68;
/// HID modifier mask bits of a key record (left-hand keys). A thumb's bare
/// `F13`..`F24` has mask `0x00`; the top buttons' `Alt+F23`/`Alt+F24` have
/// `0x04`. A `0x07` makes the mouse emit `Ctrl+Alt+Shift+Fxx`.
const MOD_CTRL: u8 = 0x01;
const MOD_SHIFT: u8 = 0x02;
const MOD_ALT: u8 = 0x04;
const MOD_WIN: u8 = 0x08;
/// The two top buttons hold a key record like the thumbs but have no slot in
/// the vendor table, so they are written with [`CMD_SET_BUTTON`] (measured:
/// `top_aux_01` read back `Alt+F23` after one write).
const TOP_AUX_BUTTONS: [(&str, u8); 2] = [("top_aux_01", 0x0b), ("top_aux_02", 0x0c)];
/// Buttons that must stay plain mouse buttons, as `(control id, vendor button
/// id, mouse-button value)`. Measured by scanning every id with `GET_BUTTON`:
/// these answered category `0x01`; every other id but the thumbs and the top
/// buttons answered status `0x03`.
const NATIVE_BUTTONS: [(&str, u8, u8); 7] = [
    ("mouse_left", 0x01, 1),
    ("mouse_right", 0x02, 2),
    ("wheel_click", 0x03, 3),
    ("wheel_up", 0x09, 0x09),
    ("wheel_down", 0x0a, 0x0a),
    ("mouse_4", 0x34, 4),
    ("mouse_5", 0x35, 5),
];
/// Device mode that makes the mouse emit the records written into onboard
/// memory — the only mode Sidearm works in.
pub const MODE_NORMAL: u8 = 0x00;
/// Device mode that hands the buttons to the vendor driver: Razer's engine
/// switches the mouse here on startup, and from then on the driver's own mapping
/// reaches the OS while onboard memory is ignored entirely. Anything but
/// [`MODE_NORMAL`] must be treated as a takeover.
pub const MODE_DRIVER: u8 = 0x03;

/// Transaction byte of the vendor command class (same value the captured
/// Synapse traffic uses for control commands).
const COMMAND_TX: u8 = 0x1f;
/// Command class of the device-mode get/set commands.
const CLASS_CONTROL: u8 = 0x00;
/// Command class that answers button queries.
const CLASS_BUTTON: u8 = 0x02;
const CMD_SET_MODE: u8 = 0x04;
const CMD_GET_MODE: u8 = 0x84;
/// Per-button query. Unlike the slot query the record path uses, this one answers
/// with the record the device actually holds, and it needs no preceding write.
const CMD_GET_BUTTON: u8 = 0x8c;
/// Per-button write, the twin of [`CMD_GET_BUTTON`]: arguments are profile,
/// button, layer, then the action (`category, len, value…`). The slot path's
/// "record marker" `0a 02 0c 01` is this same header (length, class, command,
/// profile) — proven on hardware by writing the right button back with it.
const CMD_SET_BUTTON: u8 = 0x0c;
/// Category byte of a button record that acts as a plain mouse button; its
/// value is the button number (`1` left, `2` right, `3` middle).
const CATEGORY_MOUSE: u8 = 0x01;
/// `GET_BUTTON` arguments: profile, button, layer, then padding.
const GET_BUTTON_ARGS: [u8; 10] = [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
const COMMAND_STATUS_OK: u8 = 0x02;
const COMMAND_STATUS_BUSY: u8 = 0x01;
/// The device answers `busy` while it finishes a write; retry, don't fail.
const COMMAND_RETRIES: u32 = 8;
const COMMAND_SETTLE: std::time::Duration = std::time::Duration::from_millis(50);
const COMMAND_BUSY_WAIT: std::time::Duration = std::time::Duration::from_millis(200);
/// A mode change is not visible immediately: for about a second the device still
/// answers from the mode it is leaving (measured on hardware — a `SET Normal`
/// whose read-back said `Driver` was `Normal` one second later). Never call that a
/// failed takeover.
const MODE_CONFIRM_ATTEMPTS: u32 = 4;
const MODE_CONFIRM_WAIT: std::time::Duration = std::time::Duration::from_millis(250);
/// Category byte of a button record that emits a key.
const CATEGORY_KEY: u8 = 0x02;

/// Thumb controls of the built-in Naga device: the leading slice of
/// [`crate::config::NAGA_CONTROL_IDS`], `thumb_01` through `thumb_12`.
const THUMB_CONTROL_COUNT: usize = 12;
/// Registry home of device interface symbolic link names.
const DEVICE_CLASSES: &str = r"SYSTEM\CurrentControlSet\Control\DeviceClasses";
/// Interface class that owns the HID collections (not the HID *class* guid).
const HID_INTERFACE_GUID: &str = "{4d1e55b2-f16f-11cf-88cb-001111000030}";

/// Checksum byte: XOR of payload bytes 2..=87, excluding status and slot.
pub fn checksum(payload: &[u8; PAYLOAD_LEN]) -> u8 {
    payload[2..88].iter().fold(0u8, |acc, &byte| acc ^ byte)
}

/// Onboard slot for a thumb button id, following the vendor's own allocation.
pub fn slot_for_button(button_id: u8) -> Option<u8> {
    let index = button_id.checked_sub(BUTTON_ID_BASE)? as usize;
    SLOT_FOR_BUTTON.get(index).copied()
}

/// Vendor button id of a control whose onboard record emits a key: the thumbs
/// (`thumb_01` is the first button) and the two top buttons.
pub fn button_id_for_control(control_id: &str) -> Option<u8> {
    if let Some(index) = crate::config::NAGA_CONTROL_IDS
        .iter()
        .take(THUMB_CONTROL_COUNT)
        .position(|id| *id == control_id)
    {
        return Some(BUTTON_ID_BASE + index as u8);
    }
    TOP_AUX_BUTTONS
        .iter()
        .find(|(id, _)| *id == control_id)
        .map(|(_, button_id)| *button_id)
}

/// HID modifier mask of a hotkey's modifiers.
fn modifier_mask(modifiers: &crate::hotkeys::HotkeyModifiers) -> u8 {
    [
        (modifiers.ctrl, MOD_CTRL),
        (modifiers.shift, MOD_SHIFT),
        (modifiers.alt, MOD_ALT),
        (modifiers.win, MOD_WIN),
    ]
    .iter()
    .filter(|(held, _)| *held)
    .fold(0, |mask, (_, bit)| mask | bit)
}

/// HID usage for a key name; only `F13` through `F24` fit onboard memory.
pub fn usage_for_key(display_name: &str) -> Option<u8> {
    let number = display_name.strip_prefix('F')?.parse::<u8>().ok()?;
    if (13..=24).contains(&number) {
        Some(USAGE_F13 + (number - 13))
    } else {
        None
    }
}

/// Onboard target of one binding: the button id, the HID modifier mask and the
/// HID usage it should emit. Only `F13`..`F24` (with any modifiers) are
/// accepted — keys no application uses, so the mouse can never fire a real
/// shortcut on its own. Shared by the write path and the drift check so both
/// agree on what "this binding is in place" means.
fn binding_target(binding: &OnboardBinding) -> Result<(u8, u8, u8), String> {
    let button_id = button_id_for_control(&binding.control_id)
        .ok_or_else(|| format!("{} has no onboard key record", binding.control_id))?;
    let spec = crate::hotkeys::parse_hotkey(&binding.encoded_key)?;
    let usage = usage_for_key(&spec.key.display_name)
        .ok_or_else(|| format!("{} cannot live in onboard memory", spec.key.display_name))?;
    Ok((button_id, modifier_mask(&spec.modifiers), usage))
}

/// Key bindings (thumbs and top buttons) the mouse should hold on its own, mirroring the frontend's
/// `onboardBindings` (`src/lib/onboard-mapping.ts`): the standard layer only —
/// the mouse has no notion of Sidearm's layers, so hypershift stays a software
/// concern.
pub fn onboard_bindings(config: &crate::config::AppConfig) -> Vec<OnboardBinding> {
    thumb_bindings(&config.encoder_mappings)
}

/// The filter [`onboard_bindings`] applies, split out so it is testable without
/// building a whole config.
fn thumb_bindings(mappings: &[crate::config::EncoderMapping]) -> Vec<OnboardBinding> {
    mappings
        .iter()
        .filter(|mapping| mapping.layer == crate::config::Layer::Standard)
        .filter(|mapping| button_id_for_control(mapping.control_id.as_str()).is_some())
        .map(|mapping| OnboardBinding {
            control_id: mapping.control_id.as_str().to_string(),
            encoded_key: mapping.encoded_key.clone(),
        })
        .collect()
}

/// One requested button binding in the app's own terms: which control, and the
/// key it should emit. The config owns these; we only translate them.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardBinding {
    pub control_id: String,
    pub encoded_key: String,
}

/// What the mouse currently does, in the app's own terms: which control emits
/// which key. The frontend can show this without knowing the vendor protocol.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardMapping {
    pub control_id: String,
    pub encoded_key: String,
}

/// Which bindings the mouse confirmed, and which it did not.
#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardReport {
    pub confirmed: Vec<OnboardMapping>,
    pub failed: Vec<String>,
}

/// One button record as the device holds it, read back with the per-button query
/// (no write in front of it, unlike the slot query the record path uses).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonRecord {
    /// `0x02` when the button emits a key — the only category this app writes.
    pub category: u8,
    /// `0x00` for a bare `F13`..`F24`.
    pub modifier: u8,
    /// HID usage: `0x68`..`0x73` are `F13`..`F24`.
    pub usage: u8,
}

impl ButtonRecord {
    /// A mouse-category record carries the button number in the byte a key
    /// record uses for its modifier.
    fn is_mouse_button(&self, value: u8) -> bool {
        self.category == CATEGORY_MOUSE && self.modifier == value
    }
}

/// Every button that must stay a plain mouse button, as `(control id, vendor
/// button id, value)`.
pub fn native_buttons() -> &'static [(&'static str, u8, u8)] {
    &NATIVE_BUTTONS
}

/// Open handle to the vendor HID collection plus the feature-report plumbing.
pub struct VendorDevice {
    handle: HANDLE,
}

impl VendorDevice {
    /// Opens the vendor collection of `vid:pid`, discovering its interface path
    /// in the registry. Returns `Err(String)` with an OS-error description.
    pub fn open(vid: u16, pid: u16) -> Result<Self, String> {
        let path = find_interface_path(vid, pid)?;
        let mut wide: Vec<u16> = path.encode_utf16().collect();
        wide.push(0);
        // No requested rights: the vendor collection is held by Razer's driver
        // and refuses GENERIC_READ|GENERIC_WRITE to non-owners.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                0,
                0x00000003, // FILE_SHARE_READ | FILE_SHARE_WRITE
                std::ptr::null(),
                3,          // OPEN_EXISTING
                0x00000080, // FILE_ATTRIBUTE_NORMAL
                INVALID_HANDLE_VALUE,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(format!("{}: CreateFileW({path})", Error::last_os_error()));
        }
        Ok(Self { handle })
    }

    /// Writes one button record into its slot and verifies it read back.
    /// `Err` means the mouse did not echo what we wrote.
    pub fn write_button(&self, button_id: u8, usage: u8, modifier: u8) -> Result<(), String> {
        let Some(slot) = slot_for_button(button_id) else {
            return self.write_key_command(button_id, modifier, usage);
        };
        let payload = build_record(slot, button_id, usage, modifier);
        for _ in 0..2 {
            self.set_payload(&payload)?;
            for _ in 0..2 {
                let read = self.get_payload(slot)?;
                if record_matches(&payload, &read) {
                    return Ok(());
                }
            }
        }
        Err(format!("mouse did not confirm button {button_id:#02x} -> usage {usage:#02x}"))
    }

    /// A key record for a button with no vendor slot (the top buttons), written
    /// with [`CMD_SET_BUTTON`] and confirmed by reading it back.
    fn write_key_command(&self, button_id: u8, modifier: u8, usage: u8) -> Result<(), String> {
        let args = [0x01, button_id, 0x00, CATEGORY_KEY, 0x02, modifier, usage, 0x00, 0x00, 0x00];
        self.command(CLASS_BUTTON, CMD_SET_BUTTON, &args)?;
        if self.holds(button_id, modifier, usage)? {
            Ok(())
        } else {
            Err(format!("mouse did not confirm button {button_id:#02x} -> usage {usage:#02x}"))
        }
    }

    /// Writes every requested binding in order, stopping at the first failed
    /// write. Bindings onboard memory cannot express are skipped, as in
    /// [`Self::drifted_bindings`]: they were never the mouse's job.
    pub fn apply_bindings(&self, bindings: &[OnboardBinding]) -> Result<(), String> {
        for binding in bindings {
            let Ok((button_id, modifier, usage)) = binding_target(binding) else {
                continue;
            };
            self.write_button(button_id, usage, modifier)?;
        }
        Ok(())
    }

    /// Confirms which bindings the mouse actually holds, by writing each one
    /// and requiring the mouse to echo it back. The vendor collection answers
    /// only with what the previous write stored, so the mouse cannot be read:
    /// a `GET_REPORT` after selecting a slot returns the marker we just wrote,
    /// never the slot's earlier content (proven on hardware, twice).
    pub fn verify(&self, bindings: &[OnboardBinding]) -> Result<OnboardReport, String> {
        let mut report = OnboardReport::default();
        for binding in bindings {
            let Ok((button_id, modifier, usage)) = binding_target(binding) else {
                continue;
            };
            if self.write_button(button_id, usage, modifier).is_ok() {
                report.confirmed.push(OnboardMapping {
                    control_id: binding.control_id.clone(),
                    encoded_key: binding.encoded_key.clone(),
                });
            } else {
                report.failed.push(binding.control_id.clone());
            }
        }
        Ok(report)
    }

    /// Current device mode: [`MODE_NORMAL`], [`MODE_DRIVER`], or a value this app
    /// does not know. Read from the device, not echoed from our last write — the
    /// command class answers with its own state (proven: it reported `Driver`
    /// while Razer's engine was running, and `Driver` is never written here).
    pub fn mode(&self) -> Result<u8, String> {
        let reply = self.command(CLASS_CONTROL, CMD_GET_MODE, &[0x00, 0x00])?;
        Ok(reply[8])
    }

    /// Puts the device back into [`MODE_NORMAL`] so the mouse emits the records
    /// written here, and returns the mode it replaced. The read-back is retried
    /// ([`MODE_CONFIRM_ATTEMPTS`]) because a transition is not a failure — only a
    /// mode that is still wrong after every attempt is an error.
    pub fn ensure_normal_mode(&self) -> Result<u8, String> {
        let before = self.mode()?;
        if before == MODE_NORMAL {
            return Ok(before);
        }
        self.command(CLASS_CONTROL, CMD_SET_MODE, &[MODE_NORMAL, 0x00])?;
        let mut last = "no reply".to_string();
        for attempt in 0..MODE_CONFIRM_ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(MODE_CONFIRM_WAIT);
            }
            match self.mode() {
                Ok(MODE_NORMAL) => return Ok(before),
                Ok(after) if after == MODE_DRIVER => {
                    last = "Razer's driver re-asserted driver mode".to_string();
                }
                Ok(after) => last = format!("device stayed in mode {after:#04x}"),
                Err(message) => last = message,
            }
        }
        Err(last)
    }

    /// The record the device holds for one thumb button, read without writing
    /// anything. This is the only honest way to see what another program stored —
    /// the slot query answers with whatever the last write put there.
    pub fn read_button(&self, button_id: u8) -> Result<ButtonRecord, String> {
        let mut args = GET_BUTTON_ARGS;
        args[1] = button_id;
        let reply = self.command(CLASS_BUTTON, CMD_GET_BUTTON, &args)?;
        let record = &reply[8..18];
        Ok(ButtonRecord {
            category: record[3],
            modifier: record[5],
            usage: record[6],
        })
    }

    /// Puts every listed native control back to its plain mouse button; `Err`
    /// names the first one the device did not confirm.
    pub fn restore_native_buttons(&self, control_ids: &[&str]) -> Result<(), String> {
        for (control_id, button_id, value) in NATIVE_BUTTONS {
            if control_ids.contains(&control_id) {
                self.restore_native_button(button_id, value)
                    .map_err(|error| format!("{control_id}: {error}"))?;
            }
        }
        Ok(())
    }

    /// Whether the button still acts as the plain mouse button `value`. Razer's
    /// engine stores its own profile here too, and with Hypershift assigned to a
    /// button (category `0x0c`) the firmware swallows that button — the software
    /// Hypershift trigger then never reaches the mouse hook (measured: the right
    /// button read `0c 01 01` on both layers while Synapse's layer-1 shortcuts
    /// were emitted by the mouse itself). Read twice before it counts, like
    /// [`Self::drifted_bindings`].
    pub fn native_button_drifted(&self, button_id: u8, value: u8) -> Result<bool, String> {
        for _ in 0..2 {
            if self.read_button(button_id)?.is_mouse_button(value) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Writes the standard-layer record of `button_id` back to the plain mouse
    /// button `value` and requires the device to read it back.
    pub fn restore_native_button(&self, button_id: u8, value: u8) -> Result<(), String> {
        let args = [0x01, button_id, 0x00, CATEGORY_MOUSE, 0x01, value, 0x00, 0x00, 0x00, 0x00];
        self.command(CLASS_BUTTON, CMD_SET_BUTTON, &args)?;
        if self.read_button(button_id)?.is_mouse_button(value) {
            Ok(())
        } else {
            Err(format!("button {button_id:#04x} did not read back as mouse button {value}"))
        }
    }

    /// Control ids whose stored record no longer matches `bindings` — the buttons
    /// another program rewrote (Razer stores its own profile in the same slots).
    /// Read-only, so it is safe to run on a timer while the user works. Bindings
    /// this app cannot express are skipped: they were never written here.
    ///
    /// A mismatch is read twice before it counts: a read that catches the device
    /// mid-transition answers with plausible but wrong bytes (measured on hardware
    /// while the mode was being switched back).
    pub fn drifted_bindings(&self, bindings: &[OnboardBinding]) -> Result<Vec<String>, String> {
        let mut drifted = Vec::new();
        for binding in bindings {
            let Ok((button_id, modifier, usage)) = binding_target(binding) else {
                continue;
            };
            if self.holds(button_id, modifier, usage)? {
                continue;
            }
            if self.holds(button_id, modifier, usage)? {
                continue;
            }
            drifted.push(binding.control_id.clone());
        }
        Ok(drifted)
    }

    /// Control ids of [`NATIVE_BUTTONS`] that no longer act as their plain mouse
    /// button — Hypershift (`0x0c`), a key or a macro stored over them.
    pub fn drifted_native_buttons(&self) -> Result<Vec<&'static str>, String> {
        let mut drifted = Vec::new();
        for (control_id, button_id, value) in NATIVE_BUTTONS {
            if self.native_button_drifted(button_id, value)? {
                drifted.push(control_id);
            }
        }
        Ok(drifted)
    }

    /// Whether the device's stored record for `button_id` is the key `usage` with
    /// the modifier mask `modifier`, as this app writes it.
    fn holds(&self, button_id: u8, modifier: u8, usage: u8) -> Result<bool, String> {
        let held = self.read_button(button_id)?;
        Ok(held.category == CATEGORY_KEY && held.modifier == modifier && held.usage == usage)
    }

    /// Sends one command-class request and returns the reply payload, whose byte
    /// 0 carries the status. The device answers `busy` while it finishes an
    /// internal write, so a busy reply is retried instead of reported.
    fn command(&self, class: u8, command: u8, args: &[u8]) -> Result<[u8; PAYLOAD_LEN], String> {
        let mut payload = [0u8; PAYLOAD_LEN];
        payload[1] = COMMAND_TX;
        payload[5] = args.len() as u8;
        payload[6] = class;
        payload[7] = command;
        payload[8..8 + args.len()].copy_from_slice(args);
        payload[88] = checksum(&payload);
        for _ in 0..COMMAND_RETRIES {
            self.set_payload(&payload)?;
            std::thread::sleep(COMMAND_SETTLE);
            let reply = self.read_reply()?;
            match reply[0] {
                COMMAND_STATUS_OK => return Ok(reply),
                COMMAND_STATUS_BUSY => std::thread::sleep(COMMAND_BUSY_WAIT),
                status => {
                    return Err(format!(
                        "command {class:#04x}/{command:#04x} answered status {status:#04x}"
                    ));
                }
            }
        }
        Err(format!("command {class:#04x}/{command:#04x} stayed busy"))
    }

    /// Reads a feature reply without selecting a slot: command-class replies
    /// describe the device's own state, not a slot's content.
    fn read_reply(&self) -> Result<[u8; PAYLOAD_LEN], String> {
        self.get_payload(0)
    }

    fn set_payload(&self, payload: &[u8; PAYLOAD_LEN]) -> Result<(), String> {
        let mut buffer = [0u8; PAYLOAD_LEN + 1];
        buffer[0] = REPORT_ID;
        buffer[1..].copy_from_slice(payload);
        if !unsafe {
            HidD_SetFeature(self.handle, buffer.as_ptr() as *const core::ffi::c_void,
                            buffer.len() as u32)
        } {
            return Err(format!("HidD_SetFeature: {}", Error::last_os_error()));
        }
        Ok(())
    }

    fn get_payload(&self, slot: u8) -> Result<[u8; PAYLOAD_LEN], String> {
        let mut buffer = [0u8; PAYLOAD_LEN + 1];
        buffer[0] = REPORT_ID;
        buffer[1] = slot;
        if !unsafe {
            HidD_GetFeature(self.handle, buffer.as_mut_ptr() as *mut core::ffi::c_void,
                            buffer.len() as u32)
        } {
            return Err(format!("HidD_GetFeature: {}", Error::last_os_error()));
        }
        let mut payload = [0u8; PAYLOAD_LEN];
        payload.copy_from_slice(&buffer[1..]);
        Ok(payload)
    }
}

impl Drop for VendorDevice {
    fn drop(&mut self) {
        unsafe {
            use windows_sys::Win32::Foundation::CloseHandle;
            CloseHandle(self.handle);
        }
    }
}

fn build_record(slot: u8, button_id: u8, usage: u8, modifier: u8) -> [u8; PAYLOAD_LEN] {
    let mut payload = [0u8; PAYLOAD_LEN];
    payload[0] = 0x00;
    payload[1] = slot;
    payload[5..9].copy_from_slice(&RECORD_MARKER);
    payload[9] = button_id;
    // Which of the button's two records the firmware emits: only the `0x00`
    // layer reaches the OS, a `0x01` here stores a copy the mouse never sends.
    payload[10] = 0x00;
    payload[11] = 0x02;
    payload[12] = 0x02;
    payload[13] = modifier;
    payload[14] = usage;
    payload[88] = checksum(&payload);
    payload
}

fn record_matches(written: &[u8; PAYLOAD_LEN], read: &[u8; PAYLOAD_LEN]) -> bool {
    written[5..15] == read[5..15] && written[88] == read[88]
}

fn find_interface_path(vid: u16, pid: u16) -> Result<String, String> {
    let needle = format!("VID_{vid:04X}&PID_{pid:04X}");
    let classes_path: Vec<u16> = DEVICE_CLASSES
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut classes: HKEY = std::ptr::null_mut();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            classes_path.as_ptr(),
            0,
            KEY_READ,
            &mut classes,
        )
    };
    if status != 0 {
        return Err(format!("RegOpenKeyExW({DEVICE_CLASSES}): {}", Error::last_os_error()));
    }
    let result = find_class_key(classes, &needle);
    unsafe { RegCloseKey(classes) };
    result
}

fn find_class_key(classes: HKEY, needle: &str) -> Result<String, String> {
    let mut index = 0u32;
    loop {
        let mut name = [0u16; 64];
        let mut len = name.len() as u32;
        if unsafe { RegEnumKeyExW(classes, index, name.as_mut_ptr(), &mut len,
                                  std::ptr::null_mut(), std::ptr::null_mut(),
                                  std::ptr::null_mut(), std::ptr::null_mut()) }
            != 0
        {
            return Err("no HID interface class found in the registry".to_string());
        }
        let candidate = String::from_utf16_lossy(&name[..len as usize]);
        if candidate.eq_ignore_ascii_case(HID_INTERFACE_GUID) {
            return find_leaf(classes, candidate, needle);
        }
        index += 1;
    }
}

/// Opens the class key through the already-open `DeviceClasses` handle: its name
/// is only a subkey, so it cannot be resolved from the root of HKLM.
fn find_leaf(parent: HKEY, class: String, needle: &str) -> Result<String, String> {
    let mut class_key: HKEY = std::ptr::null_mut();
    let wide: Vec<u16> = class
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let status = unsafe { RegOpenKeyExW(parent, wide.as_ptr(), 0, KEY_READ, &mut class_key) };
    if status != 0 {
        return Err(format!(
            "RegOpenKeyExW({DEVICE_CLASSES}\\{class}): {}",
            Error::last_os_error()
        ));
    }
    let mut index = 0u32;
    let result = loop {
        let mut name = [0u16; 512];
        let mut len = name.len() as u32;
        if unsafe { RegEnumKeyExW(class_key, index, name.as_mut_ptr(), &mut len,
                                  std::ptr::null_mut(), std::ptr::null_mut(),
                                  std::ptr::null_mut(), std::ptr::null_mut()) }
            != 0
        {
            break Err("our device has no HID interface in the registry".to_string());
        }
        let leaf = String::from_utf16_lossy(&name[..len as usize]);
        if leaf.starts_with("##?#") && leaf[4..].to_uppercase().contains(needle) {
            break Ok(format!(r"\\?\{}", &leaf[4..]));
        }
        index += 1;
    };
    unsafe { RegCloseKey(class_key) };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Slot payloads captured from Synapse's own `GET_REPORT` replies, with the
    /// checksum byte it stored at offset 88.
    /// Slot, the ten record bytes the vendor traffic shows at offset 5, and the
    /// checksum byte the mouse itself stored for that slot.
    const CAPTURED: &[(u8, [u8; 10], u8)] = &[
        (0x07, [0x0a, 0x02, 0x0c, 0x01, 0x40, 0x01, 0x02, 0x02, 0x07, 0x68], 0x2b),
        (0x1c, [0x0a, 0x02, 0x0c, 0x01, 0x41, 0x01, 0x02, 0x02, 0x07, 0x69], 0x2b),
        (0x1d, [0x0a, 0x02, 0x0c, 0x01, 0x42, 0x01, 0x02, 0x02, 0x07, 0x6a], 0x2b),
        (0x1e, [0x0a, 0x02, 0x0c, 0x01, 0x43, 0x01, 0x02, 0x02, 0x07, 0x6b], 0x2b),
        (0x00, [0x0a, 0x02, 0x0c, 0x01, 0x44, 0x01, 0x02, 0x02, 0x07, 0x6c], 0x2b),
        (0x01, [0x0a, 0x02, 0x0c, 0x01, 0x45, 0x01, 0x02, 0x02, 0x07, 0x6d], 0x2b),
        (0x02, [0x0a, 0x02, 0x0c, 0x01, 0x46, 0x01, 0x02, 0x02, 0x07, 0x6e], 0x2b),
        (0x03, [0x0a, 0x02, 0x0c, 0x01, 0x47, 0x01, 0x02, 0x02, 0x07, 0x6f], 0x2b),
        (0x08, [0x0a, 0x02, 0x0c, 0x01, 0x48, 0x01, 0x02, 0x02, 0x07, 0x70], 0x3b),
        (0x04, [0x0a, 0x02, 0x0c, 0x01, 0x49, 0x01, 0x02, 0x02, 0x07, 0x71], 0x3b),
        (0x05, [0x0a, 0x02, 0x0c, 0x01, 0x4a, 0x01, 0x02, 0x02, 0x07, 0x72], 0x3b),
        (0x06, [0x0a, 0x02, 0x0c, 0x01, 0x4b, 0x01, 0x02, 0x02, 0x07, 0x73], 0x3b),
        (0x1b, [0x0a, 0x02, 0x0c, 0x01, 0x4b, 0x00, 0x02, 0x02, 0x00, 0x73], 0x3d),
        (0x1a, [0x02, 0x07, 0x80, 0x00, 0xb3, 0x00, 0x00, 0x00, 0x00, 0x00], 0x36),
        (0x18, [0x02, 0x07, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], 0x81),
    ];

    /// What an empty slot carries instead of a record.
    const EMPTY_REGION: [u8; 10] =
        [0x02, 0x07, 0x80, 0x00, 0xb3, 0x00, 0x00, 0x00, 0x00, 0x00];

    fn captured_payload(slot: u8, region: [u8; 10]) -> [u8; PAYLOAD_LEN] {
        let mut payload = [0u8; PAYLOAD_LEN];
        payload[0] = 0x02;
        payload[1] = slot;
        payload[5..15].copy_from_slice(&region);
        payload
    }

    #[test]
    fn checksum_matches_the_captured_vectors() {
        for (slot, region, expected) in CAPTURED {
            let payload = captured_payload(*slot, *region);
            assert_eq!(checksum(&payload), *expected, "slot {slot:#02x}");
        }
    }

    #[test]
    fn empty_slots_carry_the_vendor_marker_not_a_record() {
        let empty = captured_payload(0x1a, EMPTY_REGION);
        assert_ne!(&empty[5..9], &RECORD_MARKER);
        let written = captured_payload(0x1c, [0x0a, 0x02, 0x0c, 0x01, 0x41,
                                              0x01, 0x02, 0x02, 0x07, 0x69]);
        assert_eq!(&written[5..9], &RECORD_MARKER);
        assert_eq!(written[9], 0x41);
    }

    #[test]
    fn thumb_buttons_map_onto_the_vendor_slots() {
        assert_eq!(slot_for_button(0x40), Some(0x07));
        assert_eq!(slot_for_button(0x44), Some(0x00));
        assert_eq!(slot_for_button(0x4b), Some(0x06));
        assert_eq!(slot_for_button(0x34), None);
    }

    /// The whole chain the app depends on: config control id to the slot and
    /// usage the captured vendor traffic actually stored for that button.
    #[test]
    fn thumb_controls_resolve_onto_the_captured_slots_and_usages() {
        const EXPECTED: [(u8, u8, u8); 12] = [
            (0x40, 0x07, 0x68), (0x41, 0x1c, 0x69), (0x42, 0x1d, 0x6a), (0x43, 0x1e, 0x6b),
            (0x44, 0x00, 0x6c), (0x45, 0x01, 0x6d), (0x46, 0x02, 0x6e), (0x47, 0x03, 0x6f),
            (0x48, 0x08, 0x70), (0x49, 0x04, 0x71), (0x4a, 0x05, 0x72), (0x4b, 0x06, 0x73),
        ];
        for (index, (button_id, slot, usage)) in EXPECTED.iter().copied().enumerate() {
            let control = format!("thumb_{:02}", index + 1);
            assert_eq!(button_id_for_control(&control), Some(button_id), "{control}");
            assert_eq!(slot_for_button(button_id), Some(slot), "{control}");
            assert_eq!(usage_for_key(&format!("F{}", 13 + index)), Some(usage), "{control}");
        }
        assert_eq!(button_id_for_control("mouse_left"), None);
        assert_eq!(usage_for_key("F12"), None);
    }

    #[test]
    fn written_record_is_verified_against_its_own_echo() {
        let written = build_record(0x1c, 0x41, 0x69, 0x07);
        let mut echoed = written;
        echoed[0] = 0x02;
        assert!(record_matches(&written, &echoed));
        let mut stale = captured_payload(0x1c, EMPTY_REGION);
        stale[88] = checksum(&stale);
        assert!(!record_matches(&written, &stale));
    }

    /// Hardware only: opens the live vendor collection and writes the factory
    /// `F13`..`F24` mapping the capture recorded, so every write must be echoed
    /// by the mouse, and `verify` must then confirm all twelve bindings.
    /// Run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn writes_the_captured_mapping_into_the_live_device() {
        let device = VendorDevice::open(0x1532, 0x00B4)
            .expect("connect the Razer Naga V2 HyperSpeed to run this test");
        let bindings: Vec<OnboardBinding> = (0..12)
            .map(|index| OnboardBinding {
                control_id: format!("thumb_{:02}", index + 1),
                encoded_key: format!("F{}", 13 + index),
            })
            .collect();
        device
            .apply_bindings(&bindings)
            .expect("the mouse must echo every button write");
        let report = device
            .verify(&bindings)
            .expect("the mouse must confirm the bindings it stores");
        assert!(report.failed.is_empty(), "no thumb binding may fail to confirm");
        assert_eq!(report.confirmed.len(), bindings.len(), "every binding is confirmed");
    }

    /// Records as the device answered them: the right button after Synapse
    /// stored Hypershift on it (`0c 01 01`), and after it was written back.
    #[test]
    fn hypershift_on_the_trigger_is_not_a_mouse_button() {
        let parse = |bytes: [u8; 7]| ButtonRecord {
            category: bytes[3],
            modifier: bytes[5],
            usage: bytes[6],
        };
        let hypershift = parse([0x01, 0x02, 0x01, 0x0c, 0x01, 0x01, 0x00]);
        let right = parse([0x01, 0x02, 0x00, 0x01, 0x01, 0x02, 0x00]);
        let left = parse([0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00]);
        assert!(!hypershift.is_mouse_button(2));
        assert!(right.is_mouse_button(2));
        assert!(!left.is_mouse_button(2), "a left click on the right button is drift too");
        assert!(native_buttons().contains(&("mouse_right", 0x02, 2)));
        assert!(native_buttons().contains(&("mouse_4", 0x34, 4)));
    }

    #[test]
    fn mode_values_name_the_takeover() {
        assert_eq!(MODE_NORMAL, 0x00);
        assert_eq!(MODE_DRIVER, 0x03);
    }

    #[test]
    fn binding_target_accepts_f13_to_f24_on_key_buttons() {
        let binding = |control: &str, key: &str| OnboardBinding {
            control_id: control.to_string(),
            encoded_key: key.to_string(),
        };
        assert_eq!(binding_target(&binding("thumb_01", "F13")), Ok((0x40, 0x00, 0x68)));
        assert_eq!(binding_target(&binding("thumb_12", "F24")), Ok((0x4b, 0x00, 0x73)));
        // The top buttons, with the mask the hardware read back for `Alt+F23`.
        assert_eq!(binding_target(&binding("top_aux_01", "Alt+F23")), Ok((0x0b, 0x04, 0x72)));
        assert_eq!(
            binding_target(&binding("thumb_01", "Ctrl+Alt+Shift+F13")),
            Ok((0x40, 0x07, 0x68))
        );
        assert!(binding_target(&binding("mouse_left", "F13")).is_err());
        assert!(binding_target(&binding("thumb_01", "F12")).is_err());
    }

    /// The same filter `src/lib/onboard-mapping.ts` applies: standard layer and
    /// thumb controls only, because only those reach the mouse's own memory.
    #[test]
    fn thumb_bindings_keep_the_standard_layer_only() {
        use crate::config::{ControlId, EncoderMapping, Layer, MappingSource};
        let mapping = |control: &str, layer: Layer, key: &str| EncoderMapping {
            control_id: ControlId::new(control),
            layer,
            encoded_key: key.to_string(),
            source: MappingSource::Detected,
            verified: false,
        };
        let bindings = thumb_bindings(&[
            mapping("thumb_01", Layer::Standard, "F13"),
            mapping("thumb_01", Layer::Hypershift, "F14"),
            mapping("mouse_left", Layer::Standard, "F15"),
        ]);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].control_id, "thumb_01");
        assert_eq!(bindings[0].encoded_key, "F13");
    }
}
