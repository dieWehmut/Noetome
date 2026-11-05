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

dto!(FlowProfile {
    #[serde(deserialize_with = "one")]
    schema_version: u32,
    #[serde(deserialize_with = "profile_producer")]
    producer: String,
    profile_id: String,
    app: String,
    app_version: String,
    mermaid_version: String,
    flowchart_support: FlowchartSupport,
    #[serde(deserialize_with = "encoding")]
    label_encoding: String,
    html_labels: bool
});
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
