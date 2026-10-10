//! Typed immutable normalized wire records.
use crate::error::Result;
use crate::ids;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub enum NodeKind {
    #[serde(rename = "SourceFile")]
    SourceFile,
    #[serde(rename = "DocumentSection")]
    DocumentSection,
    #[serde(rename = "CodeSymbol")]
    CodeSymbol,
    #[serde(rename = "ExternalSymbol")]
    ExternalSymbol,
    #[serde(rename = "BinaryFunction")]
    BinaryFunction,
    #[serde(rename = "DataType")]
    DataType,
    #[serde(rename = "TypeField")]
    TypeField,
    #[serde(rename = "Concept")]
    Concept,
    #[serde(rename = "Asset")]
    Asset,
    #[serde(rename = "AssetBlock")]
    AssetBlock,
    #[serde(rename = "AssetField")]
    AssetField,
    #[serde(rename = "Material")]
    Material,
    #[serde(rename = "TextureBinding")]
    TextureBinding,
    #[serde(rename = "ConversionStep")]
    ConversionStep,
    #[serde(rename = "ShaderModule")]
    ShaderModule,
    #[serde(rename = "ShaderBinding")]
    ShaderBinding,
    #[serde(rename = "Capture")]
    Capture,
    #[serde(rename = "DrawEvent")]
    DrawEvent,
    #[serde(rename = "GpuResource")]
    GpuResource,
    #[serde(rename = "RuntimeObservation")]
    RuntimeObservation,
    #[serde(rename = "ResearchClaim")]
    ResearchClaim,
}
impl NodeKind {
    /// Exact ontology wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SourceFile => "SourceFile",
            Self::DocumentSection => "DocumentSection",
            Self::CodeSymbol => "CodeSymbol",
            Self::ExternalSymbol => "ExternalSymbol",
            Self::BinaryFunction => "BinaryFunction",
            Self::DataType => "DataType",
            Self::TypeField => "TypeField",
            Self::Concept => "Concept",
            Self::Asset => "Asset",
            Self::AssetBlock => "AssetBlock",
            Self::AssetField => "AssetField",
            Self::Material => "Material",
            Self::TextureBinding => "TextureBinding",
            Self::ConversionStep => "ConversionStep",
            Self::ShaderModule => "ShaderModule",
            Self::ShaderBinding => "ShaderBinding",
            Self::Capture => "Capture",
            Self::DrawEvent => "DrawEvent",
            Self::GpuResource => "GpuResource",
            Self::RuntimeObservation => "RuntimeObservation",
            Self::ResearchClaim => "ResearchClaim",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
pub enum Predicate {
    #[serde(rename = "CONTAINS")]
    Contains,
    #[serde(rename = "DECLARES")]
    Declares,
    #[serde(rename = "DEFINES")]
    Defines,
    #[serde(rename = "HAS_FIELD")]
    HasField,
    #[serde(rename = "REFERENCES")]
    References,
    #[serde(rename = "CALLS_DIRECT")]
    CallsDirect,
    #[serde(rename = "MAY_CALL")]
    MayCall,
    #[serde(rename = "READS_FIELD")]
    ReadsField,
    #[serde(rename = "WRITES_FIELD")]
    WritesField,
    #[serde(rename = "HAS_TYPE")]
    HasType,
    #[serde(rename = "INHERITS")]
    Inherits,
    #[serde(rename = "HAS_VTABLE_SLOT")]
    HasVtableSlot,
    #[serde(rename = "DOCUMENTS")]
    Documents,
    #[serde(rename = "NAMES")]
    Names,
    #[serde(rename = "DESCRIBES_CONCEPT")]
    DescribesConcept,
    #[serde(rename = "USES_TEXTURE")]
    UsesTexture,
    #[serde(rename = "HAS_MATERIAL")]
    HasMaterial,
    #[serde(rename = "HAS_BINDING")]
    HasBinding,
    #[serde(rename = "INPUT_TO")]
    InputTo,
    #[serde(rename = "PRODUCES")]
    Produces,
    #[serde(rename = "DERIVED_FROM")]
    DerivedFrom,
    #[serde(rename = "PRUNED_REFERENCE")]
    PrunedReference,
    #[serde(rename = "HANDLES_CONCEPT")]
    HandlesConcept,
    #[serde(rename = "CORRESPONDS_TO")]
    CorrespondsTo,
    #[serde(rename = "CAPTURED_IN")]
    CapturedIn,
    #[serde(rename = "BINDS_RESOURCE")]
    BindsResource,
    #[serde(rename = "OBSERVED_AT")]
    ObservedAt,
    #[serde(rename = "OBSERVED_CALL")]
    ObservedCall,
    #[serde(rename = "IMPORTS")]
    Imports,
    #[serde(rename = "EVOLVES_TO")]
    EvolvesTo,
    #[serde(rename = "SUPPORTS")]
    Supports,
    #[serde(rename = "CHALLENGES")]
    Challenges,
    #[serde(rename = "DUPLICATES")]
    Duplicates,
    #[serde(rename = "RELATED_TO")]
    RelatedTo,
    #[serde(rename = "DEPENDS_ON")]
    DependsOn,
}
impl Predicate {
    /// Exact ontology wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "CONTAINS",
            Self::Declares => "DECLARES",
            Self::Defines => "DEFINES",
            Self::HasField => "HAS_FIELD",
            Self::References => "REFERENCES",
            Self::CallsDirect => "CALLS_DIRECT",
            Self::MayCall => "MAY_CALL",
            Self::ReadsField => "READS_FIELD",
            Self::WritesField => "WRITES_FIELD",
            Self::HasType => "HAS_TYPE",
            Self::Inherits => "INHERITS",
            Self::HasVtableSlot => "HAS_VTABLE_SLOT",
            Self::Documents => "DOCUMENTS",
            Self::Names => "NAMES",
            Self::DescribesConcept => "DESCRIBES_CONCEPT",
            Self::UsesTexture => "USES_TEXTURE",
            Self::HasMaterial => "HAS_MATERIAL",
            Self::HasBinding => "HAS_BINDING",
            Self::InputTo => "INPUT_TO",
            Self::Produces => "PRODUCES",
            Self::DerivedFrom => "DERIVED_FROM",
            Self::PrunedReference => "PRUNED_REFERENCE",
            Self::HandlesConcept => "HANDLES_CONCEPT",
            Self::CorrespondsTo => "CORRESPONDS_TO",
            Self::CapturedIn => "CAPTURED_IN",
            Self::BindsResource => "BINDS_RESOURCE",
            Self::ObservedAt => "OBSERVED_AT",
            Self::ObservedCall => "OBSERVED_CALL",
            Self::Imports => "IMPORTS",
            Self::EvolvesTo => "EVOLVES_TO",
            Self::Supports => "SUPPORTS",
            Self::Challenges => "CHALLENGES",
            Self::Duplicates => "DUPLICATES",
            Self::RelatedTo => "RELATED_TO",
            Self::DependsOn => "DEPENDS_ON",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Reviewed,
    Restricted,
}
impl Visibility {
    /// Stored visibility name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Reviewed => "reviewed",
            Self::Restricted => "restricted",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    Extracted,
    Community,
    Observed,
    Manual,
    Hypothesis,
}
impl Provenance {
    /// Stored provenance name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Extracted => "extracted",
            Self::Community => "community",
            Self::Observed => "observed",
            Self::Manual => "manual",
            Self::Hypothesis => "hypothesis",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub kind: String,
    pub origin_uri: String,
    pub visibility: Visibility,
    pub license_expression: Option<String>,
    pub lineage_roots: Vec<String>,
    pub redistribution: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extraction {
    pub tool: String,
    pub version: String,
    pub configuration_sha256: String,
    pub scope_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityCounts {
    pub inputs_enumerated: Option<u64>,
    pub inputs_attempted: Option<u64>,
    pub records_emitted: Option<u64>,
    pub inputs_failed: Option<u64>,
    pub inputs_skipped: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub name: String,
    pub status: String,
    pub detail: String,
    pub counts: CapabilityCounts,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub id: String,
    pub source_id: String,
    pub revision: String,
    pub content_sha256: String,
    pub extraction: Extraction,
    pub scope: Value,
    pub dependencies: Vec<String>,
    pub visibility: Visibility,
    pub capabilities: Vec<Capability>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub media_type: String,
    pub byte_size: u64,
    pub visibility: Visibility,
    pub blob_sha256: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub id: String,
    pub artifact_id: String,
    pub locator: Value,
    pub method: String,
    pub provenance_class: Provenance,
    pub visibility: Visibility,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub logical_key: String,
    pub display_name: String,
    pub attributes: Value,
    pub evidence_ids: Vec<String>,
    pub visibility: Visibility,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceLink {
    pub id: String,
    pub role: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub id: String,
    pub subject: String,
    pub predicate: Predicate,
    pub object: String,
    pub attributes: Value,
    pub evidence: Vec<EvidenceLink>,
    pub provenance_class: Provenance,
    pub initial_state: String,
    pub visibility: Visibility,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub format: String,
    pub schema_version: u32,
    pub source: Source,
    pub snapshot: Snapshot,
    pub artifacts: Vec<Artifact>,
    pub evidence: Vec<Evidence>,
    pub nodes: Vec<Node>,
    pub assertions: Vec<Assertion>,
}
impl Snapshot {
    /// Identity from upstream inputs, excluding labels and runs.
    pub fn expected_id(&self) -> Result<String> {
        let mut deps = self.dependencies.clone();
        deps.sort();
        deps.dedup();
        let d = ids::digest(&serde_json::to_value(deps)?)?;
        Ok(ids::id(
            "snap",
            &[
                &self.source_id,
                &self.revision,
                &self.content_sha256,
                &self.extraction.tool,
                &self.extraction.version,
                &self.extraction.configuration_sha256,
                &self.extraction.scope_sha256,
                &d,
            ],
        ))
    }
}
impl Assertion {
    /// Assertion identity preserves ordered roles and original provenance.
    pub fn expected_id(&self, snapshot: &str) -> Result<String> {
        let mut links = self.evidence.clone();
        links.sort_by(|a, b| (&a.id, &a.role).cmp(&(&b.id, &b.role)));
        links.dedup();
        Ok(ids::id(
            "as",
            &[
                snapshot,
                &self.subject,
                self.predicate.as_str(),
                &self.object,
                &ids::digest(&self.attributes)?,
                &ids::digest(&serde_json::to_value(links)?)?,
                self.provenance_class.as_str(),
            ],
        ))
    }
}
impl std::str::FromStr for NodeKind {
    type Err = serde_json::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        serde_json::from_value(serde_json::Value::String(s.into()))
    }
}
impl std::str::FromStr for Predicate {
    type Err = serde_json::Error;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        serde_json::from_value(serde_json::Value::String(s.into()))
    }
}
