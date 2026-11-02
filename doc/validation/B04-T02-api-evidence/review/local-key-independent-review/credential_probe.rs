// Independent probe using only the explicitly labelled synthetic credential.
use noetome_cli::credential_handoff::TransientCredential;
fn main() {
    let key = TransientCredential::new(b"SYNTHETIC_REVIEW_KEY_20261102").unwrap();
    for arg in std::env::args().skip(1) {
        let path = std::path::Path::new(&arg);
        let raw = std::fs::read(path).unwrap();
        let result = key.validate_no_echo(&raw);
        println!("{}: {}", path.file_name().unwrap().to_string_lossy(),
            result.err().map(|e| e.code).unwrap_or("accepted"));
    }
}
