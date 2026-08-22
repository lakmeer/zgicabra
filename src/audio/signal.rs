
//
// SharedSignal: one atomic-backed copy of the live performance signal (see
// zgicabra::SignalState), built once in AudioOutput::new and cloned (cheap --
// each field is an Arc<AtomicU32> underneath, via fundsp's Shared) into
// Engine and every Voice. Nobody keeps a private snapshot copy any more --
// main.rs's hydra-rate run_engine_loop writes the latest values in with
// set(), and any audio-thread stage reads straight off the atomics via the
// field's own .value(), at whatever rate it needs, no broadcast step.
//

use fundsp::prelude64::*;

use crate::zgicabra::SignalState;

#[derive(Clone)]
pub struct SharedSignal {
    pub level:        Shared,
    pub bend:         Shared,
    pub filter:       Shared,
    pub fuzz:         Shared,
    pub width:        Shared,
    pub thump:        Shared,
    pub velocity:     Shared,
    pub acceleration: Shared,
}

impl SharedSignal {
    pub fn new () -> SharedSignal {
        SharedSignal {
            level:        shared(0.0),
            bend:         shared(0.0),
            filter:       shared(0.0),
            fuzz:         shared(0.0),
            width:        shared(0.0),
            thump:        shared(0.0),
            velocity:     shared(0.0),
            acceleration: shared(0.0),
        }
    }

    // Called once per hydra tick (main.rs's run_engine_loop) with the
    // latest control-thread snapshot.
    pub fn set (&self, signal: &SignalState) {
        self.level.set_value(signal.level);
        self.bend.set_value(signal.bend);
        self.filter.set_value(signal.filter);
        self.fuzz.set_value(signal.fuzz);
        self.width.set_value(signal.width);
        self.thump.set_value(signal.thump);
        self.velocity.set_value(signal.velocity);
        self.acceleration.set_value(signal.acceleration);
    }
}
