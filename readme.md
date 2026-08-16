
# Zgicabra RS

- Binary must be run with libsixense_x64.so in working directory
- Run `run.sh` for dev. This will watch source files.

## Usage

```sh
zgicabra           # run with the terminal UI (default)
zgicabra --gui      # also open the imgui voice-params/mock-hydra tuner window
zgicabra --debug    # suppress the terminal UI, print verbose diagnostics instead
zgicabra --test     # run the audio self-test (needs --gui)
```

`--debug` is the flag to reach for when diagnosing controller/engine
issues: it turns off the terminal UI and streams verbose per-frame and
per-event logging to stderr (hydra controller telemetry, dispatched note/
voice events, raw HID read errors) instead.

## Sixense SDK Linking

`src/hydra.rs` depends on `libsixense_x64.so`, kept in the `libs` folder and
copied to the build target folder during build. Additionally, `libsixense_x64`
has been `patchelf`'d to modify it's rpath to `$ORIGIN`, rather than using the
system default. An unpatched copy is retained as reference.

`libstdc++.so.6` is *not* vendored alongside it — the nix C++ toolchain's own
libstdc++ already covers the `GLIBCXX_3.4.11`-vintage symbols the Sixense blob
needs. Don't vendor a copy; a mismatched version has caused runtime crashes.

`libsixense.so`, and `sixense.h` are not used but are retained for reference.


## Setup requirements

- cargo-watch

### Build toolchain (NixOS / Linux performance machine)

- Nixpkgs' stable-channel `rustc`/`cargo` (e.g. 24.05's 1.77) is too old for
  this project's `Cargo.lock` — a transitive dep (`moxcms`, via `image`)
  requires `edition2024`, unsupported before rustc ~1.85. Pull `cargo`/
  `rustc` from `nixpkgs-unstable` instead of `environment.systemPackages`'
  plain `pkgs.cargo`/`pkgs.rustc` — see the `unstable` overlay in
  `configuration.nix`.
- `alsa-lib.dev` (not plain `alsa-lib`) must be in
  `environment.systemPackages`. `cpal`'s `alsa-sys` build script needs
  `alsa.pc` via pkg-config to link `libasound`; that file lives in
  `alsa-lib`'s `dev` output, which the default `alsa-lib` output does not
  include.
- `SDL2.dev` (not plain `SDL2`) must be in `environment.systemPackages` too,
  same reasoning — `SDL.h`/`sdl2.pc` live in the `dev` output. `sdl3` (plain,
  it's a single-output derivation) must also be present: `SDL2` on modern
  nixpkgs is `sdl2-compat`, a shim that `dlopen()`s `libSDL3.so` at runtime.
  See "SDL2 / shell.nix" below for the full story.
- `environment.variables.PKG_CONFIG_PATH = "/run/current-system/sw/lib/pkgconfig";`
  must be set system-wide. Unlike `nix-shell -p`, `environment.systemPackages`
  does not add installed packages' pkgconfig dirs to `PKG_CONFIG_PATH`
  automatically — without this, pkg-config can't find `alsa.pc`/`sdl2.pc`
  even once they're symlinked into the system profile. Takes a fresh
  shell/login after `nixos-rebuild switch` to pick up.
- `midir` (MIDI controller input for the mock Hydra backend, see
  `src/hydra/midi.rs`) is scoped to macOS-only in `Cargo.toml`
  (`target.'cfg(all(target_os = "macos", target_arch = "x86_64"))'.dependencies`).
  The Linux performance machine always has real Hydra hardware and this
  MusNix-based audio setup has no ALSA dev headers by default, so `midir`
  (which needs `alsa-sys` on Linux) must never be a plain dependency here.
- user needs these usergroups:
  ```nix
    extraGroups = [ "plugdev" "networkmanager" "wheel" "audio" "input" ];
  ```

### Required system packages

```nix

  cargo         # from nixpkgs-unstable, not the stable channel
  rustc         # from nixpkgs-unstable, not the stable channel
  alsa-lib.dev
  SDL2.dev
  sdl3


```
### `snd-virmidi`

- Kernel module `snd-virmidi` is enabled in nix config as:
```nix
boot.kernelModules = [ "snd-virmidi" ];
```

### udev Rules

Userspace needs permission to access the USB device that connects to the Hydra.
Rules are provided in `sys/udev-rules` to allow this.

#### udev Setup (untested)

To set up a new system, deploy the rules file to `/etc/udev/rules.d/`:
```sh
sudo cp sys/udev-rules /etc/udev/rules.d/99-sixense-hydra.rules
```
Then reload the rules:
```sh
sudo udevadm control --reload-rules
sudo udevadm trigger
```

`trigger` is not strictly necessary, but will re-announce the device which will
allow an already-connected Hyrda to be pucked up after the rules change.
