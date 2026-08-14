# AGENTS.md

| ⚠️ Important:
| - Full documentation on FunDSP is available in ref/fundsp.md.
| - List of available FunDSP builtins is available in ref/fundsp-modules.md.

Working notes for agents touching this codebase — architecture, the fundsp
API surface actually in use, and the conventions this project has settled
on. Read this before touching `src/audio/` or `src/gui.rs`.

For agents, recommend NOT touching src/zgicabra.rs - this is the core of
how the instrument works and is under full control of the developer.

**This file was rewritten 2026-08-12 - If you're reading an old copy of this
file from context/memory, throw it out and re-read this one.

## What this is

A Rust synth/controller app built around a Razer Hydra-style two-wand
controller ("Sixense Hydra"). Wand motion/triggers/buttons drive a live audio
engine (or, alternately, OSC out to a DAW). There's an optional imgui
tuning/debug GUI for editing engine parameters and driving a mock version
of the controller from a keyboard/mouse/MIDI controller when no real
hardware is attached.

Entry point: `src/main.rs`. Run modes selected by CLI flags
(`tools::parse_args`): `--audio` (default) vs `--osc` output backend,
`--gui` to open the tuner window, `--no-ui` to suppress the terminal UI,
`--test` to run the audio self-test

## About the Hardware

The Sixense Hydra (sold as Razer Hydra) is a two-handed motion-aware game
controller. It is no longer produced or sold. The SDK for the device as been
adapted here into a live-performance musical instrument. In this the hardware
controller is called "Hydra", the musical instrument developed on top of the
SDK is called "zgicabra" (Lojban; "a musical apparatus"). 

## Platform Notes

This project runs on 2 machine with different inteded uses and different
hardware profiles. Confirm which environment you are running in when working.

### The Performance Box

- A MusNix (NixOS distro) linux
- running on a small Lenovo NUC in an Intel i5
- X86_64
- very limited CPU

This machine is intended to be brought on stage and run headless as a
stand-alone musical instrument. It will not always have a monitor or keyboard
attached. It should contain only the very bare minimum software required to
successfully run the zgicabra program, and should boot it directly from
cold start so that it works on stage without user intervention.

| ⚠️ Important:
| When working on this machine, do not attempt to read or write the
| /etc/nixos/configuration.nix file. Show the user the necessary commands
| and wait for them to do it themselves.

### The Testing Box

- MacBook Pro Laptop
- USB-C only
- Hydra hardware is not well supported on this machine
- The `src/hydra/mock.rs` backend is used to stand for the real hardware
- MacOS X86_64

This machine is more ergonomic for developing the software and experimenting
with the synth engine.

## Process / thread model

- **No real hardware on macOS dev machines** — `src/hydra/` has `sdk.rs`
  (Linux, real Sixense SDK), `hid.rs` (macOS, raw USB HID), and `mock.rs`
  (keyboard/mouse/MIDI-driven stand-in, active whenever no real backend
  connects). `src/hydra/real.rs` no longer exists (removed) — don't look
  for it, don't re-add it.
- **Engine loop** (`main.rs::run_engine_loop`): polls hydra → derives
  `Zgicabra` state (`zgicabra::update`) → emits `DeltaEvent`s (note on/off,
  voice cycle, etc.) and a continuous `SignalState` → feeds both to
  whatever implements `output::DeltaConsumer` (`audio::AudioOutput` or
  `osc::OscOutput`). Runs on the main thread normally; moves to a
  background thread when `--gui` is set, because...
- **GUI thread**: winit/AppKit require the window + event loop on the
  *process's* main thread on macOS, so `gui::run()` owns `main()` whenever
  `--gui` is passed, and the engine loop above is spawned instead. See the
  comment block at the top of `main.rs`'s `if args.gui` branch.
- **Audio thread**: cpal owns a real-time callback thread running the
  actual DSP graph (`audio::build_stream`'s closure). Never blocks, never
  allocates, never locks (see the NAM section below for the *one* mutex in
  the whole audio path, and why it's fine).
- **`--test` self-test** (`main.rs::run_self_test`, needs `--gui`
  — errors out otherwise): spawned on its own thread alongside the engine
  loop when `--test` is passed. Holds a synthetic A4 note through
  `AudioHandles::audition_note`, captures ~0.1s of raw cpal output via
  `AudioCapture` (`audio/mod.rs`), and reports PASS/FAIL on whether the
  buffer is non-zero — this isolates "engine produces no signal" from
  "OS/device audio routing is broken" (the latter needs a human to
  actually listen, which the self-test also does for 3 real seconds).
  This is the closest thing this project has to an automated audio-path
  check; there's still no unit-test coverage of the DSP graph itself.

## Cross-thread communication: no channels, only atomics

There is **no mpsc/crossbeam channel anywhere** in this codebase, and no
`Arc<Mutex<_>>` for engine state. Every value that needs to cross from the
GUI thread (or the engine-loop thread) into the real-time audio thread is
one of:

- **`fundsp::shared::Shared`** — an `Arc`-wrapped atomic f32 cell,
  `.value()` / `.set_value()`. This is what almost everything uses: note
  freq/gate, every live-tunable synth parameter, voice selection.
- **`tools::AtomicF32`** — the project's own hand-rolled lock-free f32 cell
  (bit-cast through `AtomicU32`), used where `fundsp::Shared` isn't already
  in scope (mock hydra stick position, wand rotation telemetry,
  `zgicabra::SignalOverride`, MIDI CC/pitch-bend values from `hydra::mock`).
- **`Arc<AtomicBool>`** for flags (quit signal, trigger/button state,
  override-enabled toggles).

The pattern throughout: a struct on the audio-engine side owns the
"master" `Shared`/`AtomicF32` cell; a cheap `.clone()` of it (just bumps
an `Arc` refcount) is handed out to the GUI. GUI writes, audio thread
reads on its next tick/block — no synchronization needed beyond the
atomic itself, because these are all *control-rate* values (racing a
single float write against a read is fine; there's no need for
sample-accuracy here).

**Never introduce a channel or a mutex for new engine parameters.** If you
need a new live-tunable value, make it a `Shared`, construct it on the
`AudioOutput`/`Engine` side, and hand a clone to the GUI via
`AudioHandles`.

## fundsp: what's actually in play

`fundsp` (v0.23) provides two related traits — know which one you're
implementing:

- **`AudioNode`** (`audionode.rs`): the generic, compile-time-sized trait.
  `type Inputs: Size<f32>`, `type Outputs: Size<f32>` (fundsp's own
  typenum-style sizes: `U0`, `U1`, `U2`, `U7`, ...), `fn tick(&mut self,
  input: &Frame<f32, Self::Inputs>) -> Frame<f32, Self::Outputs>`.
  Requires `Self: Clone`. This is what you implement for a new
  self-contained DSP voice/effect (see `src/audio/growl.rs`'s `GrowlVoice`
  or `src/audio/basic.rs`'s `BasicVoice` for the current examples).
  `An<X>` wraps an `AudioNode` and makes it composable with `>>` / `|` /
  etc combinator syntax, and gets a **blanket impl of `AudioUnit`** for
  free — that's the bridge to the next trait.
- **`AudioUnit`** (`audiounit.rs`): the dynamic, runtime-sized,
  object-safe trait — `fn tick(&mut self, input: &[f32], output: &mut
  [f32])`, `fn inputs(&self) -> usize`, `fn outputs(&self) -> usize`, plus
  `fn process_buffer(...)` / block-processing entry points for the NAM
  convolution/model path (`nam-rs`'s own chunking, capped at
  `NAM_BLOCK_CAP` scratch buffers on this project's side). `Box<dyn
  AudioUnit>` **is `Clone`** — the trait has a `DynClone` supertrait and
  fundsp calls `dyn_clone::clone_trait_object!(AudioUnit)` — so it's safe
  to hold `Box<dyn AudioUnit>` fields on a `#[derive(Clone)]` struct
  (used for `Engine::envelope`, `ReverbFx::tail`, and the orphaned
  `gen_node.rs`/`fx_node.rs` impls).

Practical rule of thumb used in this codebase: implement `AudioNode`
by hand when you know the exact input/output arity at compile time (most
voices/effects); reach for `Box<dyn AudioUnit>` when a value crosses an
API boundary that wants type erasure, or when nam-rs's own `Model` type
(not `Clone`) needs wrapping (see `NamModelSlot`'s `Arc<Mutex<Model>>`
below).

### `prelude64` gotchas (bit us during a past pass — check before assuming)

`use fundsp::prelude64::*;` is what every file under `src/audio/` imports.
It's a **specialized, non-generic re-export** of fundsp's generic prelude,
monomorphized to `f64` internally:

- Functions that are generic in `fundsp::prelude` (e.g. `sine::<F:
  Real>()`, `pink::<F: Float>()`, `poly_saw::<F: Real>()`) are **plain,
  non-generic functions in `prelude64`** — calling `sine::<f64>()` is a
  compile error ("takes 0 generic arguments"). Just call `sine()`.
- Functions whose generic prelude signature takes `f64` args (e.g.
  `reverb_stereo(room_size: f64, time: f64, damping: f64)`) take **`f32`**
  in `prelude64` — it casts internally before calling the f64 version. Get
  this backwards and you'll see "expected f32, found f64".
- When in doubt about a function's real signature, grep the *installed*
  crate source rather than guessing from docs/memory:
  `~/.cargo/registry/src/*/fundsp-0.23.0/src/prelude64.rs` (and
  `prelude.rs` for the generic originals, `oscillator.rs`/`wavetable.rs`/
  `noise.rs`/`audionode.rs` for concrete `Inputs`/`Outputs` typenum
  values). This saved a lot of guess-and-recompile churn.

### Other fundsp facts worth knowing

- `get_mono()` / `filter_mono()` are default methods **on the `AudioUnit`
  trait itself** (not an extension trait) — callable directly on `&mut
  Box<dyn AudioUnit>` or `&mut An<X>` without extra imports.
- `ConstantFrame` (used by `dc()`/`constant()`) is impl'd for any `T:
  Float` (so `dc(0.5f32)` works directly, no wrapping needed) and for
  tuples up to 10 elements (for multi-channel constants).
- Every hand-written `AudioNode` in this repo picks a private `const ID:
  u64` in the `0x7A_xx` range as a "namespace" for this project's custom
  nodes (fundsp uses `ID` for its own internal hashing, not correctness
  you need to reason about — just don't collide with fundsp's own IDs,
  which stay well below this range). Current allocations: `0x7A_10`
  (`FmVoice`, orphaned), `0x7A_11` (`ReeseVoice`, orphaned), `0x7A_12`
  (`NamStage`), `0x7A_20` (orphaned `gen_node`/`fx_node` adapters),
  `0x7A_24` (`ReverbFx`), `0x7A_30` (`BasicOscGen`, orphaned), `0x7A_40`
  (`GrowlVoice`), `0x7A_41` (`BasicVoice`), `0x7A_50` (`GorgleGen`),
  `0x7A_51` (`GorgleVoice`). Grep for `const ID: u64 = 0x7A_` before
  picking a new one.
- `MAX_BUFFER_SIZE`/`NAM_BLOCK_CAP` bound any single NAM-stage
  block call — the NAM convolver re-runs its WaveNet dilation machinery
  per call, so driving it one sample at a time (`tick()`) causes audible
  stutter; it must be driven in batched blocks via `process_block()`. See
  the comment above `NamStage::process_block` in `src/audio/nam.rs`, and
  `Engine::tick_pre_nam`/`run_nam`/`tick_post_nam` in `mod.rs` for how
  `build_stream` batches a whole cpal callback into blocks around it.

## Audio engine architecture (`src/audio/`)

This is **not** one big fundsp-composed graph wired with `>>`/`|`. It's a
hand-driven `Engine` struct, manually sequenced once per sample/block
inside `build_stream`'s cpal callback (`src/audio/mod.rs`):

```
Engine::tick_pre_nam()  -->  Engine::run_nam() [NamStage L/R]  -->  Engine::tick_post_nam() [reverb + limiter]
     (per-sample)              (per-block, <=NAM_BLOCK_CAP)              (per-sample)
```

- **`Engine`** (mod.rs, private — `AudioOutput` is the public handle) owns
  every sound-generating piece and mixes them each sample. Current pieces:
  `GrowlVoice` and `BasicVoice` (see the `Voice` trait below — both wired
  in parallel, each silences itself when `voice_selected` doesn't match
  its own `INDEX`), two `main_sub` oscillators (triangle/saw, crossfaded
  by `main_sub_wave`, feeds the NAM-processed dry path), `dry_sub` (a
  fixed sine one octave down that bypasses NAM/reverb/limiter entirely),
  and an `adsr_live` envelope gated by `gate: Shared`. `tick_thump()`
  layers a percussive pitch-decay bump onto `base_freq` on note-on.
  **`FmVoice`/`ReeseVoice`/`StutterGen`/`Crusher`/`MoogFilterFx`/
  `LowpassFx`/`BasicOscGen` are not wired into `Engine`** — their source
  files (`fm.rs`, `reese.rs`, `stutter.rs`, `crusher.rs`, `filter.rs`,
  `gen_node.rs`) still exist and compile (declared as `mod`s in mod.rs)
  but nothing in `Engine::tick_pre_nam`/`tick_post_nam` references them.
  Same convention this project has always used for unreferenced-but-kept
  code (see "Conventions" below) — don't delete them, and don't assume
  they're wired in just because they compile.
- **`NamStage`** (`nam.rs`): amp-model (NAM neural amp sim) stage,
  block-driven, with a live crossover split (low band stays dry, high band
  goes through the model) and a dry/wet blend. The *fixed* "amp" stage in
  `Engine` (`amp_l`/`amp_r`, two fully independent `NamStage` instances,
  one per channel so L/R WaveNet dilation state never mixes) always runs
  exactly one hardcoded model (`AMP_MODEL = "lowgain"`, loaded via
  `nam::load_named_model`) — there's no Bypass slot and no cycling on this
  path. `NamStage` *also* still supports the older "discovered `*.nam`
  files + cycle by index, index 0 = Bypass" shape
  (`load_nam_models`/`NamModelCycler`), but **nothing in `Engine` or
  `AudioHandles` uses that path anymore** — no GUI model-picker exists
  today. Both stay compiled/orphaned per the same convention as the
  voice/gen modules above. The one `Mutex` in the audio path
  (`NamModelSlot.model: Arc<Mutex<Model>>`) wraps each `Model` purely
  because `nam-rs`'s `Model` isn't `Clone` and `AudioNode`/structural
  bounds require `Self: Clone`; it's never actually contended since only
  the audio callback thread touches it.
- **Reverb** (`reverb.rs`, `ReverbFx`): fixed FDN tail
  (`reverb_stereo`), genuinely stereo (mono-sums input, produces a
  distinct L/R tail). `room_size`/`decay`/`damp` are baked in at
  `Engine::new()` time from `AudioOutput::new()`'s initial `Shared`
  values — editing the `reverb_size`/`reverb_decay`/`reverb_damp` knobs in
  the GUI has **no live effect**, only a restart applies new values (same
  documented caveat this project has always had for reverb tail params).
  `reverb_dry` (the wet/dry balance) *is* fully live, and `reverb_bypass`
  is a normal bypass checkbox that skips the stage entirely (preserving
  whatever stereo width the voice produced, since `ReverbFx` itself
  mono-sums before its tail).
- **Limiter** (`compressor.rs`, `Compressor`): plain downward-only,
  stereo-linked peak compressor (`AFollow` envelope over `max(|l|,|r|)`,
  same gain reduction on both channels) used as the final safety-limiter
  stage. Distinct from the (orphaned) OTT-style `Crusher` in `crusher.rs`,
  which this does not reuse. `limiter_thresh` is live; fixed `RATIO`,
  `ATTACK`, `RELEASE` constants in `compressor.rs`.

### One module per voice — `*Params`/`*Handle`/`*Voice` live next to the patch, not in `voice.rs`

`voice.rs` holds only the shared contract: the `Voice` trait, the
`VoiceParams` trait, and `ThumpMod` (`pub(super)`, since every concrete
voice uses it). Each concrete voice's `*Params`/`*Handle`/`*Voice` struct
trio and their impls (`AudioNode`, `Voice`, `VoiceParams`) live in that
voice's own patch file, appended after its `GenNode`/generator impl:
`GrowlParams`/`GrowlHandle`/`GrowlVoice` in `growl.rs` (below
`WavetableGen`), `GorgleParams`/`GorgleHandle`/`GorgleVoice` in `gorgle.rs`
(below `GorgleGen`), `BasicParams`/`BasicHandle`/`BasicVoice` in
`basic.rs` (no separate generator to wrap, so the whole voice lives in its
own file). `mod.rs` re-exports each `*Handle`/`*Params` pair from its own
module (`pub use growl::{GrowlHandle, GrowlParams}`, etc) so
`gui.rs`/`AudioHandles` keep importing from `crate::audio` either way —
**when adding a new voice, put its structs in its own file and re-export
from `mod.rs`, don't add them to `voice.rs`.**

### The `Voice` trait (`voice.rs`) — replaces the old ParamSpec/weight-matrix system

**If you remember an older version of this doc describing `ParamSpec`,
`VoiceParams::entries()`, `AuditionVoice`, or a generic GUI table that
auto-picked up new params — that system is gone.** There is no more
per-field weight matrix (`weight_pitch`/`weight_width`/etc blended by
`param_factor`), no more single enumeration point the GUI iterates
generically. Every param today is a plain `Shared` cell, and every GUI
control for it is a hand-written call in `gui.rs` (see below) — adding a
new tunable value means adding both the `Shared` plumbing *and* a
`draw_knob_row`/`draw_module_card` line in `gui.rs`, there's no more
"just add one array entry and the GUI picks it up" shortcut.

What replaced it:

- **`Voice` trait** (`voice.rs`): `AudioNode<Inputs = U2, Outputs = U2>`
  plus a `const INDEX: usize`, `fn name()`, `fn set_signal(&mut self,
  signal: &SignalState)` (read-only hook for internal modulation — both
  current voices no-op this), and `fn on_block_start(&mut self, len:
  usize)` (extension point for a future voice that needs block-driven
  inference, called once per cpal callback chunk). Every `Voice` is wired
  in parallel into `Engine` and ticked every sample; input `[freq,
  selected]`, and each impl returns silence unless `selected as usize ==
  Self::INDEX` — so only the actually-selected voice burns CPU despite the
  whole graph staying wired.
- **Three concrete voices exist today**: `GrowlVoice` (`INDEX = 0`, wraps
  `WavetableGen` in `growl.rs` — see below), `BasicVoice` (`INDEX = 1`, four
  fundsp builtin oscillators sin/tri/square/saw, independently level-mixed;
  a test voice, not a real patch), and `GorgleVoice` (`INDEX = 2`, wraps
  `GorgleGen` in `gorgle.rs` — see below). `VOICE_NAMES` in `gui.rs` and the
  `voice_selected` `Shared` index must stay in sync with each `Voice`'s
  `INDEX` if you add a fourth.
- **`VoiceParams` trait** (`voice.rs`, distinct from the old removed
  `VoiceParams` *struct*): `fn voice_name() -> &'static str`, `fn
  fields(&self) -> Vec<(&'static str, f32)>`, `fn from_fields(&[(String,
  f32)]) -> Self` — the shape `snapshot.rs` needs to save/load one voice's
  params as flat text (see Persistence below). `GrowlParams`/`BasicParams`/
  `GorgleParams` implement it.
- **`*Handle` structs** (`GrowlHandle`, `BasicHandle`, `GorgleHandle`): just
  the live `Shared` cells for one voice's params, cheap to clone (`Arc`
  bumps), what `AudioHandles`/`gui.rs` hold. The matching `*Voice` struct
  (`GrowlVoice`, `BasicVoice`, `GorgleVoice`) owns the real DSP state *and*
  a clone of the same handle — GUI writes go straight through the shared
  `Shared` cell, no sync needed. See "One module per voice" above for where
  each of these actually lives.

### `WavetableGen` (`growl.rs`) — Growl's patch

Reproduces one specific Vital synth patch (`growl.vital` in the repo root
— reference file, not loaded at runtime) as a fundsp `AudioNode` graph, 4
macro knobs (`bass_drive`, `filter`, `space`, `warp`) exposed via
`GrowlHandle`. `WavetableGen::clone()` resets to a fresh, un-warmed-up
instance (required by `AudioNode: Clone`, but means a cloned instance
doesn't carry over live oscillator/filter state — only ever clone it at
construction time, not mid-stream). `growl.rs` also holds
`GrowlParams`/`GrowlHandle`/`GrowlVoice` (appended after `WavetableGen`) —
the `Voice`-trait wrapper around it, plus the bolted-on NAM amp stage
(`nam_crossover`/`nam` model cycler on `GrowlHandle`, `NamStage` on
`GrowlVoice`, run from `on_block_start`).

### `GorgleGen` (`gorgle.rs`) — Gorgle's patch

Reproduces `gorgle.vital` (repo root, reference only) the same way Growl
reproduces its patch: one self-contained fundsp `AudioNode`, 4 macro knobs
(`wobble`, `ambience`, `girgle`, `grind`) exposed via `GorgleHandle`. A
meaningfully different patch, not a copy-paste of growl.rs's structure —
3 oscillators (one InharmonicScale-morphed and phase-warped, one LowPass-
morphed 16-voice unison, one silent-until-`girgle` FM layer) feeding two
*comb* filters in series (Vital `FilterModel::kComb`, hand-rolled feedback
delay lines — see `CombFilter` in `gorgle.rs`, ported from
`vital_test/src/synthesis/filters/comb_filter.cpp:35-50`) rather than
growl.rs's Smear-morph-into-a-highpass approach. Same `Clone` caveat as
`WavetableGen`: resets to a fresh instance, only clone at construction.
Same bundling as `growl.rs`: `GorgleParams`/`GorgleHandle`/`GorgleVoice`
are appended after `GorgleGen` in this same file (no separate NAM stage on
this one).

### Adding a new Voice

Follow `BasicVoice` (simplest complete example, `basic.rs`): give it its
own module (or append to an existing patch file if it wraps a generator
that already has one, e.g. `growl.rs`/`gorgle.rs`) — don't add it to
`voice.rs`. Pick an unused `INDEX`, `#[derive(Clone)]` struct, implement
`AudioNode<Inputs = U2, Outputs = U2>` + `Voice` (silence-unless-selected
check first thing in `tick()`), forward `set_sample_rate` to every child
unit you own. Add its `*Params`/`*Handle` pair if it has GUI-tunable params
(same shape as `GrowlParams`/`GrowlHandle`), re-export both from `mod.rs`,
wire the handle through `AudioOutput`/`Engine` construction and
`AudioHandles`, add it to `Engine` as a field ticked every sample, and add
both a `VOICE_NAMES` entry and a `draw_voice_*` match arm in `gui.rs`.

### Persistence

No `serde` (or any serialization crate) anywhere in this project — don't
add one for something this small. `src/audio/snapshot.rs` saves one
voice's live params (via its `VoiceParams::fields()`) as a flat
`name=value` text file, one new file per save named
`snapshots/{voice_name}_{NNNN}.snap` (monotonic per-voice index, scanned
from existing files, never overwrites) — **not** the old timestamped
`snapshots/<unix-secs>.snap` single-file-for-everything format from
before this refactor; old snapshots in that format won't parse under the
current loader (they used dotted `field.cell` keys like
`attack.default`/`attack.pitch` from the removed weight-matrix system) —
if you find any, they're stale and safe to delete. `snapshot.rs` has this
project's only `#[cfg(test)]` unit test (save/load round-trip + monotonic
indexing) — everything else is still verified by ear (see "Conventions").

## GUI (`src/gui.rs`)

Dear ImGui via `imgui` + `imgui-winit-support` + `imgui-glow-renderer`,
windowed with `winit`/`glutin`/`glow`. **Immediate mode** — `draw_ui()`
runs fresh every frame from `gui::run`'s winit event loop
(`RedrawRequested`), there's no retained widget tree. Any state that needs
to persist *across* frames (which snapshot is selected, whether the
audition-note button is currently held) has to live as a local variable in
`gui::run()`'s own scope and be threaded in by `&mut` each frame — see
`AuditionState`/`SnapshotBrowser` for the pattern. Everything else (knob
values, voice selection) is read straight out of the same `Shared` cells
the audio thread reads, so there's nothing to keep in sync — the widget
*is* the state.

- **Custom-drawn controls, not imgui's built-in widgets, for most knobs.**
  `knob()` and `xy_pad()` (top of gui.rs) are hand-drawn on `ui.get_window_draw_list()`
  — click-drag-vertical-delta to change value (`knob`) or click-anywhere-in-box
  (`xy_pad`), not imgui's `Slider`/`Drag`. `imgui::Drag` is still used for
  exactly one thing: the audition-note pitch field. Every knob's `(lo,
  hi)` range is a **hardcoded literal at its call site** in `gui.rs` (e.g.
  `("amp_boost", "boost", 1.0, 4.0, &audio.amp_boost)` in
  `draw_engine_panel`) — there is no more data-driven range table; if you
  add a param, its range lives only in the `gui.rs` call site (and
  wherever else clamps it, if anywhere).
- **`AudioHandles`** (`audio::mod.rs`) is the single bundle of everything
  the GUI needs from a running `AudioOutput`: `audition_note`,
  `voice_selected` + one `*Handle` per voice (`growl`, `basic`), the
  main/dry sub + thump knobs, `amp_*`/`reverb_*`/`limiter_*` cells, and
  `capture` (for `--test`). `AudioOutput::handles()` builds one.
  `main.rs` threads a single `Option<AudioHandles>` into `gui::run`
  (`None` when running the OSC backend, which has no tunable params).
  **When adding a new audio-thread handle for the GUI, add a field here**
  rather than a new positional parameter on `gui::run`/`draw_ui`.
- **Module-card pattern**: `draw_module_card(ui, title, bypass_cell,
  size, body_fn)` — a bordered `child_window` with a title, an optional
  top-right bypass checkbox bound to a `Shared` (checked = bypassed), and
  a body closure. `draw_knob_row(ui, &[(id, label, lo, hi, cell), ...])`
  lays out a `Shared`-backed knob per entry side-by-side inside one. This
  is the repeated shape for every card in `draw_engine_panel` (Main Sub,
  Dry Sub, Thump, Amp, Reverb, Limiter) — reuse it for a new fixed-knob
  card rather than hand-rolling layout.
- **Voice selector / snapshot browser**: `draw_voice_selector` cycles
  `voice_selected` through `VOICE_NAMES` with `< Voice`/`Voice >` buttons
  (same "< label >" idiom used elsewhere for cyclers, e.g.
  `NamModelCycler::cycle` in the orphaned nam.rs path). `SnapshotBrowser`
  (GUI-thread-local state, see above) drives save/load against whichever
  voice is currently selected.
- **Hydra panel** (`draw_hydra_panel`): shows mock wand controls (two
  `xy_pad` joysticks + trigger/button toggles) when running on the mock
  backend, or a plain "hardware connected" message on real hardware; a
  center column of `signal_override_row`s (one per `SignalState` field —
  bend/filter/fuzz/width/thump/velocity/acceleration/jerk) lets the GUI
  take a field over from whatever normally computes it, via
  `ZgicabraBridge`/`SignalOverride` (`zgicabra.rs`) — same override
  mechanism `hydra::mock`'s MIDI listener uses to push CC/pitch-bend
  values in (see below), so a MIDI controller and the GUI's own knobs
  would fight over the same field if both drove it live.
- **Screenshot self-check**: `ZGICABRA_GUI_SCREENSHOT=<path>` env var
  makes the window capture its own framebuffer to a PNG on frame 10 and
  exit — bypasses macOS's screen-recording permission prompt, useful for
  an agent/CI verifying a GUI change actually rendered without a human
  watching. See `save_screenshot` in gui.rs.

## Hydra / mock backend (`src/hydra/`)

- **`Backend` trait** (`mod.rs`): `update`, `should_quit`,
  `take_voice_cycle`/`take_tune_cycle` (net cycle-direction accumulators,
  drained each engine tick), `mock_controls() -> Option<MockControls>`
  (`None` on real hardware), `take_midi_notes() -> Vec<DeltaEvent>`
  (drained MIDI note on/off events, default no-op — only `MockBackend`
  actually implements it).
- **`MockBackend`** (`mock.rs`) owns an **optional MIDI input connection**
  via `hydra::midi` (`midi.rs`), a small platform-split module rather than
  inline `#[cfg]`s in `mock.rs`/`main.rs`: `midi::connect(notes) ->
  (MidiState, Connection)` has a `real` implementation (behind
  `#[cfg(all(target_os = "macos", target_arch = "x86_64"))]`, backed by
  `midir`) and a `stub` implementation for every other target that returns
  an inert `MidiState` (all-zero atomics, `connected: false`) and a
  zero-sized `Connection` — so `mock.rs` and `main.rs` call the same API
  unconditionally and never need their own `#[cfg]`. `midir` itself is
  scoped macOS-only in `Cargo.toml` (see readme.md's Build Toolchain
  section) — the Linux performance machine always has real Hydra hardware
  and this MusNix audio setup has no ALSA dev headers, so `midir`
  (`alsa-sys` on Linux) must never be a plain dependency.
  On construction `midi::connect` grabs the first available MIDI input
  port, if any (quietly proceeds without one otherwise — same "works fine
  with nothing plugged in" ethos as everything else here). CC 1-4
  (filter/width/fuzz/thump) and pitch-bend feed straight into `AtomicF32`
  cells in `MidiState` that `main.rs::run_engine_loop` pushes onto
  `ZgicabraBridge`'s `SignalOverride`s each tick (`mc.midi.connected`
  guards this — see the block right after `zgicabra::update` in
  `run_engine_loop`); Note On/Off go through the normal `DeltaEvent`
  pipeline instead (`hydra::take_midi_notes`), monophonic/last-note-
  priority same as a single wand trigger. `MockControls` (the GUI-facing
  handle) exposes the same `midi: MidiState` read-only, so `gui.rs` *could*
  show MIDI state, though nothing currently renders it.
- Keyboard mapping unchanged from before: `z`/`.` toggle wand triggers,
  `a`/`s` cycle voice, `-`/`=` cycle tune, arrow keys drive the left
  stick to full deflection (toggle, not held — terminals don't deliver
  key-up). Right wand stick + all 4-button rows per wand are GUI-only
  (`xy_pad`/`toggle_checkbox` in gui.rs), no keyboard equivalent.

## Conventions / things to not re-litigate

- `main.rs` has `#![allow(dead_code, unused_imports, unused_variables)]`
  at the top — this project tolerates unreferenced code (fm.rs, reese.rs,
  stutter.rs, crusher.rs, filter.rs, gen_node.rs, fx_node.rs, and
  nam.rs's cycling path — see above) sitting around for later reuse
  rather than deleting-then-recreating. **Don't "clean up" unreferenced
  audio modules unless asked**, and don't assume something is wired into
  `Engine` just because its `mod` declaration compiles — check
  `Engine::tick_pre_nam`/`run_nam`/`tick_post_nam` for what's actually
  ticked.
- `Cargo.toml`'s `[profile.dev] debug-assertions = false` exists to work
  around an imgui-rs 0.12 UB precondition check that only trips in debug
  builds on an empty draw list — unrelated to anything in this project's
  own code, don't second-guess it.
- `NAM_SAMPLE_RATE = 48_000` is forced for the output device
  (`pick_output_config`) because every NAM model in `nam/` was captured at
  that rate — don't let a generator/effect assume `DEFAULT_SR` or any
  other rate; always honor `set_sample_rate()`.
- Verification is still mostly ears-driven — `cargo build` + actually
  running `--gui ` (or `--gui --test` for the automated
  signal-present check, see above) is the primary loop this project uses.
  `snapshot.rs` has one `#[cfg(test)]` unit test; nothing else does.
  `cargo build` clean is necessary but not sufficient; flag when you
  can't verify audibly.
- `zgicabra.rs`'s `SignalState.lfo: [f32; 4]` field, and its doc comment
  referencing `audio::VoiceParams' lfo_rate/lfo_depth` and
  `VoiceEngine::tick`, are **stale leftovers from the removed
  weight-matrix system** — nothing sets or reads `.lfo` anywhere in the
  current engine. Don't treat the comment as describing real behavior;
  it's dead and safe to delete whenever someone's touching that file, but
  hasn't been asked for yet.
- Grep the *installed* crate source under
  `~/.cargo/registry/src/*/fundsp-0.23.0/src/` before assuming a fundsp
  function's signature/generic-ness, especially anything reached through
  `prelude64` — see the gotchas above. Guessing costs more compile-error
  round trips than a 10-second grep.

## Non-Rust reference material in the repo root (not part of the build)

- `growl.vital` — the source Vital synth patch that `growl.rs` reproduces.
  Reference only, never loaded at runtime.
- `gorgle.vital` — the source Vital synth patch that `gorgle.rs`
  reproduces. Reference only, never loaded at runtime.
- `panel.html`/`panel.html.png` — a saved snapshot of a separate sibling
  web project (`zgi-panel`, Svelte-based) that gui.rs's layout comments
  occasionally reference as "the layout mockup." Not part of this repo's
  own UI, not built by anything here.
- `vital_test/` — a vendored copy of the Vital synth's own source tree
  (has its own nested `.git`), kept as a reference for `.vital` patch
  format / DSP behavior. Not a Cargo workspace member.
- `hid_test/` — a standalone side Cargo project (own `Cargo.toml`/target)
  for probing the macOS raw-HID path independent of the main binary; not
  built as part of the main `cargo build`.
