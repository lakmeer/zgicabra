# AGENTS.md

| ⚠️ Full FunDSP docs: `ref/fundsp.md`. FunDSP builtins list: `ref/fundsp-modules.md`.


Working notes for agents touching this codebase. Read before touching
`src/audio/`, `src/zgicabra.rs`, or `src/ui/`.

Do not touch `src/zgicabra.rs` unless asked — it's the core instrument
logic, under the developer's direct control.

## Project Style Guide

- ⚠️ Comments should be minimal and necessary.

## What this is

A Rust synth/controller app built around a Razer Hydra-style two-wand
controller ("Sixense Hydra"). Wand motion/triggers/buttons drive a live
audio engine, rendered live to a terminal UI (`src/ui/`, termion +
drawille — no GUI/imgui path exists any more). A mock backend stands in
for keyboard/mouse/MIDI when no real hardware is attached
(`src/hydra/mock.rs`).

Entry point: `src/main.rs`. CLI flags (`tools::parse_args`): `--debug` to
suppress the terminal UI and print verbose diagnostics to stderr instead,
`--test` to run the audio self-test (see "Debug logging" below).

## Hardware

The Sixense Hydra (Razer Hydra) is a discontinued two-handed motion
controller; its SDK is adapted here into a performance instrument. The
controller is "Hydra", the instrument is "zgicabra" (Lojban; "a musical
apparatus"). An 8-knob MIDI CC controller (fixed AKAI-style layout, see
`src/audio/cc_input.rs`) is a second, optional live-tuning input.

## Platform notes

Two machines, different roles — confirm which one you're on.

### Performance box
- MusNix (NixOS) on a Lenovo NUC, i5, x86_64, no flakes, very limited CPU.
- Runs headless on stage, boots straight to the instrument; no monitor or
  keyboard guaranteed.

| ⚠️ Do not read or write `/etc/nixos/configuration.nix` on this machine —
| show the user the commands and let them run it.
| ⚠️ Static linking is an architectural goal: one binary, no external runtime deps.

**Performance-mode launch**: both `sys/config.nix`'s boot service and
`bin/perform` (dev-box vt3 smoke test) run zgicabra via
`kmscon --login -- bin/zgicabra-launch`, never the raw binary. `--login`
wipes the exec'd child's environment entirely (even `PATH`) — confirmed by
capture, not assumption — so anything the real binary needs (`PATH`,
`XDG_RUNTIME_DIR`, `PIPEWIRE_RUNTIME_DIR` for `cpal`'s ALSA/PipeWire
backend) is rebuilt inside `zgicabra-launch` right before `exec`, not set
upstream of `kmscon`. Missing `XDG_RUNTIME_DIR` surfaces as `snd_pcm_open`
failing with `Host is down` (can't reach the PipeWire socket). The box
also never logs in, so `users.users.zgicabra.linger` (`sys/config.nix`) is
what keeps a PipeWire session running at all for that wrapper to reach. If
you touch either launch path, keep them pointed at `zgicabra-launch`, not
the binary.

### Testing box
- MacBook Pro, USB-C only, macOS x86_64. Hydra hardware isn't well
  supported here — `src/hydra/mock.rs` stands in. More ergonomic for
  development.

## Process / thread model

- `src/hydra/` backends: `sdk.rs` (Linux, real Sixense SDK), `hid.rs`
  (macOS, raw USB HID), `mock.rs` (keyboard/mouse/MIDI stand-in, active
  whenever no real backend connects). All implement the `Backend` trait
  (`mod.rs`).
- **Engine loop** (`main.rs::run_engine_loop`, runs on the main thread —
  there is no separate GUI thread any more): polls hydra → derives
  `Zgicabra` state (`zgicabra::update`) → emits `DeltaEvent`s plus a
  continuous `SignalState` → draws the terminal UI (`ui::draw_all`,
  skipped under `--debug`) → feeds both straight to `audio::AudioOutput`
  (the only output backend).
- **Audio thread**: cpal owns a real-time callback thread running the DSP
  graph (`audio::build_stream`). Never blocks or allocates contentiously.
  Also drains the CC-input ring buffer and calls each selected voice's
  `on_block_start` once per callback block, before the per-sample tick
  loop.
- **`--test` self-test** (`main.rs::run_self_test`): holds a synthetic A4
  note, captures ~0.1s of raw cpal output via `AudioCapture`, reports
  PASS/FAIL on non-zero signal — isolates "engine produces no signal" from
  "OS/device routing is broken". Runs on its own thread, signals the main
  loop to quit via a shared `AtomicBool` when done.

## Cross-thread communication

No mpsc/crossbeam channel and no `Arc<Mutex<_>>` for **engine parameters**
(control values crossing from the main/hydra thread into the real-time
audio thread) anywhere. Every such value is one of:

- **`fundsp::shared::Shared`** — `Arc`-wrapped atomic f32 cell,
  `.value()`/`.set_value()`. Used for nearly everything: note freq/gate,
  every live-tunable param, voice selection. `audio::signal::SharedSignal`
  bundles one `Shared` per `zgicabra::SignalState` field (plus `env`,
  audio-thread-only) and is cloned into `Engine` and every `Voice`.
- **`tools::AtomicF32`** — hand-rolled lock-free f32 cell, used where
  `Shared` isn't already in scope (mock hydra stick position, wand
  telemetry, MIDI CC/pitch-bend).
- **`Arc<AtomicBool>`** for flags (quit signal, trigger/button state,
  per-voice "dirty, needs persisting" flags).
- **`rtrb`** wait-free SPSC ring buffer for the one *event* stream that
  isn't a live-updating value: MIDI CC messages from the second, dedicated
  CC-tuning connection (`audio::cc_input::CcInput`) — a ring buffer, not a
  `Shared`, because CC messages are discrete events the audio thread must
  not miss, popped once per cpal callback block.

## fundsp: what's actually in play

`fundsp` (v0.23) has two traits — know which you're implementing:

- **`AudioNode`** (`audionode.rs`): compile-time-sized. `type
  Inputs`/`type Outputs` (typenum sizes: `U0`, `U1`, `U2`, ...), `fn
  tick(&mut self, input: &Frame<f32, Self::Inputs>) -> Frame<f32,
  Self::Outputs>`, requires `Self: Clone`. `An<X>` wraps one for
  `>>`/`|` combinator syntax and gets a blanket `AudioUnit` impl for free.
- **`AudioUnit`** (`audiounit.rs`): dynamic, runtime-sized, object-safe —
  `fn tick(&mut self, input: &[f32], output: &mut [f32])`. `Box<dyn
  AudioUnit>` is `Clone` (via `DynClone`), so it's safe to hold as a field
  on a `#[derive(Clone)]` struct (used for the crusher, reverb tail, per-
  voice NAM chains, etc).

Rule of thumb: implement `AudioNode` for a self-contained hand-written
node (see `NamNode` in `nam_node.rs`); reach for `Box<dyn AudioUnit>` for
type erasure or to wrap nam-rs's non-`Clone` `Model`.

### `prelude64` gotchas

`use fundsp::prelude64::*;` is what every file under `src/audio/`
imports — a non-generic re-export monomorphized to `f64`:

- Functions generic in `fundsp::prelude` (`sine::<F: Real>()`, `pink::<F:
  Float>()`, ...) are plain non-generic functions here — call `sine()`,
  not `sine::<f64>()`.
- Functions whose generic signature takes `f64` args (`reverb_stereo(
  room_size: f64, ...)`) take **`f32`** in `prelude64`.
- When unsure of a function's real signature, grep the installed crate
  source (`~/.cargo/registry/src/*/fundsp-0.23.0/src/prelude64.rs`,
  `prelude.rs`, `oscillator.rs`/`wavetable.rs`/`noise.rs`/`audionode.rs`)
  rather than guessing.

### Other fundsp facts

- `get_mono()`/`filter_mono()` are default methods on `AudioUnit` itself —
  callable directly, no extra imports.
- Every hand-written `AudioNode` here picks a private `const ID: u64` in
  the `0x7A_xx` range (fundsp's own IDs stay well below it) —
  `nam_node.rs` (`0x7A_70`), `crusher.rs` (`0x7A_C1`). Grep `const ID: u64`
  before picking a new one. `sample.rs` uses a bare `9001` — an outlier,
  don't copy it; use `0x7A_xx`.
- `NAM_SAMPLE_RATE = 48_000` is forced for the output device because every
  NAM model in `nam/` was captured at that rate.

## Audio engine architecture (`src/audio/`)

A hand-driven `Engine` struct (`engine.rs`, private — `AudioOutput`,
`src/audio/mod.rs`, is the public handle), ticked once per sample from
cpal's callback (`mod.rs::build_stream`):

- Ticks every `Voice` in `Engine::voices` in parallel (only the selected
  one renders; the rest get `on_silence()`), plus `main_sub`/`dry_sub`
  (plain triangle/sine sub-oscillators, not `Voice`s) gated by a shared
  `adsr_live` envelope.
- Sums the result, soft-clips (`tanh`), runs it through a global reverb
  (`reverb_stereo`, bypassable) and a stereo-linked safety limiter
  (bypassable), applies master volume, clamps to `[-1, 1]`.
- `dry_sub` bypasses reverb/limiter, mixed in post.
- Feeds an output peak/RMS meter (`fundsp::monitor`) off the final signal,
  read by the UI's level meters.

Each voice owns and runs **its own** NAM amp-sim chain internally (see
`nam_node.rs`/`nam_graph.rs`/`nam.rs` below) — there is no engine-level
fixed amp stage any more; `NamStage`'s old block-driven cycling shape
(`nam.rs`) is present but unused/dead, don't assume it's wired anywhere.

### NAM (neural amp sim), split three ways

- **`nam.rs`**: model loading/selection. NAM `*.nam` model files are
  embedded into the binary at compile time (no reliable runtime folder
  next to the systemd-launched binary on the performance box) and
  discovered/loaded into `NamModelSlot`s (`Arc<Mutex<Model>>` — required
  only because nam-rs's `Model` isn't `Clone`; each slot is touched by one
  audio-thread owner, never contended).
- **`nam_node.rs`**: `NamNode`, the one hand-written `AudioNode` — pure
  model inference, 1 in/1 out. fundsp hands a node ≤64 samples per call,
  far below what nam-rs wants efficiently, so this node buffers into a
  `window`-sized chunk and runs one `process_buffer` per window (latency =
  `window` samples, decoupled from cpal's callback size).
- **`nam_graph.rs`**: everything else (crossover split, gain staging, DC
  blocking, dry/wet blend, mid/side collapse) as ordinary fundsp graph
  expressions/combinators around `NamNode` — routing, not a hand-written
  node, because routing is what combinators are for.

### One module per voice, `#[derive(Voice)]` generates the boilerplate

`voice.rs` holds the shared contract: the `Voice` trait (object-safe,
what `Engine` holds as `Box<dyn Voice>`), the `VoiceDsp` trait (what an
author actually implements), `ViewFields`, `ThumpMod`, `KnobPickup`. Each
concrete voice's struct + `impl VoiceDsp` lives in its own file:
`ReeseVoice` (`reese.rs`), `GrowlVoice` (`growl.rs`, wraps `WavetableGen`),
`BasicVoice` (`basic.rs`), `SwarmVoice` (`swarm.rs`). `mod.rs` re-exports
each generated `*View` so callers import from `crate::audio`.

**Four voice slots** (`VOICE_COUNT = 4`, `engine.rs`): `voice_a` (Reese,
`index = 0`, detuned-unison-saw bass), `voice_b` (Growl, `index = 1`,
wavetable + NAM), `voice_c` (Basic, `index = 2`, four builtin oscillators
— test voice, not a real patch), `voice_d` (Swarm, `index = 3`, 5
orbiting oscillators + mid/side NAM). `Engine::voices` order must match
`index`; `Engine::voice_names()` / the CC5-8 knob-selection scheme both
key off it.

## The `Voice` macro

`#[derive(Voice)]` (`voice-macro/`, `zgicabra_voice_macro::Voice`)
generates, from one annotated struct: the read-only `*View` struct +
`view()`, `fields()`/`apply()` (persistence), the `knob_*`/
`selected_knob`/`set_knob_value` methods, `UI_RANGES`/`KNOB_NAMES`/
`KNOB_RANGES` tables, an optional `new()`, and `impl Voice for` (index,
name, `set_sample_rate`, and the `tick()` wrapper that applies pitch-thump
before calling your `render()`). You write: the annotated struct + `impl
VoiceDsp` (`render`, optionally `on_block_start`/`on_silence`/
`on_set_sample_rate`).

Required fields: `sig: SharedSignal` always; `thump: ThumpMod` unless this
voice never pitch-thumps; `selected_knob: Shared` + `knob_pickup:
KnobPickup` if it has any `#[knob]` field.

Field attributes:
- `#[knob(range=1.0..10.0, set=|v| 1.0+v*9.0, default=1.0)]` — a
  persisted, CC-tunable `Shared`, collected in struct-declaration order
  into this voice's knob list. `set` maps a raw `0..1` CC level to the
  real value (identity if omitted); `default` is only needed for a
  generated `new()`.
- `#[live(range=0.0..5.0)]` — a `Shared` written by `render()`, visible in
  `*View`/`UI_RANGES`, never persisted or CC-set. Seeded to `shared(0.0)`.
- `#[node]` / `#[node(each)]` / `#[node(init = sine())]` — a sub-node
  whose `set_sample_rate` is forwarded (`each` for an array/Vec of them).
  `init` only matters for a generated `new()`.
- `#[view]` — a non-`Shared` field that still belongs in the `*View`.
- Unannotated fields are internal state, excluded from `*View` and
  `set_sample_rate` — only allowed under `#[voice(new = manual)]`.

Struct-level: `#[voice(index = N, label = "Name")]` required;
`new = manual` when construction needs pre-init logic (a generated `new()`
can't handle plain/`#[view]` fields, or a `#[node]`/`#[knob]` missing
`init`/`default`); `thump = manual` when the voice applies thump itself
inside `render()` instead of having the wrapper do it first (`SwarmVoice`,
which chases an origin point before thumping it).

Minimal shape (see `basic.rs` for the simplest real example, `reese.rs`
for a fully-loaded one):

```rust
#[derive(Clone, Voice)]
#[voice(index = 2, label = "Basic")]
pub struct BasicVoice {
    #[node] osc: An<Sine<f64>>,
    #[knob(range = 0.0..1.0, default = 0.5)] pub level_input: Shared,
    selected_knob: Shared,
    knob_pickup:   KnobPickup,
    thump: ThumpMod,
    sig:   SharedSignal,
}

impl VoiceDsp for BasicVoice {
    fn render (&mut self, freq: f32, _thump_mult: f32) -> Frame<f32, U2> {
        let s = self.osc.filter_mono(freq) * self.level_input.value() * self.sig.env.value();
        Frame::from([s, s])
    }
}
```

`render()` is responsible for reading `self.sig.env` and shaping its own
output by it — `Engine` does not gate a voice's output by the note
envelope itself (this is what lets `ReeseVoice` deliberately leave its
feedback loop ungated while everything else multiplies by `env`).

### Live tuning: CC5-8

CC1-4 (from the wand/mock, arbitrated with the 8-knob controller in
`SharedSignal::set`) drive the global filter/width/fuzz/thump signals.
CC5/6 move the **selected voice's** `selected_knob`/knob value; CC7/8 do
the same for `Engine`'s own fixed knob list (`ENGINE_KNOB_RANGES`).
Switching voices retargets CC5/6 to the newly-selected voice. Every knob
write goes through `KnobPickup` (soft/absolute-pot takeover) so a knob
never jumps when it's freshly selected.

### Persistence

No `serde` anywhere. `snapshot.rs` does two things: numbered one-shot
dumps (`snapshots/{voice_name}_{NNNN}.snap`, monotonic index, never
overwrites) and an always-current pair (`config/{voice_name}.state` +
`config/selected`, version-controlled, loaded on startup, rewritten
whenever `AudioHandles::persist_dirty_voices` sees a voice's dirty flag
set by a live CC edit). Flat `name=value` text either way.

## UI (`src/ui/`)

Terminal UI via `termion` (cursor/clear/color escapes) + `drawille`
(braille-cell canvas plotting, for waveforms/meters) — redrawn every
engine-loop tick (`ui::draw_all`), not retained/immediate-mode like a GUI
toolkit. `--debug` suppresses this entirely in favor of stderr logging.
Three stacked panels: `panel_main.rs` (wand/note state), `panel_voice.rs`
(selected voice's knobs, generically over any `*View::knobs()`),
`panel_debug.rs` (history/delta-event log, output meters via
`comp_meter.rs`). `tw.rs`/`utils.rs` hold shared terminal-drawing helpers
(cursor positioning, color codes, box-drawing).

## Hydra / mock backend (`src/hydra/`)

- **`Backend` trait** (`mod.rs`): `update`, `should_quit`,
  `take_voice_cycle`/`take_tune_cycle` (net cycle-direction accumulators),
  `mock_controls() -> Option<MockControls>` (`None` on real hardware),
  `take_midi_notes() -> Vec<DeltaEvent>` (only `MockBackend` implements
  it).
- **`MockBackend`** (`mock.rs`) owns an optional MIDI input via
  `hydra::midi` (`midi.rs`): `midi::connect(notes) -> (MidiState,
  Connection)` has a `real` impl (macOS x86_64 only, via `midir`) and a
  `stub` impl elsewhere (inert `MidiState`, zero-sized `Connection`) — no
  `#[cfg]` needed at the call site. `midir` is scoped macOS/Linux-only in
  `Cargo.toml`, and also backs the unrelated `audio::cc_input` live-tuning
  path on both platforms (cpal already pulls in ALSA on Linux, so this
  isn't an extra dependency there).
- Keyboard: `z`/`.` toggle wand triggers, `a`/`s` cycle voice, `-`/`=`
  cycle tune, arrow keys drive the left stick to full deflection (toggle,
  not held).

## Debug logging

Run with `--debug` to suppress the terminal UI and print verbose per-
frame/per-event diagnostics straight to stderr instead (hydra frame
telemetry, `DeltaEvent`s as they're handled, CC messages, raw HID read
errors). First thing to reach for when tracking down a controller/engine
issue instead of guessing from the TUI.

Under the hood: `tools::parse_args` flips a crate-wide `AtomicBool`
(`tools::DEBUG_ENABLED`), and `crate::dbg!(...)` (defined in `tools.rs`,
`#[macro_export]`'d to the crate root) wraps `eprintln!` gated on that
flag — a no-op when `--debug` isn't passed. Use `crate::dbg!(...)` for new
verbose/diagnostic logging; reserve plain `eprintln!` for user-facing
errors that should print regardless (failed snapshot load/save, bad CLI
flag).

## Other top-level files

- `src/zgicabra.rs` — core instrument logic (don't touch unless asked).
  Turns raw `HydraState` into `Zgicabra` (wand/note/signal state) each
  tick via `update()`; exports `SignalState` (continuous performance
  signal piped to audio every tick), `DeltaEvent` (discrete note/voice/
  panic events), `Wand`/`NoteState`/`Hand`/`Direction`/`Joystick`.
- `src/functions.rs` — small standalone math helpers (`hyp`,
  `rad_to_cycles`, `button_mask`, `smoothstep`), no shared state.
- `src/plot.rs` — vendored/trimmed terminal line-chart lib (from
  textplots-rs), used by `ui/panel_debug.rs`'s history plots.
- `src/tools.rs` — `AtomicF32`, `Args`/`parse_args`, `linexp`, signal
  handlers, the debug-logging machinery above.

## Conventions

- `main.rs` has `#![allow(dead_code, unused_imports, unused_variables)]` —
  this project keeps unreferenced modules around for later reuse (`fm.rs`
  is the clearest example — see its module doc for *why* it's kept
  despite being unused; `nam.rs`'s `NamStage`/cycling path is currently
  dead too). **Don't "clean up" unreferenced audio modules unless asked**,
  and don't assume a module is wired into `Engine` just because it
  compiles — check `engine.rs`.
- Verification is mostly ears-driven: `cargo build` + running the binary
  (or `--test` for the automated signal-present check) is the primary
  loop. `cargo build` clean is necessary but not sufficient; flag when you
  can't verify audibly.
- Tests exist (`#[cfg(test)]`) in `snapshot.rs`, `growl.rs`, and
  `nam_graph.rs` — not just snapshot.rs any more; check for more before
  assuming a module is untested.
- Grep the installed crate source under
  `~/.cargo/registry/src/*/fundsp-0.23.0/src/` before assuming a fundsp
  function's signature, especially through `prelude64` — see the gotchas
  above.
- Cargo workspace members: `["voice-macro"]` only. `hid_test/` is a
  separate, non-member Cargo project (own `Cargo.toml`/target) for probing
  the macOS raw-HID path; not built by the main `cargo build`.
