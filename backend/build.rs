//! Makes sure release binaries always contain the UI.

use std::path::Path;

fn main() {
    let dist = Path::new("../frontend/dist");
    // Re-embed when the UI is rebuilt.
    println!("cargo:rerun-if-changed=../frontend/dist");
    println!("cargo:rerun-if-env-changed=ICECUBE_ALLOW_NO_UI");

    let release = std::env::var("PROFILE").as_deref() == Ok("release");
    let allowed = std::env::var("ICECUBE_ALLOW_NO_UI").is_ok_and(|v| v == "1");
    if release && !allowed && !dist.join("index.html").exists() {
        panic!(
            "\n\nfrontend/dist/index.html not found: a release build would ship without the UI.\n\
             Build it first:   cd frontend && bun install && bun run build   (or: make build)\n\
             To build anyway:  ICECUBE_ALLOW_NO_UI=1 cargo build --release\n\n"
        );
    }
}
