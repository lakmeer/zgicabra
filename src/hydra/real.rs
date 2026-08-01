
//
// Real Hydra backend
//
// Talks to actual Hydra hardware via the Sixense SDK. Only ever compiled on
// linux x86_64, since libsixense_x64.so is a Linux ELF binary.
//

use std::time::{Instant,Duration};
use std::thread::sleep;

use libc::c_int;

use super::ControllerFrame;

// How long to wait for a first frame before assuming no hardware is attached
// and letting the caller fall back to the mock backend.
const DETECT_TIMEOUT: Duration = Duration::from_secs(2);

#[link(name="sixense_x64")]
extern "C" {
    fn sixenseInit();
    fn sixenseExit();
    fn sixenseGetNewestData(which: c_int, data: *mut ControllerFrame);
}

// Returns false (without leaving the connection open) if no controller responds
// within DETECT_TIMEOUT.
pub fn try_start () -> bool {
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
            return false;
        }

        sleep(Duration::from_millis(10));
    }

    println!("✅");
    true
}

pub fn stop () {
    println!("Hydra::stop - closing down... ");
    unsafe { sixenseExit(); }
    println!("Hydra::stop - done.");
}

pub fn update (controllers: &mut [ ControllerFrame; 2 ]) {
    let mut frame = ControllerFrame::new();

    read_frame(0, &mut frame);
    controllers[(frame.which_hand - 1) as usize] = frame;

    read_frame(1, &mut frame);
    controllers[(frame.which_hand - 1) as usize] = frame;
}

fn read_frame (which: i32, frame_data: &mut ControllerFrame) {
    unsafe { sixenseGetNewestData(which, frame_data); }
}
