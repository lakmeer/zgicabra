
use std::time::{Instant,Duration};
use std::thread::sleep;

use crate::sixense;
use crate::sixense::ControllerFrame;
use crate::history::History;

pub const HISTORY_WINDOW: usize = 20;


//
// HydraState
//
// Manages a block of memory in which we can record and manipulate incoming hydra data
//

pub struct HydraState {
    pub initialised:    bool,
    pub timestamp:      Instant,
    pub timedelta:      Duration,
    pub temp_frame:     ControllerFrame,
    pub frame_history:  [ History<ControllerFrame>; 2 ],
}

impl HydraState {
    pub fn new() -> HydraState {
        HydraState {
            initialised: false,
            timestamp: Instant::now(),
            timedelta: Duration::from_millis(0),
            temp_frame: ControllerFrame::new(),
            frame_history: [
                History::new(HISTORY_WINDOW),
                History::new(HISTORY_WINDOW),
            ]
        }
    }

    pub fn get_latest_frame(&self, controller_index: usize) -> Option<&ControllerFrame> {
        self.frame_history[controller_index].last()
    }

    pub fn get_nth_most_recent_frame(&self, controller_index: usize, nth: usize) -> Option<&ControllerFrame> {
        self.frame_history[controller_index].get_from_end(nth)
    }
}



//
// Functions
// TODO: Learn what the correct thing is to do with the unsafes here
//

pub fn start (state: &mut HydraState) {
    println!("Hydra::start - init connection... ");
    unsafe { sixense::sixenseInit(); }
    state.initialised = true;
    println!("Hydra::start - awaiting first frame...");

    let mut i = 0;
    while state.temp_frame.sequence_number == 0 && i < 1000 {
        read_frame(0, &mut state.temp_frame);
        sleep(Duration::from_millis(10));
        i += 1;
    }

    println!("Hydra::start - first frame received ✅");
}

pub fn stop (state: &mut HydraState) {
    println!("Hydra::stop - closing down... ");
    unsafe { sixense::sixenseExit(); }
    state.initialised = false;
    println!("Hydra::stop - done.");
}

pub fn update (state: &mut HydraState) {
    state.frame_history[read_frame_to_hand(0, &mut state.temp_frame) as usize].push(state.temp_frame);
    state.frame_history[read_frame_to_hand(1, &mut state.temp_frame) as usize].push(state.temp_frame);

    state.timedelta = Instant::now().duration_since(state.timestamp);
    state.timestamp = Instant::now();
}

pub fn read_frame (which: i32, frame_data: &mut ControllerFrame) {
    unsafe { sixense::sixenseGetNewestData(which, frame_data); }
}

pub fn read_frame_to_hand (which: i32, frame_data: &mut ControllerFrame) -> i32 {
    unsafe { sixense::sixenseGetNewestData(which, frame_data); }
    (frame_data.which_hand - 1) as i32
}

