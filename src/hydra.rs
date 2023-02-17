
use std::time::{Instant,Duration};
use std::thread::sleep;

use crate::sixense;
use crate::sixense::ControllerFrame;


//
// HydraState
//
// Manages a block of memory in which we can record and manipulate incoming hydra data
//

pub struct HydraState {
    pub initialised: bool,
    pub timestamp:   Instant,
    pub timedelta:   Duration,
    pub temp_frame:  ControllerFrame,
    pub controllers: [ ControllerFrame; 2 ],
}

impl HydraState {
    pub fn new() -> HydraState {
        HydraState {
            initialised: false,
            timestamp: Instant::now(),
            timedelta: Duration::from_millis(0),
            temp_frame: ControllerFrame::new(),
            controllers: [ ControllerFrame::new(), ControllerFrame::new() ],
        }
    }
}



//
// Functions
// TODO: Learn what the correct thing is to do with the unsafes here
//

pub fn start (state: &mut HydraState) {
    print!("Hydra::start - init connection... ");
    unsafe { sixense::sixenseInit(); }
    state.initialised = true;
    println!("✅");

    print!("Hydra::start - awaiting first frame...");
    while state.temp_frame.which_hand == 0 {
        read_frame(0, &mut state.temp_frame);
        sleep(Duration::from_millis(10));
    }
    println!("✅");
}

pub fn stop (state: &mut HydraState) {
    println!("Hydra::stop - closing down... ");
    unsafe { sixense::sixenseExit(); }
    state.initialised = false;
    println!("Hydra::stop - done.");
}

pub fn update (state: &mut HydraState) {

    unsafe { sixense::sixenseGetNewestData(0, &mut state.temp_frame); }
    let hand = (state.temp_frame.which_hand - 1) as usize;
    state.controllers[hand] = state.temp_frame;

    unsafe { sixense::sixenseGetNewestData(1, &mut state.temp_frame); }
    let hand = (state.temp_frame.which_hand - 1) as usize;
    state.controllers[hand] = state.temp_frame;

    state.timedelta = Instant::now().duration_since(state.timestamp);
    state.timestamp = Instant::now();
}

pub fn read_frame (which: i32, frame_data: &mut ControllerFrame) {
    unsafe { sixense::sixenseGetNewestData(which, frame_data); }
}


