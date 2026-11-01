//! Real Python/CLI PDF tests against immutable, independently authored fixtures.
//! The private fake adapters below test failures; they never stand in for PDFs.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use noetome_cli::pdf_parse::{
    PdfLimits, PdfParserSpec, PdfRequest, PdfResponse, parse_pdf, validate_pdf,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[path = "support/pdf_fake.rs"]
mod fake;

const EXPECTED_SHA: &str = "b9ee562d3b180754cdde8aca5de76cb79baf877204ed5384bd35652c63caca77";
static CLI_CALLS: AtomicUsize = AtomicUsize::new(0);
static HOST_CALLS: AtomicUsize = AtomicUsize::new(0);
static CORE_MUTATIONS: AtomicUsize = AtomicUsize::new(0);

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FixtureManifest {
    schema_version: u32,
    fixture_family: String,
    source_license: String,
    text_format_version: String,
    coordinate_contract: String,
    authoring_script_sha256: String,
    cases: Vec<AuthoredCase>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuthoredCase {
    case_id: String,
    raw: AuthoredRaw,
    designed_page_count: Option<u32>,
    expected_confirmed_page_count: Option<u32>,
    expected_label: ExpectedHostStatus,
    expected_exit_code: u8,
    expected_response_status: noetome_cli::sidecar::ResponseStatus,
    expected_issue_category: Option<String>,
    pages: Vec<AuthoredPage>,
    independent_designed_pages: Option<Vec<AuthoredPage>>,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ExpectedHostStatus {
    Ready,
    NeedsReview,
    Failed,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuthoredRaw {
    relative_path: String,
    byte_length: u64,
    sha256: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuthoredPage {
    physical_page_index: u32,
    displayed_physical_page: u32,
    printed_page_label: Option<String>,
    expected_status: noetome_cli::sidecar::UnitStatus,
    raw_media_box: [f64; 4],
    raw_crop_box: [f64; 4],
    rotation: u32,
    user_unit: f64,
    display_size_pt: [f64; 2],
    expected_glyph_count: Option<u32>,
    authored_glyph_stream_utf8: Option<String>,
    anchors: Vec<AuthoredAnchor>,
    text: Option<AuthoredText>,
    visual_objects_expected: bool,
    image: Option<AuthoredImage>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuthoredAnchor {
    excerpt_utf8: String,
    author_text_matrix: [f64; 6],
    font: String,
    font_size_raw: u32,
    char_advance_raw: f64,
    display_baseline_pt: [f64; 2],
    display_envelope_pt: [f64; 4],
    endpoint_windows_pt: Option<EndpointWindows>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct EndpointWindows {
    left: [f64; 2],
    top: [f64; 2],
    right: [f64; 2],
    bottom: [f64; 2],
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuthoredText {
    utf8: String,
    byte_length: u64,
    sha256: String,
    line_ranges: Vec<AuthoredRange>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuthoredRange {
    ordinal: u32,
    start_byte: u64,
    end_byte: u64,
    excerpt_utf8: String,
    excerpt_sha256: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuthoredImage {
    pixel_width: u32,
    pixel_height: u32,
    raw_pixel_sha256: String,
    raw_placement_box: [f64; 4],
}

type TestCase = (&'static str, fn());

fn main() {
    let tests: &[TestCase] = &[
        (
            "authored_fixture_bytes_and_expected_are_frozen",
            authored_fixture_bytes_and_expected_are_frozen,
        ),
        (
            "all_fifteen_real_cli_cases_match_author_manifest",
            all_fifteen_real_cli_cases_match_author_manifest,
        ),
        (
            "real_result_page_text_geometry_and_resource_mutations_fail_core",
            real_result_page_text_geometry_and_resource_mutations_fail_core,
        ),
        (
            "self_consistent_parser_position_forgery_fails_independent_author_expected",
            self_consistent_parser_position_forgery_fails_independent_author_expected,
        ),
        (
            "real_cli_argument_existing_root_and_timeout_failures_preserve_inputs",
            real_cli_argument_existing_root_and_timeout_failures_preserve_inputs,
        ),
        (
            "real_host_read_policy_and_adapter_failures_are_bounded",
            real_host_read_policy_and_adapter_failures_are_bounded,
        ),
        (
            "actual_python_timeout_and_cancel_after_handshake_end_process",
            actual_python_timeout_and_cancel_after_handshake_end_process,
        ),
        (
            "real_parser_resource_inventory_addition_is_rejected",
            real_parser_resource_inventory_addition_is_rejected,
        ),
        (
            "actual_windows_locked_input_and_hardlinked_boundaries_are_checked",
            actual_windows_locked_input_and_hardlinked_boundaries_are_checked,
        ),
        (
            "actual_report_publication_cancel_boundary_preserves_pending_evidence",
            actual_report_publication_cancel_boundary_preserves_pending_evidence,
        ),
    ];
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--list") {
        for (name, _) in tests {
            println!("{name}: test");
        }
        return;
    }
    let filter = args.iter().position(|arg| arg == "--exact").map(|index| {
        args.get(index + 1)
            .expect("--exact requires a test name")
            .as_str()
    });
    let selected: Vec<_> = tests
        .iter()
        .filter(|(name, _)| filter.is_none_or(|value| value == *name))
        .collect();
    assert!(!selected.is_empty(), "No PDF test matches selector");
    let mut failures = 0;
    for (name, test) in &selected {
        let result = std::panic::catch_unwind(*test);
        println!(
            "{name} ... {}",
            if result.is_ok() { "ok" } else { "FAILED" }
        );
        failures += usize::from(result.is_err());
    }
    println!(
        "PDF evidence: real_cli_calls={}, direct_host_calls={}, real_result_core_mutations={}",
        CLI_CALLS.load(Ordering::Relaxed),
        HOST_CALLS.load(Ordering::Relaxed),
        CORE_MUTATIONS.load(Ordering::Relaxed)
    );
    println!(
        "PDF tests: {} passed; {failures} failed",
        selected.len() - failures
    );
    if failures != 0 {
        std::process::exit(1);
    }
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn fixture() -> PathBuf {
    repo().join("fixtures/pdf-text-v1")
}
fn python() -> PathBuf {
    let selected=std::env::var_os("NOETOME_TEST_PDF_PYTHON").expect("Set NOETOME_TEST_PDF_PYTHON to the explicitly selected locked Python runtime for real PDF tests");
    let path = PathBuf::from(selected);
    assert!(
        path.is_absolute() && path.is_file(),
        "The selected PDF Python must exist at an absolute path"
    );
    path
}
fn adapter() -> PathBuf {
    repo().join("sidecars/pdf_parser/adapter.py")
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn manifest() -> Value {
    let manifest: FixtureManifest =
        serde_json::from_slice(&fs::read(fixture().join("expected.json")).unwrap()).unwrap();
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.fixture_family, "pdf-text-v1");
    serde_json::to_value(manifest).unwrap()
}
fn case(id: &str) -> Value {
    manifest()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["case_id"] == id)
        .unwrap()
        .clone()
}
fn input(id: &str) -> PathBuf {
    fixture().join(case(id)["raw"]["relative_path"].as_str().unwrap())
}
fn sandbox(label: &str) -> PathBuf {
    let parent = repo().join(".artifacts/B03-T02-fixture/rust-tests");
    fs::create_dir_all(&parent).unwrap();
    let path = parent.join(format!("{label}-{}", Uuid::new_v4()));
    fs::create_dir(&path).unwrap();
    fs::write(
        path.join("outside-sentinel.txt"),
        b"Independent outside sentinel; keep exact bytes.\n",
    )
    .unwrap();
    path
}
fn fingerprint(path: &Path) -> (String, u64, std::time::SystemTime) {
    let metadata = fs::metadata(path).unwrap();
    (
        sha(&fs::read(path).unwrap()),
        metadata.len(),
        metadata.modified().unwrap(),
    )
}
fn run_cli(input: &Path, new: &Path, extra: &[&str]) -> Output {
    CLI_CALLS.fetch_add(1, Ordering::Relaxed);
    Command::new(env!("CARGO_BIN_EXE_noetome"))
        .arg("source")
        .arg("parse-pdf")
        .arg(input)
        .arg(new)
        .arg("--python")
        .arg(python())
        .args(extra)
        .current_dir(repo())
        .output()
        .unwrap()
}
fn record(root: &Path, name: &str, output: &Output) {
    let value = json!({"exit_code":output.status.code(),"stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr),"stdout_hex":output.stdout.iter().map(|v|format!("{v:02x}")).collect::<String>(),"stderr_hex":output.stderr.iter().map(|v|format!("{v:02x}")).collect::<String>()});
    fs::write(
        root.join(format!("{name}.json")),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
}
fn artifact_bytes(response: &Value, root: &Path) -> BTreeMap<String, Vec<u8>> {
    response["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            let path = item["relative_path"].as_str().unwrap();
            (path.to_owned(), fs::read(root.join(path)).unwrap())
        })
        .collect()
}
fn artifact_path(response: &Value, id: &str) -> String {
    response["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["artifact_id"] == id)
        .unwrap()["relative_path"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn read_inventory(response: &Value, bytes: &BTreeMap<String, Vec<u8>>) -> Value {
    let path = artifact_path(
        response,
        response["inventory"]["artifact_id"].as_str().unwrap(),
    );
    serde_json::from_slice(&bytes[&path]).unwrap()
}
fn numbers(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.000001
}
fn raw_to_media(x: f64, y: f64, media: &[f64], rotation: u64) -> (f64, f64) {
    let u = x - media[0];
    let v = y - media[1];
    let width = media[2] - media[0];
    let height = media[3] - media[1];
    match rotation {
        0 => (u, height - v),
        90 => (v, u),
        180 => (width - u, v),
        270 => (height - v, width - u),
        _ => panic!("invalid authored rotation"),
    }
}
fn crop_offset(page: &Value) -> (f64, f64) {
    let media = numbers(&page["raw_media_box"]);
    let crop = numbers(&page["raw_crop_box"]);
    let mut left = f64::INFINITY;
    let mut top = f64::INFINITY;
    for x in [crop[0], crop[2]] {
        for y in [crop[1], crop[3]] {
            let (a, b) = raw_to_media(x, y, &media, page["rotation"].as_u64().unwrap());
            left = left.min(a);
            top = top.min(b);
        }
    }
    (left, top)
}

/// Uses author literals/coordinates. This does not create expected from output.
fn verify_author(
    response: &Value,
    bytes: &BTreeMap<String, Vec<u8>>,
    expected: &Value,
) -> Result<(), String> {
    macro_rules! require {
        ($ok:expr,$message:expr) => {
            if !$ok {
                return Err($message.to_owned());
            }
        };
    }
    require!(
        response["input"]["sha256"] == expected["raw"]["sha256"],
        "raw SHA differs from immutable author input"
    );
    require!(
        response["page_count"] == expected["expected_confirmed_page_count"],
        "full confirmed page count differs from author expected"
    );
    require!(
        response["status"] == expected["expected_response_status"],
        "response status differs from author policy"
    );
    let pages = response["pages"].as_array().unwrap();
    let expected_pages = expected["pages"].as_array().unwrap();
    require!(pages.len() == expected_pages.len(), "incomplete page list");
    if pages.is_empty() {
        require!(
            bytes.is_empty(),
            "unconfirmed page tree must not fabricate artifacts"
        );
        return Ok(());
    }
    let inventory = read_inventory(response, bytes);
    require!(
        inventory["page_count"] == expected["expected_confirmed_page_count"],
        "primitive page count differs"
    );
    for (index, (actual, want)) in pages.iter().zip(expected_pages).enumerate() {
        require!(
            actual["physical_page_index"] == index as u64,
            "wrong physical page index"
        );
        require!(
            actual["printed_page_label"].is_null(),
            "fabricated printed label"
        );
        require!(
            actual["status"] == want["expected_status"],
            "wrong per-page supported/review/failure state"
        );
        require!(
            actual["glyph_count"] == want["expected_glyph_count"],
            "missing or fabricated glyphs"
        );
        let geometry = &actual["geometry"];
        require!(
            numbers(&geometry["media_box"]) == numbers(&want["raw_media_box"]),
            "raw MediaBox differs"
        );
        require!(
            numbers(&geometry["crop_box"]) == numbers(&want["raw_crop_box"]),
            "raw CropBox differs"
        );
        require!(
            geometry["rotation"] == want["rotation"]
                && close(
                    geometry["user_unit"].as_f64().unwrap(),
                    want["user_unit"].as_f64().unwrap()
                ),
            "rotation/UserUnit differs"
        );
        let size = numbers(&want["display_size_pt"]);
        require!(
            close(geometry["display_width_pt"].as_f64().unwrap(), size[0])
                && close(geometry["display_height_pt"].as_f64().unwrap(), size[1]),
            "display pt dimensions differ"
        );
        let source = &inventory["pages"][index];
        if actual["status"] == "failed" {
            require!(
                actual["text_artifact"].is_null()
                    && actual["regions"].as_array().unwrap().is_empty(),
                "failed page fabricated text/regions"
            );
            require!(
                source["glyphs"].is_null()
                    && source["decode_error"]
                        .as_str()
                        .is_some_and(|v| !v.is_empty()),
                "failed page missing real decode diagnostic"
            );
            continue;
        }
        let glyphs = source["glyphs"].as_array().unwrap();
        let primitive_text: String = glyphs
            .iter()
            .map(|glyph| glyph["text"].as_str().unwrap())
            .collect();
        require!(
            primitive_text == want["authored_glyph_stream_utf8"].as_str().unwrap(),
            "author stream text was lost/reordered/fabricated"
        );
        if want["visual_objects_expected"] == true {
            let object_count: u64 = source["visual_objects"]
                .as_object()
                .unwrap()
                .values()
                .map(|v| v.as_u64().unwrap())
                .sum();
            require!(object_count > 0, "visual source evidence omitted");
        }
        // Check every original author line using its primitive ordinals, even
        // for table/multicolumn text whose semantic line grouping is unverified.
        let (crop_left, crop_top) = crop_offset(want);
        let unit = want["user_unit"].as_f64().unwrap();
        let mut cursor = 0;
        for anchor in want["anchors"].as_array().unwrap() {
            let count = anchor["excerpt_utf8"].as_str().unwrap().len();
            let mut bbox = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for glyph in &glyphs[cursor..cursor + count] {
                let v = numbers(&glyph["media_bbox"]);
                bbox[0] = bbox[0].min((v[0] - crop_left) * unit);
                bbox[1] = bbox[1].min((v[1] - crop_top) * unit);
                bbox[2] = bbox[2].max((v[2] - crop_left) * unit);
                bbox[3] = bbox[3].max((v[3] - crop_top) * unit);
            }
            cursor += count;
            if let Some(windows) = anchor["endpoint_windows_pt"].as_object() {
                for (axis, key) in ["left", "top", "right", "bottom"].iter().enumerate() {
                    let window = numbers(&windows[*key]);
                    require!(
                        bbox[axis] >= window[0] - 0.000001 && bbox[axis] <= window[1] + 0.000001,
                        "glyph bbox points to a different author region"
                    );
                }
            } else {
                let envelope = numbers(&anchor["display_envelope_pt"]);
                require!(
                    bbox[0] >= envelope[0]
                        && bbox[1] >= envelope[1]
                        && bbox[2] <= envelope[2]
                        && bbox[3] <= envelope[3],
                    "rotated glyph position is outside author envelope"
                );
            }
        }
        if !want["text"].is_null() {
            let id = actual["text_artifact"]["artifact_id"].as_str().unwrap();
            let path = artifact_path(response, id);
            let actual_text = &bytes[&path];
            let expected_text = &want["text"];
            require!(
                actual_text == expected_text["utf8"].as_str().unwrap().as_bytes(),
                "independent full page text/order differs"
            );
            require!(
                sha(actual_text) == expected_text["sha256"].as_str().unwrap(),
                "independent text SHA differs"
            );
            let ranges = expected_text["line_ranges"].as_array().unwrap();
            let regions = actual["regions"].as_array().unwrap();
            require!(
                regions.len() == ranges.len(),
                "line/region count differs from author text"
            );
            for (region, range) in regions.iter().zip(ranges) {
                require!(
                    region["content"]["start_byte"] == range["start_byte"]
                        && region["content"]["end_byte"] == range["end_byte"],
                    "author UTF8 half-open range differs"
                );
                require!(
                    region["locator"]["physical_page_index"] == index as u64,
                    "region assigned to wrong page"
                );
                let ordinal = range["ordinal"].as_u64().unwrap() as usize;
                let bounds = &want["anchors"][ordinal]["endpoint_windows_pt"];
                let bbox = numbers(&region["locator"]["bbox_pt"]);
                for (axis, key) in ["left", "top", "right", "bottom"].iter().enumerate() {
                    let window = numbers(&bounds[*key]);
                    require!(
                        bbox[axis] >= window[0] - 0.000001 && bbox[axis] <= window[1] + 0.000001,
                        "line region bbox points to wrong author location"
                    );
                }
            }
        }
    }
    Ok(())
}

fn authored_fixture_bytes_and_expected_are_frozen() {
    let expected = fs::read(fixture().join("expected.json")).unwrap();
    assert_eq!(sha(&expected), EXPECTED_SHA);
    let freeze: Value =
        serde_json::from_slice(&fs::read(fixture().join("freeze.json")).unwrap()).unwrap();
    assert_eq!(freeze["expected_sha256"], EXPECTED_SHA);
    assert_eq!(freeze["before_first_adapter_extraction"], true);
    assert_eq!(freeze["pdf_count"], 15);
    assert_eq!(
        sha(&fs::read(fixture().join("author.py")).unwrap()),
        freeze["authoring_script_sha256"]
    );
    for item in manifest()["cases"].as_array().unwrap() {
        let bytes =
            fs::read(fixture().join(item["raw"]["relative_path"].as_str().unwrap())).unwrap();
        assert_eq!(sha(&bytes), item["raw"]["sha256"]);
        assert_eq!(
            bytes.len() as u64,
            item["raw"]["byte_length"].as_u64().unwrap()
        );
    }
}

fn all_fifteen_real_cli_cases_match_author_manifest() {
    let root = sandbox("matrix");
    let sentinel = root.join("outside-sentinel.txt");
    let before = fingerprint(&sentinel);
    let mut evidence = Vec::new();
    for expected in manifest()["cases"].as_array().unwrap() {
        let id = expected["case_id"].as_str().unwrap();
        let input = fixture().join(expected["raw"]["relative_path"].as_str().unwrap());
        let source_before = fingerprint(&input);
        let new = root.join(id);
        let started = Instant::now();
        let output = run_cli(&input, &new, &[]);
        let elapsed = started.elapsed();
        record(&root, id, &output);
        assert_eq!(
            output.status.code().unwrap(),
            expected["expected_exit_code"].as_i64().unwrap() as i32,
            "{id}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let saved =
            fs::read(new.join("parse.json")).expect("Host must persist the same real report");
        assert!(
            !new.join(".parse.pending.json").exists(),
            "a committed report must not leave an uncommitted candidate"
        );
        assert_eq!(
            output.stdout, saved,
            "stdout must be the persisted report bytes"
        );
        let report: Value = serde_json::from_slice(&saved).unwrap();
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["producer"], "parser");
        assert_eq!(report["operation"], "source_parse_pdf");
        assert_eq!(report["status"], expected["expected_label"]);
        assert_eq!(report["input"]["sha256"], expected["raw"]["sha256"]);
        assert_eq!(
            sha(&fs::read(new.join("input/raw.bin")).unwrap()),
            source_before.0
        );
        assert!(!new.join("project.sqlite").exists() && !new.join("Vault").exists());
        let response = &report["response"];
        assert!(
            !response.is_null(),
            "{id}: honest partial/failed PDF response must remain inspectable"
        );
        let bytes = artifact_bytes(response, &new);
        verify_author(response, &bytes, expected)
            .unwrap_or_else(|message| panic!("{id}: {message}"));
        let display = report["page_display"]
            .as_array()
            .expect("Host must expose verified physical page numbering");
        let wanted_pages = expected["pages"].as_array().unwrap();
        assert_eq!(display.len(), wanted_pages.len());
        for (index, (shown, wanted)) in display.iter().zip(wanted_pages).enumerate() {
            assert_eq!(shown["physical_page_idx"], index as u64);
            assert_eq!(
                shown["displayed_physical_page"],
                wanted["displayed_physical_page"]
            );
            assert_eq!(shown["displayed_physical_page"], index as u64 + 1);
            assert!(shown["printed_page_label"].is_null());
            assert_eq!(shown["status"], wanted["expected_status"]);
        }
        assert_eq!(
            report["native_exit_code"], 0,
            "{id}: actual parser must exit successfully with its honest response"
        );
        if expected["expected_label"] == "ready" {
            assert_eq!(report["validation_status"], "passed");
            assert!(report["failure"].is_null());
        } else {
            assert_eq!(report["validation_status"], "passed");
            assert!(!report["failure"].is_null());
            assert!(
                report["violations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|issue| issue["rule_id"] == "PDF_REPORTED_INCOMPLETE")
            );
            assert!(
                !response["issues"].as_array().unwrap().is_empty(),
                "{id}: missing actual review/failure issue"
            );
        }
        evidence.push(json!({"case_id":id,"exit":output.status.code(),"elapsed_ms":elapsed.as_millis(),"input_bytes":source_before.1,"artifact_bytes":bytes.values().map(|v|v.len()).sum::<usize>(),"page_count":response["page_count"],"report":new.join("parse.json")}));
        assert_eq!(
            fingerprint(&input),
            source_before,
            "fixture was modified by {id}"
        );
        assert_eq!(
            fingerprint(&sentinel),
            before,
            "outside sentinel modified by {id}"
        );
    }
    fs::write(
        root.join("matrix-evidence.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}

fn baseline(id: &str, label: &str) -> (Value, Value, BTreeMap<String, Vec<u8>>) {
    let root = sandbox(label);
    let new = root.join("parse");
    let result = run_cli(&input(id), &new, &[]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    record(&root, "baseline", &result);
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(!new.join(".parse.pending.json").exists());
    let response = report["response"].clone();
    let request = report["request"].clone();
    let bytes = artifact_bytes(&response, &new);
    verify_author(&response, &bytes, &case(id)).unwrap();
    (request, response, bytes)
}
fn core(request: &Value, response: &Value, bytes: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let request: PdfRequest = serde_json::from_value(request.clone()).unwrap();
    let response: PdfResponse = serde_json::from_value(response.clone()).unwrap();
    validate_pdf(
        &request,
        &response,
        bytes,
        &PdfLimits::default(),
        &AtomicBool::new(false),
    )
    .iter()
    .map(|v| v.rule_id.clone())
    .collect()
}
fn synchronize_resource(
    response: &mut Value,
    bytes: &mut BTreeMap<String, Vec<u8>>,
    path: &str,
    data: Vec<u8>,
) {
    let digest = sha(&data);
    let length = data.len();
    bytes.insert(path.to_owned(), data);
    let mut id = String::new();
    for item in response["artifacts"].as_array_mut().unwrap() {
        if item["relative_path"] == path {
            item["sha256"] = json!(digest);
            item["byte_length"] = json!(length);
            id = item["artifact_id"].as_str().unwrap().to_owned();
        }
    }
    if response["inventory"]["artifact_id"] == id {
        response["inventory"]["sha256"] = json!(digest);
    }
    for page in response["pages"].as_array_mut().unwrap() {
        if page["text_artifact"]["artifact_id"] == id {
            page["text_artifact"]["sha256"] = json!(digest);
        }
        for region in page["regions"].as_array_mut().unwrap() {
            if region["content"]["artifact_id"] == id {
                region["content"]["sha256"] = json!(digest);
            }
        }
    }
}
fn real_result_page_text_geometry_and_resource_mutations_fail_core() {
    let (request, response, resources) = baseline("N01", "core-mutations");
    assert!(core(&request, &response, &resources).is_empty());
    let mutations: &[(&str, fn(&mut Value, &mut BTreeMap<String, Vec<u8>>))] = &[
        ("missing-page", |r, _| {
            r["pages"].as_array_mut().unwrap().clear();
        }),
        ("wrong-page-index", |r, _| {
            r["pages"][0]["physical_page_index"] = json!(1);
        }),
        ("wrong-region-page", |r, _| {
            r["pages"][0]["regions"][0]["locator"]["physical_page_index"] = json!(1);
        }),
        ("invented-printed-label", |r, _| {
            r["pages"][0]["printed_page_label"] = json!("99");
        }),
        ("within-page-wrong-bbox", |r, _| {
            for axis in [1, 3] {
                let v = r["pages"][0]["regions"][0]["locator"]["bbox_pt"][axis]
                    .as_f64()
                    .unwrap();
                r["pages"][0]["regions"][0]["locator"]["bbox_pt"][axis] = json!(v + 36.0);
            }
        }),
        ("out-of-bounds-bbox", |r, _| {
            r["pages"][0]["regions"][0]["locator"]["bbox_pt"][2] = json!(9999);
        }),
        ("zero-bbox", |r, _| {
            r["pages"][0]["regions"][0]["locator"]["bbox_pt"] = json!([72, 60, 72, 60]);
        }),
        ("missing-glyph-reference", |r, _| {
            r["pages"][0]["regions"][0]["glyph_ordinals"]
                .as_array_mut()
                .unwrap()
                .pop();
        }),
        ("wrong-utf8-range", |r, _| {
            r["pages"][0]["regions"][0]["content"]["end_byte"] = json!(1);
        }),
        ("raw-offset-disguised-as-text", |r, _| {
            r["pages"][0]["regions"][0]["content"]["end_byte"] = json!(931);
        }),
        ("wrong-artifact-reference", |r, _| {
            r["pages"][0]["regions"][0]["content"]["artifact_id"] = json!("Missing_text");
        }),
        ("path-escape", |r, _| {
            r["artifacts"][0]["relative_path"] = json!("../outside-sentinel.txt");
        }),
        ("incorrect-resource-sha", |r, _| {
            r["artifacts"][0]["sha256"] = json!("0".repeat(64));
        }),
        ("physical-resource-byte-change", |_, b| {
            b.get_mut("artifacts/P0_text.txt").unwrap()[0] = b'X';
        }),
        ("synced-missing-body", |r, b| {
            let data = b"SINGLE ALPHA\nOne column has four lines.\nSINGLE OMEGA\n".to_vec();
            synchronize_resource(r, b, "artifacts/P0_text.txt", data);
        }),
        ("synced-wrong-reading-order", |r, b| {
            let data=b"SINGLE OMEGA\nOne column has four lines.\nHeading 99 is not a page label.\nSINGLE ALPHA\n".to_vec();
            synchronize_resource(r, b, "artifacts/P0_text.txt", data);
        }),
        ("utf8-boundary-cut", |r, b| {
            synchronize_resource(r, b, "artifacts/P0_text.txt", "AéB\n".as_bytes().to_vec());
            r["pages"][0]["regions"][0]["content"]["start_byte"] = json!(2);
            r["pages"][0]["regions"][0]["content"]["end_byte"] = json!(3);
        }),
    ];
    for (name, mutate) in mutations {
        let mut candidate = response.clone();
        let mut bytes = resources.clone();
        mutate(&mut candidate, &mut bytes);
        let violations = core(&request, &candidate, &bytes);
        CORE_MUTATIONS.fetch_add(1, Ordering::Relaxed);
        assert!(
            !violations.is_empty(),
            "real result mutation {name} was accepted"
        );
        println!("mutation {name}: {violations:?}");
    }
    let (request, response, bytes) = baseline("N02", "page-mutations");
    for count_change in [false, true] {
        let mut candidate = response.clone();
        candidate["pages"].as_array_mut().unwrap().remove(1);
        if count_change {
            candidate["page_count"] = json!(2);
        }
        assert!(!core(&request, &candidate, &bytes).is_empty());
        CORE_MUTATIONS.fetch_add(1, Ordering::Relaxed);
    }
    let mut swapped = response.clone();
    swapped["pages"].as_array_mut().unwrap().swap(0, 2);
    assert!(!core(&request, &swapped, &bytes).is_empty());
    CORE_MUTATIONS.fetch_add(1, Ordering::Relaxed);
}
fn self_consistent_parser_position_forgery_fails_independent_author_expected() {
    let (request, mut response, mut bytes) = baseline("N01", "self-consistent-forgery");
    let original_response = response.clone();
    let original_bytes = bytes.clone();
    let mut inventory = read_inventory(&response, &bytes);
    // Shift all first-line primitive boxes and its region by 20pt, preserving
    // text, order, SHA and in-page geometry. Core can prove internal consistency;
    // only the separate author coordinates prove this location is wrong.
    for glyph in inventory["pages"][0]["glyphs"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .take(12)
    {
        for axis in [0, 2] {
            let v = glyph["media_bbox"][axis].as_f64().unwrap();
            glyph["media_bbox"][axis] = json!(v + 20.0);
        }
    }
    for axis in [0, 2] {
        let v = response["pages"][0]["regions"][0]["locator"]["bbox_pt"][axis]
            .as_f64()
            .unwrap();
        response["pages"][0]["regions"][0]["locator"]["bbox_pt"][axis] = json!(v + 20.0);
    }
    synchronize_resource(
        &mut response,
        &mut bytes,
        "artifacts/pdf_inventory.bin",
        serde_json::to_vec(&inventory).unwrap(),
    );
    assert!(
        core(&request, &response, &bytes).is_empty(),
        "self-consistent data should expose the boundary between Core consistency and PDF semantics"
    );
    assert!(
        verify_author(&response, &bytes, &case("N01")).is_err(),
        "independent expected must reject self-consistent fake position"
    );
    CORE_MUTATIONS.fetch_add(1, Ordering::Relaxed);
    // A same-width Courier character can also be forged consistently in both
    // primitives and page text. This is not detectable from parser evidence
    // alone; the previously fixed author literal is the independent witness.
    let mut response = original_response;
    let mut bytes = original_bytes;
    let mut inventory = read_inventory(&response, &bytes);
    inventory["pages"][0]["glyphs"][0]["text"] = json!("F");
    synchronize_resource(
        &mut response,
        &mut bytes,
        "artifacts/pdf_inventory.bin",
        serde_json::to_vec(&inventory).unwrap(),
    );
    let mut text = bytes["artifacts/P0_text.txt"].clone();
    text[0] = b'F';
    synchronize_resource(&mut response, &mut bytes, "artifacts/P0_text.txt", text);
    assert!(
        core(&request, &response, &bytes).is_empty(),
        "self-consistent invented text is outside the Core's semantic proof boundary"
    );
    assert!(
        verify_author(&response, &bytes, &case("N01")).is_err(),
        "independent author literal must reject invented text"
    );
    CORE_MUTATIONS.fetch_add(1, Ordering::Relaxed);
}

fn real_cli_argument_existing_root_and_timeout_failures_preserve_inputs() {
    let root = sandbox("cli-protection");
    let sentinel = root.join("outside-sentinel.txt");
    let before = fingerprint(&sentinel);
    let source = input("N01");
    let source_before = fingerprint(&source);
    let existing = root.join("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("manual.txt"), b"Keep manual content").unwrap();
    let manual = fingerprint(&existing.join("manual.txt"));
    let result = run_cli(&source, &existing, &[]);
    record(&root, "exists", &result);
    assert_ne!(result.status.code(), Some(0));
    assert_eq!(fingerprint(&existing.join("manual.txt")), manual);
    let timeout = run_cli(&source, &root.join("timeout"), &["--timeout-ms", "1"]);
    record(&root, "timeout", &timeout);
    assert_eq!(timeout.status.code(), Some(3));
    let invalid = Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(["source", "parse-pdf"])
        .arg(&source)
        .arg(root.join("missing-python"))
        .output()
        .unwrap();
    CLI_CALLS.fetch_add(1, Ordering::Relaxed);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(!root.join("missing-python").exists());
    assert_eq!(fingerprint(&source), source_before);
    assert_eq!(fingerprint(&sentinel), before);
}
fn host_call(
    spec: &PdfParserSpec,
    source: &Path,
    new: &Path,
    limits: PdfLimits,
    cancel: &AtomicBool,
) -> Result<noetome_cli::pdf_parse::PdfParseOutput, noetome_cli::sidecar::SidecarError> {
    HOST_CALLS.fetch_add(1, Ordering::Relaxed);
    let result = parse_pdf(spec, source, new, limits, cancel);
    match &result {
        Ok(output) if !output.failed => assert!(!new.join(".parse.pending.json").exists()),
        Err(_) => {
            if let Ok(bytes) = fs::read(new.join("parse.json")) {
                let report: Value = serde_json::from_slice(&bytes).unwrap();
                assert_ne!(
                    report["status"], "ready",
                    "an API failure cannot leave a formal ready report"
                );
            }
        }
        _ => {}
    }
    result
}
fn assert_failed(
    result: Result<noetome_cli::pdf_parse::PdfParseOutput, noetome_cli::sidecar::SidecarError>,
    label: &str,
) {
    match result {
        Ok(output) => assert!(output.failed, "{label} returned a ready report"),
        Err(error) => {
            assert!(!error.code.is_empty(), "{label} needs a stable diagnostic");
            println!("failure {label}: {}", error.code);
        }
    }
}
fn real_host_read_policy_and_adapter_failures_are_bounded() {
    let root = sandbox("host-policies");
    let sentinel = root.join("outside-sentinel.txt");
    let before = fingerprint(&sentinel);
    let spec = PdfParserSpec {
        python: python(),
        adapter: adapter(),
    };
    let cancel = AtomicBool::new(false);
    assert_failed(
        host_call(
            &spec,
            &root.join("does-not-exist.pdf"),
            &root.join("unread"),
            PdfLimits::default(),
            &cancel,
        ),
        "missing input",
    );
    for kind in ["input", "pages", "glyphs", "regions", "objects", "artifact"] {
        let mut limits = PdfLimits::default();
        let source = match kind {
            "pages" => input("N02"),
            "objects" => input("C01"),
            _ => input("N01"),
        };
        match kind {
            "input" => limits.transport.max_input_bytes = 1,
            "pages" => limits.policy.max_pages = 2,
            "glyphs" => limits.policy.max_glyphs = 1,
            "regions" => limits.policy.max_regions = 1,
            "objects" => limits.policy.max_objects = 0,
            "artifact" => limits.transport.max_artifact_bytes = 1,
            _ => unreachable!(),
        }
        assert_failed(
            host_call(&spec, &source, &root.join(kind), limits, &cancel),
            kind,
        );
    }
    for scenario in ["exit_nonzero", "extra_stdout", "stdout_limit"] {
        let script = fake::write_adapter(&root, scenario, None);
        let spec = PdfParserSpec {
            python: python(),
            adapter: script,
        };
        let mut limits = PdfLimits::default();
        limits.transport.max_stdout_bytes = 1024;
        assert_failed(
            host_call(&spec, &input("N01"), &root.join(scenario), limits, &cancel),
            scenario,
        );
    }
    if let Some(selected) = std::env::var_os("NOETOME_TEST_PDF_MISMATCH_PYTHON") {
        let mismatch_python = PathBuf::from(selected);
        assert!(mismatch_python.is_absolute() && mismatch_python.is_file());
        let version = Command::new(&mismatch_python)
            .args([
                "-I",
                "-B",
                "-c",
                "import platform;print(platform.python_version())",
            ])
            .output()
            .unwrap();
        assert!(version.status.success());
        assert_ne!(
            String::from_utf8(version.stdout).unwrap().trim(),
            "3.12.14",
            "select a real installed Python version distinct from the lock"
        );
        CLI_CALLS.fetch_add(1, Ordering::Relaxed);
        let result = Command::new(env!("CARGO_BIN_EXE_noetome"))
            .arg("source")
            .arg("parse-pdf")
            .arg(input("N01"))
            .arg(root.join("runtime-mismatch"))
            .arg("--python")
            .arg(&mismatch_python)
            .output()
            .unwrap();
        record(&root, "actual-runtime-mismatch", &result);
        assert_eq!(
            result.status.code(),
            Some(2),
            "real runtime/config mismatch is CLI input classification"
        );
        let report: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(report["native_exit_code"], 2);
        assert!(report["response"].is_null());
        let diagnostics: Vec<u8> = serde_json::from_value(report["diagnostics"].clone()).unwrap();
        assert!(String::from_utf8_lossy(&diagnostics).contains("PDF_RUNTIME_MISMATCH"));
    } else {
        // A genuine native Python child invokes the real lock checker against a
        // deliberately different test-only lock. No PDF success is fabricated.
        let script = fake::write_adapter(&root, "runtime_mismatch_configured", Some(&adapter()));
        let mismatch_spec = PdfParserSpec {
            python: python(),
            adapter: script,
        };
        let output = host_call(
            &mismatch_spec,
            &input("N01"),
            &root.join("configured-runtime-mismatch"),
            PdfLimits::default(),
            &cancel,
        )
        .unwrap();
        assert!(output.failed);
        assert_eq!(output.exit_code, 2);
        assert_eq!(output.report.native_exit_code, Some(2));
        assert!(output.report.response.is_none());
        assert!(
            String::from_utf8_lossy(&output.report.diagnostics).contains("PDF_RUNTIME_MISMATCH")
        );
        println!(
            "runtime mismatch: actual native child, test-configured lock; alternate installed runtime was not selected"
        );
    }
    assert_eq!(fingerprint(&sentinel), before);
}
fn actual_python_timeout_and_cancel_after_handshake_end_process() {
    let root = sandbox("python-lifecycle");
    let script = fake::write_adapter(&root, "sleep", None);
    let spec = PdfParserSpec {
        python: python(),
        adapter: script,
    };
    for (name, cancel_after_handshake) in [("timeout", false), ("cancel", true)] {
        let new = root.join(name);
        let handshake = new.join("artifacts/handshake.bin");
        let cancel = Arc::new(AtomicBool::new(false));
        let pending = Arc::clone(&cancel);
        let watcher = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(8);
            while Instant::now() < deadline {
                if let Ok(data) = fs::read(&handshake) {
                    if let Ok(value) = serde_json::from_slice::<Value>(&data) {
                        let pid = value["pid"].as_u64().unwrap() as u32;
                        let observer = noetome_sidecar_platform::ProcessObserver::open(pid)
                            .expect("capture real live Python process");
                        let creation_time = observer.creation_time();
                        assert_eq!(observer.wait_exit(Duration::ZERO).unwrap(), None);
                        if cancel_after_handshake {
                            pending.store(true, Ordering::Release);
                        }
                        let exit = observer.wait_exit(Duration::from_secs(8)).unwrap();
                        drop(observer); // Release process references before Host Job cleanup completes.
                        return Some((pid, creation_time, exit));
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            None
        });
        let mut limits = PdfLimits::default();
        if !cancel_after_handshake {
            limits.transport.timeout = Duration::from_millis(700);
        }
        let started = Instant::now();
        let result = host_call(&spec, &input("N01"), &new, limits, &cancel);
        let (pid, creation_time, exit) = watcher
            .join()
            .unwrap()
            .expect("timeout/cancel requires actual Python handshake");
        assert!(exit.is_some(), "real Python process did not exit");
        assert!(started.elapsed() < Duration::from_secs(8));
        let expected_code = if cancel_after_handshake {
            "SIDECAR_CANCELLED"
        } else {
            "SIDECAR_TIMEOUT"
        };
        match result {
            Ok(output) => {
                assert!(
                    !cancel_after_handshake,
                    "cancel must stop report publication"
                );
                assert!(output.failed);
                assert_eq!(output.exit_code, 3);
                assert_eq!(output.report.failure.as_ref().unwrap().code, expected_code);
                assert_eq!(output.report.pid, Some(pid));
            }
            Err(error) => {
                assert_eq!(error.code, expected_code, "{error:?}");
                assert_eq!(error.pid, Some(pid));
            }
        }
        println!(
            "actual Python {name}: pid={pid}, creation_time={creation_time}, native_exit={exit:?}"
        );
    }
}
fn real_parser_resource_inventory_addition_is_rejected() {
    let root = sandbox("actual-resource-race");
    let script = fake::write_adapter(&root, "extra_resource", Some(&adapter()));
    let spec = PdfParserSpec {
        python: python(),
        adapter: script,
    };
    let result = host_call(
        &spec,
        &input("N01"),
        &root.join("parse"),
        PdfLimits::default(),
        &AtomicBool::new(false),
    );
    match result {
        Ok(output) => {
            assert!(output.failed);
            let report: Value = serde_json::from_str(&output.json).unwrap();
            assert!(
                report["response"].is_null(),
                "unknown resource must invalidate verified page claims"
            );
        }
        Err(error) => assert!(!error.code.is_empty()),
    }
    assert!(
        root.join("parse/artifacts/pdf_inventory.bin").is_file(),
        "the underlying real parser must actually have produced its inventory"
    );
    assert!(
        root.join("parse/artifacts/unlisted.bin").is_file(),
        "the real extra-resource fault must exist"
    );
}
#[cfg(windows)]
fn actual_windows_locked_input_and_hardlinked_boundaries_are_checked() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = sandbox("windows-input");
    let source = root.join("source.pdf");
    fs::copy(input("N01"), &source).unwrap();
    let before = fingerprint(&source);
    let spec = PdfParserSpec {
        python: python(),
        adapter: adapter(),
    };
    let guard = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&source)
        .unwrap();
    assert_failed(
        host_call(
            &spec,
            &source,
            &root.join("locked"),
            PdfLimits::default(),
            &AtomicBool::new(false),
        ),
        "real locked file",
    );
    drop(guard);
    assert_eq!(fingerprint(&source), before);
    fs::hard_link(&source, root.join("same-bytes-link.pdf")).unwrap();
    let alias = root.join("same-bytes-link.pdf");
    let alias_before = fingerprint(&alias);
    let result = host_call(
        &spec,
        &source,
        &root.join("hardlink"),
        PdfLimits::default(),
        &AtomicBool::new(false),
    )
    .expect("an explicitly selected read-only input may have another hardlink");
    assert!(!result.failed);
    assert_eq!(fingerprint(&source), before);
    assert_eq!(fingerprint(&alias), alias_before);
    let source_handle = fs::File::open(&source).unwrap();
    let snapshot_handle = fs::File::open(root.join("hardlink/input/raw.bin")).unwrap();
    assert_eq!(
        noetome_sidecar_platform::file_link_count(&source_handle).unwrap(),
        2
    );
    assert_eq!(
        noetome_sidecar_platform::file_link_count(&snapshot_handle).unwrap(),
        1
    );
    drop(source_handle);
    drop(snapshot_handle);
    let script = fake::write_adapter(&root, "resource_hardlink", Some(&adapter()));
    let resource_spec = PdfParserSpec {
        python: python(),
        adapter: script,
    };
    let result = host_call(
        &resource_spec,
        &input("N01"),
        &root.join("linked-resource"),
        PdfLimits::default(),
        &AtomicBool::new(false),
    );
    match result {
        Ok(output) => {
            assert!(output.failed);
            assert_eq!(output.exit_code, 2);
            assert!(output.report.response.is_none());
        }
        Err(error) => assert_eq!(error.code, "SIDECAR_RESOURCE_INVALID"),
    }
    let resource_alias = root.join("resource-hardlink-alias.txt");
    assert_eq!(
        fs::read(&resource_alias).unwrap(),
        case("N01")["pages"][0]["text"]["utf8"]
            .as_str()
            .unwrap()
            .as_bytes()
    );
    assert_eq!(
        noetome_sidecar_platform::file_link_count(&fs::File::open(&resource_alias).unwrap())
            .unwrap(),
        2
    );
}
#[cfg(not(windows))]
fn actual_windows_locked_input_and_hardlinked_boundaries_are_checked() {
    panic!("Windows x64 behavior requires the supported Windows test host");
}

fn actual_report_publication_cancel_boundary_preserves_pending_evidence() {
    let root = sandbox("report-publication");
    let original = fs::read(input("N01")).unwrap();
    let startxref = original
        .windows(b"startxref\n".len())
        .rposition(|window| window == b"startxref\n")
        .unwrap();
    let footer = original[startxref..].to_vec();
    let mut padded = original;
    padded.resize(padded.len() + 32 * 1024 * 1024, b' ');
    padded.push(b'\n');
    padded.extend_from_slice(&footer);
    let source = root.join("selected-padded.pdf");
    fs::write(&source, &padded).unwrap();
    let source_before = fingerprint(&source);
    let spec = PdfParserSpec {
        python: python(),
        adapter: adapter(),
    };
    let mut evidence = Vec::new();
    for (name, before_publication) in [("pending-cancel", true), ("published-cancel", false)] {
        let new = root.join(name);
        let watched = new.join(if before_publication {
            ".parse.pending.json"
        } else {
            "parse.json"
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let trigger = Arc::clone(&cancel);
        let started = Instant::now();
        let watcher = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(25);
            while Instant::now() < deadline {
                if let Ok(bytes) = fs::read(&watched) {
                    if bytes.ends_with(b"\n") && serde_json::from_slice::<Value>(&bytes).is_ok() {
                        trigger.store(true, Ordering::Release);
                        return Some((bytes, started.elapsed().as_millis()));
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            None
        });
        let result = host_call(&spec, &source, &new, PdfLimits::default(), &cancel);
        let api_elapsed = started.elapsed().as_millis();
        let (observed_bytes, observed_ms) = watcher.join().unwrap().expect(
            "must observe a complete LF-terminated report at the requested publication stage",
        );
        assert!(cancel.load(Ordering::Acquire));
        let observed: Value = serde_json::from_slice(&observed_bytes).unwrap();
        assert_eq!(observed["status"], "ready");
        assert_eq!(observed["input"]["sha256"], source_before.0);
        if before_publication {
            let error = result.expect_err("cancelled uncommitted report must not be published");
            assert_eq!(error.code, "SIDECAR_CANCELLED");
            assert!(!new.join("parse.json").exists());
            assert_eq!(
                fs::read(new.join(".parse.pending.json")).unwrap(),
                observed_bytes,
                "retain the exact uncommitted evidence bytes"
            );
        } else {
            let output = result
                .expect("cancellation after delivery may not invalidate the delivered result");
            assert!(!output.failed);
            assert_eq!(output.exit_code, 0);
            assert_eq!(output.json.as_bytes(), observed_bytes);
            assert_eq!(fs::read(new.join("parse.json")).unwrap(), observed_bytes);
            assert!(!new.join(".parse.pending.json").exists());
        }
        assert_eq!(fingerprint(&source), source_before);
        evidence.push(json!({"scenario":name,"source_sha256":source_before.0,"source_byte_length":source_before.1,"observed_complete_report_sha256":sha(&observed_bytes),"observer_elapsed_ms":observed_ms,"api_elapsed_ms":api_elapsed,"pending_exists":new.join(".parse.pending.json").exists(),"formal_report_exists":new.join("parse.json").exists()}));
    }
    fs::write(
        root.join("publication-cancel-evidence.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}
