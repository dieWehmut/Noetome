//! Trusted, single, offline mathematical OCR invocation. No source declaration enters this wire request.
use crate::{
    knowledge_coordinate_input::{Held, fingerprint},
    project::{self, ProjectError},
    scan_input::{check, error, parse},
    scan_model::*,
    sidecar_model::{Fingerprint, Limits},
    sidecar_process::{Control, ParserCommand},
    sidecar_workspace::{Directory, GuardedFile},
};
use std::{collections::BTreeMap, ffi::OsString, path::PathBuf, time::SystemTime};
pub(crate) const TIMEOUT_MS: u64 = 120000;
pub(crate) const RESPONSE_BYTES: u64 = 131072;
pub(crate) const DIAGNOSTIC_BYTES: u64 = 65536;
pub(crate) const MODEL_FILES: [(&str, &str, u64); 4] = [
    (
        "weights.pth",
        "a63d9141c53d266cb682fb5a8bd83bd5cbe283145e0e78ebdc0f895195a1dfaa",
        102113875,
    ),
    (
        "image_resizer.pth",
        "1c3820659985ad142b526490bb25c23d977176ac2073591b3bddada692718458",
        19441973,
    ),
    (
        "config.yaml",
        "a9b18acf6718ffed79ab669c9957c46936714f4b983705f0b1361d90f4ea89aa",
        917,
    ),
    (
        "tokenizer.json",
        "1dc27b18d6a518d0d5ff3f4bb7bd98521fe80ad39e5b2a246d4109f1bb9d5019",
        24174,
    ),
];
pub(crate) fn model_fingerprints() -> BTreeMap<String, Fingerprint> {
    MODEL_FILES
        .iter()
        .map(|(n, s, b)| {
            (
                n.to_string(),
                Fingerprint {
                    sha256: s.to_string(),
                    byte_length: *b,
                },
            )
        })
        .collect()
}
pub(crate) struct Runtime {
    pub(crate) models: Held,
    executable: PathBuf,
    executable_guard: GuardedFile,
    executable_bytes: Fingerprint,
    executable_modified: SystemTime,
    executable_parent: Directory,
}
pub(crate) struct Capture {
    pub(crate) raw: Option<Vec<u8>>,
    pub(crate) diagnostics: Vec<u8>,
    pub(crate) transport_issue: Option<String>,
}
impl Runtime {
    pub(crate) fn open(c: &Control) -> Result<Self, ProjectError> {
        let get = |name| {
            std::env::var_os(name).filter(|s|!s.is_empty()).ok_or_else(||error("SCAN_RUNTIME_CONFIG",2,"set explicit NOETOME_SCAN_PYTHON and NOETOME_SCAN_MODEL_DIR; no automatic installation or download"))
        };
        let python = PathBuf::from(get("NOETOME_SCAN_PYTHON")?);
        let model = PathBuf::from(get("NOETOME_SCAN_MODEL_DIR")?);
        if !python.is_absolute() || !model.is_absolute() {
            return Err(error(
                "SCAN_RUNTIME_CONFIG",
                2,
                "OCR interpreter and model directory must be absolute explicitly selected paths",
            ));
        }
        let executable = project::check_components(&python)?;
        let executable_parent = Directory::capture(
            executable
                .parent()
                .ok_or_else(|| error("SCAN_RUNTIME_CONFIG", 2, "interpreter parent missing"))?,
            "SCAN_RUNTIME_CHANGED",
        )
        .map_err(|e| error("SCAN_RUNTIME_CONFIG", 2, e))?;
        let executable_modified = std::fs::metadata(&executable)
            .and_then(|m| m.modified())
            .map_err(|e| error("SCAN_RUNTIME_CONFIG", 2, e))?;
        let (executable_guard, exe) =
            GuardedFile::open(&executable, 64 * 1024 * 1024, c, "SCAN_RUNTIME_CHANGED")
                .map_err(|e| error("SCAN_RUNTIME_CONFIG", 2, e))?;
        let mut models = Held::open(&model).map_err(|e| error("SCAN_RUNTIME_CONFIG", 2, e))?;
        models
            .exact(
                &MODEL_FILES
                    .iter()
                    .map(|(n, _, _)| n.to_string())
                    .collect::<Vec<_>>(),
            )
            .map_err(|e| error("SCAN_RUNTIME_CONFIG", 2, e))?;
        for (name, sha, size) in MODEL_FILES {
            models
                .add(name, size, c)
                .map_err(|e| error("SCAN_RUNTIME_CONFIG", 2, e))?;
            let actual = fingerprint(
                models
                    .bytes(name)
                    .map_err(|e| error("SCAN_RUNTIME_CONFIG", 2, e))?,
            );
            if actual.sha256 != sha || actual.byte_length != size {
                return Err(error(
                    "SCAN_MODEL_PIN",
                    2,
                    "selected OCR weights/config/tokenizer differ from the fixed version",
                ));
            }
        }
        let value = Self {
            models,
            executable,
            executable_guard,
            executable_bytes: fingerprint(&exe),
            executable_modified,
            executable_parent,
        };
        value.verify(c)?;
        Ok(value)
    }
    pub(crate) fn verify(&self, c: &Control) -> Result<(), ProjectError> {
        check(c)?;
        self.models
            .verify(c)
            .map_err(|e| error("SCAN_RUNTIME_CHANGED", 3, e))?;
        self.executable_parent
            .verify("SCAN_RUNTIME_CHANGED")
            .map_err(|e| error("SCAN_RUNTIME_CHANGED", 3, e))?;
        self.executable_guard
            .verify(c, "SCAN_RUNTIME_CHANGED")
            .map_err(|e| error("SCAN_RUNTIME_CHANGED", 3, e))?;
        if std::fs::metadata(&self.executable)
            .and_then(|m| m.modified())
            .map_err(|e| error("SCAN_RUNTIME_CHANGED", 3, e))?
            != self.executable_modified
        {
            return Err(error(
                "SCAN_RUNTIME_CHANGED",
                3,
                "interpreter modified during request",
            ));
        }
        Ok(())
    }
    pub(crate) fn fingerprints(&self) -> BTreeMap<String, Fingerprint> {
        let mut pins = model_fingerprints();
        pins.insert("python".into(), self.executable_bytes.clone());
        pins.insert(
            "adapter.py".into(),
            fingerprint(include_bytes!("../../../sidecars/scan_formula/adapter.py")),
        );
        pins
    }
    pub(crate) fn invoke(&self, bytes: Vec<u8>, c: &Control) -> Result<Capture, ProjectError> {
        self.verify(c)?;
        let command = ParserCommand {
            program: self.executable.as_os_str().to_owned(),
            args: vec![
                OsString::from("-I"),
                OsString::from("-B"),
                OsString::from("-X"),
                OsString::from("utf8"),
                OsString::from("-c"),
                OsString::from(include_str!("../../../sidecars/scan_formula/adapter.py")),
            ],
        };
        let limits = Limits {
            request_bytes: 65536,
            response_bytes: RESPONSE_BYTES,
            diagnostic_bytes: DIAGNOSTIC_BYTES,
            timeout_ms: TIMEOUT_MS,
            ..Limits::default()
        };
        Ok(
            match crate::sidecar_process::transport(&command, bytes, &limits, c) {
                Ok((raw, diagnostics)) => Capture {
                    raw: Some(raw),
                    diagnostics,
                    transport_issue: None,
                },
                Err(e) => Capture {
                    raw: None,
                    transport_issue: Some(e.code.to_string()),
                    diagnostics: e.diagnostics,
                },
            },
        )
    }
}
pub(crate) fn response(
    raw: &[u8],
    request: &RecognitionRequest,
) -> Result<RecognitionResponse, ProjectError> {
    if raw.len() > RESPONSE_BYTES as usize {
        return Err(error(
            "SCAN_RECOGNITION_LIMIT",
            3,
            "OCR response byte limit",
        ));
    }
    let r: RecognitionResponse =
        parse(raw).map_err(|e| error("SCAN_RECOGNITION_PROTOCOL", 3, e))?;
    if r.producer != "pix2tex"
        || r.package_version != "0.1.4"
        || r.request_id != request.request_id
        || r.input.sha256 != request.input.sha256
        || r.input.byte_length != request.input.byte_length
        || r.model.release != "v0.0.1"
        || r.model.files != model_fingerprints()
        || r.confidence.is_some()
        || r.elapsed_ms > TIMEOUT_MS
        || r.issues.len() > 16
        || r.issues.iter().any(|i| {
            !crate::knowledge_coordinate_input::plain(&i.code)
                || !crate::knowledge_coordinate_input::plain(&i.message)
        })
        || r.recognized_tex
            .as_ref()
            .is_some_and(|t| t.is_empty() || t.len() > request.limits.max_tex_bytes as usize)
    {
        return Err(error(
            "SCAN_RECOGNITION_PROTOCOL",
            3,
            "OCR version, source identity, fixed model metadata or bounded response differs",
        ));
    }
    let p = &r.policy;
    if p.device != "cpu"
        || p.seed != 42
        || p.temperature != 0.2
        || !p.resize
        || p.max_seq_len != 512
        || p.decoding != "greedy_argmax_v1"
        || p.network_calls != 0
        || p.paid_calls != 0
        || p.automatic_downloads
        || p.clipboard
    {
        return Err(error(
            "SCAN_RECOGNITION_POLICY",
            3,
            "OCR fixed offline policy differs",
        ));
    }
    if r.runtime.python != "3.12.14" || r.runtime.packages.len() != 12 {
        return Err(error(
            "SCAN_RECOGNITION_RUNTIME",
            3,
            "OCR fixed runtime differs",
        ));
    }
    for (name, version) in [
        ("pix2tex", "0.1.4"),
        ("torch", "2.5.1+cpu"),
        ("torchvision", "0.20.1+cpu"),
        ("numpy", "1.26.4"),
        ("albumentations", "1.3.1"),
        ("opencv-python-headless", "4.10.0.84"),
        ("pandas", "2.2.3"),
        ("transformers", "4.49.0"),
        ("timm", "0.5.4"),
        ("x-transformers", "0.15.0"),
        ("tokenizers", "0.21.4"),
        ("Pillow", "13.0.0"),
    ] {
        if r.runtime.packages.get(name).map(String::as_str) != Some(version) {
            return Err(error(
                "SCAN_RECOGNITION_RUNTIME",
                3,
                "OCR fixed package versions differ",
            ));
        }
    }
    if !((r.status == "recognized" && r.recognized_tex.is_some() && r.issues.is_empty())
        || (r.status == "failed" && r.recognized_tex.is_none() && !r.issues.is_empty()))
    {
        return Err(error(
            "SCAN_RECOGNITION_PROTOCOL",
            3,
            "OCR success/failure status does not match its actual fields",
        ));
    }
    Ok(r)
}
