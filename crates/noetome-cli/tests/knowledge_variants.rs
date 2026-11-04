use serde_json::{Value,json};
use std::{fs,path::Path};
#[path="support/coordination_variants_case.rs"]
mod support;
use support::*;
#[test]
fn conditional_variants_preserve_all_author_fields() {
    let f = VariantsCase::prepare("conditional");
    let r = f.coordinate();
    f.assert_protected();
    println!("root={}", f.root.display());
    assert_eq!(r.status.code(),Some(0),"{}", String::from_utf8_lossy(&r.stderr));
    let report: Value = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(report["schema_version"],2);
    assert_eq!(report["totals"]["retained_fact_groups"],2);
    assert_eq!(report["totals"]["condition_variants"],2);
}
