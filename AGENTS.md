# AGENTS.md

| ⚠️ Full FunDSP docs: `ref/fundsp.md`. FunDSP builtins list: `ref/fundsp-modules.md`.

Working notes for agents touching this codebase — architecture, the fundsp
API surface in use, and this project's conventions. Read before touching
`src/audio/` or `src/gui.rs`.

Do not touch `src/zgicabra.rs` unless asked — it's the core instrument
logic, under the developer's direct control.

## What this is

A Rust synth/controller app built around a Razer Hydra-style two-wand
controller ("Sixense Hydra"). Wand motion/triggers/buttons drive a live
audio engine (or OSC out to a DAW). An optional imgui debug GUI edits
engine parameters and can drive a mock controller from keyboard/mouse/MIDI
when no real hardware is attached.

Entry point: `src/main.rs`. CLI flags (`tools::parse_args`): `--gui` to
open the tuner window, `--debug` to suppress the terminal UI and print
verbose diagnostics (see "Debug logging" below), `--test` to run the
audio self-test.

## Hardware

The Sixense Hydra (Razer Hydra) is a discontinued two-handed motion
controller; its SDK is adapted here into a performance instrument. The
controller is "Hydra", the instrument is "zgicabra" (Lojban; "a musical
apparatus").

## Platform notes

Two machines, different roles — confirm which one you're on.

### Performance box
- MusNix (NixOS) on a Lenovo NUC, i5, x86_64, no flakes, very limited CPU.
- Runs headless on stage, boots straight to the instrument; no monitor or
  keyboard guaranteed.

| ⚠️ Do not read or write `/etc/nixos/configuration.nix` on this machine —
| show the user the commands and let them run it.
| ⚠️ Static linking is an architectural goal: one binary, no external
| runtime deps.

### Testing box
- MacBook Pro, USB-C only, macOS x86_64. Hydra hardware isn't well
  supported here — `src/hydra/mock.rs` stands in. More ergonomic for
  development.

## Process / thread model

- `src/hydra/` backends: `sdk.rs` (Linux, real Sixense SDK), `hid.rs`
  (macOS, raw USB HID), `mock.rs` (keyboard/mouse/MIDI stand-in, active
  whenever no real backend connects).
- **Engine loop** (`main.rs::run_engine_loop`): polls hydra → derives
  `Zgicabra` state (`zgicabra::update`) → emits `DeltaEvent`s plus a
  continuous `SignalState` → feeds both straight to `audio::AudioOutput`
  (the only output backend; the old OSC/`DeltaConsumer` trait indirection
  was removed once the native audio engine became the sole target). Runs
  on the main thread, or a background thread when `--gui` is set.
- **GUI thread**: winit/AppKit require the window + event loop on the
  process's main thread on macOS, so `gui::run()` owns `main()` whenever
  `--gui` is passed, and the engine loop is spawned instead.
- **Audio thread**: cpal owns a real-time callback thread running the DSP
  graph (`audio::build_stream`). Never blocks, allocates, or locks
  contentiously (see the one `Mutex` in the NAM section below).
- **`--test` self-test** (`main.rs::run_self_test`, requires `--gui`):
  holds a synthetic A4 note, captures ~0.1s of raw cpal output via
  `AudioCapture`, reports PASS/FAIL on non-zero signal — isolates "engine
  produces no signal" from "OS/device routing is broken" (the latter needs
  a human to listen for 3 seconds). Only automated audio-path check; no
  unit-test coverage of the DSP graph itself.

## Cross-thread communication: no channels, only atomics

No mpsc/crossbeam channel and no `Arc<Mutex<_>>` for engine state anywhere.
Every value crossing from the GUI/engine-loop thread into the real-time
audio thread is one of:

- **`fundsp::shared::Shared`** — `Arc`-wrapped atomic f32 cell,
  `.value()`/`.set_value()`. Used for nearly everything: note freq/gate,
  every live-tunable param, voice selection.
- **`tools::AtomicF32`** — hand-rolled lock-free f32 cell, used where
  `Shared` isn't already in scope (mock hydra stick position, wand
  telemetry, `SignalOverride`, MIDI CC/pitch-bend).
- **`Arc<AtomicBool>`** for flags (quit signal, trigger/button state,
  override toggles).

A struct on the audio-engine side owns the "master" cell; a cheap
`.clone()` goes to the GUI. GUI writes, audio thread reads on its next
tick — no extra sync needed since these are control-rate values.

**Never introduce a channel or mutex for new engine parameters.** Make it
a `Shared`, construct it on the `AudioOutput`/`Engine` side, hand a clone
to the GUI via `AudioHandles`.

## fundsp: what's actually in play

`fundsp` (v0.23) has two traits — know which you're implementing:

- **`AudioNode`** (`audionode.rs`): compile-time-sized. `type
  Inputs`/`type Outputs` (typenum sizes: `U0`, `U1`, `U2`, ...), `fn
  tick(&mut self, input: &Frame<f32, Self::Inputs>) -> Frame<f32,
  Self::Outputs>`, requires `Self: Clone`. Implement this for a new
  self-contained DSP voice/effect (see `GrowlVoice`/`BasicVoice`). `An<X>`
  wraps one for `>>`/`|` combinator syntax and gets a blanket `AudioUnit`
  impl for free.
- **`AudioUnit`** (`audiounit.rs`): dynamic, runtime-sized, object-safe —
  `fn tick(&mut self, input: &[f32], output: &mut [f32])`, plus
  block-processing entry points used by the NAM convolution path.
  `Box<dyn AudioUnit>` is `Clone` (via `DynClone`), so it's safe to hold as
  a field on a `#[derive(Clone)]` struct (`Engine::envelope`,
  `ReverbFx::tail`).

Rule of thumb: implement `AudioNode` when arity is known at compile time
(most voices/effects); reach for `Box<dyn AudioUnit>` for type erasure or
to wrap nam-rs's non-`Clone` `Model` (see `NamModelSlot`).

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
- `ConstantFrame` (used by `dc()`/`constant()`) works for any `T: Float`
  and for tuples up to 10 elements.
- Every hand-written `AudioNode` here picks a private `const ID: u64` in
  the `0x7A_xx` range (fundsp's own IDs stay well below it). Current:
  `0x7A_10` (`FmVoice`, orphaned), `0x7A_11` (`ReeseVoice`), `0x7A_12`
  (`NamStage`), `0x7A_20` (orphaned `gen_node`/`fx_node`), `0x7A_24`
  (`ReverbFx`), `0x7A_30` (`BasicOscGen`, orphaned), `0x7A_40`
  (`GrowlVoice`), `0x7A_41` (`BasicVoice`), `0x7A_60` (`BlankVoice`). Grep
  `const ID: u64 = 0x7A_` before picking a new one.
- `NAM_BLOCK_CAP` bounds any single NAM-stage block call — the convolver
  re-runs its WaveNet dilation machinery per call, so per-sample `tick()`
  causes audible stutter; must be driven in batches via `process_block()`
  (see `NamStage::process_block` and `Engine::tick_pre_nam`/`run_nam`/
  `tick_post_nam`).

## Audio engine architecture (`src/audio/`)

Not one big fundsp-composed graph — a hand-driven `Engine` struct,
manually sequenced once per sample/block inside `build_stream`'s cpal
callback:

```
Engine::tick_pre_nam()  -->  Engine::run_nam() [NamStage L/R]  -->  Engine::tick_post_nam() [reverb + limiter]
     (per-sample)              (per-block, <=NAM_BLOCK_CAP)              (per-sample)
```

- **`Engine`** (mod.rs, private — `AudioOutput` is the public handle)
  mixes every sound-generating piece each sample: the four `Voice` impls
  (below, each silences itself unless selected), a `main_sub` triangle
  oscillator, `dry_sub` (fixed sine an octave down, bypasses
  NAM/reverb/limiter), and an
  `adsr_live` envelope gated by `gate`. `tick_thump()` layers a percussive
  pitch-decay bump onto `base_freq` on note-on. `FmVoice`/`StutterGen`/
  `Crusher`/`MoogFilterFx`/`LowpassFx`/`BasicOscGen` (`fm.rs`, `stutter.rs`,
  `crusher.rs`, `filter.rs`, `gen_node.rs`) compile but are **not wired
  into `Engine`** — don't assume a module is live just because it
  compiles; check `Engine::tick_pre_nam`/`run_nam`/`tick_post_nam`.
- **`NamStage`** (`nam.rs`): NAM neural amp-sim stage, block-driven, with
  a live low/high crossover split and dry/wet blend. The fixed "amp" stage
  in `Engine` (`amp_l`/`amp_r`, independent instances so L/R WaveNet state
  never mixes) always runs one hardcoded model (`AMP_MODEL = "lowgain"`) —
  no bypass slot, no cycling. `NamStage` also supports a `*.nam`-file-
  discovery + cycle-by-index shape (`load_nam_models`/`NamModelCycler`),
  unused by `Engine`/`AudioHandles` today. The one `Mutex` in the audio
  path (`NamModelSlot.model: Arc<Mutex<Model>>`) exists only because
  `nam-rs`'s `Model` isn't `Clone`; never contended since only the audio
  thread touches it.
- **Reverb** (`reverb.rs`, `ReverbFx`): fixed FDN tail, genuinely stereo.
  `room_size`/`decay`/`damp` are baked in at `Engine::new()` time —
  editing those GUI knobs has no live effect, only a restart applies them.
  `reverb_dry` is live; `reverb_bypass` skips the stage entirely
  (preserving stereo width, since the tail itself mono-sums).
- **Limiter** (`compressor.rs`, `Compressor`): downward-only,
  stereo-linked peak compressor, final safety stage. `limiter_thresh` is
  live; `RATIO`/`ATTACK`/`RELEASE` are fixed constants.

### One module per voice

`voice.rs` holds only the shared contract: the `Voice` trait, `VoiceParams`
trait, and `ThumpMod` (`pub(super)`). Each concrete voice's
`*Params`/`*Handle`/`*Voice` trio lives in its own patch file:
`GrowlParams`/`GrowlHandle`/`GrowlVoice` in `growl.rs` (below
`WavetableGen`), `ReeseParams`/`ReeseHandle`/`ReeseVoice` in `reese.rs`,
`BasicParams`/`BasicHandle`/`BasicVoice` in `basic.rs`. `mod.rs`
re-exports each `*Handle`/`*Params` pair so `gui.rs`/`AudioHandles` import
from `crate::audio`. **When adding a voice, put its structs in its own
file, not `voice.rs`.** `BlankVoice` (`blank.rs`) is the exception — no
params, no `*Handle`, nothing to re-export.

### The `Voice` trait (`voice.rs`)

Every param is a plain `Shared` cell; every GUI control for it is a
hand-written call in `gui.rs` — adding a tunable value means adding both
the `Shared` plumbing and a `draw_knob_row`/`draw_module_card` line, no
generic pickup mechanism exists.

- **`Voice` trait**: `AudioNode<Inputs = U2, Outputs = U2>` plus `const
  INDEX: usize`, `fn name()`, `fn set_signal(&mut self, bend, filter,
  fuzz, width, thump: f32)` (both current voices no-op this), `fn
  on_block_start(&mut self, len: usize)` (extension point for a future
  block-driven voice). Every `Voice` is wired in parallel into `Engine`
  and ticked every sample with input `[freq, selected]`; each impl returns
  silence unless `selected as usize == Self::INDEX`.
- **Four voice slots**: `voice_a` (`ReeseVoice`, `INDEX = 0`), `voice_b`
  (`GrowlVoice`, `INDEX = 1`, wraps `WavetableGen`), `voice_c`
  (`BasicVoice`, `INDEX = 2`, four fundsp builtin oscillators,
  independently level-mixed — a test voice, not a real patch), `voice_d`
  (`BlankVoice`, `INDEX = 3`, always-silent placeholder). The struct field
  letter always matches the slot's `INDEX`, not which voice type occupies
  it — reordering means renaming the fields to match. `VOICE_NAMES` in
  `gui.rs` and `voice_selected` must stay in sync with each `Voice::INDEX`.
- **`VoiceParams` trait**: `fn voice_name()`, `fn fields(&self) ->
  Vec<(&'static str, f32)>`, `fn from_fields(&[(String, f32)]) -> Self` —
  the shape `snapshot.rs` needs. `GrowlParams`/`BasicParams`/`ReeseParams`
  implement it; `BlankVoice` has no params, so `gui.rs`'s save/load match
  arms just skip index `3`.
- **`*Handle` structs**: just the live `Shared` cells, cheap to clone,
  held by `AudioHandles`/`gui.rs`. The matching `*Voice` struct owns the
  real DSP state plus a clone of the same handle — GUI writes go straight
  through the shared cell. `BlankVoice` is constructed with no handle at
  all.

### `WavetableGen` (`growl.rs`) — Growl's patch

Reproduces one specific Vital synth patch as a fundsp `AudioNode` graph, 4
macro knobs (`bass_drive`, `filter`, `space`, `warp`) via `GrowlHandle`.
`WavetableGen::clone()` resets to a fresh, un-warmed-up instance — only
clone at construction, not mid-stream. Also holds the bolted-on NAM amp
stage (`nam_crossover`/`nam` model cycler on `GrowlHandle`, `NamStage` on
`GrowlVoice`, run from `on_block_start`).

### `BlankVoice` (`blank.rs`)

`voice_d` is an intentionally empty slot — a trivial `AudioNode`/`Voice`
impl that unconditionally returns silence, no params or state. Reuse this
pattern to blank out a slot rather than leaving a half-removed voice
around.

### Adding a new Voice

Follow `BasicVoice` (`basic.rs`, simplest example): own module (or append
to an existing patch file that already wraps a generator, e.g.
`growl.rs`) — not `voice.rs`. Pick an unused `INDEX`, `#[derive(Clone)]`
struct, implement `AudioNode<Inputs = U2, Outputs = U2>` + `Voice`
(silence-unless-selected check first in `tick()`), forward
`set_sample_rate` to every child unit. Add its `*Params`/`*Handle` pair if
it has tunable params, re-export from `mod.rs`, wire the handle through
`AudioOutput`/`Engine`/`AudioHandles`, add it to `Engine` as a field
ticked every sample, add a `VOICE_NAMES` entry and `draw_voice_*` match
arm in `gui.rs`.

### Persistence

No `serde` anywhere — don't add one for something this small.
`src/audio/snapshot.rs` saves one voice's live params as flat
`name=value` text, one file per save named
`snapshots/{voice_name}_{NNNN}.snap` (monotonic per-voice index, scanned
from existing files, never overwrites). `snapshot.rs` has this project's
only `#[cfg(test)]` unit test; everything else is verified by ear.

## GUI (`src/gui.rs`)

Dear ImGui via `imgui` + `imgui-winit-support` + `imgui-glow-renderer`,
windowed with `winit`/`glutin`/`glow`. **Immediate mode** — `draw_ui()`
runs fresh every frame, no retained widget tree. State that must persist
across frames (selected snapshot, whether the test note is currently held)
lives as a local variable in `gui::run()`'s scope, threaded in by `&mut`
each frame (see `SnapshotBrowser`). Everything else reads straight out of
the same `Shared` cells the audio thread reads.

- **Custom-drawn controls for most knobs.** `knob()`/`xy_pad()` are
  hand-drawn on `ui.get_window_draw_list()` — not imgui's `Slider`/`Drag`.
  `imgui::Drag` is used for exactly one control: the held test-tone's note
  field. Every knob's `(lo, hi)` range is a hardcoded literal at its call
  site in `gui.rs`.
- **`AudioHandles`** (`audio::mod.rs`) bundles everything the GUI needs
  from a running `AudioOutput`: `test_tone`, `voice_selected` + one
  `*Handle` per voice slot, the main/dry-sub + thump knobs, `amp_*`/
  `reverb_*`/`limiter_*` cells, and `capture` (for `--test`).
  `AudioOutput::handles()` builds one; `main.rs` threads it into
  `gui::run`. Add new audio-thread handles as fields here, not positional
  params.
- **Module-card pattern**: `draw_module_card(ui, title, bypass_cell, size,
  body_fn)` — bordered child window, optional bypass checkbox, body
  closure. `draw_knob_row(ui, &[(id, label, lo, hi, cell), ...])` lays out
  knobs side-by-side. Reuse for new fixed-knob cards.
- **Voice selector / snapshot browser**: `draw_voice_selector` cycles
  `voice_selected` through `VOICE_NAMES`. `SnapshotBrowser` drives
  save/load against whichever voice is selected.
- **Hydra panel** (`draw_hydra_panel`): mock wand controls (joysticks +
  trigger/button toggles) when on the mock backend, else a "hardware
  connected" message. A center column of `signal_override_row`s lets the
  GUI take a `SignalState` field over from whatever normally computes it,
  via `ZgicabraBridge`/`SignalOverride` — the same mechanism
  `hydra::mock`'s MIDI listener uses, so a MIDI controller and the GUI
  knobs fight over the same field if both drive it live.
- **Screenshot self-check**: `ZGICABRA_GUI_SCREENSHOT=<path>` makes the
  window capture its framebuffer to PNG on frame 10 and exit — bypasses
  macOS's screen-recording prompt, useful for verifying a GUI change
  rendered without a human watching.

## Hydra / mock backend (`src/hydra/`)

- **`Backend` trait** (`mod.rs`): `update`, `should_quit`,
  `take_voice_cycle`/`take_tune_cycle` (net cycle-direction accumulators),
  `mock_controls() -> Option<MockControls>` (`None` on real hardware),
  `take_midi_notes() -> Vec<DeltaEvent>` (only `MockBackend` implements
  it).
- **`MockBackend`** (`mock.rs`) owns an optional MIDI input via
  `hydra::midi` (`midi.rs`): `midi::connect(notes) -> (MidiState,
  Connection)` has a `real` impl (macOS x86_64 only, via `midir`) and a
  `stub` impl elsewhere (inert `MidiState`, zero-sized `Connection`) —
  `mock.rs`/`main.rs` call the same API unconditionally, no `#[cfg]`
  needed at the call site. `midir` is scoped macOS-only in `Cargo.toml` —
  the Linux performance box has no ALSA dev headers, so it must never be a
  plain dependency. CC 1-4 (filter/width/fuzz/thump) and pitch-bend feed
  `AtomicF32` cells that `run_engine_loop` pushes onto `SignalOverride`s
  each tick when `mc.midi.connected`; Note On/Off go through the normal
  `DeltaEvent` pipeline, monophonic/last-note-priority like a wand
  trigger.
- Keyboard: `z`/`.` toggle wand triggers, `a`/`s` cycle voice, `-`/`=`
  cycle tune, arrow keys drive the left stick to full deflection (toggle,
  not held). Right wand stick + button rows are GUI-only, no keyboard
  equivalent.

## Debug logging

Run with `--debug` for diagnosis — it suppresses the terminal UI and
prints verbose per-frame/per-event diagnostics straight to stderr instead
(hydra frame telemetry, `DeltaEvent`s as they're handled, raw HID
read errors from `src/hydra/hid.rs`). This is the first thing to reach
for when tracking down a controller/engine issue instead of guessing from
the TUI.

Under the hood: `tools::parse_args` flips a crate-wide `AtomicBool`
(`tools::DEBUG_ENABLED`, via `tools::set_debug_enabled`/`debug_enabled`),
and `crate::dbg!(...)` (defined in `tools.rs`, `#[macro_export]`'d to the
crate root) wraps `eprintln!` gated on that flag — a no-op when `--debug`
isn't passed. Use `crate::dbg!(...)` for any new verbose/diagnostic
logging; reserve plain `eprintln!` for actual user-facing error surfaces
(failed snapshot load/save, bad CLI flag, etc.) that should print
regardless of `--debug`.

## Conventions

- `main.rs` has `#![allow(dead_code, unused_imports, unused_variables)]` —
  this project keeps unreferenced modules around for later reuse (`fm.rs`,
  `stutter.rs`, `crusher.rs`, `filter.rs`, `gen_node.rs`, `fx_node.rs`,
  `nam.rs`'s cycling path). **Don't "clean up" unreferenced audio modules
  unless asked**, and don't assume a module is wired into `Engine` just
  because it compiles — check `Engine::tick_pre_nam`/`run_nam`/
  `tick_post_nam`.
- `Cargo.toml`'s `[profile.dev] debug-assertions = false` works around an
  imgui-rs 0.12 UB check that only trips in debug builds on an empty draw
  list — don't second-guess it.
- `NAM_SAMPLE_RATE = 48_000` is forced for the output device because
  every NAM model in `nam/` was captured at that rate — always honor
  `set_sample_rate()`, never assume `DEFAULT_SR`.
- Verification is mostly ears-driven: `cargo build` + running `--gui` (or
  `--gui --test` for the automated signal-present check) is the primary
  loop. `cargo build` clean is necessary but not sufficient; flag when you
  can't verify audibly.
- Grep the installed crate source under
  `~/.cargo/registry/src/*/fundsp-0.23.0/src/` before assuming a fundsp
  function's signature, especially through `prelude64` — see the gotchas
  above.

## Non-Rust material

- `hid_test/` — standalone side Cargo project (own `Cargo.toml`/target)
  for probing the macOS raw-HID path; not built by the main `cargo build`.
