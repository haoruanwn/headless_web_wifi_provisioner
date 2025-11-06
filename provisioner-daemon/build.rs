use std::env;
use std::path::PathBuf;

fn main() {
    // 1. Get Cargo's output directory (e.g., .../target/release)
    //    We jump 3 levels up from OUT_DIR (e.g., .../target/release/build/provisioner-daemon-abc/out)
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    
    // Cargo uses `CARGO_TARGET_DIR` (if set) or `OUT_DIR/../../../` as the target directory
    let target_dir = env::var("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|_| out_dir.join("../../../"));

    // 2. Define our UI source directory (relative to provisioner-daemon/build.rs)
    let src_ui_dir = PathBuf::from("../ui");
    
    // 3. Define the destination UI directory (e.g., .../target/release/ui)
    let dest_ui_dir = target_dir.join("ui");

    // 4. Tell Cargo to re-run this script if the UI directory changes
    println!("cargo:rerun-if-changed=../ui");

    // 5. Perform the copy (only if the UI directory actually exists)
    if src_ui_dir.exists() {
        let mut options = fs_extra::dir::CopyOptions::new();
        options.overwrite = true;     // Overwrite old files
        options.content_only = true; // Only copy the contents (i.e., copy what's inside 'ui' to 'target/release/ui')

        // Ensure the destination directory exists
        std::fs::create_dir_all(&dest_ui_dir).expect("Failed to create destination UI directory");

        // Recursively copy
        if let Err(e) = fs_extra::dir::copy(&src_ui_dir, &dest_ui_dir, &options) {
             panic!("Failed to copy UI assets from {:?} to {:?}: {}", src_ui_dir, dest_ui_dir, e);
        }
    } else {
        panic!("UI source directory not found at {:?}. CWD is {:?}", src_ui_dir, env::current_dir().unwrap());
    }
}
