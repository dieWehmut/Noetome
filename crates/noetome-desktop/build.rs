fn main() {
    println!(
        "cargo:rustc-env=NOETOME_TARGET={}",
        std::env::var("TARGET").unwrap()
    );
    tauri_build::build();
}
