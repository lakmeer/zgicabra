
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
    pub level:  Shared,
    pub bend:   Shared,
    pub filter: Shared,
    pub width:  Shared,
    pub depth:  Shared,
    pub alpha:  Shared,
    pub omega:  Shared,
    pub vel:    Shared,
    pub acc:    Shared,
    pub env:    Shared, // Unique
}

impl SharedSignal {
    pub fn new () -> SharedSignal {
        SharedSignal {
            level:  shared(0.0),
            bend:   shared(0.0),
            filter: shared(0.0),
            width:  shared(0.0),
            depth:  shared(0.0),
            alpha:  shared(0.0),
            omega:  shared(0.0),
            vel:    shared(0.0),
            acc:    shared(0.0),
            env:    shared(0.0),
        }
    }

    pub fn set (&self, signal: &SignalState, cc_connected: bool) {
        self.level.set_value(signal.level);
        self.bend.set_value(signal.bend);
        self.depth.set_value(signal.depth);
        if cc_connected {
            if signal.filter != self.filter.value() { self.filter.set_value(signal.filter); }
            if signal.alpha   != self.alpha.value()   { self.alpha.set_value(signal.alpha); }
            if signal.width  != self.width.value()  { self.width.set_value(signal.width); }
            if signal.omega  != self.omega.value()  { self.omega.set_value(signal.omega); }
        } else {
            self.filter.set_value(signal.filter);
            self.alpha.set_value(signal.alpha);
            self.width.set_value(signal.width);
            self.omega.set_value(signal.omega);
        }
        self.vel.set_value(signal.vel);
        self.acc.set_value(signal.acc);
    }
}
