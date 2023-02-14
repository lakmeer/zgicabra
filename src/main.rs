
use termion;
use termion::screen::IntoAlternateScreen;
use std::io::{Write, Read, stdout};

mod sixense;
mod utils;

use sixense::HydraFrame;
use utils::format_frame;


//
// Helpers
//

fn new_blank_frame() -> HydraFrame {
    HydraFrame {
        pos: [0.0, 0.0, 0.0],
        rot_mat: [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
        joystick_x: 0.0,
        joystick_y: 0.0,
        trigger: 0.0,
        buttons: 0,
        sequence_number: 0,
        rot_quat: [0.0, 0.0, 0.0, 0.0],
        firmware_revision: 0,
        hardware_revision: 0,
        packet_type: 0,
        magnetic_frequency: 0,
        enabled: 0,
        controller_index: 0,
        is_docked: 0,
        which_hand: 0,
        hemi_tracking_enabled: 0,
    }
}



//
// Main
//

fn main() {

    // Array of 2 frames
    let mut temp = new_blank_frame();
    let mut frames = [new_blank_frame(), new_blank_frame()];

    unsafe {
        sixense::init();

        println!("Waiting for Hydra...");

        // Loop until we get a frame
        while temp.sequence_number == 0 {
            sixense::sixenseGetNewestData(0, &mut temp);
        }

        println!("First frame received");

        let mut screen = stdout().into_alternate_screen().unwrap();

        loop {

            // Read both hands, index by self-disclosed handedness (permits hand swapping)
            sixense::read_frame(0, &mut temp);
            frames[temp.which_hand as usize - 1] = temp;

            sixense::read_frame(1, &mut temp);
            frames[temp.which_hand as usize - 1] = temp;

            write!(screen, "{}L> {}", termion::cursor::Goto(1,1), format_frame(frames[0])).unwrap();
            write!(screen, "{}R> {}", termion::cursor::Goto(1,2), format_frame(frames[1])).unwrap();

            screen.flush().unwrap();

            std::thread::sleep(std::time::Duration::from_millis(10));

            if std::io::stdin().bytes().next().and_then(|result| result.ok()).is_some() {
                break;
            }
        }

        sixense::exit();
    }
}

