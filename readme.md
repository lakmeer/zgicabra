
# Zgicabra

Zgicabra is a musical instrument built on the Sixense Hydra SDK.

It is designed to run headlessly on a small NUC for live stage performance as a
bass monosynth, and displays a textmode UI for a portable USB monitor.

## Usage

```sh
zgicabra            # run with the terminal UI (default)
zgicabra --debug    # suppress the terminal UI, print verbose diagnostics instead
zgicabra --test     # run the audio self-test
```

`--debug` is the flag to reach for when diagnosing controller/engine
issues: it turns off the terminal UI and streams verbose per-frame and
per-event logging to stderr (hydra controller telemetry, dispatched note/
voice events, raw HID read errors) instead.

## Sixense SDK Linking

`src/hydra.rs` depends on `libsixense_x64.so`, kept in the `libs` folder and
copied to the build target folder during build. Additionally, `libsixense_x64`
has been `patchelf`'d to modify its rpath to `$ORIGIN`, rather than using the
system default. An unpatched copy is retained as reference.

`libstdc++.so.6` is vendored alongside it too, copied fresh into the build
target folder on every build (resolved via `cc -print-file-name`, not
checked into `libs/`) so the binary can be exec'd directly — e.g. by the
boot-time systemd service — without a system-wide `LD_LIBRARY_PATH`. 

`libsixense.so`, and `sixense.h` are not used but are retained for reference.

## Setup requirements

Apply `sys/nixos-config.nix` to your performance machine to create a
boot-to-instrument target for systemd. Includes a `dev` specialisation
that adds an alternative grub entry for full desktop development.

## Accreditation

### Vital Synth

Part of the audio chain is derived from the [Vital](https://github.com/mtytel/vital),
which is GPL3. While this repo doesn't contain any source code from Vital, some
internal algorithms were directly adapted from the original Vital codebase. In
accordance with Vital's [readme](https://github.com/mtytel/vital#what-can-you-do-with-the-source)
file, this project is not distributed in an app store, does not use any of the
Vital trademarks for marketing, connect to any the mentioned online services,
or redistribute any of its built-in presets. Projects forked from this one
should adhere to the same restrictions.

### NAM Models

This project uses [Neural Amp Modeller](https://github.com/sdatkinson/NeuralAmpModelerPlugin)
by Steven Atkinson, under the MIT license. It also includes several model files from
various community uploaders to the library at [TONE3000](https://tone3000.com/search):

| File | Model Name | Author/Uploader | Source URL |
|---|---|---|---|
| 6505.nam     | Bass Driver 6505+                            | Ratchetstrap Media   | https://www.tone3000.com/tones/metal-bass-pack-5278 |
| bass.nam     | Ampeg SVT - Gain 10 Ultra Lo and Hi SM57     | tone3000 (official)  | https://www.tone3000.com/tones/ampeg-svt-classic-with-6x10-28202 |
| lowgain.nam  | LowGain BASS drive                           | sergiogbass          | https://www.tone3000.com/tones/metal-punchy-bass-tone-66292 |
| mesa.nam     | FR MBDR MW Red Mdn - 1 - 4FB LL SM57a        | outmodedelectronics  | https://www.tone3000.com/tones/mesa-dual-rectifier-mw-red-modern-mesa-4x12-full-rig-69206 |
| pickle.nam   | Hartke LH1000 Bass Amp Head Full Rigs Pack 1 | StudioAmpCaptures918 | https://www.tone3000.com/tones/hartke-lh1000-bass-amp-gallien-krueger-4x10-cab-bass-rigs-amp-head-63862 |
| sansamp.nam  | Sansamp Bass Driver (Driven)                 | everipper            | https://www.tone3000.com/tones/tech21-sansamp-bass-driver-31010 |
| wetbass.nam  | WETBASS                                      | chechogonzalez2016   | https://www.tone3000.com/tones/metal-punchy-bass-tone-66292 |
| mesa_ir.wav  | Mesa Boogie ST 4x12A V30 - Vintage 421 Enhanced | tonefactor        | https://www.tone3000.com/tones/mesa-boogie-st-4x12a-v30-vintage-421-enhanced-79857 |

These models are included in this repo since they have been selected
specifically as an integral part of the sound design. If you are the author of
a model that is included here, and you would rather not have your work
redistributed, please contact me or open an issue and I will remove it.

### Samples

This project contains audio samples from various sources under various permissive
licenses. Some samples have been modified from their original forms.

| File | Source | License |
|---|---|---|
| kick_dry.wav     | https://pixabay.com/sound-effects/musical-kick-greg-232043/           | [Pixabay Content License](https://pixabay.com/service/license-summary/) |
| kick_deep.wav    | https://pixabay.com/sound-effects/musical-awesome-house-kick-98685/   | [Pixabay Content License](https://pixabay.com/service/license-summary/) |
| kick_pitched.wav | https://pixabay.com/sound-effects/musical-kick-183936/                | [Pixabay Content License](https://pixabay.com/service/license-summary/) |
| pluck.wav        | https://pixabay.com/sound-effects/musical-clean-fingered-bass-101922/ | [Pixabay Content License](https://pixabay.com/service/license-summary/) |

