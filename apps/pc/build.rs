use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    tauri_build::build();
    embed_env();
}

/// Embed the repository-root `.env` into the desktop binary.
///
/// CI checkouts do not contain `.env` (it is gitignored), so a missing file
/// yields an empty constant and the application simply shows no embedded
/// configuration. Local builds with `.env` present include every line.
fn embed_env() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let env_file = manifest.join("../../.env");
    println!("cargo:rerun-if-changed={}", env_file.display());

    let raw = fs::read_to_string(&env_file).unwrap_or_default();
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let generated = format!("pub const RAW: &str = {:?};\n", raw);
    fs::write(out_dir.join("embedded_env.rs"), generated).expect("write embedded_env.rs");
}
