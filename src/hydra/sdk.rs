
//
// SDK Hydra backend
//
// Talks to Hydra hardware via the Sixense SDK. Only compiled on linux
// x86_64 -- libsixense_x64.so is a Linux ELF binary, and the same SDK is
// fatally broken on modern macOS (see hid.rs).
//

use std::time::{Instant,Duration};
use std::thread::sleep;

use libc::{c_int, c_void};
use termion::input::{TermRead, Keys};
use termion::AsyncReader;

use super::{Backend, CbreakGuard, ControllerFrame};

// How long to wait for a first frame before assuming no hardware is attached
// and letting the caller fall back to the mock backend.
const DETECT_TIMEOUT: Duration = Duration::from_secs(2);

#[link(name="sixense_x64")]
extern "C" {
    fn sixenseInit();
    fn sixenseExit();
    fn sixenseGetNewestData(which: c_int, data: *mut ControllerFrame);

    // Not part of the public sixense.h API, but exported by the .so regardless.
    // USBDetector's background hotplug-rescan thread (find_devices() -> hid_close()
    // -> libusb_close()) races the USB read thread's libusb_handle_events() during
    // sixenseExit() teardown -- this is the confirmed cause of every SIGSEGV we've
    // caught (coredumpctl backtraces always land in one of these two threads,
    // fighting over the same libusb handle). We only ever have one Hydra plugged
    // in for the whole session, so we don't need live hot-plug detection of new
    // controllers -- stopping this thread right after our own detection succeeds
    // removes one side of the race entirely.
    #[link_name = "_ZN11USBDetector16get_instance_ptrEv"]
    fn usb_detector_instance () -> *mut c_void;
    #[link_name = "_ZN11USBDetector19stop_hotplug_threadEv"]
    fn usb_detector_stop_hotplug_thread (this: *mut c_void);
}

pub struct SdkBackend {
    keys: Keys<AsyncReader>,
    _cbreak_guard: CbreakGuard, // restores the terminal on drop
}

impl SdkBackend {
    // None (with the connection closed back down) if no controller responds
    // within DETECT_TIMEOUT.
    pub fn try_start () -> Option<SdkBackend> {
        print!("Hydra::start - init connection... ");
        unsafe { sixenseInit(); }
        println!("✅");

        print!("Hydra::start - awaiting first frame...");
        let deadline = Instant::now() + DETECT_TIMEOUT;
        let mut frame = ControllerFrame::new();

        while frame.which_hand == 0 {
            read_frame(0, &mut frame);

            if Instant::now() >= deadline {
                println!(" ❌ no hardware detected");
                unsafe { sixenseExit(); }
                return None;
            }

            sleep(Duration::from_millis(10));
        }

        println!("✅");

        unsafe {
            let detector = usb_detector_instance();
            if !detector.is_null() {
                usb_detector_stop_hotplug_thread(detector);
                println!("Hydra::start - stopped SDK hotplug thread (avoids sixenseExit() teardown race)");
            }
        }

        Some(SdkBackend {
            keys: termion::async_stdin().keys(),
            _cbreak_guard: CbreakGuard::enable(),
        })
    }
}

impl Backend for SdkBackend {
    fn update (&mut self, controllers: &mut [ ControllerFrame; 2 ]) {
        let mut frame = ControllerFrame::new();

        read_frame(0, &mut frame);
        controllers[(frame.which_hand - 1) as usize] = frame;

        read_frame(1, &mut frame);
        controllers[(frame.which_hand - 1) as usize] = frame;
    }

    // Trait default blocks on a canonical-mode stdin read; CbreakGuard puts
    // stdin in cbreak mode so this can poll non-blocking instead.
    fn should_quit (&mut self) -> bool {
        self.keys.next().is_some()
    }
}

impl Drop for SdkBackend {
    fn drop (&mut self) {
        println!("Hydra::stop - closing down... ");
        unsafe { sixenseExit(); }
        println!("Hydra::stop - done.");
    }
}

fn read_frame (which: i32, frame_data: &mut ControllerFrame) {
    unsafe { sixenseGetNewestData(which, frame_data); }
}
