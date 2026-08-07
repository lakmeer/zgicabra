# AGENTS.md

| ⚠️ Important: Full documentation on FunDSP is available in ref/fundsp.md.

Working notes for agents touching this codebase — architecture, the fundsp
API surface actually in use, and the conventions this project has settled
on. Read this before touching `src/audio/` or `src/gui.rs`.

## What this is

A Rust synth/controller app built around a Razer Hydra-style two-wand
controller ("Sixense"). Wand motion/triggers/buttons drive a live audio
engine (or, alternately, OSC out to a DAW). There's an optional imgui
tuning/debug GUI for editing engine parameters and driving a mock version
of the controller from a keyboard/mouse when no real hardware is attached.

Entry point: `src/main.rs`. Run modes selected by CLI flags
(`tools::parse_args`): `--audio` (default) vs `--osc` output backend,
`--gui` to open the tuner window, `--no-ui` to suppress the terminal UI.

## Process / thread model

- **No real hardware on macOS dev machines** — `src/hydra/` has `sdk.rs`
  (Linux, real Sixense SDK), `hid.rs` (macOS, raw USB HID), and `mock.rs`
  (keyboard/mouse-driven stand-in, active whenever no real backend
  connects). `src/hydra/real.rs` no longer exists (removed) — don't look
  for it, don't re-add it.
- **Engine loop** (`main.rs::run_engine_loop`): polls hydra → derives
  `Zgicabra` state (`zgicabra::update`) → emits `DeltaEvent`s (note on/off,
  voice change, etc.) and a continuous `SignalState` → feeds both to
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

## Cross-thread communication: no channels, only atomics

There is **no mpsc/crossbeam channel anywhere** in this codebase, and no
`Arc<Mutex<_>>` for engine state. Every value that needs to cross from the
GUI thread (or the engine-loop thread) into the real-time audio thread is
one of:

- **`fundsp::shared::Shared`** — an `Arc`-wrapped atomic f32 cell,
  `.value()` / `.set_value()`. This is what almost everything uses: note
  freq/gate, every live-tunable synth parameter, NAM model/IR selection,
  the audition-voice generator selectors.
- **`tools::AtomicF32`** — the project's own hand-rolled lock-free f32 cell
  (bit-cast through `AtomicU32`), used where `fundsp::Shared` isn't already
  in scope (mock hydra stick position, `zgicabra::SignalOverride`).
- **`Arc<AtomicBool>`** for flags (quit signal, override-enabled toggles).

The pattern throughout: a struct on the audio-engine side owns the
"master" `Shared`/`AtomicF32` cell; a cheap `.clone()` of it (just bumps
an `Arc` refcount) is handed out to the GUI. GUI writes, audio thread
reads on its next tick/block — no synchronization needed beyond the
atomic itself, because these are all *control-rate* values (racing a
single float write against a read is fine; there's no need for
sample-accuracy here).

**Never introduce a channel or a mutex for new engine parameters.** If you
need a new live-tunable value, make it a `Shared` (or a `ParamSpec`, see
below), construct it on the `AudioOutput`/`VoiceEngine` side, and hand a
clone to the GUI.

## fundsp: what's actually in play

`fundsp` (v0.23) provides two related traits — know which one you're
implementing:

- **`AudioNode`** (`audionode.rs`): the generic, compile-time-sized trait.
  `type Inputs: Size<f32>`, `type Outputs: Size<f32>` (fundsp's own
  typenum-style sizes: `U0`, `U1`, `U2`, `U7`, ...), `fn tick(&mut self,
  input: &Frame<f32, Self::Inputs>) -> Frame<f32, Self::Outputs>`.
  Requires `Self: Clone`. This is what you implement for a new
  self-contained DSP voice/effect (see `src/audio/stutter.rs`,
  `src/audio/audition.rs`, `src/audio/nam.rs`'s `NamStage` for examples).
  `An<X>` wraps an `AudioNode` and makes it composable with `>>` / `|` /
  etc combinator syntax, and gets a **blanket impl of `AudioUnit`** for
  free — that's the bridge to the next trait.
- **`AudioUnit`** (`audiounit.rs`): the dynamic, runtime-sized,
  object-safe trait — `fn tick(&mut self, input: &[f32], output: &mut
  [f32])`, `fn inputs(&self) -> usize`, `fn outputs(&self) -> usize`, plus
  `fn process(&mut self, size: usize, input: &BufferRef, output: &mut
  BufferMut)` for block processing (used for the NAM convolution/model
  path, capped at `MAX_BUFFER_SIZE` per call). `Box<dyn AudioUnit>` **is
  `Clone`** — the trait has a `DynClone` supertrait and fundsp calls
  `dyn_clone::clone_trait_object!(AudioUnit)` — so it's safe to hold
  `Box<dyn AudioUnit>` fields on a `#[derive(Clone)]` struct (used
  throughout `VoiceEngine` and `AuditionVoice`'s generator list).

Practical rule of thumb used in this codebase: implement `AudioNode`
by hand when you know the exact input/output arity at compile time (most
voices/effects); reach for `Box<dyn AudioUnit>` when you need to hold
**heterogeneous concrete types** in one field/collection (e.g.
`AuditionVoice` holding over a dozen different generator types), or when a value
crosses an API boundary that wants type erasure (`VoiceEngine.noise`,
`VoiceEngine.envelope`, `AudioOutput`'s `post_nam`/`reverb`).

### `prelude64` gotchas (bit us during this pass — check before assuming)

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
  (`FmVoice`, currently unreferenced — see below), `0x7A_11` (`ReeseVoice`,
  same), `0x7A_12` (`NamStage`), `0x7A_13` (`AuditionVoice`), `0x7A_14`
  (`StutterVoice`), `0x7A_20` (the `ParamSpec` → `AudioNode` adapter, see
  below). Next free custom ID: `0x7A_15` (or `0x7A_21`+ for adapters).
- `MAX_BUFFER_SIZE` bounds any single `AudioUnit::process()` call — the
  NAM convolver in particular re-runs its FFT block machinery per call, so
  driving it one sample at a time (`tick()`) causes audible stutter; it
  must be driven in `<=MAX_BUFFER_SIZE` chunks via `process()`. See the
  comment above `impl AudioNode for NamStage` in `src/audio/nam.rs`.

## Audio engine architecture (`src/audio/`)

This is **not** one big fundsp-composed graph wired with `>>`/`|`. It's
three hand-driven stages, manually sequenced once per sample/block inside
`build_stream`'s cpal callback (`src/audio/mod.rs`):

```
VoiceEngine::tick()  -->  NamStage::process_buffer()  -->  post_nam (filter+amp)  -->  reverb_stereo
   (per-sample)              (per-block, <=MAX_BUFFER_SIZE)   (per-sample)              (per-sample)
```

- **`VoiceEngine`** (mod.rs): owns every sound-generating piece and mixes
  them into a `(dry, bypass)` tuple per sample. Current pieces: 3x
  `AuditionVoice` (A/B/C, cycle through fundsp generators — see below), 2x
  plain `sine()` sub-oscillators (`sub` feeds the NAM-processed dry path,
  `bypass_sub` skips NAM entirely and gets summed back in post-NAM), a
  filtered-noise layer, `StutterVoice` (triangle × white-noise ring mod),
  and an `adsr_live` envelope gated by `gate: Shared`. `tick_thump()`
  layers a percussive pitch-decay bump onto `base_freq` on note-on.
  **FM (`FmVoice`) and Reese (`ReeseVoice`) voices were removed from the
  graph** (not deleted — `src/audio/fm.rs`/`src/audio/reese.rs` still
  exist on disk but are unreferenced by any `mod` declaration, so they
  don't compile into the binary). Re-wiring them means adding `mod fm;
  mod reese;` back, restoring their fields/params, and re-adding their
  contribution to `dry` in `VoiceEngine::tick()`.
- **`NamStage`** (`nam.rs`): amp-model (NAM neural amp sim) + cab IR
  convolution, block-driven. Model selection is a `Shared` index into a
  `Vec<Option<Arc<Mutex<Model>>>>` discovered at startup from `*.nam`
  files in `nam/` (index 0 is always "Bypass"). The one `Mutex` in the
  audio path wraps each `Model` purely because `nam-rs`'s `Model` isn't
  `Clone` and `AudioNode` requires `Self: Clone`; it's never actually
  contended since only the audio callback thread touches it. IR
  (cab-simulation impulse response) selection works the same way from
  `nam/ir/*.wav`, with a lazy-rebuild-on-change convolver (`apply_ir`) —
  that's the reference pattern for "expensive resource selected by a live
  index, only rebuilt when the index actually changes."
- **`post_nam`** (`build_post_nam` in mod.rs): filter (live cutoff driven
  by the `filter` `Shared`/SignalState) + amp, both boxed `Box<dyn
  AudioUnit>`.
- **Reverb**: `reverb_stereo(room_size, time, damping)`, all four knobs
  (`reverb_room_size`, `reverb_time`, `reverb_damping`, `reverb_level`) are
  ordinary `ParamSpec` entries in `VoiceParams`, same as everything else.
  `room_size`/`time`/`damping` are still baked into fundsp's FDN at
  `AudioOutput::new()` time (fundsp takes them as plain args, not live
  inputs — `param_factor(&voice_params.reverb_room_size, &SignalState::new())`
  is read once there to get each default), so editing them needs a
  process restart, same caveat as `attack`/`release` below. `reverb_level`
  (the dry/wet mix) *is* fully live: `VoiceEngine::tick()` now returns a
  3-tuple `(dry, bypass, reverb_level)` instead of 2, and `build_stream`
  carries `reverb_level` through its own per-block scratch array (same
  shape as the existing `bypass_scratch`) so it can be read per sample
  after the NAM block-processing pass. This is also where the engine
  switched from broadcasting one mono sample to every output channel to
  writing true L/R — if you touch `build_stream`'s final write loop,
  preserve that. If `VoiceEngine::tick()`'s return arity changes again,
  update both call sites (`build_stream`'s per-sample loop) together.

### The parameter/weight-matrix system (`ParamSpec` / `VoiceParams`)

This is the mechanism that makes "every experiment's params tweakable in
the GUI" *automatic* — new params need **zero new GUI code**. Understand
this before adding a knob.

- **`ParamSpec`** (mod.rs) is 10 `Shared` cells: `default`, `lo`, `hi`,
  `curve` (Linear/Exp), and 7 `weight_*` cells — one per `SignalState`
  field (`pitch`(=bend), `width`, `filter`, `fuzz`, `thump`, `velocity`,
  `acceleration`). `param_factor(spec, signal)` blends `default` toward
  `lo`/`hi` per-weight, so a param can be a static value (all weights
  zero, the common case) or something that swings with wand motion/state
  live, without changing any call site.
- **`VoiceParams`** is just a flat struct of named `ParamSpec` fields;
  `VoiceParams::entries()` returns them all as a fixed-size array in
  display order — **this is the single enumeration point** the GUI table
  (`gui.rs::draw_voice_params`), and the snapshot save/load
  (`audio::snapshot`), both iterate over. **To add a new tunable param:
  add one `ParamSpec::new(...)` field + one line in `entries()`'s array —
  nothing else.** The GUI grid, the snapshot format, and live audio-thread
  reads all pick it up automatically.
- Caveat inherited from the original design and still true: most params
  are read live every sample/control-tick via `param_factor`, but
  `attack`/`release` (baked into `adsr_live(...)` at `VoiceEngine::new()`
  time) only take effect after restarting the audio backend — fundsp
  doesn't expose ADSR times as live audio-rate inputs. Same caveat now
  applies to reverb's `room_size`/`time`/`damping`.

### Adding a new generator/voice type

Follow `src/audio/stutter.rs` (simplest complete example) or
`src/audio/audition.rs` (the "own every variant, switch via `Shared`
index" pattern, mirrors `NamStage`'s model-switching): `#[derive(Clone)]`
struct, pick an unused `0x7A_xx` ID, implement `AudioNode` with the
narrowest `Inputs`/`Outputs` that fit, forward `set_sample_rate` to every
child unit you own. Wire it into `VoiceEngine` (field + construction +
`set_sample_rate` + a line in `tick()`'s mix), add a `ParamSpec` for its
level if it should have a GUI-tunable level, and if it needs a live
selector, add a `Shared` cell (constructed in `AudioOutput::new()`,
threaded through `VoiceEngine::new()`, exposed via `AudioOutput::handles()`
— see `AuditionCycler` for the GUI-facing wrapper shape).

### Persistence

No `serde` (or any serialization crate) anywhere in this project — don't
add one for something this small. `src/audio/snapshot.rs` dumps the
entire `VoiceParams` weight matrix as a flat `name.cell=value` text file
per line, one new timestamped file per save (`snapshots/<unix-secs>.snap`,
never overwrites), reloaded by matching `spec.name`/`cell_name` strings
back onto `entries()`/`cells()`. If new `VoiceParams` fields are added
later, old snapshots still load fine (unknown lines are silently
skipped); they just won't set the new field.

## GUI (`src/gui.rs`)

Dear ImGui via `imgui` + `imgui-winit-support` + `imgui-glow-renderer`,
windowed with `winit`/`glutin`/`glow`. **Immediate mode** — `draw_ui()`
runs fresh every frame from `gui::run`'s winit event loop
(`RedrawRequested`), there's no retained widget tree. Any state that needs
to persist *across* frames (which snapshot is selected, whether the
audition-note button is currently held) has to live as a local variable in
`gui::run()`'s own scope and be threaded in by `&mut` each frame — see
`AuditionState`/`SnapshotBrowser` for the pattern. Everything else
(slider values, cycler selections) is read straight out of the same
`Shared`/`ParamSpec` cells the audio thread reads, so there's nothing to
keep in sync — the widget *is* the state.

- **`AudioHandles`** (`audio::mod.rs`) is the single bundle of everything
  the GUI needs from a running `AudioOutput` that *isn't* already reachable
  through `voice_params` — `nam_models`/`nam_irs` cyclers, `audition_note`,
  three `audition_a/b/c` cyclers. (Reverb has no entry here: all four of
  its params are plain `ParamSpec`s inside `voice_params` now, rendered by
  the generic table — no dedicated handle needed. That's the pattern to
  prefer: only add a field to `AudioHandles` for something that *can't* be
  a `ParamSpec`, e.g. because it's a selector index into a name list, or a
  cross-thread action trigger like `audition_note`.) `AudioOutput::handles()`
  builds one. `main.rs` threads a single `Option<AudioHandles>` into
  `gui::run` (`None` when running the OSC backend, which has no tunable
  params). **When adding a new audio-thread handle for the GUI, add a
  field here rather than a new positional parameter on
  `gui::run`/`draw_ui`** — that parameter list was already refactored once
  specifically to avoid this growing unbounded.
- **Cycler pattern**: any "pick one of N things, remember the selection"
  control (NAM model, cab IR, audition generator A/B/C) is a tiny
  `#[derive(Clone)]` struct wrapping a `Shared` index (+ an `Arc<Vec<...>>`
  name list, or a fixed `const` array for compile-time-known lists like
  `audition::GENERATOR_NAMES`), with `.selected_name()` and `.cycle(delta:
  i32)` (wrapping via `rem_euclid`). Rendered as `< label` / `label >`
  buttons plus a text line — see `draw_nam_model`/`draw_audition_voice`.
  Reuse this shape for any future "cycle through fixed options" control
  rather than inventing a new one.
- **Slider pattern**: `imgui::Drag::new(id).speed(...).range(lo,
  hi).build(ui, &mut value)` — read the `Shared`/atomic into a local
  `value`, pass `&mut value`, write back only if `build()` returns `true`
  (it returns `true` exactly on the frames the value changed). `drag_speed
  (lo, hi)` picks a step size proportional to the control's own range so a
  0..1 knob and a 100..14000 knob both feel reasonable to drag.
- `draw_voice_params` needs no changes when `VoiceParams::entries()`
  changes size — it iterates the array generically.

## Conventions / things to not re-litigate

- `main.rs` has `#![allow(dead_code, unused_imports, unused_variables)]`
  at the top — this project tolerates unreferenced code (like the
  currently-orphaned `fm.rs`/`reese.rs`) sitting around for later reuse
  rather than deleting-then-recreating. Don't "clean up" unreferenced
  audio modules unless asked.
- `Cargo.toml`'s `[profile.dev] debug-assertions = false` exists to work
  around an imgui-rs 0.12 UB precondition check that only trips in debug
  builds on an empty draw list — unrelated to anything in this project's
  own code, don't second-guess it.
- `NAM_SAMPLE_RATE = 48_000` is forced for the output device
  (`pick_output_config`) because every NAM model in `nam/` was captured at
  that rate — don't let a generator/effect assume `DEFAULT_SR` or any
  other rate; always honor `set_sample_rate()`.
- No test suite exists for the audio engine (it's ears-driven — `cargo
  build` + actually running `--gui --audio` is the verification loop this
  project uses). `cargo build` clean is necessary but not sufficient;
  flag when you can't verify audibly.
- Grep the *installed* crate source under
  `~/.cargo/registry/src/*/fundsp-0.23.0/src/` before assuming a fundsp
  function's signature/generic-ness, especially anything reached through
  `prelude64` — see the gotchas above. Guessing costs more compile-error
  round trips than a 10-second grep.
