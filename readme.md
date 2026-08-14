
# Zgicabra RS

- Binary must be run with libsixense_x64.so in working directory
- Run `run.sh` for dev. This will watch source files.

## Setup requirements

- Bitwig Studio
  - DrivenByMoss Extension Package for OSC Extension
  - OSC Controller added and active
  - Set OSC Controller -> Protocol -> Value Resolution to 16384
  - OSC listen port should be 8000 (default)

## Sixense SDK Linking

`src/hydra.rs` depends on `libsixense_x64.so` which in turn depends on
`libstdc++.so.6`. Both of these are kept in the `libs` folder and copied to 
the build target folder during build. Additionally, `libsixense_x64` has been
`patchelf`'d to modify it's rpath to `$ORIGIN`, rather than using the system
default. An unpatched copy is retained as reference.

`libsixense.so`, and `sixense.h` are not used but are retained for reference.


## System Dependencies

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
- `environment.variables.PKG_CONFIG_PATH = "/run/current-system/sw/lib/pkgconfig";`
  must be set system-wide. Unlike `nix-shell -p`, `environment.systemPackages`
  does not add installed packages' pkgconfig dirs to `PKG_CONFIG_PATH`
  automatically — without this, pkg-config can't find `alsa.pc` even once
  it's symlinked into the system profile. Takes a fresh shell/login after
  `nixos-rebuild switch` to pick up.
- `midir` (MIDI controller input for the mock Hydra backend, see
  `src/hydra/midi.rs`) is scoped to macOS-only in `Cargo.toml`
  (`target.'cfg(all(target_os = "macos", target_arch = "x86_64"))'.dependencies`).
  The Linux performance machine always has real Hydra hardware and this
  MusNix-based audio setup has no ALSA dev headers by default, so `midir`
  (which needs `alsa-sys` on Linux) must never be a plain dependency here.

### shell.nix

The GUI (`--gui`) uses SDL2 + glow + Dear ImGui for windowing/rendering.
`sdl2` is built with the `bundled` + `static-link` features. `cpal` also
depends on ALSA. These deps are captured in `shell.nix`.

If `direnv` is available, normal `cargo` build commands will work. If not,
use `nix-shell --run 'cargo ...'` instead.

`shell.nix` also sets two env vars the SDL2 source build needs, so they
don't need to be exported by hand:
- `CMAKE_POLICY_VERSION_MINIMUM=3.5` — CMake 4 dropped support for the
  old-style `cmake_minimum_required()` SDL2's vendored source declares.
- `CFLAGS=-std=gnu17` — GCC 15 defaults to C23, where `bool`/`true`/`false`
  are keywords; SDL2's `src/joystick/hidapi/SDL_hidapi_steam.c` (written
  for the pre-C99-`stdbool.h` era) redeclares them as an enum, which only
  compiles under an older C standard.

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
