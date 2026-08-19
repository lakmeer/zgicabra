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
    let vendored_dir = target_dir.join("libs");
    fs::create_dir_all(&vendored_dir).unwrap();

    let src = libs_dir.join("libsixense_x64.so");
    let dst = vendored_dir.join("libsixense_x64.so");
    // The vendored lib is read-only, and fs::copy can't overwrite an
    // existing read-only destination, so clear it first.
    let _ = fs::remove_file(&dst);
    fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {:?} -> {:?}: {}", src, dst, e));
    println!("cargo:rerun-if-changed={}", src.display());

    // Vendor libstdc++.so.6 too, so the binary works when exec'd directly
    // (e.g. by the boot-time systemd service) without a system-wide
    // LD_LIBRARY_PATH hack. Resolved fresh from the active `cc` on every
    // build (not checked into libs/) so it can never go stale relative to
    // the toolchain actually doing the linking -- a stale vendored copy is
    // what previously shadowed the real system libstdc++ and broke anything
    // needing a newer symbol version (e.g. libjack.so.0's CXXABI_1.3.15).
    let cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let output = std::process::Command::new(&cc)
        .arg("-print-file-name=libstdc++.so.6")
        .output()
        .unwrap_or_else(|e| panic!("failed to run `{} -print-file-name=libstdc++.so.6`: {}", cc, e));
    let libstdcxx_src = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    if libstdcxx_src.is_absolute() {
        let dst = vendored_dir.join("libstdc++.so.6");
        let _ = fs::remove_file(&dst);
        fs::copy(&libstdcxx_src, &dst)
            .unwrap_or_else(|e| panic!("copy {:?} -> {:?}: {}", libstdcxx_src, dst, e));
    }
}
