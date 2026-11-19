// Expose the target triple: the self-updater downloads the release asset built
// for the same target (`armaxsync-server-<target>`).
fn main() {
    println!(
        "cargo:rustc-env=ARMAXSYNC_TARGET={}",
        std::env::var("TARGET").unwrap()
    );
    println!("cargo:rerun-if-changed=build.rs");
}
