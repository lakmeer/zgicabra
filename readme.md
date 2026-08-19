
# Zgicabra RS

- Run `run.sh` for dev. This will watch source files.

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
has been `patchelf`'d to modify it's rpath to `$ORIGIN`, rather than using the
system default. An unpatched copy is retained as reference.

`libstdc++.so.6` is vendored alongside it too, copied fresh into the build
target folder on every build (resolved via `cc -print-file-name`, not
checked into `libs/`) so the binary can be exec'd directly — e.g. by the
boot-time systemd service — without a system-wide `LD_LIBRARY_PATH`. A
*stale* checked-in copy previously shadowed the real system libstdc++ and
broke anything needing a newer symbol version (e.g. libjack.so.0's
`CXXABI_1.3.15`); copying fresh from the active toolchain on each build
avoids that.

`libsixense.so`, and `sixense.h` are not used but are retained for reference.

## NAM Models

The `nam/*.nam` amp models are compiled straight into the binary
(`src/audio/nam.rs`, via `include_dir!`) rather than read from disk at
runtime — same motivation as vendoring `libstdc++.so.6` above: the boot-time
systemd service execs the binary from `target/release` with no working
directory guarantee of a sibling `nam/` folder. Add/remove a `.nam` file and
rebuild to change the embedded set; no separate copy step needed.

## Performance-mode launch (KMSCON)

Both the boot-time systemd service (`sys/config.nix`) and `bin/perform`
(manual vt3 test from the dev desktop) run zgicabra by having KMSCON
`--login`-exec it on a VT. `--login` wipes the exec'd child's environment
entirely — confirmed by capturing it: even `PATH` is gone. Nothing set
upstream of `kmscon` (`sudo env VAR=val kmscon ...`, or a systemd unit's
`Environment=`) survives.

Because of this, neither launch path execs the `zgicabra` binary directly —
both point KMSCON's `-- ARGV` at `bin/zgicabra-launch`, a small wrapper that
rebuilds `PATH`/`XDG_RUNTIME_DIR`/`PIPEWIRE_RUNTIME_DIR` from scratch
immediately before `exec`ing the real binary. Without a correct
`XDG_RUNTIME_DIR`, `cpal`'s ALSA backend fails with `snd_pcm_open` reporting
`Host is down` — it can't find the PipeWire socket to connect to.

The performance box also never logs in (no keyboard, no getty), so without
`users.users.zgicabra.linger = true;` (`sys/config.nix`) there is no
`user@1000` session and no PipeWire daemon running at all for that wrapper
to reach.

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
- `environment.variables.PKG_CONFIG_PATH = "/run/current-system/sw/lib/pkgconfig";`
  must be set system-wide. Unlike `nix-shell -p`, `environment.systemPackages`
  does not add installed packages' pkgconfig dirs to `PKG_CONFIG_PATH`
  automatically — without this, pkg-config can't find `alsa.pc`/`sdl2.pc`
  even once they're symlinked into the system profile. Takes a fresh
  shell/login after `nixos-rebuild switch` to pick up.
- user needs these usergroups:
  ```nix
    extraGroups = [ "plugdev" "networkmanager" "wheel" "audio" "input" ];
  ```

### `snd-virmidi`

- Kernel module `snd-virmidi` is enabled in nix config as:
```nix
boot.kernelModules = [ "snd-virmidi" ];
```

### udev Rules

Userspace needs permission to access the USB device that connects to the Hydra.
Rules are provided in `sys/udev-rules` to allow this.

On the NixOS performance box, `sys/nixos-config.nix` now applies these (and
the rest of the machine's zgicabra-specific config) automatically on
`nixos-rebuild switch` — see `sys/README.md`. The manual steps below are
still accurate as a fallback / for a non-NixOS setup.
