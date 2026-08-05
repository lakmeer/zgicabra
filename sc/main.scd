/*
Minimal proof-of-concept patch for zgicabra's stdin-driven SuperCollider
backend (see src/sc.rs). One persistent saw-wave voice, controlled entirely
through the ~noteOn/~noteOff/~setFilter/~shutdown functions below.

zgicabra spawns a bare `sclang` (no file argument, so it stays in
interactive stdin-reading mode) and writes this file's contents into its
stdin as a single flattened line, followed by further one-line commands
that call the functions defined here. sclang's stdin reader evaluates one
line at a time, so this file must stay flattenable to one line -- no `//`
line comments (they'd swallow everything flattened onto the same line
after them); use only this delimited block-comment style.
*/

(
"Booting zgicabra SC backend...".postln;

s = Server.default;

~shutdown = {
	"Shutting down zgicabra SC backend...".postln;
	s.quit;
	AppClock.sched(0.3, { 0.exit; nil });
};

s.waitForBoot({
	SynthDef(\zgicabraSaw, { |out=0, freq=220, cutoff=4000, gate=0, amp=0.3|
		var env = EnvGen.kr(Env.asr(0.01, 1, 0.3), gate, doneAction: 0);
		var sig = Saw.ar(freq ! 2);
		sig = RLPF.ar(sig, cutoff.clip(40, 18000), 0.3);
		Out.ar(out, sig * env * amp);
	}).add;

	s.sync;

	~voice = Synth(\zgicabraSaw);

	~noteOn = { |note| ~voice.set(\freq, note.midicps, \gate, 1); };
	~noteOff = { ~voice.set(\gate, 0); };
	~setFilter = { |val| ~voice.set(\cutoff, val.linexp(0, 1, 100, 8000)); };

	"ZGICABRA_READY".postln;
});
)
