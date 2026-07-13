
# Zgicabra RS

## Commands

- Run `bacon check` for dev
- Run `run.sh` for prod


## Sixense SDK Linking

`src/hydra.rs` depends on `libsixense_x64.so` which in turn depends on
`libstdc++.so.6`. Both of these are kept in the `libs` folder and copied to 
the build target folder during build. Additionally, `libsixense_x64` has been
`patchelf`'d to modify it's rpath to `$ORIGIN`, rather than using the system
default. An unpatched copy is retained as reference.

`libsixense.so`, and `sixense.h` are not used but are retained for reference.

