use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let target_os   = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    // Only linux x86_64 links the vendored Sixense SDK (libsixense_x64.so).
    // macOS talks to the Hydra directly over HID (see src/hydra/hid.rs) and
    // needs nothing from libs/. Any other target uses the mock backend.
    if target_os != "linux" || target_arch != "x86_64" {
        return;
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let libs_dir = PathBuf::from(&manifest_dir).join("libs");

    println!("cargo:rustc-link-search=native={}", libs_dir.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/libs");

    // winit dlopens libX11/libXcursor/libXi/libXrandr/libxkbcommon/libGL at
    // runtime rather than linking them, so they never make it onto the
    // RUNPATH via normal linking. On NixOS these live in the nix-ld library
    // dir (programs.nix-ld.libraries) instead of a standard search path, so
    // dlopen can't find them unless we add that dir to our own RUNPATH too
    // (glibc's dlopen does consult the caller's RUNPATH).
    println!("cargo:rerun-if-env-changed=NIX_LD_LIBRARY_PATH");
    if let Ok(nix_ld_lib_path) = env::var("NIX_LD_LIBRARY_PATH") {
        for dir in env::split_paths(&nix_ld_lib_path) {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.display());
        }
    }

    // OUT_DIR looks like target/<profile>/build/<crate>-<hash>/out
    // walk up 3 levels to reach target/<profile>/
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = out_dir.ancestors().nth(3).expect("bad OUT_DIR layout");

    // Cargo puts target/<profile> (and .../deps) on LD_LIBRARY_PATH for
    // every build script it runs, which takes precedence over a binary's
    // own RUNPATH. Dropping our vendored libstdc++.so.6 straight into
    // target/<profile> therefore shadows the real system libstdc++ for
    // *any* build-script subprocess -- notably sdl2-sys's cmake invocation,
    // which then fails to load with a missing CXXABI symbol. Keeping the
    // vendored libs in a subdirectory (and pointing our rpath at it above)
    // keeps them off that search path.
    let vendored_dir = target_dir.join("libs");
    fs::create_dir_all(&vendored_dir).unwrap();

    for file in ["libsixense_x64.so", "libstdc++.so.6"] {
        let src = libs_dir.join(file);
        let dst = vendored_dir.join(file);
        // The vendored libs are read-only, and fs::copy can't overwrite an
        // existing read-only destination, so clear it first.
        let _ = fs::remove_file(&dst);
        fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {:?} -> {:?}: {}", src, dst, e));
        println!("cargo:rerun-if-changed={}", src.display());
    }
}
