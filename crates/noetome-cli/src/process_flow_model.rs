//! Strict, source-backed flow plans; no caller-supplied diagram syntax.
use crate::process_model::{ProcessDocument, RelationKind, StepKind};
use serde::{Deserialize, Serialize};

fn nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
macro_rules! fixed {
    ($name:ident, $ty:ty, $value:expr) => {
        fn $name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<$ty, D::Error> {
            let value = <$ty>::deserialize(d)?;
            if value == $value {
                Ok(value)
            } else {
                Err(serde::de::Error::custom("unsupported flow fixed field"))
            }
        }
    };
}
fixed!(one, u32, 1);
fixed!(producer, String, "knowledge_flow");
fixed!(profile_producer, String, "flow_render_profile");
fixed!(scope, String, "active_flow_v1");
fixed!(origin, String, "deterministic_planner");
fixed!(pending, String, "pending");
fixed!(science, String, "not_run");
fixed!(research, String, "not_requested");
fixed!(no, bool, false);
fixed!(encoding, String, "decimal_ascii_punctuation_v1");
fixed!(direction, String, "TD");

macro_rules! dto {
    ($name:ident {$($(#[$attr:meta])* $field:ident:$ty:ty),* $(,)?}) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct $name { $($(#[$attr])* pub(crate) $field:$ty),* }
    };
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FlowchartSupport {
    Supported,
    Unverified,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Representation {
    Flowchart,
    OrderedList,
}

fn profile_version<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    match u32::deserialize(d)? {
        version @ (1 | 2) => Ok(version),
        _ => Err(serde::de::Error::custom("unsupported flow profile schema")),
    }
}

fn present_nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<Option<T>>, D::Error> {
    Ok(Some(Option::deserialize(d)?))
}

fn source_sha<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(
            "flow renderer requires a lowercase SHA256",
        ))
    }
}

dto!(FlowRenderer {
    name: String,
    version: String,
    #[serde(deserialize_with = "source_sha")]
    source_sha256: String
});

dto!(FlowProfileWire {
    #[serde(deserialize_with = "profile_version")]
    schema_version: u32,
    #[serde(deserialize_with = "profile_producer")]
    producer: String,
    profile_id: String,
    app: String,
    app_version: String,
    #[serde(deserialize_with = "nullable")]
    mermaid_version: Option<String>,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")]
    renderer: Option<Option<FlowRenderer>>,
    flowchart_support: FlowchartSupport,
    #[serde(deserialize_with = "encoding")]
    label_encoding: String,
    html_labels: bool
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "FlowProfileWire", into = "FlowProfileWire")]
pub(crate) struct FlowProfile {
    pub(crate) schema_version: u32,
    pub(crate) producer: String,
    pub(crate) profile_id: String,
    pub(crate) app: String,
    pub(crate) app_version: String,
    pub(crate) mermaid_version: Option<String>,
    pub(crate) renderer: Option<FlowRenderer>,
    pub(crate) flowchart_support: FlowchartSupport,
    pub(crate) label_encoding: String,
    pub(crate) html_labels: bool,
}

impl TryFrom<FlowProfileWire> for FlowProfile {
    type Error = &'static str;

    fn try_from(value: FlowProfileWire) -> Result<Self, Self::Error> {
        let renderer = match value.schema_version {
            1 if value.mermaid_version.is_some() && value.renderer.is_none() => None,
            2 if value.mermaid_version.is_none() => Some(
                value
                    .renderer
                    .flatten()
                    .ok_or("schema2 requires an explicit renderer")?,
            ),
            _ => return Err("flow profile fields do not match its schema"),
        };
        Ok(Self {
            schema_version: value.schema_version,
            producer: value.producer,
            profile_id: value.profile_id,
            app: value.app,
            app_version: value.app_version,
            mermaid_version: value.mermaid_version,
            renderer,
            flowchart_support: value.flowchart_support,
            label_encoding: value.label_encoding,
            html_labels: value.html_labels,
        })
    }
}

impl From<FlowProfile> for FlowProfileWire {
    fn from(value: FlowProfile) -> Self {
        Self {
            schema_version: value.schema_version,
            producer: value.producer,
            profile_id: value.profile_id,
            app: value.app,
            app_version: value.app_version,
            mermaid_version: value.mermaid_version,
            renderer: if value.schema_version == 1 {
                None
            } else {
                Some(value.renderer)
            },
            flowchart_support: value.flowchart_support,
            label_encoding: value.label_encoding,
            html_labels: value.html_labels,
        }
    }
}
dto!(FlowIssue {code:String, object_ids:Vec<String>, message:String});
dto!(FallbackReason {code:String, object_ids:Vec<String>, message:String});
dto!(FlowDecision {rule_id:String, reason:String, fallback_reasons:Vec<FallbackReason>});
dto!(FlowNode {
    node_id: String,
    step_id: String,
    kind: StepKind,
    text: String
});
dto!(FlowEdge {
    edge_id:String, relation_id:String, left_node_id:String, right_node_id:String,
    kind:RelationKind,
    #[serde(deserialize_with="nullable")] branch_outcome:Option<bool>, text:String
});
dto!(FlowDiagram {#[serde(deserialize_with="direction")] direction:String, nodes:Vec<FlowNode>, edges:Vec<FlowEdge>});
dto!(FlowLink {target_path:String, target_heading:String, display_text:String, source_object_ids:Vec<String>});
dto!(FlowText {block_id:String, text:String, source_object_ids:Vec<String>});
dto!(FlowListItem {item_id:String, text:String, source_object_ids:Vec<String>, links:Vec<FlowLink>});
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum FlowBlock {
    Diagram {
        block_id: String,
    },
    Text {
        content: FlowText,
    },
    OrderedList {
        block_id: String,
        items: Vec<FlowListItem>,
    },
    Links {
        block_id: String,
        links: Vec<FlowLink>,
    },
}
dto!(FlowSection {section_id:String, heading:String, blocks:Vec<FlowBlock>});
dto!(FlowNote {note_id:String, output_path:String, title:String, sections:Vec<FlowSection>});
dto!(FlowPlan {
    #[serde(deserialize_with="one")] schema_version:u32,
    #[serde(deserialize_with="producer")] producer:String,
    #[serde(deserialize_with="scope")] scope:String,
    #[serde(deserialize_with="origin")] representation_origin:String,
    request_id:String,
    #[serde(deserialize_with="one")] revision:u32,
    #[serde(deserialize_with="pending")] knowledge_status:String,
    #[serde(deserialize_with="science")] scientific_quality:String,
    #[serde(deserialize_with="research")] research_status:String,
    #[serde(deserialize_with="no")] published:bool,
    document:ProcessDocument, profile:FlowProfile, representation:Representation,
    decision:FlowDecision, topological_order:Vec<String>,
    #[serde(deserialize_with="nullable")] diagram:Option<FlowDiagram>, notes:Vec<FlowNote>
});
dto!(FlowContentMapping {
    section_id:String, block_id:String, kind:String,
    #[serde(deserialize_with="nullable")] item_id:Option<String>,
    source_object_ids:Vec<String>, start_byte:u64, end_byte:u64
});
dto!(FlowLinkMapping {
    section_id:String, block_id:String,
    #[serde(deserialize_with="nullable")] item_id:Option<String>,
    target_path:String, target_heading:String, display_text:String,
    source_object_ids:Vec<String>, start_byte:u64, end_byte:u64
});
dto!(FlowRenderedFile {
    relative_path:String, note_id:String, plan_revision:u32, content_sha256:String,
    byte_length:u64, content_mappings:Vec<FlowContentMapping>, links:Vec<FlowLinkMapping>
});

#[derive(Debug)]
pub(crate) struct FlowRenderedNote {
    pub(crate) file: FlowRenderedFile,
    pub(crate) bytes: Vec<u8>,
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    fn engine_profile() -> serde_json::Value {
        serde_json::json!({
            "schema_version": 2, "producer": "flow_render_profile",
            "profile_id": "obsidian_1_15_0_pele_flowchart", "app": "Obsidian", "app_version": "1.15.0",
            "mermaid_version": null,
            "renderer": {"name": "pele", "version": "1.8.0", "source_sha256": "9bf34b3a9a19351dc69d0d238e365560331a8ae392c41656df7f5090653253d0"},
            "flowchart_support": "supported", "label_encoding": "decimal_ascii_punctuation_v1", "html_labels": false
        })
    }

    #[test]
    fn each_profile_schema_round_trips_without_fabricating_a_mermaid_version() {
        let old = serde_json::to_value(crate::process_flow_plan::tests::profile()).unwrap();
        assert!(!old.as_object().unwrap().contains_key("renderer"));
        for value in [old, engine_profile()] {
            let profile: FlowProfile = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(profile).unwrap(), value);
        }
    }

    #[test]
    fn profile_schema_rejects_mixed_missing_null_and_unbounded_engine_fields() {
        let original = engine_profile();
        for field in [
            "schema_version",
            "producer",
            "profile_id",
            "app",
            "app_version",
            "mermaid_version",
            "renderer",
            "flowchart_support",
            "label_encoding",
            "html_labels",
        ] {
            let mut bad = original.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<FlowProfile>(bad).is_err(),
                "{field}"
            );
        }
        for (field, value) in [
            ("schema_version", serde_json::json!(3)),
            ("renderer", serde_json::Value::Null),
            ("mermaid_version", serde_json::json!("12.1.0")),
        ] {
            let mut bad = original.clone();
            bad[field] = value;
            assert!(
                serde_json::from_value::<FlowProfile>(bad).is_err(),
                "{field}"
            );
        }
        for value in [
            "",
            "bad",
            "9BF34B3A9A19351DC69D0D238E365560331A8AE392C41656DF7F5090653253D0",
        ] {
            let mut bad = original.clone();
            bad["renderer"]["source_sha256"] = value.into();
            assert!(serde_json::from_value::<FlowProfile>(bad).is_err());
        }
        let mut bad = original;
        bad["renderer"]["verified"] = true.into();
        assert!(serde_json::from_value::<FlowProfile>(bad).is_err());
        let mut old = serde_json::to_value(crate::process_flow_plan::tests::profile()).unwrap();
        old["renderer"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<FlowProfile>(old).is_err());
    }
}
