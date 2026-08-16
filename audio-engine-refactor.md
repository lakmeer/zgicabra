# Audio engine refactor: unify Voice structs + MIDI-CC live tuning

This document is self-contained — written so an agent with no memory of the
conversation that produced it can execute the work. It describes the
*current* state of `src/audio/` (with `file:line` citations), the target
design, and an ordered list of concrete edits.

## Why

`src/audio/` currently defines three types per voice (`reese.rs`, `growl.rs`,
`basic.rs`, `swarm.rs`):

- `*Params` — a plain `Copy` struct of raw `f32`s. Used only as a
  `Default`-able snapshot shape for `src/audio/snapshot.rs`'s save/load and
  as the seed value passed to `*Handle::new`.
- `*Handle` — same field list as `*Params`, but every field is
  `fundsp::prelude64::Shared` (a cheap-clone `Arc<AtomicU32>` wrapper,
  lock-free get/set). This is what the GUI thread holds so it can read/write
  live values while the audio thread's own copy (inside `*Voice`, same
  underlying `Arc`) sees the change with no locking.
- `*Voice` — owns the real DSP nodes plus one `*Handle`.

This triple exists to arbitrate **two concurrent writers** of the same
value: the GUI thread (knob drag) and the audio thread (nothing, currently —
today the audio thread only *reads* param values, except `GrowlHandle`'s
`filter_live`/`warp_live`/`freq_mult_live` fields, which the audio thread
writes each tick for the GUI to read — see `src/audio/growl.rs:357-359` and
`:450-460`).

**Decision made this session:** replace GUI knob-dragging as the live-tuning
mechanism with a MIDI CC controller consumed directly inside the audio
thread. Once the audio thread is the *only* writer of every per-voice value
(both tunable params and telemetry like Growl's `_live` fields), the
write-arbitration reason for having two separate types (`Params` vs
`Handle`) disappears. `*Params` and `*Handle` collapse into `*Voice` itself:
fields anything outside the audio thread needs to *read* stay `Shared`
(cheap, lock-free, single-writer-many-reader is exactly what `Shared` is
good at); fields nothing outside the audio thread needs stay plain.

Also decided:
- The new CC pipeline is a **second, separate MIDI connection**, independent
  of the existing note/Program-Change simulation path in
  `src/hydra/midi.rs` — that file's `connect_midi` (`src/hydra/midi.rs:91-144`)
  is **not modified**.
- `src/audio/snapshot.rs` is **dropped for this pass** — disconnect its call
  sites, don't delete the module, leave it as unused/dead code to revisit
  later.
- GUI's per-voice knobs become **read-only display**. Global mix-stage knobs
  (master_vol, reverb, limiter, amp — the ones on `AudioOutput`/`Engine`
  directly, not per-voice) are **untouched**, stay GUI-editable.
- The CC pipeline must build and work on **both** the macOS dev machine and
  the Linux performance rig. The current `midir` platform gate
  (`Cargo.toml:33-35`: `cfg(all(target_os = "macos", target_arch = "x86_64"))`)
  is based on a stale comment claiming the Linux/MusNix box has no ALSA —
  that's false; `cpal` (`Cargo.toml:18`, unconditional dependency) already
  requires ALSA on Linux and works fine there today. Fixing this gate/comment
  is part of the plan (item 0 below).

## Current-state facts (verified, with citations)

### `src/audio/voice.rs`
```rust
pub trait Voice: AudioNode<Inputs = U2, Outputs = U2> {
    const INDEX: usize;
    fn name (&self) -> &'static str;
    fn set_signal (&mut self, bend: f32, filter: f32, fuzz: f32, width: f32, thump: f32);
    fn on_block_start (&mut self, _block_len: usize) {}
}
```
(`voice.rs:17-29`). Also defines `pub(super) struct ThumpMod` (`voice.rs:34-70`,
unaffected by this refactor) and:
```rust
pub trait VoiceParams: Default {
    fn voice_name () -> &'static str;
    fn fields (&self) -> Vec<(&'static str, f32)>;
    fn from_fields (fields: &[(String, f32)]) -> Self;
}
```
(`voice.rs:74-78`) — this trait is deleted as part of dropping `snapshot.rs`
support.

### Per-voice `Params`/`Handle` shape (all four voices follow this pattern; `growl.rs` shown as reference)

`growl.rs:294-394`:
```rust
#[derive(Clone, Copy)]
pub struct GrowlParams {
    pub bass_drive: f32, pub filter: f32, pub space: f32, pub warp: f32, pub nam_crossover: f32,
}
impl Default for GrowlParams { /* fixed defaults */ }
impl VoiceParams for GrowlParams { /* fields()/from_fields() by string name */ }

#[derive(Clone)]
pub struct GrowlHandle {
    pub bass_drive_input: Shared, pub filter_input: Shared, pub space_input: Shared,
    pub warp_input: Shared, pub nam_crossover_input: Shared,
    pub filter_live: Shared, pub warp_live: Shared, pub freq_mult_live: Shared,
}
impl GrowlHandle {
    pub fn new (params: &GrowlParams) -> GrowlHandle { /* shared(params.x) per field, _live fields shared(0.0) */ }
    pub fn params (&self) -> GrowlParams { /* .value() per field */ }
    pub fn load (&self, params: &GrowlParams) { /* .set_value() per field */ }
}
```
`GrowlVoice` (`growl.rs:399-413`) owns `handle: GrowlHandle` plus the real
`WavetableGen`/`NamStage` DSP state. `filter_live`/`warp_live`/`freq_mult_live`
are written every tick at `growl.rs:450-460` — this is the one place in the
codebase today where the audio thread already writes into a `Shared` cell
for external (GUI/ui.rs) consumption; it's the template for what every
tunable param becomes under the new design.

Exact same three-type shape, different field lists, for:
- `ReeseParams`/`ReeseHandle`/`ReeseVoice` — `reese.rs:30-40`, `86-135`, `137-156` (fields: detune, sub_level, drive, cutoff, resonance, lfo_rate, lfo_depth, width — 8 total, no `_live` fields today).
- `BasicParams`/`BasicHandle`/`BasicVoice` — `basic.rs:12-19`, `56-93`, `96-105` (fields: sin_level, tri_level, square_level, saw_level, saturation — 5 total, no `_live` fields today).
- `SwarmParams`/`SwarmHandle`/`SwarmVoice` — `swarm.rs:121-128`, `165-211`, `284-303` (fields: chase_factor, radius, orbit_speed, phaser_depth, xover_freq — 5 total, plus `nam_lo`/`nam_hi: NamModelCycler` on the handle for NAM model selection, which is a separate concern — leave `NamModelCycler` as-is, just fold the plain `Shared` fields).

### `src/audio/mod.rs` top-level structure

- `AudioHandles` (`mod.rs:132-164`) — GUI-facing clone bundle: `test_tone`,
  `voice_selected`, `voice_a: ReeseHandle`, `voice_b: GrowlHandle`,
  `voice_c: BasicHandle`, `voice_d: SwarmHandle`, ~15 more top-level `Shared`
  fields (mix/amp/reverb/limiter/master_vol), `capture`, `errors`.
- `AudioOutput` (`mod.rs:166-208`) — same field list as `AudioHandles` minus
  `test_tone`, plus `stream: cpal::Stream`. Public control-thread type.
- `AudioOutput::handles()` (`mod.rs:211-239`) — builds an `AudioHandles` by
  cloning every field individually (~25 lines).
- `Engine::new(...)` (`mod.rs:476-520`) — **30 positional parameters**,
  called once from `AudioOutput::new()` (`mod.rs:294-338`) with matching
  30 arguments (mostly `.clone()` of the same `Shared` cells `AudioOutput`
  keeps).
- `Engine` (`mod.rs:431-473`) — the real per-sample DSP graph, owns
  `voice_a: ReeseVoice`, `voice_b: GrowlVoice`, `voice_c: BasicVoice`,
  `voice_d: SwarmVoice` plus `amp_l`/`amp_r: nam::NamStage` (two independent
  instances — comment at `mod.rs:455-456` explains why, do not merge),
  `reverb: ReverbFx`, `limiter: Compressor`. Moved wholesale into the `cpal`
  audio callback closure via `build_stream` (`mod.rs:651-717`) — this
  Engine/AudioOutput split is structural (real-time audio thread vs control
  thread) and is **not** touched by this refactor, only simplified.
- `build_stream` callback body (`mod.rs:669-717`): per-block loop already
  calls `on_block_start` conditionally per selected voice
  (`mod.rs:687-691`) before the per-sample `tick_pre_nam`/`run_nam`/
  `tick_post_nam` calls (`mod.rs:693-709`) — the new CC-queue drain goes
  right next to the existing `on_block_start` dispatch, same per-block cadence.

### `src/hydra/midi.rs` (existing MIDI infra — reference, not modified except the Cargo.toml gate)

- Uses `midir = "0.11.0"` (`Cargo.toml:35`), gated
  `cfg(all(target_os = "macos", target_arch = "x86_64"))` (`midi.rs:50`);
  non-matching targets get an inert stub (`midi.rs:165-174`).
- `connect_midi` (`midi.rs:91-144`): `MidiInput::new("zgicabra")`, takes the
  **first available port** (`midi.rs:95-96`), connects with a closure that:
  - Note On/Off/Note-On-vel-0 → `DeltaEvent::NoteStart/NoteChange/NoteEnd`,
    monophonic last-note-priority, pushed onto
    `notes: Arc<Mutex<VecDeque<DeltaEvent>>>` (`midi.rs:121-134`).
  - Program Change → `DeltaEvent::VoiceChange` (`midi.rs:138-140`).
  - CC 1-4, 7-8 (`CC_FILTER`/`CC_WIDTH`/`CC_FUZZ`/`CC_THUMP`/`CC_ROT_LEFT`/
    `CC_ROT_RIGHT` constants, `midi.rs:59-64`) → stored into
    `MidiState`'s `Arc<AtomicF32>` fields (`midi.rs:105-116`); all other CC
    numbers are silently dropped (`midi.rs:114`, `_ => {}`).
  - Pitch bend → `MidiState.bend` (`midi.rs:117-120`).
  - No port found → prints a message, returns `None`/inert state
    (`midi.rs:157-159`) — this is the no-op-safe fallback pattern to mirror
    for the new CC connection.
- Consumed via `MockBackend` (`src/hydra/mock.rs:146-148,166-167,205-207`)
  and `hydra::take_midi_notes` (`src/hydra/mod.rs:229-231`), merged into
  `delta_events` in the main loop (`src/main.rs:181-182`). CC-derived
  continuous values (filter/width/fuzz/thump/bend) go through a *different*
  path — not `DeltaEvent`, but a `SignalOverride` bridge
  (`src/main.rs:184-200`) that only runs `if mc.midi.connected` (i.e. only
  when the mock/macOS backend is active). **None of this existing CC
  handling is per-voice — it only ever feeds the four global performance
  signals.** The new pipeline is additive, for a second connection, targeting
  per-voice params instead.
- No ring-buffer/lock-free-queue crate exists in `Cargo.toml` today — the
  existing hand-off uses `Arc<Mutex<VecDeque<DeltaEvent>>>` and
  `crate::tools::AtomicF32`. Fine for control-thread-to-control-thread
  hand-off; **not** what should be used for the new audio-thread-internal
  path (a `Mutex` inside a real-time audio callback risks priority
  inversion/glitches) — this refactor adds a proper lock-free SPSC queue
  instead (see item 3 below).

### `src/gui.rs` (imgui-based control panel)

- `draw_shared_knob` (`gui.rs:145-150`) — **always editable**: reads
  `cell.value()`, drag-writes back via `cell.set_value(value)`. No read-only
  variant exists anywhere in `gui.rs` today.
- `draw_knob_row` (`gui.rs:293-298`) — thin loop calling `draw_shared_knob`
  per `(id, label, lo, hi, &Shared)` tuple; every per-voice knob funnels
  through this.
- Per-voice draw functions, all take a `&*Handle` and call `draw_knob_row`:
  - `draw_voice_growl(ui, growl: &GrowlHandle)` — `gui.rs:313-321`, 5 knobs
    (the `_input` fields only; `_live` fields are **not** drawn in `gui.rs`
    at all today — only printed via raw `print!` in `src/ui.rs:107-128`'s
    termion panel, comment there says "Only Growl is wired up so far").
  - `draw_voice_basic(ui, basic: &BasicHandle)` — `gui.rs:323-331`, 5 knobs.
  - `draw_voice_reese(ui, reese: &ReeseHandle)` — `gui.rs:333-346`, 8 knobs
    across two rows.
  - `draw_voice_swarm(ui, handle: &SwarmHandle)` — `gui.rs:349-369`, 5 knobs
    + two NAM-model-cycler button rows. **Not currently called** from
    `draw_voice_card` — the 4th card slot is a static "(no patch — silent)"
    placeholder (`gui.rs:391-393`). Out of scope to wire this up; leave as
    dead-but-compiling like today, just update its signature for
    consistency with the other three.
  - `draw_voice_card(ui, audio: &AudioHandles)` — `gui.rs:373-394`, calls
    the three wired voice draw fns with `&audio.voice_a/b/c`
    (`gui.rs:379-393`).
- Snapshot save/load buttons: `gui.rs:403-405` (save), `gui.rs:444-446`
  (load, calls `audio.voice_a/b/c.load(&Params::from_fields(...))`) — these
  are the call sites to remove when dropping `snapshot.rs` support.

### `Cargo.toml` relevant deps
- `fundsp = "0.23.0"` (`:16`), `cpal = "0.18.1"` (`:18`),
  `midir = "0.11.0"` under the macOS-only target gate (`:33-35`),
  `hidapi = "2"` alongside it (`:34`, needed for real Hydra hardware — do
  **not** change this gate, only `midir`'s).
- No ring-buffer crate present.

## Plan

### 0. Fix the stale platform gate (do this first, small and independent)

In `Cargo.toml`:
- Update the comment at `Cargo.toml:27-32` — remove the claim that the
  Linux/MusNix performance box lacks ALSA (false; `cpal` already requires
  and gets ALSA there).
- Relax `midir`'s target gate (`Cargo.toml:33-35`) so it also builds for the
  Linux performance rig's target triple, not just
  `cfg(all(target_os = "macos", target_arch = "x86_64"))`. Leave `hidapi`'s
  gate (`Cargo.toml:34`) as-is — that one's real-hardware-only, unrelated to
  this fix.
- Verify: `cargo build` on both macOS and the Linux target succeeds with
  `midir` compiled in (not the inert stub).

### 1. Collapse `*Params`/`*Handle` into `*Voice`, per voice

For each of `reese.rs`, `growl.rs`, `basic.rs`, `swarm.rs`:
- Delete the `*Params` struct and its `Default`/`VoiceParams` impls.
- Delete the `*Handle` struct and its `new`/`params`/`load` impls.
- Add every former field directly onto the `*Voice` struct: `Shared` for
  anything that needs external (GUI) visibility (all former `*_input`
  fields, plus any existing `_live` fields like Growl's), plain otherwise.
  Voice's own `new(...)` constructor takes whatever raw config it needs
  (equivalent to what `*Params::default()` used to seed) and builds its own
  `Shared` cells directly — e.g. `GrowlVoice::new` no longer takes a
  `GrowlHandle` parameter, it builds `bass_drive: shared(0.8)` etc. inline
  (same default values currently in `impl Default for GrowlParams`,
  `growl.rs:303-313`, and the equivalent `Default` impls in the other three
  files).
- `SwarmVoice` keeps `nam_lo`/`nam_hi: NamModelCycler` as a separate concern
  (unrelated to this refactor's `Shared`-vs-plain question) — just move it
  from `SwarmHandle` onto `SwarmVoice` directly.

In `voice.rs`:
- Delete the `VoiceParams` trait (`voice.rs:74-78`) entirely.
- Add to the `Voice` trait (`voice.rs:17-29`):
  ```rust
  fn apply_cc (&mut self, cc: u8, value: f32);
  ```
- Each voice implements it with a small match, e.g. for Growl:
  ```rust
  fn apply_cc (&mut self, cc: u8, value: f32) {
      match cc {
          20 => self.bass_drive.set_value(value),
          21 => self.filter.set_value(value),
          22 => self.space.set_value(value),
          23 => self.warp.set_value(value),
          24 => self.nam_crossover.set_value(value),
          _ => {},
      }
  }
  ```
  (Pick a non-overlapping CC number range per voice, distinct from the
  already-used CC1-4,7-8 in `hydra/midi.rs` — e.g. reserve CC 20+ for
  per-voice params, document the mapping in a comment since there's no
  registry file for this today.)

### 2. Simplify `mod.rs`'s three-struct field duplication

- Introduce one bag type (name it `Handles`) holding every `Shared`/
  `*Handle`-successor field that both `AudioOutput` and `AudioHandles`
  currently declare separately (`mod.rs:132-164` and `:166-208`) — this is
  now a bag of **read-only-from-outside** `Shared` clones per voice (since
  writes are audio-thread/MIDI-CC-only now) plus the existing global
  performance-signal and mix-stage `Shared` cells (those still work exactly
  as today, `AudioOutput::handle_signal`/`handle_event` at `mod.rs:724-751`
  are unaffected).
- `AudioOutput` becomes `{ handles: Handles, capture: AudioCapture, errors: AudioErrors, stream: cpal::Stream }`.
- `AudioOutput::handles()` (`mod.rs:211-239`) collapses to cloning `self.handles` plus `test_tone`/`capture`/`errors`.
- `Engine::new(...)` (`mod.rs:476-520`) takes the relevant subset directly
  rather than 30 positional params — pass `Handles` (or split pieces of it)
  in as needed; construct each concrete `*Voice` from its own defaults per
  item 1 (no longer takes a `*Handle` argument at all, since that type is
  gone) plus whatever per-voice config *does* still need to come from
  outside (none currently, given all per-voice values are self-initialized
  defaults now).

### 3. New CC ingestion pipeline

- Add a ring-buffer crate to `Cargo.toml` (e.g. `rtrb` — small, wait-free
  SPSC, designed for exactly this real-time-audio hand-off; there is no
  existing crate in the dependency tree to reuse for this, confirmed above).
  This is the one new external dependency this plan introduces.
- New module `src/audio/cc_input.rs`:
  - Opens a **second**, independent `midir` connection (separate
    `MidiInputConnection` instance from `hydra/midi.rs`'s), scoped to CC
    messages only (`status & 0xF0 == 0xB0`).
  - Same no-port-found fallback behavior as `hydra/midi.rs:157-159` — no
    panic, no blocking, engine just never receives CC events.
  - Producer side (MIDI callback thread) does a non-blocking `try_push` of
    raw `(cc: u8, value: f32)` (value pre-normalized `/127.0` like the
    existing code at `midi.rs:106`) onto the ring buffer. Drop-on-full is
    correct here — these are advisory current-knob-positions, not events
    needing guaranteed delivery.
  - Exposes the consumer half (`rtrb::Consumer<(u8, f32)>`) to `mod.rs`.
- In `Engine` (`mod.rs:431-473`): add a field for the ring-buffer consumer.
- In `build_stream`'s callback (`mod.rs:669-717`), right next to the
  existing per-block `on_block_start` dispatch (`mod.rs:687-691`): drain
  everything currently queued in the CC consumer, look up
  `engine.voice_selected.value() as usize`, and dispatch each `(cc, value)`
  to whichever concrete voice matches (mirroring the
  `if selected == ReeseVoice::INDEX { ... }` pattern already used at
  `mod.rs:688-691`), calling that voice's `apply_cc(cc, value)`.

### 4. GUI: read-only per-voice display

- Add `draw_shared_meter` next to `draw_shared_knob` (`gui.rs:145-150`) —
  same rendering, no drag-write-back (just display `cell.value()`, e.g. via
  a disabled/non-interactive variant of the existing `knob()` widget, or a
  plain label — match whatever the existing `knob()` helper supports for a
  read-only mode; if it doesn't support one, render via `ui.text` instead).
- Update `draw_voice_growl`/`draw_voice_basic`/`draw_voice_reese`/
  `draw_voice_swarm` (`gui.rs:313-369`) to:
  - Take `&GrowlVoice`/`&ReeseVoice`/`&BasicVoice`/`&SwarmVoice` (or a
    reference to whatever read-only view type wraps the `Shared` clones
    exposed via the `Handles` bag from item 2) instead of `&*Handle`.
  - Use `draw_shared_meter` instead of `draw_shared_knob` for every
    per-voice field.
  - For Growl specifically, also render `filter_live`/`warp_live`/
    `freq_mult_live` (currently only in `ui.rs`'s termion panel,
    `ui.rs:107-128`) via the same meter widget, for parity with what the
    other voices should eventually grow.
- `draw_voice_card` (`gui.rs:373-394`) call sites update to match the new
  parameter types.
- Global mix-stage knobs elsewhere in `gui.rs` (master_vol, reverb, limiter,
  amp — not per-voice) keep using `draw_shared_knob`, unchanged.
- Remove the snapshot save/load buttons (`gui.rs:403-405`, `:444-446`) —
  leave `snapshot.rs` itself compiling but unreferenced.

### 5. Cleanup pass

- `src/ui.rs`'s `draw_audio_panel` (`ui.rs:107-128`) can now read Growl's
  `_live` fields the same way it does today (field access paths change from
  `audio.voice_b.filter_live` to wherever they land on the new struct —
  update the path, logic unchanged).
- Grep for any remaining `GrowlParams`/`ReeseParams`/`BasicParams`/
  `SwarmParams`/`GrowlHandle`/`ReeseHandle`/`BasicHandle`/`SwarmHandle`
  imports across the crate (`mod.rs:38-42`'s `pub use` lines are the export
  points to also delete) and fix remaining call sites.

## Verification

- `cargo build` — zero errors, zero warnings about unused imports/dead code
  from the deleted types (aside from `snapshot.rs` itself, which is
  expected to go unreferenced this pass).
- `cargo build` on the Linux performance-rig target specifically, to confirm
  the `midir` gate fix (item 0) actually compiles there, not just macOS.
- `cargo test`.
- Manual, no MIDI CC controller plugged in: app starts, GUI renders current
  (fixed-at-default) per-voice values correctly, no panics, matches today's
  "no MIDI controller found" no-op behavior.
- Manual, CC controller plugged in: turn a knob mapped to a per-voice param
  (start with Growl, the reference voice), confirm audible change and GUI
  meter updates live; repeat for at least one other voice.
- Manual: switch `voice_selected` mid-performance, confirm subsequent CC
  messages retarget to the newly active voice (not the previous one).
- Manual: confirm the existing note/PC MIDI path (`hydra/midi.rs`) and the
  existing global-signal CC path (filter/width/fuzz/thump/bend via
  `main.rs:184-200`) still work unchanged — this refactor must not regress
  them.
