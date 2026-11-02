//! Integration tests exercise the normal (non-cfg(test)) library and CLI.
use noetome_cli::{
    sidecar_model::Limits,
    sidecar_process::{self, ParserCommand},
};
use std::{
    fs,
    path::Path,
    process::Command,
    sync::{Arc, atomic::AtomicBool},
};
fn root() -> std::path::PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target/sidecar-integration")
        .join(uuid::Uuid::new_v4().to_string());
    fs::create_dir_all(&path).unwrap();
    path
}
#[test]
fn production_library_rejects_actual_fake_process() {
    let root = root();
    let input = root.join("input.txt");
    fs::write(&input, b"selected").unwrap();
    let cmd = ParserCommand {
        program: std::env::var_os("NOETOME_TEST_PYTHON").unwrap_or_else(|| "python".into()),
        args: vec![
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/support/fake_parser.py")
                .into_os_string(),
            "valid".into(),
        ],
    };
    let error = sidecar_process::run(
        &cmd,
        &input,
        &root.join("new"),
        Limits::default(),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap_err();
    assert_eq!(error.code, "SIDECAR_PRODUCER");
    assert_eq!(fs::read(input).unwrap(), b"selected");
}
#[test]
fn cli_has_no_fake_command_or_flag() {
    for args in [
        vec!["--fake"],
        vec!["sidecar", "--fake"],
        vec!["source", "parse-pdf", "--fake"],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&out.stderr).contains("CLI_INVALID_ARGUMENT"));
    }
}
#[test]
fn project_import_cannot_take_fake_producer_flag() {
    let root = root();
    let project = root.join("project");
    let input = root.join("input.txt");
    fs::write(&input, b"selected").unwrap();
    assert!(
        Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(["project", "init"])
            .arg(&project)
            .output()
            .unwrap()
            .status
            .success()
    );
    let config = fs::read(project.join("project.json")).unwrap();
    let database = fs::read(project.join("project.sqlite")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(["source", "import"])
        .arg(&project)
        .arg(&input)
        .args(["--producer", "fake"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read(project.join("project.json")).unwrap(), config);
    assert_eq!(fs::read(project.join("project.sqlite")).unwrap(), database);
}
