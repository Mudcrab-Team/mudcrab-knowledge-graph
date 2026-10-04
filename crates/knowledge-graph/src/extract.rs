//! Serialization-only extraction builder shared by graph-side adapters.
use crate::{error::Result, ids, model::*};
use serde_json::{Value, json};
use std::collections::BTreeMap;
/// Records and immutable artifact bytes, generated from a declared upstream fingerprint.
pub struct ExtractionBuilder {
    pub bundle: Bundle,
    pub bytes: BTreeMap<String, Vec<u8>>,
}
impl ExtractionBuilder {
    /// Establish snapshot identity before creating any graph records.
    pub fn new(
        source: Source,
        revision: &str,
        content: &str,
        tool: &str,
        config: Value,
        scope: Value,
        dependencies: Vec<String>,
    ) -> Result<Self> {
        let mut snapshot = Snapshot {
            id: String::new(),
            source_id: source.id.clone(),
            revision: revision.into(),
            content_sha256: content.into(),
            extraction: Extraction {
                tool: tool.into(),
                version: env!("CARGO_PKG_VERSION").into(),
                configuration_sha256: ids::digest(&config)?,
                scope_sha256: ids::digest(&scope)?,
            },
            scope,
            dependencies,
            visibility: source.visibility,
            capabilities: vec![Capability {
                name: tool.into(),
                status: "partial".into(),
                detail: "declared selection only".into(),
                counts: CapabilityCounts {
                    inputs_enumerated: None,
                    inputs_attempted: None,
                    records_emitted: None,
                    inputs_failed: None,
                    inputs_skipped: None,
                },
            }],
        };
        snapshot.id = snapshot.expected_id()?;
        Ok(Self {
            bundle: Bundle {
                format: "mudcrab-kg-bundle".into(),
                schema_version: 1,
                source,
                snapshot,
                artifacts: vec![],
                evidence: vec![],
                nodes: vec![],
                assertions: vec![],
            },
            bytes: BTreeMap::new(),
        })
    }
    /// Add verified immutable bytes with a portable artifact path.
    pub fn artifact(&mut self, path: &str, media: &str, raw: Vec<u8>) -> Result<String> {
        ids::portable_path(path)?;
        let hash = ids::sha256(&raw);
        let id = ids::id("art", &[&self.bundle.snapshot.id, path, &hash]);
        if !self.bytes.contains_key(&id) {
            self.bundle.artifacts.push(Artifact {
                id: id.clone(),
                path: path.into(),
                sha256: hash.clone(),
                media_type: media.into(),
                byte_size: raw.len() as u64,
                visibility: self.bundle.source.visibility,
                blob_sha256: Some(hash),
            });
            self.bytes.insert(id.clone(), raw);
        }
        Ok(id)
    }
    /// Create a locator from literal extraction or a qualified interpretation.
    pub fn evidence(
        &mut self,
        artifact: &str,
        locator: Value,
        method: &str,
        class: Provenance,
    ) -> Result<String> {
        let id = ids::id(
            "ev",
            &[
                &self.bundle.snapshot.id,
                artifact,
                &ids::digest(&locator)?,
                method,
                class.as_str(),
            ],
        );
        if !self.bundle.evidence.iter().any(|e| e.id == id) {
            self.bundle.evidence.push(Evidence {
                id: id.clone(),
                artifact_id: artifact.into(),
                locator,
                method: method.into(),
                provenance_class: class,
                visibility: self.bundle.source.visibility,
            });
        }
        Ok(id)
    }
    /// Create an independently citable version-scoped node.
    pub fn node(
        &mut self,
        kind: NodeKind,
        key: &str,
        name: &str,
        attributes: Value,
        evidence: &[String],
    ) -> String {
        let id = ids::id("node", &[&self.bundle.snapshot.id, kind.as_str(), key]);
        if !self.bundle.nodes.iter().any(|n| n.id == id) {
            self.bundle.nodes.push(Node {
                id: id.clone(),
                kind,
                logical_key: key.into(),
                display_name: name.into(),
                attributes,
                evidence_ids: evidence.to_vec(),
                visibility: self.bundle.source.visibility,
            });
        }
        id
    }
    /// Add evidence-backed assertion. Manual mappings and hypotheses always start proposed.
    pub fn edge(
        &mut self,
        subject: &str,
        predicate: Predicate,
        object: &str,
        attributes: Value,
        evidence: &[String],
        class: Provenance,
    ) -> Result<String> {
        let mut a = Assertion {
            id: String::new(),
            subject: subject.into(),
            predicate,
            object: object.into(),
            attributes,
            evidence: evidence
                .iter()
                .map(|id| EvidenceLink {
                    id: id.clone(),
                    role: "supports".into(),
                })
                .collect(),
            provenance_class: class,
            initial_state: if matches!(class, Provenance::Manual | Provenance::Hypothesis) {
                "proposed"
            } else {
                "admitted"
            }
            .into(),
            visibility: self.bundle.source.visibility,
        };
        a.id = a.expected_id(&self.bundle.snapshot.id)?;
        let id = a.id.clone();
        if !self.bundle.assertions.iter().any(|a| a.id == id) {
            self.bundle.assertions.push(a);
        }
        Ok(id)
    }
    /// Attach measured capability counts to a completed extraction selection.
    pub fn capability(
        &mut self,
        name: &str,
        status: &str,
        detail: &str,
        attempted: u64,
        failed: u64,
        skipped: u64,
    ) {
        let emitted = (self.bundle.artifacts.len()
            + self.bundle.evidence.len()
            + self.bundle.nodes.len()
            + self.bundle.assertions.len()) as u64;
        self.bundle.snapshot.capabilities.push(Capability {
            name: name.into(),
            status: status.into(),
            detail: detail.into(),
            counts: CapabilityCounts {
                inputs_enumerated: Some(attempted + skipped),
                inputs_attempted: Some(attempted),
                records_emitted: Some(emitted),
                inputs_failed: Some(failed),
                inputs_skipped: Some(skipped),
            },
        });
    }
}
/// Preserve JSON numeric observations as exact typed bit patterns, not identity floats.
pub fn tagged_numbers(v: Value) -> Value {
    match v {
        Value::Number(ref n) if n.is_f64() => {
            let f = n.as_f64().expect("float");
            json!({"type":"f64","bits":format!("0x{:016x}",f.to_bits()),"display":f.to_string()})
        }
        Value::Array(a) => Value::Array(a.into_iter().map(tagged_numbers).collect()),
        Value::Object(o) => {
            Value::Object(o.into_iter().map(|(k, v)| (k, tagged_numbers(v))).collect())
        }
        v => v,
    }
}
