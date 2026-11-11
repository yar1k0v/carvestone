//! Names the commit in `stone --version` (`STONE_BUILD`), so every local build can be traced.
//! Gives `stone.exe` its icon. The `.ico` is packed from the PNGs in `assets/` (rendered from
//! `assets/stone.svg`), so the PNGs are the only source. Other targets get no icon.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

const SIZES: [u32; 4] = [16, 32, 48, 256];

/// `+<hash>` or `+<hash>.dirty`; empty when git is missing or this is not a checkout, so a build
/// from a source archive still works.
fn build_suffix(dir: &Path) -> String {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let Some(hash) = git(&["rev-parse", "--short=7", "HEAD"]).filter(|h| !h.is_empty()) else {
        return String::new();
    };
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
    format!("+{hash}{}", if dirty { ".dirty" } else { "" })
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let manifest = env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    println!("cargo:rustc-env=STONE_BUILD={}", build_suffix(Path::new(&manifest)));
    for git_file in ["HEAD", "index"] {
        let path = Path::new(&manifest).join("../../.git").join(git_file);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join("stone.ico");
    let assets = Path::new(&manifest).join("../../assets");

    let mut icons = ico::IconDir::new(ico::ResourceType::Icon);
    for size in SIZES {
        let path = assets.join(format!("stone-{size}.png"));
        println!("cargo:rerun-if-changed={}", path.display());
        let file = fs::File::open(&path).unwrap_or_else(|e| panic!("cannot open {}: {e}", path.display()));
        let image = ico::IconImage::read_png(std::io::BufReader::new(file))
            .unwrap_or_else(|e| panic!("{} is not a PNG: {e}", path.display()));
        let entry = ico::IconDirEntry::encode(&image).unwrap_or_else(|e| panic!("cannot pack {}: {e}", path.display()));
        icons.add_entry(entry);
    }
    let file = fs::File::create(&out).unwrap_or_else(|e| panic!("cannot write {}: {e}", out.display()));
    icons
        .write(file)
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", out.display()));

    winresource::WindowsResource::new()
        .set_icon(out.to_str().expect("OUT_DIR is valid UTF-8"))
        .compile()
        .unwrap_or_else(|e| panic!("cannot embed the icon (the Windows SDK's rc.exe is needed): {e}"));
}
