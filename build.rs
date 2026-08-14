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

    // OUT_DIR looks like target/<profile>/build/<crate>-<hash>/out
    // walk up 3 levels to reach target/<profile>/
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = out_dir.ancestors().nth(3).expect("bad OUT_DIR layout");

    // $ORIGIN/libs at runtime resolves relative to the final binary
    // (target/<profile>/), not the source repo's libs/ dir, so the vendored
    // Sixense blob needs a copy to live there too.
    //
    // Don't also vendor libstdc++.so.6 here: since $ORIGIN/libs sits first
    // on our RUNPATH, a stale copy shadows the real system libstdc++ and
    // breaks anything needing a newer symbol version (e.g. libjack.so.0's
    // CXXABI_1.3.15). The nix toolchain's own libstdc++ already covers the
    // old Sixense blob's GLIBCXX_3.4.11-vintage needs.
    let vendored_dir = target_dir.join("libs");
    fs::create_dir_all(&vendored_dir).unwrap();

    let src = libs_dir.join("libsixense_x64.so");
    let dst = vendored_dir.join("libsixense_x64.so");
    // The vendored lib is read-only, and fs::copy can't overwrite an
    // existing read-only destination, so clear it first.
    let _ = fs::remove_file(&dst);
    fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {:?} -> {:?}: {}", src, dst, e));
    println!("cargo:rerun-if-changed={}", src.display());
}
