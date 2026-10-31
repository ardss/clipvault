fn main() {
    // Single source for the app identifier: cvlog, the injector handshake
    // dir/event and the single-instance window names read CV_IDENTIFIER so a
    // build with a changed identifier (e.g. the com.clipvault.test isolation
    // builds used for acceptance) is fully isolated — data dir, log and paste
    // channel all follow. Plain string scan: no build-dep needed.
    println!("cargo:rerun-if-changed=tauri.conf.json");
    let conf = std::fs::read_to_string("tauri.conf.json").unwrap_or_default();
    let id = conf
        .split("\"identifier\"")
        .nth(1)
        .and_then(|rest| rest.split('"').nth(1))
        .unwrap_or("com.clipvault.app")
        .to_string();
    println!("cargo:rustc-env=CV_IDENTIFIER={id}");
    // Integration-test binaries (tests/*.rs) statically import
    // comctl32!TaskDialogIndirect (via the tauri → wry dependency chain) — an
    // export that only exists in comctl32 v6, which Windows resolves through a
    // side-by-side manifest dependency. The real app gets one from
    // tauri-build's embedded manifest; cargo test exes get none and fail to
    // load with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139). Declare the v6
    // dependency for test targets only, so the linker writes it into their
    // generated manifest.
    #[cfg(target_os = "windows")]
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' \
         name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
         publicKeyToken='6595b64144ccf1df' language='*' processorArchitecture='*'"
    );
    tauri_build::build()
}
