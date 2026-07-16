use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DOM_ARTIFACT_SCHEMA: &str = "rustqjsdom.artifact";
pub const DOM_ARTIFACT_VERSION: u32 = 2;

/// Stable, typed output of the renderer-free DOM pipeline.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomArtifact {
    pub schema: String,
    pub schema_version: u32,
    pub source: SourceMetadata,
    pub timings: TimingMetadata,
    pub document: DomNode,
    pub style_index: StyleIndex,
    /// Renderer-neutral resource references. The browser host decides whether
    /// and how to resolve, fetch, decode, cache, or merely log each request.
    pub asset_index: AssetIndex,
    /// Renderer-neutral TrueSurfer descriptors. Their flexible registry metadata
    /// remains JSON until that contract settles independently of the DOM schema.
    pub widget_tree: Value,
    pub widget_stats: WidgetStats,
    pub extracted: ExtractedArtifacts,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndex {
    pub backend: String,
    pub base_href: Option<String>,
    #[serde(default)]
    pub requests: Vec<AssetRequest>,
    pub request_count: usize,
    #[serde(default)]
    pub kind_counts: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRequest {
    pub path: String,
    pub tag: String,
    pub attribute: String,
    pub raw_url: String,
    pub base_url: String,
    pub kind: String,
    pub initiator: String,
    pub media_type: String,
}

impl DomArtifact {
    pub fn validate_contract(&self) -> Result<(), String> {
        if self.schema != DOM_ARTIFACT_SCHEMA {
            return Err(format!(
                "unexpected artifact schema {:?}; expected {:?}",
                self.schema, DOM_ARTIFACT_SCHEMA
            ));
        }
        if self.schema_version != DOM_ARTIFACT_VERSION {
            return Err(format!(
                "unsupported artifact schema version {}; expected {}",
                self.schema_version, DOM_ARTIFACT_VERSION
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMetadata {
    pub url: String,
    pub bytes: usize,
    pub lines: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingMetadata {
    pub parse5_ms: u64,
    pub lightning_css_ms: u64,
    pub total_ms: u64,
}

/// Acyclic Parse5 node suitable for ownership by Solara or another Rust host.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomNode {
    pub node_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_name: Option<String>,
    #[serde(
        rename = "namespaceURI",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub namespace_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<DomAttribute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_ref: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DomNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Box<DomNode>>,
}

/// Renderer-neutral output of the post-Parse5 Lightning CSS cascade.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleIndex {
    pub backend: String,
    #[serde(default)]
    pub style_table: Vec<ComputedStyle>,
    #[serde(default)]
    pub node_style_refs: Vec<NodeStyleRef>,
    pub style_slot_count: usize,
    pub node_ref_count: usize,
    pub inline_style_count: usize,
    pub stylesheet_count: usize,
    pub external_stylesheet_count: usize,
    pub rule_count: usize,
    pub element_count: usize,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub load_errors: Vec<String>,
    pub summary: String,
}

impl StyleIndex {
    pub fn style(&self, style_ref: usize) -> Option<&ComputedStyle> {
        self.style_table.get(style_ref)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeStyleRef {
    pub path: String,
    pub style_ref: usize,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputedStyle {
    /// Winning author declarations represented as normalized CSS property names.
    /// Consumers can distinguish page CSS from renderer-neutral user-agent defaults.
    #[serde(default)]
    pub authored_properties: Vec<String>,
    pub display: Option<String>,
    pub color: Option<String>,
    pub background_color: Option<String>,
    pub font_size_px: Option<f32>,
    pub line_height_px: Option<f32>,
    pub font_weight: Option<String>,
    pub font_style: Option<String>,
    pub text_align: Option<String>,
    pub white_space: Option<String>,
    pub margin_left_px: Option<f32>,
    pub margin_top_px: Option<f32>,
    pub margin_right_px: Option<f32>,
    pub margin_bottom_px: Option<f32>,
    pub padding_left_px: Option<f32>,
    pub padding_top_px: Option<f32>,
    pub padding_right_px: Option<f32>,
    pub padding_bottom_px: Option<f32>,
    pub border_width_px: Option<f32>,
    pub border_color: Option<String>,
}

impl DomNode {
    pub fn is_element(&self) -> bool {
        self.tag_name.is_some()
    }

    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|attribute| attribute.name.eq_ignore_ascii_case(name))
            .map(|attribute| attribute.value.as_str())
    }

    pub fn walk(&self, visitor: &mut impl FnMut(&DomNode)) {
        visitor(self);
        for child in &self.children {
            child.walk(visitor);
        }
        if let Some(content) = &self.content {
            content.walk(visitor);
        }
    }

    pub fn find_element_by_id(&self, id: &str) -> Option<&DomNode> {
        if self.attribute("id") == Some(id) {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|child| child.find_element_by_id(id))
            .or_else(|| {
                self.content
                    .as_deref()
                    .and_then(|content| content.find_element_by_id(id))
            })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomAttribute {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetStats {
    pub nodes: u64,
    pub widgets: u64,
    pub text: u64,
    pub complex: u64,
    pub interactive: u64,
    #[serde(default)]
    pub tags: BTreeMap<String, u64>,
    #[serde(default)]
    pub categories: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedArtifacts {
    pub title: Option<String>,
    pub favicon_href: Option<String>,
    pub shell_html: String,
    pub body_html: String,
    #[serde(default)]
    pub body_hierarchy: Vec<Value>,
    pub body_hierarchy_summary: String,
    pub style_count: u64,
    pub style_bytes: u64,
    pub script_count: u64,
    pub script_bytes: u64,
    #[serde(default)]
    pub styles: Vec<Value>,
    #[serde(default)]
    pub scripts: Vec<Value>,
}
