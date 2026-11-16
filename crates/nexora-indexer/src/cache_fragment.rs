use crate::FileCacheKey;
use nexora_core::{AnalysisDiagnostic, NexoraError};
use nexora_graph::{CodeGraph, GraphIntegrityValidator};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheFragment {
    pub schema_version: u16,
    pub payload_hash: String,
    pub key: FileCacheKey,
    pub graph: CodeGraph,
    pub diagnostics: Vec<AnalysisDiagnostic>,
    pub computation_us: u64,
}
impl CacheFragment {
    pub fn new(
        key: FileCacheKey,
        graph: CodeGraph,
        computation_us: u64,
    ) -> Result<Self, NexoraError> {
        let mut fragment = Self {
            schema_version: 2,
            payload_hash: String::new(),
            key,
            graph,
            diagnostics: Vec::new(),
            computation_us,
        };
        fragment.payload_hash = fragment.digest()?;
        fragment.validate_for(&fragment.key)?;
        Ok(fragment)
    }
    fn digest(&self) -> Result<String, NexoraError> {
        crate::cache_fragment_digest::CacheFragmentDigest::of(&(
            self.schema_version,
            &self.key,
            &self.graph,
            &self.diagnostics,
            self.computation_us,
        ))
    }
    pub fn same_analysis(&self, other: &Self) -> Result<bool, NexoraError> {
        let hash = |f: &Self| {
            crate::cache_fragment_digest::CacheFragmentDigest::of(&(
                f.schema_version,
                &f.key,
                &f.graph,
                &f.diagnostics,
            ))
        };
        Ok(hash(self)? == hash(other)?)
    }

    pub fn validate_for(&self, key: &FileCacheKey) -> Result<(), NexoraError> {
        let files = self
            .graph
            .nodes
            .iter()
            .filter(|node| node.kind == nexora_graph::NodeKind::File)
            .collect::<Vec<_>>();
        if self.schema_version != 2
            || &self.key != key
            || !self.diagnostics.is_empty()
            || files.len() != 1
            || files[0].name != key.path
            || self
                .graph
                .nodes
                .iter()
                .any(|node| node.evidence.as_ref().is_some_and(|e| e.path != key.path))
            || self
                .graph
                .edges
                .iter()
                .any(|edge| edge.evidence.path != key.path)
        {
            return Err(NexoraError::Storage(
                "invalid cached fragment identity".into(),
            ));
        }
        GraphIntegrityValidator::validate(&self.graph)?;
        if self.digest()? != self.payload_hash {
            return Err(NexoraError::Storage(
                "cached fragment checksum mismatch".into(),
            ));
        }
        Ok(())
    }
}
