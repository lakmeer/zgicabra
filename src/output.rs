
//
// DeltaConsumer
//
// Common interface for anything that turns Zgicabra's continuous signal
// state and discrete DeltaEvents into sound. OscOutput and rs::RsOutput both
// implement this, so main.rs can pick either one behind a single trait
// object rather than branching on which backend is active at every call
// site.
//

use crate::zgicabra::{DeltaEvent,SignalState};

pub trait DeltaConsumer {
    fn panic (&mut self);
    fn handle_signal (&mut self, signal: &SignalState);
    fn handle_event (&mut self, delta: &DeltaEvent);
}
