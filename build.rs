use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let target_os   = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    println!("cargo:rustc-check-cfg=cfg(have_real_hydra)");

    // The real Sixense SDK binaries in libs/ are x86_64 builds (Linux ELF,
    // macOS Mach-O). Any other target uses the mock hydra backend, which
    // needs none of this.
    if target_arch != "x86_64" || (target_os != "linux" && target_os != "macos") {
        return;
    }

    println!("cargo:rustc-cfg=have_real_hydra");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let libs_dir = PathBuf::from(&manifest_dir).join("libs");   // ← was format!(...) producing a String

    println!("cargo:rustc-link-search=native={}", libs_dir.display());

    // OUT_DIR looks like target/<profile>/build/<crate>-<hash>/out
    // walk up 3 levels to reach target/<profile>/
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = out_dir.ancestors().nth(3).expect("bad OUT_DIR layout");

    // Both libsixense builds have been re-pointed (patchelf on Linux,
    // install_name_tool on macOS) to look for their dependencies next to the
    // executable rather than at a hardcoded system path.
    let files: &[&str] = if target_os == "linux" {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
        &["libsixense_x64.so", "libstdc++.so.6"]
    } else {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
        &["libsixense_x64.dylib"]
    };

    for file in files {
        let src = libs_dir.join(file);
        let dst = target_dir.join(file);
        fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {:?} -> {:?}: {}", src, dst, e));
        println!("cargo:rerun-if-changed={}", src.display());
    }
}
