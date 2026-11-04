use std::fs;
#[path = "support/presentation_case.rs"]
mod support;
use support::*;

// This catches a missing ordinary planner route after successful source review
// and coordination, rather than checking author-supplied rendered NotePlan.
#[test]
fn ordinary_present_chooses_table_after_source_review_and_successful_coordination() {
    let mut f = PresentationCase::prepare("two_objects");
    let coordinated = f.coordinate();
    assert_eq!(
        coordinated.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&coordinated.stderr)
    );
    assert!(f.output.join("coordination-ir.json").is_file());
    f.freeze_coordination();
    let presented = f.present();
    f.assert_protected();
    assert_eq!(
        presented.status.code(),
        Some(0),
        "ordinary knowledge present should generate a preview; root={} stderr={}",
        f.root.display(),
        String::from_utf8_lossy(&presented.stderr)
    );
    assert!(f.preview.join("plan.json").is_file());
    assert!(f.preview.join("知识比较.md").is_file());
    let plan: serde_json::Value =
        serde_json::from_slice(&fs::read(f.preview.join("plan.json")).unwrap()).unwrap();
    assert_eq!(plan["representation"], "table");
}
