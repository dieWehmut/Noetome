//! Exact renderer combinations with independent native display evidence.
//!
//! A caller's support flag cannot establish a renderer capability. These
//! predicates only identify a measured environment; source semantics and
//! representation-specific constraints are still checked by each planner.
use crate::process_flow_model::{FlowProfile, FlowchartSupport};

pub(crate) fn math(app: &str, app_version: &str, mathjax_version: &str) -> bool {
    matches!(
        (app, app_version, mathjax_version),
        ("Obsidian", "1.6.7", "3.2.2") | ("Obsidian", "1.15.0", "4.1.3")
    )
}

pub(crate) fn svg(app: &str, app_version: &str) -> bool {
    matches!((app, app_version), ("Obsidian", "1.6.7" | "1.15.0"))
}

pub(crate) fn flow(profile: &FlowProfile) -> bool {
    let legacy = profile.schema_version == 1
        && profile.profile_id == "obsidian_1_6_7_flowchart"
        && profile.app_version == "1.6.7"
        && profile.mermaid_version.as_deref() == Some("10.6.1")
        && profile.renderer.is_none()
        && profile.html_labels;
    let pele = profile.schema_version == 2
        && profile.profile_id == "obsidian_1_15_0_pele_flowchart"
        && profile.app_version == "1.15.0"
        && profile.mermaid_version.is_none()
        && !profile.html_labels
        && profile.renderer.as_ref().is_some_and(|renderer| {
            renderer.name == "pele"
                && renderer.version == "1.8.0"
                && renderer.source_sha256
                    == "9bf34b3a9a19351dc69d0d238e365560331a8ae392c41656df7f5090653253d0"
        });
    profile.producer == "flow_render_profile"
        && profile.app == "Obsidian"
        && profile.flowchart_support == FlowchartSupport::Supported
        && profile.label_encoding == "decimal_ascii_punctuation_v1"
        && (legacy || pele)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn math_requires_an_exact_measured_pair() {
        assert!(math("Obsidian", "1.6.7", "3.2.2"));
        assert!(math("Obsidian", "1.15.0", "4.1.3"));
        for pair in [
            ("1.6.8", "3.2.2"),
            ("1.6.7", "4.1.3"),
            ("1.15.0", "3.2.2"),
            ("1.15.1", "4.1.3"),
            ("future", "future"),
        ] {
            assert!(!math("Obsidian", pair.0, pair.1));
        }
        assert!(!math("other", "1.6.7", "3.2.2"));
    }

    #[test]
    fn svg_does_not_infer_support_from_a_newer_version() {
        assert!(svg("Obsidian", "1.6.7"));
        assert!(svg("Obsidian", "1.15.0"));
        assert!(!svg("Obsidian", "1.6.8"));
        assert!(!svg("Obsidian", "1.15.1"));
        assert!(!svg("other", "1.6.7"));
    }

    #[test]
    fn flow_requires_the_measured_renderer_and_label_configuration() {
        let profile = crate::process_flow_plan::tests::profile();
        assert!(flow(&profile));
        let original = serde_json::to_value(profile).unwrap();
        for (key, value) in [
            ("profile_id", "caller_verified"),
            ("app_version", "1.15.0"),
            ("mermaid_version", "12.1.0"),
            ("flowchart_support", "unverified"),
        ] {
            let mut changed = original.clone();
            changed[key] = value.into();
            assert!(!flow(&serde_json::from_value(changed).unwrap()));
        }
        let mut changed = original;
        changed["html_labels"] = false.into();
        assert!(!flow(&serde_json::from_value(changed).unwrap()));
    }

    #[test]
    fn pele_requires_the_exact_engine_source_and_svg_label_configuration() {
        let original = serde_json::json!({
            "schema_version":2,"producer":"flow_render_profile","profile_id":"obsidian_1_15_0_pele_flowchart",
            "app":"Obsidian","app_version":"1.15.0","mermaid_version":null,
            "renderer":{"name":"pele","version":"1.8.0","source_sha256":"9bf34b3a9a19351dc69d0d238e365560331a8ae392c41656df7f5090653253d0"},
            "flowchart_support":"supported","label_encoding":"decimal_ascii_punctuation_v1","html_labels":false
        });
        assert!(flow(&serde_json::from_value(original.clone()).unwrap()));
        for (key, value) in [
            ("name", "mermaid"),
            ("version", "1.8.1"),
            (
                "source_sha256",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
        ] {
            let mut changed = original.clone();
            changed["renderer"][key] = value.into();
            assert!(!flow(&serde_json::from_value(changed).unwrap()));
        }
        for (key, value) in [
            ("app", serde_json::json!("other")),
            ("app_version", serde_json::json!("1.15.1")),
            ("profile_id", serde_json::json!("caller_verified")),
            ("html_labels", serde_json::json!(true)),
        ] {
            let mut changed = original.clone();
            changed[key] = value;
            assert!(!flow(&serde_json::from_value(changed).unwrap()));
        }
    }
}
