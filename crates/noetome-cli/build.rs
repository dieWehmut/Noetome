//! Bind persisted fixture runs to the compiler sources and selected toolchain.
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn sources(path: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(path)
        .expect("compiler sources")
        .map(Result::unwrap)
    {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}
fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace = root.parent().unwrap().parent().unwrap();
    println!("cargo:rerun-if-changed={}", root.join("src").display());
    let mut files = vec![
        root.join("Cargo.toml"),
        root.join("build.rs"),
        workspace.join("Cargo.toml"),
        workspace.join("Cargo.lock"),
    ];
    sources(&root.join("src"), &mut files);
    files.sort();
    let mut digest = Sha256::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let relative = path
            .strip_prefix(workspace)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = fs::read(&path).expect("compiler source bytes");
        digest.update((relative.len() as u64).to_le_bytes());
        digest.update(relative.as_bytes());
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    let rustc = Command::new(env::var_os("RUSTC").unwrap())
        .arg("--version")
        .arg("--verbose")
        .output()
        .expect("selected compiler");
    assert!(rustc.status.success(), "selected rustc version");
    digest.update(&rustc.stdout);
    for key in [
        "TARGET",
        "PROFILE",
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_CFG_TARGET_OS",
        "CARGO_CFG_TARGET_ARCH",
    ] {
        let value = env::var(key).unwrap_or_default();
        digest.update((key.len() as u64).to_le_bytes());
        digest.update(key.as_bytes());
        digest.update((value.len() as u64).to_le_bytes());
        digest.update(value.as_bytes());
    }
    println!(
        "cargo:rustc-env=NOETOME_RUN_COMPILER_FINGERPRINT={:x}",
        digest.finalize()
    );
}
