//! Synthetic DTO checks only; actual PDF positions use a separate authoring oracle.
use super::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{Arc, atomic::AtomicBool};

fn sample() -> (Value, Response, BTreeMap<String, ResourceData>) {
    let bytes = b"A\xc3\xa9 B\n";
    let text_sha = format!("{:x}", Sha256::digest(bytes));
    let raw = b"synthetic raw bytes for DTO validation only";
    let raw_sha = format!("{:x}", Sha256::digest(raw));
    let fingerprint = json!({"sha256":raw_sha,"byte_length":raw.len()});
    let resources = json!([
        {"resource_id":"raw-pdf","relative_path":"raw.pdf","kind":"binary","fingerprint":fingerprint},
        {"resource_id":"page-text-000000","relative_path":"pages/000000.txt","kind":"utf8_text","fingerprint":{"sha256":text_sha,"byte_length":bytes.len()}}
    ]);
    let identity = serde_json::to_value(ParserIdentity::pinned()).unwrap();
    let manifest = json!({
        "schema_version":1,"format":"pdf-page-regions-v1","input_fingerprint":fingerprint,
        "parser":identity,"raw_resource_id":"raw-pdf","page_count":1,"status":"parsed",
        "pages":[{
            "physical_page_index":0,"display_page_number":1,"printed_page_label":null,"status":"parsed",
            "geometry":{"unit":"pt","origin":"crop_top_left","y_axis":"down","media_box":[0,0,600,800],"crop_box":[0,0,600,800],"rotation_degrees":0,"user_unit":1,"width_pt":600,"height_pt":800,"pdf_to_display":[1,0,0,-1,0,800]},
            "text":{"resource_id":"page-text-000000","sha256":text_sha,"byte_length":bytes.len()},
            "glyphs":[
                {"ordinal":0,"text":"A","bbox":[72,62,80,74],"font_name":"Helvetica","font_size":12,"upright":true},
                {"ordinal":1,"text":"é","bbox":[80,62,86,74],"font_name":"Helvetica","font_size":12,"upright":true},
                {"ordinal":2,"text":" ","bbox":[86,62,90,74],"font_name":"Helvetica","font_size":12,"upright":true},
                {"ordinal":3,"text":"B","bbox":[90,62,98,74],"font_name":"Helvetica","font_size":12,"upright":true}
            ],
            "regions":[{"region_id":"region-000000-000000","locator_kind":"pdf_page_region","physical_page_index":0,"bbox":[72,62,98,74],"text":{"resource_id":"page-text-000000","resource_sha256":text_sha,"start_byte":0,"end_byte":5},"glyph_indices":[0,1,2,3]}],
            "visuals":[]
        }],"resources":resources,"issues":[]
    });
    let mut response_resources = resources.as_array().unwrap().clone();
    response_resources.push(json!({"resource_id":"pdf-manifest","relative_path":"pdf-manifest.json","kind":"utf8_text","fingerprint":{"sha256":"0".repeat(64),"byte_length":1}}));
    let response: Response = serde_json::from_value(json!({
        "protocol_version":1,"request_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "workspace_capability_id":"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb","input_fingerprint":fingerprint,
        "producer":"parser","parser":{"name":"pdfplumber","version":"0.11.9","capability_version":1,"capabilities":["source_units_v1"]},
        "unit_count":1,"units":[{"unit_id":"page-000000","ordinal":0,"kind":"text","status":"parsed","text":{"resource_id":"page-text-000000","start_byte":0,"end_byte":6},"resource_ids":["raw-pdf","pdf-manifest","page-text-000000"]}],
        "resources":response_resources,"issues":[]
    })).unwrap();
    let data = BTreeMap::from([
        ("raw-pdf".into(), ResourceData::Binary(raw.to_vec())),
        (
            "page-text-000000".into(),
            ResourceData::Text(String::from_utf8(bytes.to_vec()).unwrap()),
        ),
    ]);
    (manifest, response, data)
}

fn control() -> Control {
    Control::new(30000, Arc::new(AtomicBool::new(false)))
}

fn replace_text(
    value: &mut Value,
    response: &mut Response,
    data: &mut BTreeMap<String, ResourceData>,
    bytes: &[u8],
) {
    let fp = Fingerprint {
        sha256: format!("{:x}", Sha256::digest(bytes)),
        byte_length: bytes.len() as u64,
    };
    value["pages"][0]["text"]["sha256"] = json!(fp.sha256);
    value["pages"][0]["text"]["byte_length"] = json!(fp.byte_length);
    value["resources"][1]["fingerprint"] = serde_json::to_value(&fp).unwrap();
    for region in value["pages"][0]["regions"].as_array_mut().unwrap() {
        region["text"]["resource_sha256"] = json!(fp.sha256);
    }
    response.resources[1].fingerprint = fp;
    response.units[0].text.as_mut().unwrap().end_byte = bytes.len() as u64;
    data.insert(
        "page-text-000000".into(),
        ResourceData::Text(String::from_utf8(bytes.to_vec()).unwrap()),
    );
}

#[test]
fn refuses_staggered_disjoint_line_columns_claimed_as_parsed() {
    let (mut value, mut response, mut data) = sample();
    let glyphs = json!([
        {"ordinal":0,"text":"L","bbox":[72,62,80,74],"font_name":"Helvetica","font_size":12,"upright":true},
        {"ordinal":1,"text":"R","bbox":[340,74,348,86],"font_name":"Helvetica","font_size":12,"upright":true},
        {"ordinal":2,"text":"L","bbox":[72,92,80,104],"font_name":"Helvetica","font_size":12,"upright":true},
        {"ordinal":3,"text":"R","bbox":[340,104,348,116],"font_name":"Helvetica","font_size":12,"upright":true}
    ]);
    let regions: Vec<Value>=glyphs.as_array().unwrap().iter().enumerate().map(|(i,glyph)| json!({
        "region_id":format!("region-000000-{i:06}"),"locator_kind":"pdf_page_region","physical_page_index":0,
        "bbox":glyph["bbox"],"glyph_indices":[i],"text":{"resource_id":"page-text-000000","resource_sha256":"","start_byte":i*2,"end_byte":i*2+1}
    })).collect();
    value["pages"][0]["glyphs"] = glyphs;
    value["pages"][0]["regions"] = json!(regions);
    replace_text(&mut value, &mut response, &mut data, b"L\nR\nL\nR\n");
    assert_eq!(
        validate(value, &response, &data).unwrap_err().code,
        "PDF_STATUS"
    );
}

#[test]
fn refuses_whitespace_only_page_claimed_as_parsed() {
    let (mut value, mut response, mut data) = sample();
    for glyph in value["pages"][0]["glyphs"].as_array_mut().unwrap() {
        glyph["text"] = json!(" ");
    }
    value["pages"][0]["regions"][0]["text"]["end_byte"] = json!(4);
    replace_text(&mut value, &mut response, &mut data, b"    \n");
    assert_eq!(
        validate(value, &response, &data).unwrap_err().code,
        "PDF_STATUS"
    );
}

#[test]
fn refuses_multicodepoint_glyph_claimed_as_parsed_despite_consistent_body() {
    let (mut value, mut response, mut data) = sample();
    value["pages"][0]["glyphs"][0]["text"] = json!("A ");
    value["pages"][0]["regions"][0]["text"]["end_byte"] = json!(6);
    replace_text(&mut value, &mut response, &mut data, b"A \xc3\xa9 B\n");
    assert_eq!(
        validate(value, &response, &data).unwrap_err().code,
        "PDF_STATUS"
    );
}
fn validate(
    value: Value,
    response: &Response,
    data: &BTreeMap<String, ResourceData>,
) -> Result<(), SidecarError> {
    validate_manifest(
        &serde_json::from_value(value).unwrap(),
        response,
        data,
        &control(),
    )
}
#[test]
fn accepts_declared_complete_synthetic_page_and_utf8_region() {
    let (value, response, data) = sample();
    validate(value, &response, &data).unwrap();
}
#[test]
fn accepts_review_page_with_confirmed_empty_artifact_and_no_wire_range() {
    let (mut value, mut response, mut data) = sample();
    let empty_fingerprint = Fingerprint {
        sha256: format!("{:x}", Sha256::digest(b"")),
        byte_length: 0,
    };
    value["status"] = json!("needs_review");
    value["pages"][0]["status"] = json!("needs_review");
    value["pages"][0]["glyphs"] = json!([]);
    value["pages"][0]["regions"] = json!([]);
    value["pages"][0]["text"]["sha256"] = json!(empty_fingerprint.sha256);
    value["pages"][0]["text"]["byte_length"] = json!(0);
    value["resources"][1]["fingerprint"] = serde_json::to_value(&empty_fingerprint).unwrap();
    response.resources[1].fingerprint = empty_fingerprint;
    response.units[0].status = ParseStatus::NeedsReview;
    response.units[0].kind = UnitKind::Unknown;
    response.units[0].text = None;
    let issue = Issue {
        code: "PDF_EMPTY_UNCONFIRMED".into(),
        severity: Severity::Blocking,
        message: "No confirmed text layer, requires review".into(),
        unit_ids: vec!["page-000000".into()],
        resource_ids: vec!["raw-pdf".into()],
    };
    response.issues.push(issue.clone());
    value["issues"] = serde_json::to_value(vec![issue]).unwrap();
    data.insert("page-text-000000".into(), ResourceData::Text(String::new()));
    validate(value, &response, &data).unwrap();
}
#[test]
fn rejects_missing_page_and_fake_page_count() {
    let (mut value, response, data) = sample();
    value["pages"] = json!([]);
    assert_eq!(
        validate(value, &response, &data).unwrap_err().code,
        "PDF_PAGES"
    );
}
#[test]
fn rejects_wrong_page_location() {
    let (mut value, response, data) = sample();
    value["pages"][0]["regions"][0]["physical_page_index"] = json!(1);
    assert_eq!(
        validate(value, &response, &data).unwrap_err().code,
        "PDF_LOCATOR"
    );
}
#[test]
fn rejects_in_bounds_bbox_that_does_not_enclose_its_actual_glyphs() {
    let (mut value, response, data) = sample();
    value["pages"][0]["regions"][0]["bbox"] = json!([82, 62, 108, 74]);
    assert_eq!(
        validate(value, &response, &data).unwrap_err().code,
        "PDF_LOCATOR"
    );
}
#[test]
fn rejects_missing_body_even_when_all_artifact_shas_are_valid() {
    let (mut value, response, data) = sample();
    value["pages"][0]["regions"] = json!([]);
    assert_eq!(
        validate(value, &response, &data).unwrap_err().code,
        "PDF_TEXT"
    );
}
#[test]
fn rejects_utf8_byte_range_cutting_an_actual_character() {
    let (mut value, response, data) = sample();
    value["pages"][0]["regions"][0]["text"]["start_byte"] = json!(2);
    assert_eq!(
        validate(value, &response, &data).unwrap_err().code,
        "PDF_TEXT"
    );
}
#[test]
fn rejects_wrong_crop_rotation_matrix() {
    let (mut value, response, data) = sample();
    value["pages"][0]["geometry"]["pdf_to_display"] = json!([1, 0, 0, -1, 10, 800]);
    assert_eq!(
        validate(value, &response, &data).unwrap_err().code,
        "PDF_LOCATOR"
    );
}
#[test]
fn rejects_review_or_failed_page_claimed_as_whole_document_parsed() {
    let (mut value, mut response, data) = sample();
    value["pages"][0]["status"] = json!("needs_review");
    response.units[0].status = ParseStatus::NeedsReview;
    let issue = Issue {
        code: "PDF_UNCERTAIN".into(),
        severity: Severity::Blocking,
        message: "Synthetic uncertain page".into(),
        unit_ids: vec!["page-000000".into()],
        resource_ids: vec!["raw-pdf".into()],
    };
    response.issues.push(issue.clone());
    value["issues"] = serde_json::to_value(vec![issue]).unwrap();
    assert_eq!(
        validate(value, &response, &data).unwrap_err().code,
        "PDF_STATUS"
    );
}
#[test]
fn rejects_unconfirmed_page_count_claimed_as_parsed() {
    let (mut value, response, data) = sample();
    value["page_count"] = Value::Null;
    assert!(validate(value, &response, &data).is_err());
}
#[test]
fn rejects_nonfinite_and_out_of_page_coordinates() {
    let (mut value, response, data) = sample();
    value["pages"][0]["glyphs"][0]["bbox"] = json!([-1, 62, 80, 74]);
    assert_eq!(
        validate(value, &response, &data).unwrap_err().code,
        "PDF_LOCATOR"
    );
    let (value, response, data) = sample();
    let mut manifest: Manifest = serde_json::from_value(value).unwrap();
    manifest.pages[0].glyphs[0].bbox[0] = f64::NAN;
    assert_eq!(
        validate_manifest(&manifest, &response, &data, &control())
            .unwrap_err()
            .code,
        "PDF_LOCATOR"
    );
}
#[test]
fn rejects_text_artifact_resource_sha_or_page_reference_change() {
    let (mut value, response, data) = sample();
    value["pages"][0]["text"]["sha256"] = json!("f".repeat(64));
    assert_eq!(
        validate(value, &response, &data).unwrap_err().code,
        "PDF_RESOURCE"
    );
}
