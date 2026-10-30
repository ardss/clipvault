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
    tauri_build::build()
}
