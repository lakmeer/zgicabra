// Standalone probe: does modern hidapi (unlike the frozen 2013 copy baked
// into libsixense) survive talking to the Hydra on this macOS at all?

use hidapi::HidApi;

const VENDOR_ID:  u16 = 0x1532;
const PRODUCT_ID: u16 = 0x0300;

// Feature reports + data layout ported from Monado's drv_hydra
// (src/xrt/drivers/hydra/hydra_driver.c, BSL-1.0), the only known working
// reimplementation of this handshake outside the dead Sixense SDK.

// Only 4 bytes of the 91-byte report are ever non-zero: offset 6 is a fixed
// marker, 8/9 select gamepad(0x00)/motion(0x03) mode, and 89 is a matching
// end-marker (0x05 gamepad / 0x06 motion).
fn feature_report(motion: bool) -> [u8; 91] {
    let mut r = [0u8; 91];
    r[6] = 0x01;
    r[8] = 0x04;
    r[9] = if motion { 0x03 } else { 0x00 };
    r[89] = if motion { 0x06 } else { 0x05 };
    r
}

struct Controller {
    pos: [f32; 3],
    quat: [f32; 4],
    buttons: u8,
    js: [f32; 2],
    trigger: f32,
}

fn read_i16_le(buf: &[u8], off: usize) -> i16 {
    i16::from_le_bytes([buf[off], buf[off + 1]])
}

// buf is the 22-byte per-controller slice (offset 8 or 30 into the 52-byte report).
fn parse_controller(buf: &[u8]) -> Controller {
    const POS_SCALE:  f32 = 0.001;          // mm -> m
    const ROT_SCALE:  f32 = 1.0 / 32768.0;  // int16 -> [-1, 1]
    const TRIG_SCALE: f32 = 1.0 / 255.0;

    Controller {
        // Raw wire order (x, z, y per Monado -- axis fixup not applied here,
        // this probe just needs to see plausible numbers move).
        pos: [
            read_i16_le(buf, 0) as f32 * POS_SCALE,
            read_i16_le(buf, 2) as f32 * POS_SCALE,
            read_i16_le(buf, 4) as f32 * POS_SCALE,
        ],
        quat: [
            read_i16_le(buf, 6)  as f32 * ROT_SCALE, // w
            read_i16_le(buf, 8)  as f32 * ROT_SCALE, // x
            read_i16_le(buf, 10) as f32 * ROT_SCALE, // y
            read_i16_le(buf, 12) as f32 * ROT_SCALE, // z
        ],
        buttons: buf[14],
        js: [
            read_i16_le(buf, 15) as f32 * ROT_SCALE,
            read_i16_le(buf, 17) as f32 * ROT_SCALE,
        ],
        trigger: buf[19] as f32 * TRIG_SCALE,
    }
}

// Buttons per the Monado-documented bitmask; bit 7 is unlabeled -- it sits
// on constantly at rest in our own captures, likely an idle/status flag.
const BUTTON_NAMES: [(u8, &str); 7] = [
    (1 << 2, "1"), (1 << 3, "2"), (1 << 1, "3"), (1 << 4, "4"),
    (1 << 5, "MID"), (1 << 0, "BUMP"), (1 << 6, "JOY"),
];

fn bar_frac(t: f32, width: usize) -> String {
    let pos = (t.clamp(0.0, 1.0) * (width - 1) as f32).round() as usize;
    let mut s = String::with_capacity(width + 2);
    s.push('[');
    for i in 0..width {
        s.push(if i == pos { '#' } else { '-' });
    }
    s.push(']');
    s
}

// v centered on 0, spanning [-range, range].
fn bar(v: f32, range: f32, width: usize) -> String {
    bar_frac((v + range) / (2.0 * range), width)
}

// v already in [0, 1] (trigger).
fn bar01(v: f32, width: usize) -> String {
    bar_frac(v, width)
}

fn print_wand(label: &str, c: &Controller) {
    let mag = (c.quat[0] * c.quat[0] + c.quat[1] * c.quat[1] + c.quat[2] * c.quat[2] + c.quat[3] * c.quat[3]).sqrt();
    println!("{label}");
    println!("  pos   x {} {:+.3}", bar(c.pos[0], 0.5, 20), c.pos[0]);
    println!("        y {} {:+.3}", bar(c.pos[1], 0.5, 20), c.pos[1]);
    println!("        z {} {:+.3}", bar(c.pos[2], 0.5, 20), c.pos[2]);
    println!("  quat  w{:+.2} x{:+.2} y{:+.2} z{:+.2}  (mag {:.3})", c.quat[0], c.quat[1], c.quat[2], c.quat[3], mag);
    let pressed: Vec<&str> = BUTTON_NAMES.iter().filter(|(bit, _)| c.buttons & bit != 0).map(|(_, name)| *name).collect();
    println!("  btn   [{}]  (raw {:#04x})", pressed.join(" "), c.buttons);
    println!("  joy   x {:+.2} y {:+.2}      trig {} {:.2}", c.js[0], c.js[1], bar01(c.trigger, 10), c.trigger);
}

fn open(api: &HidApi, interface: i32) -> hidapi::HidDevice {
    api.device_list()
        .find(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID && d.interface_number() == interface)
        .unwrap_or_else(|| panic!("interface {interface} not found"))
        .open_device(api)
        .unwrap_or_else(|e| panic!("failed to open interface {interface}: {e}"))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let api = HidApi::new().expect("HidApi::new failed");

    println!("Devices matching {VENDOR_ID:04x}:{PRODUCT_ID:04x}:");
    for info in api.device_list().filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID) {
        println!(
            "  interface {} - path {:?} - usage_page 0x{:04x} usage 0x{:04x}",
            info.interface_number(),
            info.path(),
            info.usage_page(),
            info.usage(),
        );
    }
    println!();

    match args.get(1).map(String::as_str) {
        // `feature <interface> <report_id>` -- dump the current feature report as-is.
        Some("feature") => {
            let interface: i32 = args[2].parse().expect("bad interface");
            let report_id: u8  = args[3].parse().expect("bad report id");
            let device = open(&api, interface);

            let mut buf = [0u8; 256];
            buf[0] = report_id;
            match device.get_feature_report(&mut buf) {
                Ok(n) => {
                    println!("feature report {report_id} on interface {interface}: {n} bytes");
                    println!("{:?}", &buf[..n]);
                },
                Err(e) => println!("get_feature_report failed: {e}"),
            }
        },
        // `motion` -- send the mode-switch feature report on interface 1,
        // then read+decode 52-byte motion reports from interface 0.
        Some("motion") => {
            let data_hid    = open(&api, 0);
            let command_hid = open(&api, 1);

            println!("Sending motion-mode feature report...");
            command_hid.send_feature_report(&feature_report(true)).expect("send_feature_report failed");

            // Throwaway get-feature, matching Monado's handshake.
            let mut throwaway = [0u8; 91];
            let _ = command_hid.get_feature_report(&mut throwaway);

            println!("Waiting for 52-byte motion reports (move the wands)...\n");

            let mut buf = [0u8; 64];
            let mut got_motion = false;
            for attempt in 0..60 {
                match data_hid.read_timeout(&mut buf, 200) {
                    Ok(52) => {
                        got_motion = true;
                        let c0 = parse_controller(&buf[8..30]);
                        let c1 = parse_controller(&buf[30..52]);
                        println!(
                            "seq={:3} | L pos=({:+.3},{:+.3},{:+.3}) quat=({:+.2},{:+.2},{:+.2},{:+.2}) btn={:#04x} js=({:+.2},{:+.2}) trig={:.2} | R pos=({:+.3},{:+.3},{:+.3}) btn={:#04x} trig={:.2}",
                            buf[7],
                            c0.pos[0], c0.pos[1], c0.pos[2],
                            c0.quat[0], c0.quat[1], c0.quat[2], c0.quat[3],
                            c0.buttons, c0.js[0], c0.js[1], c0.trigger,
                            c1.pos[0], c1.pos[1], c1.pos[2],
                            c1.buttons, c1.trigger,
                        );
                    },
                    Ok(n) if n > 0 => println!("[{attempt}] unexpected {n}-byte report: {:?}", &buf[..n]),
                    Ok(_) => {}, // timeout, keep polling
                    Err(e) => println!("[{attempt}] read error: {e}"),
                }

                // Retry the feature-report handshake if nothing's come through yet.
                if !got_motion && attempt == 25 {
                    println!("(no motion reports yet, resending feature report...)");
                    command_hid.send_feature_report(&feature_report(true)).expect("resend failed");
                }
            }

            if !got_motion {
                println!("\nNever saw a 52-byte report -- mode switch didn't take.");
            }
        },
        // `watch` -- live-redrawing dashboard, runs until Ctrl+C.
        Some("watch") => {
            let data_hid    = open(&api, 0);
            let command_hid = open(&api, 1);

            command_hid.send_feature_report(&feature_report(true)).expect("send_feature_report failed");
            let mut throwaway = [0u8; 91];
            let _ = command_hid.get_feature_report(&mut throwaway);

            let mut buf = [0u8; 64];
            let mut c0 = Controller { pos: [0.0; 3], quat: [0.0, 0.0, 0.0, 0.0], buttons: 0, js: [0.0; 2], trigger: 0.0 };
            let mut c1 = Controller { pos: [0.0; 3], quat: [0.0, 0.0, 0.0, 0.0], buttons: 0, js: [0.0; 2], trigger: 0.0 };
            let mut seq = 0u8;
            let mut resent = false;

            loop {
                match data_hid.read_timeout(&mut buf, 100) {
                    Ok(52) => {
                        seq = buf[7];
                        c0 = parse_controller(&buf[8..30]);
                        c1 = parse_controller(&buf[30..52]);
                    },
                    _ => {
                        if !resent {
                            command_hid.send_feature_report(&feature_report(true)).ok();
                            resent = true;
                        }
                    },
                }

                print!("\x1B[2J\x1B[H"); // clear screen, cursor home
                println!("Razer Hydra -- live (Ctrl+C to quit)   seq={seq}\n");
                print_wand("LEFT", &c0);
                println!();
                print_wand("RIGHT", &c1);
                use std::io::Write;
                std::io::stdout().flush().ok();
            }
        },
        // `read <interface>` (default interface 1) -- dump 10 interrupt reports.
        _ => {
            let interface: i32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
            let device = open(&api, interface);

            println!("Reading 10 reports on interface {interface} (5s timeout each) -- move/press the wand now...");

            let mut buf = [0u8; 64];
            for i in 0..10 {
                match device.read_timeout(&mut buf, 5000) {
                    Ok(n) => println!("[{i}] {n} bytes: {:?}", &buf[..n]),
                    Err(e) => println!("[{i}] read error: {e}"),
                }
            }
        },
    }
}
