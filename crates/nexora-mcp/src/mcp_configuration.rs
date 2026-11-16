use nexora_core::{NexoraError, RepositoryId, TenantId};
use nexora_indexer::SourceReader;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub struct McpConfiguration {
    pub(crate) root: PathBuf,
    pub(crate) tenant: TenantId,
    pub(crate) repository: RepositoryId,
    pub(crate) connections: PathBuf,
    pub(crate) language_dir: PathBuf,
    pub(crate) language: Option<String>,
    pub(crate) allow_index: bool,
    pub(crate) knowledge_actor: Option<String>,
}
impl McpConfiguration {
    pub fn new(
        root: &Path,
        tenant: &str,
        connections: &Path,
        language_dir: &Path,
        language: Option<String>,
        allow_index: bool,
    ) -> Result<Self, NexoraError> {
        if tenant.is_empty() || tenant.len() > 128 || tenant.chars().any(char::is_control) {
            return Err(NexoraError::Configuration(
                "MCP tenant must be 1..128 bytes without control characters".into(),
            ));
        }
        let root = SourceReader::validate_path(root)?;
        if !root.is_dir() {
            return Err(NexoraError::InvalidPath(
                "MCP root must be a directory".into(),
            ));
        }
        let repository = RepositoryId(format!(
            "{:x}",
            Sha256::digest(root.to_string_lossy().as_bytes())
        ));
        Ok(Self {
            root,
            tenant: TenantId(tenant.into()),
            repository,
            connections: connections.into(),
            language_dir: language_dir.into(),
            language,
            allow_index,
            knowledge_actor: None,
        })
    }
    pub fn with_knowledge_actor(mut self, actor: String) -> Result<Self, NexoraError> {
        if actor.trim().is_empty() || actor.len() > 128 || actor.chars().any(char::is_control) {
            return Err(NexoraError::Configuration(
                "knowledge actor must be 1..128 bytes without control characters".into(),
            ));
        }
        self.knowledge_actor = Some(actor);
        Ok(self)
    }
}
