//! The `ui` feature embeds `web/dist`; a build without it says how to make it.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../web/dist");
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    if std::env::var_os("CARGO_FEATURE_UI").is_some() && !dist.is_dir() {
        panic!("web/dist is missing; run `pnpm --dir web build`");
    }
}
