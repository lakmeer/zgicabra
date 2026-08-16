
//
// HID Hydra backend
//
// Talks to Hydra hardware directly over USB HID, bypassing the Sixense SDK
// (a closed, unmaintained SDK that segfaults on modern macOS -- its frozen
// hidapi copy mishandles a failed IOHIDManager creation).
//
// The mode-switch handshake and 52-byte report layout are ported from
// Monado's drv_hydra (src/xrt/drivers/hydra/hydra_driver.c, BSL-1.0), the
// only known working reimplementation of this device's protocol. Only
// compiled on macOS x86_64.
//

use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};
use termion::AsyncReader;
use termion::input::{Keys, TermRead};

use super::{
    Backend, CbreakGuard, ControllerFrame, LEFT_HAND, RIGHT_HAND,
    BUTTON_1, BUTTON_2, BUTTON_3, BUTTON_4, BUTTON_BUMPER, BUTTON_HOME, BUTTON_JOYCLICK,
};

const VENDOR_ID:  u16 = 0x1532;
const PRODUCT_ID: u16 = 0x0300;

// How long to wait for the mode switch to take effect before giving up (once
// with no retry, once after resending the feature report).
const DETECT_TIMEOUT: Duration = Duration::from_secs(2);

// Only 4 bytes of the 91-byte feature report are ever non-zero: offset 6 is
// a fixed marker, 8/9 select gamepad(0x00)/motion(0x03) mode, and 89 is a
// matching end-marker (0x05 gamepad / 0x06 motion).
fn feature_report(motion: bool) -> [u8; 91] {
    let mut r = [0u8; 91];
    r[6] = 0x01;
    r[8] = 0x04;
    r[9] = if motion { 0x03 } else { 0x00 };
    r[89] = if motion { 0x06 } else { 0x05 };
    r
}

fn read_i16_le(buf: &[u8], off: usize) -> i16 {
    i16::from_le_bytes([buf[off], buf[off + 1]])
}

// Raw wire bits (Monado's hydra_button_bit) don't match this codebase's
// BUTTON_* bit layout (which mirrors the official Sixense SDK's 9-bit
// scheme), so translate by meaning rather than by bit position.
fn convert_buttons(raw: u8) -> u32 {
    let mut b = 0;
    if raw & (1 << 0) != 0 { b |= BUTTON_BUMPER; }
    if raw & (1 << 1) != 0 { b |= BUTTON_3; }
    if raw & (1 << 2) != 0 { b |= BUTTON_1; }
    if raw & (1 << 3) != 0 { b |= BUTTON_2; }
    if raw & (1 << 4) != 0 { b |= BUTTON_4; }
    if raw & (1 << 5) != 0 { b |= BUTTON_HOME; }   // "middle" button
    if raw & (1 << 6) != 0 { b |= BUTTON_JOYCLICK; }
    b
}

// buf is the 22-byte per-controller slice at offset 8 or 30 in the 52-byte
// motion report.
fn parse_controller(buf: &[u8], which_hand: u8, sequence: u8) -> ControllerFrame {
    // Raw wire units are millimeters, matching this codebase's convention
    // (Monado itself divides by 1000 for OpenXR's meters) -- no scaling here.
    const POS_SCALE:  f32 = 1.0;
    const ROT_SCALE:  f32 = 1.0 / 32768.0;  // int16 -> [-1, 1]
    const TRIG_SCALE: f32 = 1.0 / 255.0;

    let mut frame = ControllerFrame::new();

    frame.which_hand      = which_hand;
    frame.enabled          = 1;
    frame.sequence_number  = sequence;

    // Wire order is (x, z, y) with y negated, quat (w, x, y, z) with y/z
    // negated -- per Monado's axis fixup (hydra_driver.c).
    frame.pos = [
        read_i16_le(buf, 0)  as f32 * POS_SCALE,
        read_i16_le(buf, 4)  as f32 * -POS_SCALE,
        read_i16_le(buf, 2)  as f32 * POS_SCALE,
    ];
    frame.rot_quat = [
        read_i16_le(buf, 6)  as f32 * -ROT_SCALE,
        read_i16_le(buf, 8)  as f32 * ROT_SCALE,
        read_i16_le(buf, 12) as f32 * -ROT_SCALE, // swap q[2] and q[3]
        read_i16_le(buf, 10) as f32 * ROT_SCALE,
    ];
    frame.buttons     = convert_buttons(buf[14]);
    frame.joystick_x  = read_i16_le(buf, 15) as f32 * ROT_SCALE;
    frame.joystick_y  = read_i16_le(buf, 17) as f32 * ROT_SCALE;
    frame.trigger     = buf[19] as f32 * TRIG_SCALE;

    frame
}

// Polls for a 52-byte motion report until `timeout` elapses. True if one arrived.
fn wait_for_motion(data_hid: &HidDevice, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 64];
    while Instant::now() < deadline {
        if let Ok(52) = data_hid.read_timeout(&mut buf, 50) {
            return true;
        }
    }
    false
}

pub struct HidBackend {
    data_hid: HidDevice,
    command_hid: HidDevice,
    keys: Option<Keys<AsyncReader>>,
    _cbreak_guard: CbreakGuard, // restores the terminal on drop
}

impl HidBackend {
    // None if the device isn't present, or never starts streaming motion
    // reports within two detect-and-retry windows.
    pub fn try_start () -> Option<HidBackend> {
        print!("Hydra::Hid::start - opening HID interfaces... ");
        let api = HidApi::new().ok()?;

        let open = |interface: i32| -> Option<HidDevice> {
            api.device_list()
                .find(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID && d.interface_number() == interface)?
                .open_device(&api)
                .ok()
        };

        let data_hid    = open(0)?;
        let command_hid = open(1)?;
        println!("✅");

        print!("Hydra::Hid::start - awaiting first motion frame...");
        command_hid.send_feature_report(&feature_report(true)).ok()?;

        // Throwaway get-feature, part of the handshake (matches Monado).
        let mut throwaway = [0u8; 91];
        let _ = command_hid.get_feature_report(&mut throwaway);

        if !wait_for_motion(&data_hid, DETECT_TIMEOUT) {
            command_hid.send_feature_report(&feature_report(true)).ok();
            if !wait_for_motion(&data_hid, DETECT_TIMEOUT) {
                println!(" ❌ no hardware detected");
                return None;
            }
        }

        println!("✅");

        // termion::async_stdin() panics its worker thread (non-fatally, but
        // noisily) if /dev/tty can't be opened, e.g. no controlling terminal.
        // Probe first and skip the keyboard-quit feature rather than crash it.
        let keys = std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty").ok()
            .map(|_| termion::async_stdin().keys());

        Some(HidBackend {
            data_hid,
            command_hid,
            keys,
            _cbreak_guard: CbreakGuard::enable(),
        })
    }
}

impl Backend for HidBackend {
    fn update (&mut self, controllers: &mut [ ControllerFrame; 2 ]) {
        // A single read per tick with a small positive timeout (0ms/purely
        // non-blocking hangs indefinitely). hidapi's queue self-caps at 30
        // reports, so staleness stays bounded without draining it ourselves.
        let mut buf = [0u8; 64];
        match self.data_hid.read_timeout(&mut buf, 5) {
            Ok(52) => {
                controllers[0] = parse_controller(&buf[8..30],  LEFT_HAND,  buf[7]);
                controllers[1] = parse_controller(&buf[30..52], RIGHT_HAND, buf[7]);
            },
            Ok(n)  => crate::dbg!("Hydra::Hid::update - hid ok-but-wrong-size {n}"),
            Err(e) => crate::dbg!("Hydra::Hid::update - hid err {e}"),
        }
    }

    // Trait default blocks on a canonical-mode stdin read (waits for Enter);
    // this backend puts stdin in cbreak mode via CbreakGuard so a non-blocking
    // check works instead.
    fn should_quit (&mut self) -> bool {
        self.keys.as_mut().is_some_and(|keys| keys.next().is_some())
    }
}

impl Drop for HidBackend {
    fn drop (&mut self) {
        println!("Hydra::Hid::stop - closing down... ");
        // Leave the device the way we found it (gamepad mode) for whatever's next.
        self.command_hid.send_feature_report(&feature_report(false)).ok();
        println!("Hydra::Hid::stop - done.");
    }
}
