
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
    pub fuzz:   Shared,
    pub width:  Shared,
    pub thump:  Shared,
    pub vel:    Shared,
    pub acc:    Shared,
    pub aux:    Shared,
    pub env:    Shared, // Unique
}

impl SharedSignal {
    pub fn new () -> SharedSignal {
        SharedSignal {
            level:  shared(0.0),
            bend:   shared(0.0),
            filter: shared(0.0),
            fuzz:   shared(0.0),
            width:  shared(0.0),
            thump:  shared(0.0),
            vel:    shared(0.0),
            acc:    shared(0.0),
            aux:    shared(0.0),
            env:    shared(0.0),
        }
    }

    // Called once per hydra tick (main.rs's run_engine_loop) with the
    // latest control-thread snapshot. `cc_connected` is whether an 8-knob
    // controller is attached (see audio::cc_input) -- when it is, CC1-4
    // write filter/width/fuzz/thump straight into these same cells from the
    // audio thread (see build_stream), so wand tracking here only wins the
    // tick if it actually produced a new value; a static wand reading
    // leaves whatever the controller last set alone. When no controller is
    // connected, wand tracking always wins, same as before this existed.
    pub fn set (&self, signal: &SignalState, cc_connected: bool) {
        self.level.set_value(signal.level);
        self.bend.set_value(signal.bend);
        if cc_connected {
            if signal.filter != self.filter.value() { self.filter.set_value(signal.filter); }
            if signal.fuzz   != self.fuzz.value()   { self.fuzz.set_value(signal.fuzz); }
            if signal.width  != self.width.value()  { self.width.set_value(signal.width); }
            if signal.thump  != self.thump.value()  { self.thump.set_value(signal.thump); }
        } else {
            self.filter.set_value(signal.filter);
            self.fuzz.set_value(signal.fuzz);
            self.width.set_value(signal.width);
            self.thump.set_value(signal.thump);
        }
        self.vel.set_value(signal.vel);
        self.acc.set_value(signal.acc);
        self.aux.set_value(signal.aux);
    }
}
