
# Zgicabra RS

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
