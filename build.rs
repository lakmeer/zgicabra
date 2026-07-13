use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let libs_dir = PathBuf::from(&manifest_dir).join("libs");   // ← was format!(...) producing a String

    println!("cargo:rustc-link-search=native={}", libs_dir.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");

    // OUT_DIR looks like target/<profile>/build/<crate>-<hash>/out
    // walk up 3 levels to reach target/<profile>/
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = out_dir.ancestors().nth(3).expect("bad OUT_DIR layout");

    for file in ["libsixense_x64.so", "libstdc++.so.6"] {
        let src = libs_dir.join(file);
        let dst = target_dir.join(file);
        fs::copy(&src, &dst).unwrap_or_else(|e| panic!("copy {:?} -> {:?}: {}", src, dst, e));
        println!("cargo:rerun-if-changed={}", src.display());
    }
}
