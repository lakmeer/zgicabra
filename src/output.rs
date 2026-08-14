
// Common interface for anything that turns Zgicabra's signal state and
// DeltaEvents into sound (OscOutput, audio::AudioOutput) -- lets main.rs
// pick either backend behind one trait object.

use crate::zgicabra::{DeltaEvent,SignalState};

pub trait DeltaConsumer {
    fn panic (&mut self);
    fn handle_signal (&mut self, signal: &SignalState);
    fn handle_event (&mut self, delta: &DeltaEvent);
}
